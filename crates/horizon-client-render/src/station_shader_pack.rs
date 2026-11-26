//! Minecraft GLSL final-stage adapter. Scene post-processing only, not a complete Iris renderer.
use horizon_shaders::{Macros, Pack, Program, Target};
use std::{collections::BTreeMap, path::Path};
/// Explicit scene contract for a final-only adapter, not arbitrary colortex emulation.
#[derive(Clone, Debug)]
pub struct FinalInput {
    pub color: String,
    pub srgb: bool,
}
impl Default for FinalInput {
    fn default() -> Self {
        Self {
            color: "colortex0".into(),
            srgb: false,
        }
    }
}
impl FinalInput {
    pub fn parse(color: &str, encoding: &str) -> Result<Self, String> {
        let slot = color
            .strip_prefix("colortex")
            .and_then(|n| n.parse::<u8>().ok());
        if !slot.is_some_and(|n| n < 16 && color == format!("colortex{n}")) {
            return Err(format!(
                "invalid final scene input: {color}; expected colortex0..15"
            ));
        }
        let srgb = match encoding {
            "linear" => false,
            "srgb" => true,
            _ => return Err(format!("invalid final scene encoding: {encoding}")),
        };
        Ok(Self {
            color: color.into(),
            srgb,
        })
    }
}
fn validate_input(
    interface: &horizon_shaders::Interface,
    input: &FinalInput,
) -> Result<(), String> {
    FinalInput::parse(&input.color, if input.srgb { "srgb" } else { "linear" })?;
    let mut mapped = interface.clone();
    for sampler in &mut mapped.samplers {
        if sampler.name == input.color {
            sampler.name = "colortex0".into();
        } else if sampler.name.starts_with("colortex")
            || (sampler.name == "gcolor" && input.color != "colortex0")
        {
            return Err(format!(
                "unbound final scene buffer {}; only {} is mapped",
                sampler.name, input.color
            ));
        }
    }
    validate_interface(&mapped)
}
pub fn load_final(path: &Path) -> Result<Program, String> {
    load_final_with_options(path, BTreeMap::new())
}
fn load_final_with_options(
    path: &Path,
    mut options: BTreeMap<String, String>,
) -> Result<Program, String> {
    options.extend(BTreeMap::from([
        ("COLORED_LIGHTS".into(), "false".into()),
        ("DISTANCE_VIEW".into(), "false".into()),
        ("DEBUG_VIEW".into(), "0".into()),
    ]));
    let pack = Pack::open(path)
        .map_err(|e| e.to_string())?
        .with_chosen(options);
    let prefix = if pack.exists("world0/final.fsh") {
        "world0/"
    } else {
        ""
    };
    let macros = Macros::standard();
    let vertex_path = if pack.exists(&format!("{prefix}final.vsh")) {
        format!("{prefix}final.vsh")
    } else {
        "final.vsh".into()
    };
    let vertex = horizon_shaders::preprocess(&pack, &vertex_path, &macros);
    let fragment = horizon_shaders::preprocess(&pack, &format!("{prefix}final.fsh"), &macros);
    if !vertex.diagnostics.is_empty() || !fragment.diagnostics.is_empty() {
        return Err(format!(
            "preprocess: {:?} {:?}",
            vertex.diagnostics, fragment.diagnostics
        ));
    }
    horizon_shaders::translate_composite(&vertex, &fragment, Target::Window)
        .map_err(|e| format!("GLSL: {e:?}"))
}
pub fn report(path: &Path) -> Result<String, String> {
    let p = load_final(path)?;
    Ok(format!(
        "Final stage translated. Uniforms: {:?}\nSamplers: {:?}\nOutputs: {:?}\nNotes: {:?}\nScope: translation report only, not a runtime compatibility claim. The generic adapter executes final only; separate Photon modes execute selected additional effects.\n",
        p.interface.uniforms, p.interface.samplers, p.interface.outputs, p.notes
    ))
}

fn validate_interface(interface: &horizon_shaders::Interface) -> Result<(), String> {
    if interface.samplers.iter().any(|s| {
        !matches!(
            s.name.as_str(),
            "colortex0" | "gcolor" | "stationAlbedo" | "stationNormal" | "depthtex0"
        ) || s.ty != "sampler2D"
    }) || !interface.images.is_empty()
        || !interface.buffers.is_empty()
        || interface.outputs != [0]
    {
        return Err(format!(
            "unsupported final-stage resources: {:?}",
            interface
        ));
    }
    for u in &interface.uniforms {
        let expected = match u.name.as_str() {
            "hz_target" | "hz_fog_color" | "hz_fog" | "hz_depth_linear" => "vec4",
            "viewWidth" | "viewHeight" | "aspectRatio" | "frameTimeCounter" | "frameTime" => {
                "float"
            }
            "frameCounter" => "int",
            _ => return Err(format!("unsupported uniform {} ({})", u.name, u.ty)),
        };
        if u.ty != expected {
            return Err(format!(
                "uniform {} requires {expected}, got {}",
                u.name, u.ty
            ));
        }
    }
    Ok(())
}

pub struct FinalStage {
    pipeline: wgpu::RenderPipeline,
    scene_data: Option<super::station_scene_data::SceneData>,
    material: Option<super::station_photon_material::Material>,
    ao: Option<super::station_photon_ao::Ao>,
    shadows: Option<super::station_photon_shadow::Shadows>,
    solas: Option<super::station_solas_post::Post>,
    flip: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    interface: horizon_shaders::Interface,
    targets: Option<(wgpu::Texture, wgpu::Texture)>,
    size: [u32; 2],
    format: wgpu::TextureFormat,
    start: std::time::Instant,
    previous: std::time::Instant,
    frame: u32,
}
impl FinalStage {
    pub fn new(
        device: &wgpu::Device,
        path: &Path,
        format: wgpu::TextureFormat,
    ) -> Result<Self, String> {
        Self::from_program(
            device,
            load_final(path)?,
            format,
            None,
            FinalInput::default(),
        )
    }
    pub fn new_with_input(
        device: &wgpu::Device,
        path: &Path,
        format: wgpu::TextureFormat,
        input: FinalInput,
    ) -> Result<Self, String> {
        Self::from_program(device, load_final(path)?, format, None, input)
    }
    fn new_solas_profile(
        device: &wgpu::Device,
        path: &Path,
        format: wgpu::TextureFormat,
        bloom: bool,
        dof: bool,
        motion: bool,
        taa: bool,
    ) -> Result<Self, String> {
        let pack = Pack::open(path).map_err(|e| e.to_string())?;
        // Read the file once so all passes use one validated snapshot, including final.
        let settings = super::station_solas_settings::load(&pack)?;
        let program = load_final_with_options(path, settings.clone())?;
        let mut stage = Self::from_program(
            device,
            program,
            format,
            None,
            FinalInput::parse("colortex1", "linear")?,
        )?;
        stage.scene_data = Some(super::station_scene_data::SceneData::new(device));
        stage.solas = Some(super::station_solas_post::Post::new(
            device, path, bloom, dof, motion, taa, settings,
        )?);
        Ok(stage)
    }
    pub fn new_solas_post(
        device: &wgpu::Device,
        path: &Path,
        format: wgpu::TextureFormat,
    ) -> Result<Self, String> {
        Self::new_solas_profile(device, path, format, false, false, false, false)
    }
    pub fn new_solas_bloom(
        device: &wgpu::Device,
        path: &Path,
        format: wgpu::TextureFormat,
    ) -> Result<Self, String> {
        Self::new_solas_profile(device, path, format, true, false, false, false)
    }
    pub fn new_solas_motion(
        device: &wgpu::Device,
        path: &Path,
        format: wgpu::TextureFormat,
    ) -> Result<Self, String> {
        Self::new_solas_profile(device, path, format, true, false, true, false)
    }
    pub fn new_solas_dof(
        device: &wgpu::Device,
        path: &Path,
        format: wgpu::TextureFormat,
    ) -> Result<Self, String> {
        Self::new_solas_profile(device, path, format, true, true, false, false)
    }
    pub fn new_solas_taa(device:&wgpu::Device,path:&Path,format:wgpu::TextureFormat)->Result<Self,String> {
        Self::new_solas_profile(device,path,format,true,false,false,true)
    }
    pub(super) fn update_dynamic_history(&mut self, vertices: &[super::Vertex]) {
        if let Some(solas) = &mut self.solas { solas.update_dynamic_history(vertices); }
    }
    pub fn scene_jitter(&self,size:[u32;2])->[f32;2] {
        self.solas.as_ref().map(|s|s.jitter(size)).unwrap_or([0.;2])
    }
    pub fn new_solas_autofocus(
        device: &wgpu::Device,
        path: &Path,
        format: wgpu::TextureFormat,
    ) -> Result<Self, String> {
        let mut stage = Self::new_solas_dof(device, path, format)?;
        stage.solas.as_mut().unwrap().enable_autofocus(device);
        Ok(stage)
    }
    pub fn new_material_debug(
        device: &wgpu::Device,
        path: &Path,
        format: wgpu::TextureFormat,
    ) -> Result<Self, String> {
        let (encode, decode) = super::station_photon_material::programs(path)?;
        let material = super::station_photon_material::Material::new(device, encode)?;
        Self::from_program(
            device,
            decode,
            format,
            Some(material),
            FinalInput::default(),
        )
    }
    pub fn new_ao(
        device: &wgpu::Device,
        path: &Path,
        format: wgpu::TextureFormat,
    ) -> Result<Self, String> {
        let (encode, _) = super::station_photon_material::programs(path)?;
        let material = super::station_photon_material::Material::new(device, encode)?;
        let mut stage = Self::from_program(
            device,
            load_final(path)?,
            format,
            Some(material),
            FinalInput::default(),
        )?;
        stage.ao = Some(super::station_photon_ao::Ao::new(device, path)?);
        Ok(stage)
    }
    pub fn shadow_cache_stats(&self) -> Option<(u64, u64)> {
        self.shadows.as_ref().map(|shadow| shadow.cache_stats())
    }
    pub fn new_shadows(
        device: &wgpu::Device,
        path: &Path,
        format: wgpu::TextureFormat,
    ) -> Result<Self, String> {
        let mut stage = Self::new_ao(device, path, format)?;
        stage.shadows = Some(super::station_photon_shadow::Shadows::new(
            device, path, false,
        )?);
        Ok(stage)
    }
    pub fn new_volumetrics(
        device: &wgpu::Device,
        path: &Path,
        format: wgpu::TextureFormat,
    ) -> Result<Self, String> {
        let mut stage = Self::new_ao(device, path, format)?;
        stage.shadows = Some(super::station_photon_shadow::Shadows::new(
            device, path, true,
        )?);
        Ok(stage)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_shadows(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cells: &std::sync::Arc<Vec<[i32; 3]>>,
        barriers: &[super::station_barrier::Barrier],
        deck: i32,
        player: [f32; 3],
    ) {
        if let Some(solas) = &mut self.solas {
            solas.set_camera(player, deck);
        }
        if let Some(shadow) = &mut self.shadows {
            shadow.prepare(device, queue, cells, barriers, deck, player);
        }
    }
    fn from_program(
        device: &wgpu::Device,
        program: Program,
        format: wgpu::TextureFormat,
        material: Option<super::station_photon_material::Material>,
        input: FinalInput,
    ) -> Result<Self, String> {
        let interface = program.interface;
        let mut check = interface.clone();
        if material.is_some() {
            check.samplers.retain(|s| s.name != "colortex1");
        }
        validate_input(&check, &input)?;
        let mut entries = super::station_shader_pipeline::sampled_entries(&interface)?;
        if material.is_some() {
            for sampler in interface.samplers.iter().filter(|s| s.name == "colortex1") {
                for entry in &mut entries {
                    if entry.binding == sampler.binding {
                        entry.ty = wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        };
                    }
                    if entry.binding == sampler.binding + 1 {
                        entry.ty =
                            wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering);
                    }
                }
            }
        }
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Minecraft final inputs"),
            entries: &entries,
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let vertex = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Minecraft final.vsh"),
            source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(program.vertex)),
        });
        let fragment = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Minecraft final.fsh"),
            source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(program.fragment)),
        });
        let pipeline = render_pipeline_mask(
            device,
            Some(&pl),
            &vertex,
            &fragment,
            "main",
            "main",
            format.remove_srgb_suffix(),
            wgpu::ColorWrites::RED | wgpu::ColorWrites::GREEN | wgpu::ColorWrites::BLUE,
        );
        let flip_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GL row orientation"),
            source: wgpu::ShaderSource::Wgsl(if input.srgb {FLIP.replace("return textureLoad(scene,vec2i(i32(p.x),i32(size.y)-1-i32(p.y)),0);", "let c=textureLoad(scene,vec2i(i32(p.x),i32(size.y)-1-i32(p.y)),0);let rgb=select(c.rgb*12.92,1.055*pow(max(c.rgb,vec3f(0.)),vec3f(1./2.4))-0.055,c.rgb>vec3f(0.0031308));return vec4f(rgb,c.a);").into()} else {FLIP.into()}),
        });
        let flip = render_pipeline(
            device,
            None,
            &flip_shader,
            &flip_shader,
            "vs",
            "fs",
            wgpu::TextureFormat::Rgba16Float,
        );
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Minecraft final uniforms"),
            size: u64::from(interface.uniform_size.max(16)),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let scene_data = (material.is_some()
            || interface.samplers.iter().any(|s| {
                matches!(
                    s.name.as_str(),
                    "stationAlbedo" | "stationNormal" | "depthtex0"
                )
            }))
        .then(|| super::station_scene_data::SceneData::new(device));
        Ok(Self {
            scene_data,
            material,
            ao: None,
            shadows: None,
            solas: None,
            pipeline,
            flip,
            layout,
            uniforms,
            interface,
            targets: None,
            size: [0; 2],
            format,
            start: std::time::Instant::now(),
            previous: std::time::Instant::now(),
            frame: 0,
        })
    }
    pub fn scene(&mut self, device: &wgpu::Device, size: [u32; 2]) -> wgpu::TextureView {
        if let Some(data) = &mut self.scene_data {
            data.resize(device, size);
        }
        if self.size != size {
            let make = |format| {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Minecraft scene target"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
            };
            self.targets = Some((make(self.format), make(wgpu::TextureFormat::Rgba16Float)));
            self.size = size;
        }
        self.targets
            .as_ref()
            .unwrap()
            .0
            .create_view(&Default::default())
    }
    pub fn scene_data_views(&self) -> Option<[wgpu::TextureView; 3]> {
        self.scene_data.as_ref().map(|data| data.views())
    }
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
    ) {
        if let Some(data) = &self.scene_data {
            data.flip(encoder);
            if let Some(material) = &mut self.material {
                material.resize(device, queue, self.size, data);
                material.draw(encoder);
            }
        }
        let (scene, flipped) = self.targets.as_ref().unwrap();
        let scene = scene.create_view(&Default::default());
        let flipped = flipped.create_view(&Default::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("flip scene"),
            layout: &self.flip.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&scene),
            }],
        });
        run_pass(encoder, &flipped, &self.flip, &group, 3);
        if let Some(ao) = &mut self.ao {
            ao.draw(
                device,
                queue,
                encoder,
                self.size,
                self.scene_data.as_ref().unwrap(),
                self.material.as_ref().unwrap(),
                &flipped,
            );
        }
        let final_color = self.ao.as_ref().map(|ao| ao.color()).unwrap_or(&flipped);
        if let Some(shadow) = &mut self.shadows {
            shadow.draw(
                device,
                queue,
                encoder,
                self.size,
                self.scene_data.as_ref().unwrap(),
                final_color,
            );
        }
        let final_color = self
            .shadows
            .as_ref()
            .map(|shadow| shadow.color())
            .unwrap_or(final_color);
        if let Some(solas) = &mut self.solas {
            solas.draw(
                device,
                queue,
                encoder,
                self.size,
                self.scene_data.as_ref().unwrap(),
                final_color,
            );
        }
        let final_color = self
            .solas
            .as_ref()
            .map(|s| s.color())
            .unwrap_or(final_color);
        let mut data = self.interface.initial_values.clone();
        data.resize(self.interface.uniform_size as usize, 0);
        let now = std::time::Instant::now();
        let dt = now.duration_since(self.previous).as_secs_f32();
        self.previous = now;
        self.frame = self.frame.wrapping_add(1);
        for u in &self.interface.uniforms {
            let values = match u.name.as_str() {
                "hz_target" => vec![
                    self.size[0] as f32,
                    self.size[1] as f32,
                    1. / self.size[0] as f32,
                    1. / self.size[1] as f32,
                ],
                "viewWidth" => vec![self.size[0] as f32],
                "viewHeight" => vec![self.size[1] as f32],
                "aspectRatio" => vec![self.size[0] as f32 / self.size[1] as f32],
                "frameTimeCounter" => vec![self.start.elapsed().as_secs_f32()],
                "frameTime" => vec![dt],
                "frameCounter" => vec![f32::from_bits(self.frame)],
                _ => continue,
            };
            let bytes = bytemuck::cast_slice(&values);
            let start = u.offset as usize;
            data[start..start + bytes.len()].copy_from_slice(bytes);
        }
        queue.write_buffer(&self.uniforms, 0, &data);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let nearest = device.create_sampler(&wgpu::SamplerDescriptor::default());
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: self.uniforms.as_entire_binding(),
        }];
        for s in &self.interface.samplers {
            entries.push(wgpu::BindGroupEntry {
                binding: s.binding,
                resource: wgpu::BindingResource::TextureView(
                    if s.name == "colortex1"
                        && let Some(material) = &self.material
                    {
                        material.view()
                    } else {
                        self.scene_data
                            .as_ref()
                            .and_then(|data| data.input(&s.name))
                            .unwrap_or(final_color)
                    },
                ),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: s.binding + 1,
                resource: wgpu::BindingResource::Sampler(
                    if s.name == "depthtex0" || (s.name == "colortex1" && self.material.is_some()) {
                        &nearest
                    } else {
                        &sampler
                    },
                ),
            });
        }
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Minecraft final"),
            layout: &self.layout,
            entries: &entries,
        });
        run_pass(encoder, output, &self.pipeline, &group, 6);
    }
}
pub(super) fn render_pipeline(
    device: &wgpu::Device,
    layout: Option<&wgpu::PipelineLayout>,
    vertex: &wgpu::ShaderModule,
    fragment: &wgpu::ShaderModule,
    vs: &str,
    fs: &str,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    render_pipeline_mask(
        device,
        layout,
        vertex,
        fragment,
        vs,
        fs,
        format,
        wgpu::ColorWrites::ALL,
    )
}
#[allow(clippy::too_many_arguments)]
fn render_pipeline_mask(
    device: &wgpu::Device,
    layout: Option<&wgpu::PipelineLayout>,
    vertex: &wgpu::ShaderModule,
    fragment: &wgpu::ShaderModule,
    vs: &str,
    fs: &str,
    format: wgpu::TextureFormat,
    mask: wgpu::ColorWrites,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Minecraft postprocessing"),
        layout,
        vertex: wgpu::VertexState {
            module: vertex,
            entry_point: Some(vs),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: fragment,
            entry_point: Some(fs),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: mask,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}
pub(super) fn run_pass(
    encoder: &mut wgpu::CommandEncoder,
    output: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
    count: u32,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Minecraft postprocessing"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: output,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, group, &[]);
    pass.draw(0..count, 0..1);
}
const FLIP: &str = "
@group(0) @binding(0) var scene: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {let p=array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.));return vec4f(p[i],0.,1.);}
@fragment fn fs(@builtin(position) p:vec4f)->@location(0) vec4f {let size=textureDimensions(scene);return textureLoad(scene,vec2i(i32(p.x),i32(size.y)-1-i32(p.y)),0);}
";

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Program {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/horizon-shaders/tests/packs/post-test");
        load_final(&path).expect("Horizon GLSL fixture")
    }
    #[test]
    fn explicit_color_slot_and_dimension_vertex_fallback() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/final-slot");
        let p = load_final(&path).unwrap();
        let input = FinalInput::parse("colortex7", "srgb").unwrap();
        validate_input(&p.interface, &input).unwrap();
        assert!(validate_input(&p.interface, &FinalInput::default()).is_err());
        let mut mixed = p.interface.clone();
        mixed.samplers.push(fixture().interface.samplers[0].clone());
        assert!(
            validate_input(&mixed, &input)
                .unwrap_err()
                .contains("unbound")
        );
    }
    #[test]
    fn final_contract_rejects_invalid_slots_encoding_and_types() {
        for name in ["colortex16", "colortex-1", "colortex01", "shadowtex0", ""] {
            assert!(FinalInput::parse(name, "srgb").is_err());
        }
        assert!(FinalInput::parse("colortex1", "guess").is_err());
        let mut p = fixture();
        p.interface.samplers[0].name = "colortex1".into();
        let input = FinalInput::parse("colortex1", "srgb").unwrap();
        validate_input(&p.interface, &input).unwrap();
        p.interface.samplers[0].ty = "sampler2DShadow".into();
        assert!(validate_input(&p.interface, &input).is_err());
    }
    #[test]
    fn translates_legacy_final_and_accepts_color_input() {
        let p = fixture();
        validate_interface(&p.interface).unwrap();
        assert_eq!(p.interface.samplers[0].name, "colortex0");
        assert_eq!(p.interface.outputs, [0]);
    }
    #[test]
    fn rejects_unbound_shadow_instead_of_rendering_garbage() {
        let mut p = fixture();
        p.interface.samplers[0].name = "shadowtex0".into();
        assert!(
            validate_interface(&p.interface)
                .unwrap_err()
                .contains("resources")
        );
    }
    #[test]
    fn native_scene_contract_translates_and_depth_is_not_filtered() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/scene-data");
        let p = load_final(&path).unwrap();
        validate_interface(&p.interface).unwrap();
        let entries = super::super::station_shader_pipeline::sampled_entries(&p.interface).unwrap();
        let depth = p
            .interface
            .samplers
            .iter()
            .find(|s| s.name == "depthtex0")
            .unwrap();
        assert!(entries.iter().any(|e| e.binding == depth.binding
            && matches!(
                e.ty,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    ..
                }
            )));
        assert!(entries.iter().any(|e| e.binding == depth.binding + 1
            && matches!(
                e.ty,
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering)
            )));
        assert_eq!(p.interface.samplers.len(), 3);
    }
    #[test]
    fn rejects_wrong_uniform_type() {
        let mut p = fixture();
        p.interface.uniforms[0].ty = "mat4".into();
        assert!(
            validate_interface(&p.interface)
                .unwrap_err()
                .contains("requires")
        );
    }
}

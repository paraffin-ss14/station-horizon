//! Original Solas composite13/14/15 subset: optional bloom, tonemap, grading, FXAA.
//! Optional original DOF, motion blur or experimental TAA. All GLSL remains in the user ZIP.
use super::station_scene_data::SceneData;
use super::station_shader_buffers::{Buffers, Route};
use super::station_shader_raster::Raster;
use super::station_solas_camera::Camera;
use horizon_shaders::{Macros, Pack, PostProgram};
use std::{collections::BTreeMap, path::Path};
fn load(
    path: &Path,
    bloom: bool,
    dof: bool,
    motion: bool,
    taa: bool,
    mut options: BTreeMap<String, String>,
) -> Result<horizon_shaders::Post, String> {
    let pack = Pack::open(path).map_err(|e| e.to_string())?;
    options.extend(BTreeMap::from([
        ("BLOOM".into(), bloom.to_string()),
        ("TAA".into(), taa.to_string()),
        ("FXAA".into(), "true".into()),
        ("DOF".into(), dof.to_string()),
        ("MANUAL_FOCUS".into(), "false".into()),
        ("DISTANT_BLUR".into(), "false".into()),
        ("MOTION_BLUR".into(), motion.to_string()),
        ("LENS_FLARE".into(), "false".into()),
    ]));
    let pack = pack.with_chosen(options);
    let mut macros = solas_macros();
    let (properties, notes) = horizon_shaders::read_properties(&pack, &mut macros);
    if !notes.is_empty() {
        return Err(format!("Solas features: {notes:?}"));
    }
    let names: &[&str] = if bloom {
        &["composite13", "composite14", "composite15"]
    } else {
        &["composite14", "composite15"]
    };
    let post = horizon_shaders::load_post_subset(&pack, "", &properties, &macros, names);
    if !post.notes.is_empty() {
        return Err(format!("Solas post: {:?}", post.notes));
    }
    if post.sizes.iter().any(Option::is_some) {
        return Err("Solas subset does not support custom buffer sizes".into());
    }
    Ok(post)
}

fn solas_macros() -> Macros {
    let mut macros = Macros::standard();
    // Solas common.glsl uses the OS flag as a numeric operand, not only defined().
    // Preserve which OS is defined, but make the active flag valid in both forms.
    for name in ["MC_OS_WINDOWS", "MC_OS_MAC", "MC_OS_LINUX", "MC_OS_OTHER"] {
        if macros.is_defined(name) {
            macros.define(name, "1");
        }
    }
    macros
}
fn input_allowed(stage: usize, name: &str, bloom: bool) -> bool {
    (bloom && stage == 0 && name == "colortex1")
        || matches!(
            (stage, name),
            (0, "colortex0" | "colortex2" | "depthtex1")
                | (1, "colortex1" | "colortex2" | "depthtex1")
                | (2, "colortex0")
        )
}
fn color_index(name: &str) -> Option<usize> {
    name.strip_prefix("colortex")?
        .parse::<usize>()
        .ok()
        .filter(|&b| b < 16)
}
/// Validity is per current readable side, reset at frame start/resize.
#[derive(Default)]
struct MipSchedule {
    valid: u16,
}
impl MipSchedule {
    fn prepare(&mut self, requested: u16) -> u16 {
        let needed = requested & !self.valid;
        self.valid |= requested;
        needed
    }
    fn invalidate(&mut self, flipped: u16) {
        self.valid &= !flipped;
    }
}
struct Pass {
    route: Route,
    raster: Raster,
    mipmaps: u16,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    interface: horizon_shaders::Interface,
}
impl Pass {
    fn new(
        device: &wgpu::Device,
        loaded: PostProgram,
        stage: usize,
        bloom: bool,
    ) -> Result<Self, String> {
        if !loaded.compute.is_empty() || loaded.geometry_triangles != 0 {
            return Err(format!(
                "{}: compute/geometry execution is not connected",
                loaded.name
            ));
        }
        let expected = if stage == 2 { vec![1] } else { vec![1, 2] };
        if loaded.buffers != expected {
            return Err(format!("{}: unexpected output buffers", loaded.name));
        }
        let raster = Raster::from_post(&loaded);
        let route = Route::new(loaded.buffers, &loaded.flips)?;
        let mipmaps = loaded.mipmaps;
        let p = loaded.program.ok_or("Solas post has no graphics program")?;
        if p.interface.outputs != if stage == 2 { vec![0] } else { vec![0, 1] }
            || !p.interface.buffers.is_empty()
            || !p.interface.images.is_empty()
            || p.interface.draw
        {
            return Err("unsupported Solas post resources".into());
        }
        for s in &p.interface.samplers {
            if s.ty != "sampler2D" || !input_allowed(stage, &s.name, bloom) {
                return Err(format!("unbound Solas post sampler {}", s.name));
            }
        }
        for u in &p.interface.uniforms {
            let ty = match u.name.as_str() {
                "hz_target" | "hz_fog" | "hz_fog_color" | "hz_depth_linear" => "vec4",
                "viewWidth" | "viewHeight" | "aspectRatio" | "shadowFade" | "timeBrightness"
                | "timeAngle" | "wetness" | "blindFactor" | "darknessFactor"
                | "centerDepthSmooth" | "frameTimeCounter" => "float",
                "cameraPosition" | "previousCameraPosition" | "sunPosition" => "vec3",
                "frameCounter" | "isEyeInWater" => "int",
                "eyeBrightnessSmooth" => "ivec2",
                "gbufferProjection"
                | "gbufferProjectionInverse"
                | "gbufferModelView"
                | "gbufferModelViewInverse"
                | "gbufferPreviousProjection"
                | "gbufferPreviousModelView" => "mat4",
                _ => return Err(format!("unbound Solas post uniform {}", u.name)),
            };
            if u.ty != ty {
                return Err(format!("wrong Solas uniform type {}", u.name));
            }
        }
        let sampled = p
            .interface
            .samplers
            .iter()
            .filter_map(|s| color_index(&s.name))
            .fold(0, |mask, b| mask | (1 << b));
        // Only sampled colortex resources need a generated chain.
        let mipmaps = mipmaps & sampled;
        let mut entries = super::station_shader_pipeline::sampled_entries(&p.interface)?;
        for s in p
            .interface
            .samplers
            .iter()
            .filter(|s| s.name == "depthtex1")
        {
            for e in &mut entries {
                if e.binding == s.binding {
                    e.ty = wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    };
                } else if e.binding == s.binding + 1 {
                    e.ty = wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering);
                }
            }
        }
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Solas post"),
            entries: &entries,
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let vs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(p.vertex)),
        });
        let fs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(p.fragment)),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("original Solas composite"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &vs,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &fs,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                targets: &raster.targets(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: p.interface.uniform_size.max(16) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            route,
            raster,
            mipmaps,
            pipeline,
            layout,
            uniform,
            interface: p.interface,
        })
    }
    fn upload(
        &self,
        queue: &wgpu::Queue,
        size: [u32; 2],
        camera: &Camera,
        previous: &Camera,
        focus: f32,
        clock: (f32, u32),
    ) {
        let mut data = self.interface.initial_values.clone();
        data.resize(self.interface.uniform_size as usize, 0);
        for u in &self.interface.uniforms {
            if u.name == "frameCounter" {
                let offset = u.offset as usize;
                data[offset..offset + 4].copy_from_slice(&(clock.1 as i32).to_ne_bytes());
                continue;
            }
            let values = match u.name.as_str() {
                "hz_target" => vec![
                    size[0] as f32,
                    size[1] as f32,
                    1. / size[0] as f32,
                    1. / size[1] as f32,
                ],
                "centerDepthSmooth" => vec![focus],
                "frameTimeCounter" => vec![clock.0],
                "previousCameraPosition" => previous.position.to_array().to_vec(),
                "gbufferPreviousProjection" => previous.projection.to_cols_array().to_vec(),
                "gbufferPreviousModelView" => previous.view.to_cols_array().to_vec(),
                "viewWidth" => vec![size[0] as f32],
                "viewHeight" => vec![size[1] as f32],
                "aspectRatio" => vec![size[0] as f32 / size[1] as f32],
                "gbufferProjection" => camera.projection.to_cols_array().to_vec(),
                "gbufferProjectionInverse" => camera.projection_inverse.to_cols_array().to_vec(),
                "gbufferModelView" => camera.view.to_cols_array().to_vec(),
                "gbufferModelViewInverse" => camera.view_inverse.to_cols_array().to_vec(),
                "timeAngle" => vec![0.25],
                "timeBrightness" | "shadowFade" => vec![1.],
                "cameraPosition" => camera.position.to_array().to_vec(),
                _ => continue,
            };
            let bytes = bytemuck::cast_slice(&values);
            let offset = u.offset as usize;
            data[offset..offset + bytes.len()].copy_from_slice(bytes);
        }
        queue.write_buffer(&self.uniform, 0, &data);
    }
}
pub struct Post {
    passes: Vec<Pass>,
    mips: [Option<super::station_shader_mips::Mips>; 16],
    buffers: Buffers,
    composite_pre: Route,
    player: [f32; 3],
    deck: i32,
    history: super::station_solas_camera::History,
    started: std::time::Instant,
    dof_depth: Option<super::station_solas_depth::Depth>,
    autofocus: Option<super::station_solas_focus::Focus>,
    taa: bool,
    reactive: super::station_solas_reactive::Reactive,
    frame: u32,
    linear: wgpu::Sampler,
    nearest: wgpu::Sampler,
}
impl Post {
    pub fn new(
        device: &wgpu::Device,
        path: &Path,
        bloom: bool,
        dof: bool,
        motion: bool,
        taa: bool,
        settings: BTreeMap<String, String>,
    ) -> Result<Self, String> {
        let post = load(path, bloom, dof, motion, taa, settings)?;
        let composite_pre = Route::new(vec![], &post.composite_pre)?;
        let config = post.config;
        let mut passes = Vec::new();
        for program in post.programs {
            let stage = match program.name.as_str() {
                "composite13" => 2,
                "composite14" => 0,
                "composite15" => 1,
                _ => return Err(format!("unexpected Solas pass {}", program.name)),
            };
            passes.push(Pass::new(device, program, stage, bloom)?);
        }
        let used = passes
            .iter()
            .fold(1 | composite_pre.used(), |mask, p| mask | p.route.used());
        let mipmaps = passes.iter().fold(0, |mask, p| mask | p.mipmaps);
        Ok(Self {
            passes,
            mips: std::array::from_fn(|b| {
                (mipmaps & (1 << b) != 0).then(|| super::station_shader_mips::Mips::new(device))
            }),
            buffers: Buffers::new(config.buffers, used),
            composite_pre,
            player: [0.; 3],
            deck: 0,
            history: Default::default(),
            started: std::time::Instant::now(),
            autofocus: None,
            taa,
            reactive: Default::default(),
            frame: 0,
            dof_depth: (dof || motion || taa)
                .then(|| super::station_solas_depth::Depth::new(device, taa)),
            linear: device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            }),
            nearest: device.create_sampler(&Default::default()),
        })
    }
    pub fn update_dynamic_history(&mut self, vertices: &[super::Vertex]) {
        if self.taa { self.reactive.update(vertices); }
    }
    pub fn jitter(&self, size: [u32; 2]) -> [f32; 2] {
        if self.taa {
            super::station_solas_camera::jitter(self.frame, size)
        } else {
            [0.; 2]
        }
    }
    pub fn enable_autofocus(&mut self, device: &wgpu::Device) {
        assert!(self.dof_depth.is_some());
        self.autofocus = Some(super::station_solas_focus::Focus::new(device));
    }
    pub fn set_camera(&mut self, player: [f32; 3], deck: i32) {
        self.player = player;
        self.deck = deck;
    }
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
        scene: &SceneData,
        source: &wgpu::TextureView,
    ) {
        self.buffers.begin_frame(device, encoder, size);
        self.buffers.seed_scene(device, encoder, source);
        self.buffers.finish(&self.composite_pre);
        if let Some(depth) = &mut self.dof_depth {
            depth.draw(device, encoder, size, scene.input("depthtex0").unwrap());
        }
        let mut camera = Camera::new(size, self.player);
        let focus = if self.dof_depth.is_some() {
            if self.taa {
                camera.remap_taa_depth();
            } else {
                camera.remap_dof_depth();
            }
            super::station_solas_depth::remap(
                super::DEPTH_BIAS - super::DEPTH_SCALE * self.deck as f32,
            )
        } else {
            0.0
        };
        if let Some(autofocus) = &mut self.autofocus {
            autofocus.update(
                device,
                queue,
                encoder,
                self.dof_depth.as_ref().unwrap().view(),
                size,
                self.player,
                focus,
            );
        }
        camera.jitter(self.jitter(size));
        if self.taa && !self.history.valid_temporal(&camera, size, self.deck) {
            self.buffers.clear_history(encoder, 2);
        }
        let previous = self.history.advance(&camera, size, self.deck);
        if self.taa && let Some(rect) = self.reactive.take_rect(&previous, size) {
            self.buffers.clear_history_rect(device, encoder, 2, rect);
        }
        let time = self.started.elapsed().as_secs_f32();
        let mut schedule = MipSchedule::default();
        for pass in &self.passes {
            let needed = schedule.prepare(pass.mipmaps);
            for b in 0..16 {
                if needed & (1 << b) != 0 {
                    let input = self.buffers.read(b as u32);
                    self.mips[b]
                        .as_mut()
                        .unwrap()
                        .generate(device, encoder, size, input);
                }
            }
            pass.upload(queue, size, &camera, &previous, focus, (time, self.frame));
            if let Some(autofocus) = &self.autofocus {
                for u in pass
                    .interface
                    .uniforms
                    .iter()
                    .filter(|u| u.name == "centerDepthSmooth")
                {
                    autofocus.copy_to(encoder, &pass.uniform, u.offset as u64);
                }
            }
            let mut entries = vec![wgpu::BindGroupEntry {
                binding: 0,
                resource: pass.uniform.as_entire_binding(),
            }];
            for s in &pass.interface.samplers {
                let view = if let Some(b) = color_index(&s.name) {
                    if pass.mipmaps & (1 << b) != 0 {
                        self.mips[b].as_ref().unwrap().view()
                    } else {
                        self.buffers.read(b as u32)
                    }
                } else {
                    self.dof_depth
                        .as_ref()
                        .map(|d| d.view())
                        .unwrap_or_else(|| scene.input("depthtex0").unwrap())
                };
                entries.push(wgpu::BindGroupEntry {
                    binding: s.binding,
                    resource: wgpu::BindingResource::TextureView(view),
                });
                entries.push(wgpu::BindGroupEntry {
                    binding: s.binding + 1,
                    resource: wgpu::BindingResource::Sampler(if s.name == "depthtex1" {
                        &self.nearest
                    } else {
                        &self.linear
                    }),
                });
            }
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &pass.layout,
                entries: &entries,
            });
            let attachments = (0..pass.interface.outputs.len())
                .map(|i| {
                    Some(wgpu::RenderPassColorAttachment {
                        view: self.buffers.write(pass.route.writes[i]),
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })
                })
                .collect::<Vec<_>>();
            let mut render = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Solas post chain"),
                color_attachments: &attachments,
                ..Default::default()
            });
            render.set_pipeline(&pass.pipeline);
            render.set_bind_group(0, &group, &[]);
            let [x, y, w, h] = pass.raster.viewport(size);
            render.set_viewport(x, y, w, h, 0.0, 1.0);
            render.draw(0..6, 0..1);
            drop(render);
            self.buffers.finish(&pass.route);
            schedule.invalidate(pass.route.flipped());
        }
        self.frame = self.frame.wrapping_add(1);
    }
    pub fn color(&self) -> &wgpu::TextureView {
        self.buffers.read(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn os_flag_is_numeric_without_defining_other_platforms() {
        let standard = Macros::standard();
        let macros = solas_macros();
        for name in ["MC_OS_WINDOWS", "MC_OS_MAC", "MC_OS_LINUX", "MC_OS_OTHER"] {
            assert_eq!(standard.is_defined(name), macros.is_defined(name));
            if standard.is_defined(name) {
                assert_eq!(macros.get(name), Some("1"));
            }
        }
    }
    #[test]
    fn mip_schedule_tracks_read_side_not_alternate_writes() {
        let mut schedule = MipSchedule::default();
        assert_eq!(schedule.prepare(3), 3);
        assert_eq!(schedule.prepare(3), 0);
        let held = Route::new(vec![1], &[(1, false)]).unwrap();
        schedule.invalidate(held.flipped());
        assert_eq!(schedule.prepare(3), 0);
        let switched = Route::new(vec![1], &[]).unwrap();
        schedule.invalidate(switched.flipped());
        assert_eq!(schedule.prepare(3), 2);
        let forced = Route::new(vec![], &[(0, true)]).unwrap();
        schedule.invalidate(forced.flipped());
        assert_eq!(schedule.prepare(3), 1);
        assert_eq!(MipSchedule::default().prepare(3), 3);
    }
    #[test]
    fn post_chain_rejects_feedback_and_unproduced_buffers() {
        assert!(input_allowed(0, "colortex0", false));
        assert!(input_allowed(1, "colortex1", false));
        assert!(!input_allowed(0, "colortex1", false));
        assert!(!input_allowed(1, "colortex0", false));
        for stage in 0..2 {
            assert!(input_allowed(stage, "colortex2", false));
            assert!(!input_allowed(stage, "colortex3", false));
            assert!(!input_allowed(stage, "shadowtex0", false));
        }
        assert!(!input_allowed(2, "colortex1", false));
        assert!(input_allowed(0, "colortex1", true));
        assert!(input_allowed(2, "colortex0", true));
        assert!(!input_allowed(2, "colortex2", true));
    }
}

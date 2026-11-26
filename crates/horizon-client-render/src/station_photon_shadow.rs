//! Opaque station voxels rendered by Photon's original shadow programs and PCF include.
//! Fixed directional-light preview; not the pack's full deferred lighting model.
use super::{
    station_barrier::Barrier,
    station_scene_data::SceneData,
    station_shader_pack::{render_pipeline, run_pass},
};
use glam::{Mat4, Vec3};
#[path = "station_photon_fog_filter.rs"]
mod fog_filter;
use horizon_shaders::{GbuffersInputs, Macros, Pack, Program, Target};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};
const RES: u32 = 1024;
fn light() -> Vec3 {
    Vec3::new(-0.7, 1.0, 0.4).normalize()
}
fn matrices() -> (Mat4, Mat4) {
    (
        Mat4::look_at_rh(light() * 40., Vec3::ZERO, Vec3::Y),
        Mat4::orthographic_rh_gl(-32., 32., -32., 32., 0.1, 100.),
    )
}

fn load(path: &Path, volumetrics: bool) -> Result<(Program, Program), String> {
    let pack = Pack::open(path)
        .map_err(|e| e.to_string())?
        .with_chosen(BTreeMap::from([
            ("COLORED_LIGHTS".into(), "false".into()),
            ("SHADOW_COLOR".into(), "false".into()),
            ("SHADOW_VPS".into(), "false".into()),
            ("WATER_CAUSTICS".into(), "false".into()),
            ("shadowMapResolution".into(), RES.to_string()),
        ]));
    let macros = Macros::standard();
    let vs = horizon_shaders::preprocess(&pack, "world0/shadow.vsh", &macros);
    let fs = horizon_shaders::preprocess(&pack, "world0/shadow.fsh", &macros);
    let quad = horizon_shaders::preprocess_source(&pack, "station_shadow_eval.vsh", QUAD, &macros);
    let source = if volumetrics {
        EVAL.replacen(
            "#version 400 compatibility",
            "#version 400 compatibility\n#define STATION_VOLUMETRICS",
            1,
        )
    } else {
        EVAL.to_owned()
    };
    let eval =
        horizon_shaders::preprocess_source(&pack, "station_shadow_eval.fsh", &source, &macros);
    for pp in [&vs, &fs, &quad, &eval] {
        if !pp.diagnostics.is_empty() {
            return Err(format!("Photon shadow includes: {:?}", pp.diagnostics));
        }
    }
    let inputs=GbuffersInputs {
 declarations:"layout(location=0) in vec3 stationPosition;".into(),
 body:"hz_Vertex=vec4(vec3(stationPosition.x,stationPosition.z,-stationPosition.y)-cameraPosition,1.0);hz_Color=vec4(1.0);hz_Normal=vec3(0.0,1.0,0.0);hz_MultiTexCoord0=vec4(0.5);hz_MultiTexCoord1=vec4(0.0);".into(),
 attributes:vec![("mc_Entity".into(),"vec3(10000.0,0.0,0.0)".into())],..Default::default()};
    Ok((
        horizon_shaders::translate_shadow(&vs, &fs, &inputs)
            .map_err(|e| format!("Photon shadow: {e:?}"))?,
        horizon_shaders::translate_composite(&quad, &eval, Target::Buffers)
            .map_err(|e| format!("Photon PCF: {e:?}"))?,
    ))
}
struct Pass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    interface: horizon_shaders::Interface,
}
impl Pass {
    fn new(
        device: &wgpu::Device,
        program: Program,
        geometry: bool,
        volumetrics: bool,
    ) -> Result<Self, String> {
        if program.interface.draw {
            return Err("shadow preview does not bind per-draw material uniforms".into());
        }
        for s in &program.interface.samplers {
            if !matches!(
                s.name.as_str(),
                "tex"
                    | "noisetex"
                    | "shadowtex0"
                    | "shadowtex1"
                    | "sourceColor"
                    | "depthtex0"
                    | "stationNormal"
            ) {
                return Err(format!("unbound shadow sampler {}", s.name));
            }
        }
        for u in &program.interface.uniforms {
            if !u.name.starts_with("hz_")
                && !matches!(
                    u.name.as_str(),
                    "shadowModelView"
                        | "shadowModelViewInverse"
                        | "shadowProjection"
                        | "shadowProjectionInverse"
                        | "gbufferModelView"
                        | "gbufferModelViewInverse"
                        | "gbufferProjection"
                        | "gbufferProjectionInverse"
                        | "cameraPosition"
                        | "stationCamera"
                        | "light_dir"
                        | "near"
                        | "far"
                        | "frameTimeCounter"
                        | "rainStrength"
                        | "wetness"
                        | "taa_offset"
                        | "world_age"
                        | "time_sunrise"
                        | "day_factor"
                        | "time_noon"
                        | "time_sunset"
                        | "time_midnight"
                        | "biome_temperature"
                        | "biome_humidity"
                        | "stationView"
                        | "stationFogTop"
                        | "stationDepth"
                        | "frameCounter"
                )
            {
                return Err(format!("unbound shadow uniform {}", u.name));
            }
        }
        let mut entries = super::station_shader_pipeline::sampled_entries(&program.interface)?;
        for s in program
            .interface
            .samplers
            .iter()
            .filter(|s| s.name == "shadowtex0")
        {
            for e in &mut entries {
                if e.binding == s.binding {
                    e.ty = wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    };
                }
                if e.binding == s.binding + 1 {
                    e.ty = wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering);
                }
            }
        }
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Photon shadows"),
            entries: &entries,
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let vs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Photon shadow vertex"),
            source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(program.vertex)),
        });
        let fs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Photon shadow fragment"),
            source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(program.fragment)),
        });
        let pipeline = if geometry {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Photon opaque shadow map"),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &vs,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: 12,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0=>Float32x3],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &fs,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        } else if volumetrics {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Photon surface and fog components"),
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
                    targets: &std::array::from_fn::<_, 3, _>(|_| {
                        Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::Rgba16Float,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })
                    }),
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        } else {
            render_pipeline(
                device,
                Some(&pl),
                &vs,
                &fs,
                "main",
                "main",
                wgpu::TextureFormat::Rgba16Float,
            )
        };
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Photon shadow uniforms"),
            size: program.interface.uniform_size.max(16) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            pipeline,
            layout,
            uniform,
            interface: program.interface,
        })
    }
    fn upload(&self, queue: &wgpu::Queue, size: [u32; 2], player: [f32; 3], deck: i32) {
        let (view, projection) = matrices();
        let mut data = self.interface.initial_values.clone();
        data.resize(self.interface.uniform_size as usize, 0);
        for u in &self.interface.uniforms {
            let v: Vec<f32> = match u.name.as_str() {
                "hz_target" => vec![
                    size[0] as f32,
                    size[1] as f32,
                    1. / size[0] as f32,
                    1. / size[1] as f32,
                ],
                "shadowModelView" => view.to_cols_array().to_vec(),
                "shadowModelViewInverse" => view.inverse().to_cols_array().to_vec(),
                "shadowProjection" => projection.to_cols_array().to_vec(),
                "shadowProjectionInverse" => projection.inverse().to_cols_array().to_vec(),
                "gbufferModelView"
                | "gbufferModelViewInverse"
                | "gbufferProjection"
                | "gbufferProjectionInverse" => Mat4::IDENTITY.to_cols_array().to_vec(),
                "cameraPosition" => vec![player[0], player[2], -player[1]],
                "stationCamera" => player.to_vec(),
                "light_dir" => light().to_array().to_vec(),
                "near" => vec![0.1],
                "far" => vec![100.],
                "time_noon" | "day_factor" => vec![1.],
                "biome_temperature" | "biome_humidity" => vec![0.5],
                "stationView" => vec![
                    super::VIEW_HALF_HEIGHT * size[0] as f32 / size[1] as f32,
                    super::VIEW_HALF_HEIGHT,
                ],
                "stationDepth" => vec![super::DEPTH_BIAS, super::DEPTH_SCALE],
                "stationFogTop" => vec![deck as f32 + 3.5],
                _ => continue,
            };
            let bytes = bytemuck::cast_slice(&v);
            let i = u.offset as usize;
            data[i..i + bytes.len()].copy_from_slice(bytes);
        }
        queue.write_buffer(&self.uniform, 0, &data);
    }
}
// Only static opaque casters and a fixed light are supported by this cache.
// Add time/light/material invalidation before enabling animated shadow inputs.
#[derive(Default)]
struct ShadowMapCache {
    camera: Option<[f32; 3]>,
}
impl ShadowMapCache {
    fn invalidate(&mut self) {
        self.camera = None;
    }
    fn needs_update(&self, camera: [f32; 3]) -> bool {
        self.camera != Some(camera)
    }
    fn recorded(&mut self, camera: [f32; 3]) {
        self.camera = Some(camera);
    }
}
pub struct Shadows {
    map: Pass,
    map_cache: ShadowMapCache,
    map_updates: u64,
    map_reuses: u64,
    eval: Pass,
    depth: wgpu::TextureView,
    color: wgpu::TextureView,
    depth_float: wgpu::TextureView,
    copy: wgpu::RenderPipeline,
    copy_group: wgpu::BindGroup,
    white: wgpu::Texture,
    white_view: wgpu::TextureView,
    white_ready: bool,
    nearest: wgpu::Sampler,
    compare: wgpu::Sampler,
    mesh: wgpu::Buffer,
    count: u32,
    world: Option<Arc<Vec<[i32; 3]>>>,
    barriers: Vec<Barrier>,
    deck: i32,
    player: [f32; 3],
    output: Option<wgpu::TextureView>,
    fog: Option<fog_filter::FogFilter>,
    size: [u32; 2],
}
fn texture(
    device: &wgpu::Device,
    size: [u32; 2],
    format: wgpu::TextureFormat,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("Photon shadow target"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}
impl Shadows {
    pub fn new(device: &wgpu::Device, path: &Path, volumetrics: bool) -> Result<Self, String> {
        let (map, eval) = load(path, volumetrics)?;
        let map = Pass::new(device, map, true, false)?;
        let eval = Pass::new(device, eval, false, volumetrics)?;
        let depth = texture(device, [RES; 2], wgpu::TextureFormat::Depth32Float);
        let color = texture(device, [RES; 2], wgpu::TextureFormat::Rgba8Unorm);
        let depth_float = texture(device, [RES; 2], wgpu::TextureFormat::R32Float);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("opaque shadow depth copy"),
            source: wgpu::ShaderSource::Wgsl(COPY.into()),
        });
        let copy = render_pipeline(
            device,
            None,
            &shader,
            &shader,
            "vs",
            "fs",
            wgpu::TextureFormat::R32Float,
        );
        let copy_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &copy.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&depth),
            }],
        });
        let white = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("opaque caster material"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let white_view = white.create_view(&Default::default());
        let mesh = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow casters"),
            size: 12,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            map,
            map_cache: ShadowMapCache::default(),
            map_updates: 0,
            map_reuses: 0,
            eval,
            depth,
            color,
            depth_float,
            copy,
            copy_group,
            white,
            white_view,
            white_ready: false,
            nearest: device.create_sampler(&Default::default()),
            compare: device.create_sampler(&wgpu::SamplerDescriptor {
                compare: Some(wgpu::CompareFunction::LessEqual),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            mesh,
            count: 0,
            world: None,
            barriers: vec![],
            deck: i32::MIN,
            player: [0.; 3],
            output: None,
            fog: volumetrics.then(|| fog_filter::FogFilter::new(device)),
            size: [0; 2],
        })
    }
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cells: &Arc<Vec<[i32; 3]>>,
        barriers: &[Barrier],
        deck: i32,
        player: [f32; 3],
    ) {
        self.player = player;
        if self.deck == deck
            && self.world.as_ref().is_some_and(|w| Arc::ptr_eq(w, cells))
            && self.barriers == barriers
        {
            return;
        }
        self.map_cache.invalidate();
        let vertices = mesh(&occluders(cells, barriers, deck));
        self.count = vertices.len() as u32;
        self.mesh = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("station exposed shadow faces"),
            size: (vertices.len() * 12).max(12) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !vertices.is_empty() {
            queue.write_buffer(&self.mesh, 0, bytemuck::cast_slice(&vertices));
        }
        self.world = Some(cells.clone());
        self.barriers = barriers.to_vec();
        self.deck = deck;
    }
    fn group(
        &self,
        device: &wgpu::Device,
        pass: &Pass,
        scene: &SceneData,
        source: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: pass.uniform.as_entire_binding(),
        }];
        for s in &pass.interface.samplers {
            let view = match s.name.as_str() {
                "shadowtex0" => &self.depth_float,
                "shadowtex1" => &self.depth,
                "sourceColor" => source,
                "depthtex0" | "stationNormal" => scene.input(&s.name).unwrap(),
                _ => &self.white_view,
            };
            entries.push(wgpu::BindGroupEntry {
                binding: s.binding,
                resource: wgpu::BindingResource::TextureView(view),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: s.binding + 1,
                resource: wgpu::BindingResource::Sampler(if s.ty == "sampler2DShadow" {
                    &self.compare
                } else {
                    &self.nearest
                }),
            });
        }
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Photon shadow scene bindings"),
            layout: &pass.layout,
            entries: &entries,
        })
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
        if !self.white_ready {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.white,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &[255; 4],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4),
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
            self.white_ready = true;
        }
        if self.size != size {
            self.output = Some(texture(device, size, wgpu::TextureFormat::Rgba16Float));
            if let Some(fog) = &mut self.fog {
                fog.resize(device, size);
            }
            self.size = size;
        }
        self.eval.upload(queue, size, self.player, self.deck);
        if self.map_cache.needs_update(self.player) {
            self.map.upload(queue, [RES; 2], self.player, self.deck);
            let group = self.group(device, &self.map, scene, source);
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Photon original shadow pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.color,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    ..Default::default()
                });
                pass.set_pipeline(&self.map.pipeline);
                pass.set_bind_group(0, &group, &[]);
                pass.set_vertex_buffer(0, self.mesh.slice(..));
                pass.draw(0..self.count, 0..1);
            }
            run_pass(encoder, &self.depth_float, &self.copy, &self.copy_group, 3);
            self.map_cache.recorded(self.player);
            self.map_updates += 1;
        } else {
            self.map_reuses += 1;
        }
        let group = self.group(device, &self.eval, scene, source);
        if let Some(fog) = &mut self.fog {
            fog.render(
                device,
                encoder,
                &self.eval.pipeline,
                &group,
                scene,
                self.output.as_ref().unwrap(),
            );
        } else {
            run_pass(
                encoder,
                self.output.as_ref().unwrap(),
                &self.eval.pipeline,
                &group,
                6,
            );
        }
    }
    pub fn cache_stats(&self) -> (u64, u64) {
        (self.map_updates, self.map_reuses)
    }
    pub fn color(&self) -> &wgpu::TextureView {
        self.output.as_ref().unwrap()
    }
}
fn occluders(cells: &[[i32; 3]], barriers: &[Barrier], deck: i32) -> BTreeSet<[i32; 3]> {
    let mut set: BTreeSet<_> = cells
        .iter()
        .copied()
        .filter(|c| c[2] >= deck && c[2] <= deck + 2)
        .collect();
    for b in barriers
        .iter()
        .filter(|b| b.cell[2] >= deck && b.cell[2] <= deck + 2)
    {
        if b.blocks_light {
            set.insert(b.cell);
        } else {
            set.remove(&b.cell);
        }
    }
    set
}
fn mesh(cells: &BTreeSet<[i32; 3]>) -> Vec<[f32; 3]> {
    let mut out = Vec::new();
    for &cell in cells {
        for axis in 0..3 {
            for sign in [-1, 1] {
                let mut neighbor = cell;
                neighbor[axis] += sign;
                if cells.contains(&neighbor) {
                    continue;
                }
                let a = (axis + 1) % 3;
                let b = (axis + 2) % 3;
                for [u, v] in [[0, 0], [1, 0], [1, 1], [0, 0], [1, 1], [0, 1]] {
                    let mut p = cell.map(|n| n as f32);
                    p[axis] += if sign > 0 { 1. } else { 0. };
                    p[a] += u as f32;
                    p[b] += v as f32;
                    out.push(p);
                }
            }
        }
    }
    out
}
const QUAD: &str = "#version 400 compatibility\nout vec2 uv;void main(){uv=gl_MultiTexCoord0.xy;gl_Position=vec4(gl_Vertex.xy*2.0-1.0,0.0,1.0);}";
const EVAL: &str = r#"#version 400 compatibility
#define WORLD_OVERWORLD
#include "/include/global.glsl"
#undef TAA
#undef TAAU
#undef SHADOW_VPS
#undef SHADOW_COLOR
#define SHADOW
uniform sampler2D sourceColor;
uniform sampler2D depthtex0;
uniform sampler2D stationNormal;
uniform sampler2D shadowtex0;
uniform sampler2DShadow shadowtex1;
uniform sampler2D noisetex;
uniform mat4 shadowModelView;
uniform mat4 shadowProjection;
uniform mat4 shadowProjectionInverse;
uniform vec3 cameraPosition;
uniform vec3 stationCamera;
uniform vec2 stationView;
uniform vec2 stationDepth;
uniform vec3 light_dir;
uniform float far;
uniform int frameCounter;
in vec2 uv;
#include "/include/lighting/shadows/pcss.glsl"
#ifdef STATION_VOLUMETRICS
// Station supplies its own atmosphere inputs; unused sky/cloud/perspective helpers are excluded.
#define INCLUDE_SKY_ATMOSPHERE
#define INCLUDE_LIGHTING_CLOUD_SHADOWS
#define INCLUDE_UTILITY_SPACE_CONVERSION
#undef AIR_FOG_CLOUDY_NOISE
#undef SEA_LEVEL
#define SEA_LEVEL 0.0
uniform float stationFogTop;
#define eyeAltitude stationFogTop
#define eye_skylight 1.0
#define sun_dir light_dir
#define blindness 0.0
#define darknessFactor 0.0
uniform float rainStrength;
const vec3 light_color = vec3(2.8,2.6,2.3);
const vec3 ambient_color = vec3(0.04,0.05,0.07);
#include "/include/fog/overworld/parameters.glsl"
const OverworldFogParameters fog_params = OverworldFogParameters(vec3(0.006,0.009,0.014),vec3(0.045),vec3(0.05));
// The original marcher fetches raw depth, while surface PCF uses a comparison sampler.
#define shadowtex1 shadowtex0
#include "/include/fog/overworld/raymarched.glsl"
#undef shadowtex1
#endif
#ifdef STATION_VOLUMETRICS
#define STATION_COLOR gl_FragData[0]
#else
#define STATION_COLOR gl_FragColor
#endif
void main(){
#ifdef STATION_VOLUMETRICS
 gl_FragData[1]=vec4(0.0);gl_FragData[2]=vec4(1.0);
#endif
 ivec2 p=ivec2(gl_FragCoord.xy);vec4 color=texelFetch(sourceColor,p,0);float d=texelFetch(depthtex0,p,0).r;
 if(d>=1.0){STATION_COLOR=color;return;}
#ifdef STATION_VOLUMETRICS
 // Preserve fully hidden pixels from the station visibility pass.
 if(max_of(color.rgb)<=1e-5){STATION_COLOR=color;return;}
#endif
 vec2 xy=(uv*2.0-1.0)*stationView+stationCamera.xy;
 float z=(stationDepth.x-d)/stationDepth.y;
 vec3 scene=vec3(xy.x,z,-xy.y)-cameraPosition;
 vec3 n=texelFetch(stationNormal,p,0).xyz;n=vec3(n.x,n.z,-n.y);
 vec3 bias=get_shadow_bias(scene,n,dot(n,light_dir),1.0);
 vec3 clip=project_ortho(shadowProjection,transform(shadowModelView,scene+bias));
 vec3 screen=distort_shadow_space(clip)*0.5+0.5;
 if(any(lessThan(screen,vec3(0.0)))||any(greaterThan(screen,vec3(1.0)))){STATION_COLOR=color;return;}
 float dither=fract(sin(dot(vec2(p),vec2(12.9898,78.233)))*43758.5453);
 float radius=sqrt(0.5)*shadow_map_pixel_size*SHADOW_PENUMBRA_SCALE;
 vec3 shadow=shadow_pcf(screen,clip,radius,dither);
 vec3 shaded=color.rgb*(vec3(0.35)+0.65*shadow);
#ifdef STATION_VOLUMETRICS
 vec3 end_pos=scene+cameraPosition;
 vec3 start_pos=vec3(end_pos.x,eyeAltitude,end_pos.z);
 // A nonzero downward segment avoids normalization of a zero vector at wall tops.
 if(start_pos.y-end_pos.y>0.001){
  mat2x3 fog=raymarch_air_fog(start_pos,end_pos,false,1.0,dither);
  gl_FragData[1]=vec4(fog[0],1.0);
  gl_FragData[2]=vec4(fog[1],1.0);
 }
#endif
 STATION_COLOR=vec4(shaded,color.a);
}
"#;
const COPY: &str = r"
@group(0) @binding(0) var source:texture_depth_2d;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {let p=array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.));return vec4f(p[i],0.,1.);}
@fragment fn fs(@builtin(position) p:vec4f)->@location(0) f32 {return textureLoad(source,vec2i(p.xy),0);}
";

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shadow_map_cache_tracks_camera_and_geometry_without_stale_reuse() {
        let mut cache = ShadowMapCache::default();
        let camera = [2., 3., 0.];
        assert!(cache.needs_update(camera));
        cache.recorded(camera);
        for _ in 0..120 {
            assert!(!cache.needs_update(camera));
        }
        for moved in [[3., 3., 0.], [2., 4., 0.], [2., 3., -4.]] {
            assert!(cache.needs_update(moved));
        }
        cache.invalidate(); // door/voxel/deck changes invalidate even at an identical camera.
        assert!(cache.needs_update(camera));
        cache.invalidate();
        assert!(cache.needs_update(camera));
        cache.recorded(camera);
        assert!(!cache.needs_update(camera));
    }
    #[test]
    fn internal_faces_are_removed() {
        assert!(mesh(&BTreeSet::new()).is_empty());
        assert_eq!(mesh(&BTreeSet::from([[0, 0, 0]])).len(), 36);
        assert_eq!(mesh(&BTreeSet::from([[0, 0, 0], [1, 0, 0]])).len(), 60);
        assert_eq!(mesh(&BTreeSet::from([[0, 0, 0], [2, 0, 0]])).len(), 72);
    }
    #[test]
    fn doors_override_voxels_and_respect_deck() {
        let cells = [[0, 0, 0], [0, 0, 1], [0, 0, 3], [0, 0, -1]];
        let mut door = Barrier {
            cell: [0, 0, 1],
            blocks_movement: true,
            blocks_sight: true,
            blocks_light: true,
        };
        assert_eq!(
            occluders(&cells, &[door.clone()], 0),
            BTreeSet::from([[0, 0, 0], [0, 0, 1]])
        );
        door.blocks_light = false;
        assert_eq!(
            occluders(&cells, &[door.clone()], 0),
            BTreeSet::from([[0, 0, 0]])
        );
        door.cell = [2, 0, 1];
        door.blocks_light = true;
        assert!(occluders(&cells, &[door], 0).contains(&[2, 0, 1]));
    }
    #[test]
    fn shadow_projection_covers_station_center() {
        let (v, p) = matrices();
        let center = (p * v).project_point3(Vec3::ZERO);
        assert!(
            center.is_finite()
                && center.x.abs() < 0.001
                && center.y.abs() < 0.001
                && center.z.abs() < 1.
        );
        let restored = (p * v).inverse().project_point3(center);
        assert!(restored.length() < 0.001);
    }
}

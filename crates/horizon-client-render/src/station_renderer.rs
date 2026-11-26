//! Resource-independent station renderer entry point, built by station/render.
//! Uses Horizon's wgpu/winit versions, without the Minecraft renderer's resource graph.
pub mod station_barrier;
pub mod station_floor;
#[path = "station_light.rs"]
pub mod station_light;
mod station_photon_ao;
mod station_photon_material;
mod station_photon_shadow;
mod station_scene_data;
mod station_shader_buffers;
mod station_shader_mips;
mod station_shader_raster;
pub mod station_shader_pack;
pub mod station_shader_pipeline;
mod station_solas_post;
mod station_solas_camera;
mod station_solas_reactive;
mod station_solas_depth;
mod station_solas_focus;
mod station_solas_settings;
pub use station_light::Light;
#[path = "station_sprite_types.rs"]
pub mod sprite_types;
mod station_light_gpu;
#[path = "station_sprite.rs"]
pub mod station_sprite;
pub use sprite_types::{SpriteLayer, SpriteStyle};
pub mod station_sprite_runtime;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
#[path = "gpu_buffer.rs"]
mod gpu_buffer;

use winit::window::Window;
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    p: [f32; 3],
    color: [f32; 4],
    uv: [f32; 2],
    mask_uv: [f32; 2],
    unshaded: f32,
}
type Cell = [i32; 3];
const VIEW_HALF_HEIGHT: f32 = 9.;
const DEPTH_SCALE: f32 = 0.01;
const DEPTH_BIAS: f32 = 0.4;
/// Cached visible faces grouped by chunk. Compare chunk contents plus adjacent cells.
#[derive(Default)]
pub struct MeshCache {
    chunks: BTreeMap<Cell, (BTreeSet<Cell>, Vec<Vertex>)>,
    pub rebuilt: usize,
    dirty: BTreeSet<Cell>,
    deck: Option<i32>,
}
impl MeshCache {
    fn invalidate_floors(&mut self, cells: impl IntoIterator<Item = Cell>) {
        for c in cells {
            for d in [[0, 0], [1, 0], [-1, 0], [0, 1], [0, -1]] {
                let p = [c[0] + d[0], c[1] + d[1], c[2]];
                self.chunks.remove(&p.map(|v| v.div_euclid(16)));
            }
        }
    }

    fn update(&mut self, cells: &[Cell], deck: i32) {
        let solid: BTreeSet<_> = cells.iter().copied().filter(|p| p[2] <= deck + 2).collect();
        let keys: BTreeSet<_> = solid.iter().map(|p| p.map(|v| v.div_euclid(16))).collect();
        self.chunks.retain(|k, _| keys.contains(k));
        self.rebuilt = 0;
        self.dirty.clear();
        let deck_changed = self.deck != Some(deck);
        self.deck = Some(deck);
        for key in keys {
            let neighborhood: BTreeSet<_> = solid
                .iter()
                .copied()
                .filter(|p| (0..3).all(|i| p[i] >= key[i] * 16 - 1 && p[i] <= key[i] * 16 + 16))
                .collect();
            if !deck_changed && self.chunks.get(&key).is_some_and(|c| c.0 == neighborhood) {
                continue;
            }
            let mut vertices = Vec::new();
            for &p in &neighborhood {
                if p.map(|v| v.div_euclid(16)) != key {
                    continue;
                }
                // With a strictly vertical camera only horizontal exposed faces are visible.
                if solid.contains(&[p[0], p[1], p[2] + 1]) {
                    continue;
                }
                let mut color = if p[0] >= 11 {
                    [0.45, 0.40, 0.35, 1.]
                } else if p[2] > deck {
                    [0.4, 0.5, 0.6, 1.]
                } else if (p[0] + p[1]) % 2 == 0 {
                    [0.21, 0.28, 0.35, 1.]
                } else {
                    [0.25, 0.32, 0.39, 1.]
                };
                if p[2] < deck {
                    for component in &mut color[..3] {
                        *component *= 0.3;
                    }
                }
                quad(
                    &mut vertices,
                    [p[0] as f32, p[1] as f32, p[2] as f32 + 1.],
                    [1., 1.],
                    color,
                    false,
                );
            }
            self.chunks.insert(key, (neighborhood, vertices));
            self.rebuilt += 1;
            self.dirty.insert(key);
        }
    }
}
// Wall RSI cardinal-corner layout: caps, one adjacent edge, inset corner, interior.
fn wall_corner(first: bool, second: bool, diagonal: bool) -> u8 {
    match (first, second, diagonal) {
        (false, false, _) => 0,
        (true, false, _) => 1,
        (false, true, _) => 2,
        (true, true, false) => 5,
        (true, true, true) => 6,
    }
}
fn quad(out: &mut Vec<Vertex>, p: [f32; 3], size: [f32; 2], color: [f32; 4], sprite: bool) {
    for i in [0, 1, 2, 0, 2, 3] {
        let uv = [[0., 0.], [1., 0.], [1., 1.], [0., 1.]][i];
        out.push(Vertex {
            p: [p[0] + uv[0] * size[0], p[1] + uv[1] * size[1], p[2]],
            color,
            uv: if sprite { uv } else { [-1., -1.] },
            mask_uv: [-1.; 2],
            unshaded: 0.,
        });
    }
}
pub struct RenderActor {
    pub id: station_sprite_runtime::SpriteId,
    pub style: SpriteStyle,
    pub position: [f32; 3],
    pub facing: [f32; 2],
    pub layers: Vec<SpriteLayer>,
}
pub struct RenderItem {
    pub id: station_sprite_runtime::SpriteId,
    pub style: SpriteStyle,
    pub layers: Vec<SpriteLayer>,
    pub sprite_size: [f32; 2],
    pub color: [f32; 4],
    pub billboard: bool,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub half: f32,
    pub held: bool,
}
fn item_mesh(vertices: &mut Vec<Vertex>, item: &RenderItem) {
    if item.billboard {
        quad(
            vertices,
            [
                item.position[0] - item.half,
                item.position[1] - item.half,
                item.position[2] + item.half,
            ],
            [item.half * 2.; 2],
            item.color,
            true,
        );
        return;
    }
    let corners = [
        [-1., -1., -1.],
        [1., -1., -1.],
        [1., 1., -1.],
        [-1., 1., -1.],
        [-1., -1., 1.],
        [1., -1., 1.],
        [1., 1., 1.],
        [-1., 1., 1.],
    ];
    let q = item.rotation;
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let positions: Vec<[f32; 3]> = corners
        .into_iter()
        .map(|v| {
            let v = v.map(|x| x * item.half);
            let u = [q[0], q[1], q[2]];
            let t = cross(u, v).map(|x| x * 2.);
            let c = cross(u, t);
            std::array::from_fn(|i| v[i] + q[3] * t[i] + c[i] + item.position[i])
        })
        .collect();
    for (face, indices) in [
        [0, 3, 2, 1],
        [4, 5, 6, 7],
        [0, 1, 5, 4],
        [3, 7, 6, 2],
        [0, 4, 7, 3],
        [1, 2, 6, 5],
    ]
    .iter()
    .enumerate()
    {
        let shade = 0.55 + face as f32 * 0.07;
        let color = if item.held {
            [0.2, 0.8, 0.3, 1.]
        } else {
            [
                item.color[0] * shade,
                item.color[1] * shade,
                item.color[2] * shade,
                item.color[3],
            ]
        };
        for i in [0, 1, 2, 0, 2, 3] {
            vertices.push(Vertex {
                p: positions[indices[i]],
                color,
                uv: [-1., -1.],
                mask_uv: [-1.; 2],
                unshaded: 0.,
            });
        }
    }
}
#[derive(PartialEq)]
struct LightKey {
    observer: [i32; 3],
    span: u32,
    lights: Vec<Light>,
    restrict: bool,
    enabled: bool,
}
pub struct Renderer {
    shader_pack: Option<station_shader_pack::FinalStage>,
    pub shader_pack_status: String,
    pub character_size: [f32; 2],
    pub actors: Vec<RenderActor>,
    pub sprite_time: f64,
    pub atlas: station_sprite::Atlas,
    pub sprite_runtime: station_sprite_runtime::SpriteRuntime,
    sprite_hits: Vec<(
        station_sprite_runtime::SpriteId,
        [f32; 3],
        station_sprite_runtime::Sample,
    )>,
    doors: BTreeSet<Cell>,
    floors: BTreeMap<Cell, station_floor::FloorTile>,
    barriers: Vec<station_barrier::Barrier>,
    pub lighting_enabled: bool,
    pub restrict_view: bool,
    pub lights: Vec<Light>,
    occlusion: station_light::Occlusion,
    light_world: Option<Arc<Vec<Cell>>>,
    light_key: Option<LightKey>,
    light_texture: wgpu::Texture,
    gpu_light: station_light_gpu::GpuLight,
    light_params: wgpu::Buffer,
    light_group: wgpu::BindGroup,
    observer: [f32; 3],
    ui: egui_wgpu::Renderer,
    ui_jobs: Vec<egui::ClippedPrimitive>,
    ui_delta: egui::TexturesDelta,
    ui_scale: f32,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    sprite_pipeline: wgpu::RenderPipeline,
    surface_pipelines: [wgpu::RenderPipeline; 2],
    sprite: wgpu::BindGroup,
    depth: wgpu::TextureView,
    cache: MeshCache,
    gpu_chunks: BTreeMap<Cell, (wgpu::Buffer, u32)>,
    last_world: Option<Arc<Vec<Cell>>>,
    camera: wgpu::Buffer,
    camera_group: wgpu::BindGroup,
    actor: wgpu::Buffer,
    actor_capacity: u64,
    pub terrain_uploads: u64,
    profile_frames: Vec<f64>,
    capture: Option<std::path::PathBuf>,
}
impl Renderer {
    pub fn new(window: Arc<Window>) -> Self {
        pollster::block_on(Self::init(window))
    }
    async fn init(window: Arc<Window>) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .unwrap();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .unwrap();
        config.usage |= wgpu::TextureUsages::COPY_SRC;
        config.view_formats.push(config.format.remove_srgb_suffix());
        surface.configure(&device, &config);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("station"),
            source: wgpu::ShaderSource::Wgsl(include_str!("station.wgsl").into()),
        });
        // Explicit layouts let terrain and alpha-sorted sprites share bind groups.
        let entry = |binding, visibility, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty,
            count: None,
        };
        let texture_type = |filterable| wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let uniform_type = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let sprite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite layout"),
            entries: &[
                entry(0, wgpu::ShaderStages::FRAGMENT, texture_type(true)),
                entry(
                    1,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
            ],
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera layout"),
            entries: &[entry(0, wgpu::ShaderStages::VERTEX, uniform_type)],
        });
        let light_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("light layout"),
            entries: &[
                entry(0, wgpu::ShaderStages::FRAGMENT, texture_type(false)),
                entry(1, wgpu::ShaderStages::FRAGMENT, uniform_type),
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("station shared layout"),
            bind_group_layouts: &[
                Some(&sprite_layout),
                Some(&camera_layout),
                Some(&light_layout),
            ],
            immediate_size: 0,
        });
        let create_pipeline = |write_depth, surface| {
            let mut targets = vec![Some(wgpu::ColorTargetState {
                format: config.format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })];
            if surface {
                targets.extend(station_scene_data::FORMATS.iter().enumerate().map(
                    |(i, &format)| {
                        Some(wgpu::ColorTargetState {
                            format,
                            blend: if i == 0 {
                                Some(wgpu::BlendState::ALPHA_BLENDING)
                            } else {
                                None
                            },
                            write_mask: wgpu::ColorWrites::ALL,
                        })
                    },
                ));
            }

            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("station"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x4,2=>Float32x2,3=>Float32x2,4=>Float32],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(if surface {"fs_surface"} else {"fs"}),
                compilation_options: Default::default(),
                targets: &targets,
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(write_depth),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        })
        };
        let pipeline = create_pipeline(true, false);
        let surface_pipelines = [create_pipeline(true, true), create_pipeline(false, true)];
        let sprite_pipeline = create_pipeline(false, false);
        let atlas = if let Some(path) = std::env::var_os("STATION_SPRITE_DIR") {
            station_sprite::Atlas::with_directory(std::path::Path::new(&path))
        } else {
            station_sprite::Atlas::bundled()
        }
        .expect("load RSI sprite atlas");
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SS14 RSI atlas"),
            size: wgpu::Extent3d {
                width: station_sprite::ATLAS_SIZE as u32,
                height: station_sprite::ATLAS_SIZE as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(station_sprite::ATLAS_SIZE as u32 * 4),
                rows_per_image: Some(station_sprite::ATLAS_SIZE as u32),
            },
            wgpu::Extent3d {
                width: station_sprite::ATLAS_SIZE as u32,
                height: station_sprite::ATLAS_SIZE as u32,
                depth_or_array_layers: 1,
            },
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
        let sprite = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sprite"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &texture.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let camera = gpu_buffer::try_create(
            &device,
            "camera matrix",
            64,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        )
        .unwrap();
        let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera"),
            layout: &pipeline.get_bind_group_layout(1),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            }],
        });
        let light_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("station linear light field"),
            size: wgpu::Extent3d {
                width: station_light_gpu::SIZE,
                height: station_light_gpu::SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        let gpu_light = station_light_gpu::GpuLight::new(&device);
        let light_params = gpu_buffer::try_create(
            &device,
            "light field bounds",
            16,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        )
        .unwrap();
        let light_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("station lighting"),
            layout: &pipeline.get_bind_group_layout(2),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &light_texture.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: light_params.as_entire_binding(),
                },
            ],
        });
        let actor = gpu_buffer::try_create(
            &device,
            "actor",
            6 * std::mem::size_of::<Vertex>() as u64,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        )
        .unwrap();
        let depth = Self::depth(&device, &config);
        let ui = egui_wgpu::Renderer::new(&device, config.format, Default::default());
        let path = std::env::args()
            .collect::<Vec<_>>()
            .windows(2)
            .find(|a| {
                matches!(
                    a[0].as_str(),
                    "--shader-pack-final"
                        | "--photon-material-debug"
                        | "--photon-ao"
                        | "--photon-shadows"
                        | "--photon-volumetrics"
                        | "--solas-post"
                        | "--solas-bloom"
                        | "--solas-dof"
                        | "--solas-autofocus"
                        | "--solas-motion"
                        | "--solas-taa"
                )
            })
            .map(|a| (a[1].clone(), a[0].clone()));
        let (shader_pack, shader_pack_status) = if let Some((path, mode)) = path {
            let material_debug = mode == "--photon-material-debug";
            let ao = mode == "--photon-ao";
            let shadows = mode == "--photon-shadows";
            let volumetrics = mode == "--photon-volumetrics";
            let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
            let build = if mode == "--solas-taa" {
                station_shader_pack::FinalStage::new_solas_taa
            } else if mode == "--solas-motion" {
                station_shader_pack::FinalStage::new_solas_motion
            } else if mode == "--solas-autofocus" {
                station_shader_pack::FinalStage::new_solas_autofocus
            } else if mode == "--solas-dof" {
                station_shader_pack::FinalStage::new_solas_dof
            } else if mode == "--solas-bloom" {
                station_shader_pack::FinalStage::new_solas_bloom
            } else if mode == "--solas-post" {
                station_shader_pack::FinalStage::new_solas_post
            } else if volumetrics {
                station_shader_pack::FinalStage::new_volumetrics
            } else if shadows {
                station_shader_pack::FinalStage::new_shadows
            } else if ao {
                station_shader_pack::FinalStage::new_ao
            } else if material_debug {
                station_shader_pack::FinalStage::new_material_debug
            } else {
                station_shader_pack::FinalStage::new
            };
            let result = if mode == "--shader-pack-final" {
                let args: Vec<_> = std::env::args().collect();
                let option = |flag: &str, default: &str| {
                    args.iter()
                        .position(|a| a == flag)
                        .map(|i| args.get(i + 1).cloned().unwrap_or_default())
                        .unwrap_or_else(|| default.into())
                };
                station_shader_pack::FinalInput::parse(
                    &option("--shader-pack-input", "colortex0"),
                    &option("--shader-pack-encoding", "linear"),
                )
                .and_then(|input| {
                    station_shader_pack::FinalStage::new_with_input(
                        &device,
                        std::path::Path::new(&path),
                        config.format,
                        input,
                    )
                })
            } else {
                build(&device, std::path::Path::new(&path), config.format)
            };
            let gpu_error = internal.pop().await.or(validation.pop().await);
            let result = match gpu_error {
                Some(e) => Err(format!("GPU: {e}")),
                None => result,
            };
            match result {
                Ok(pack) => (
                    Some(pack),
                    format!(
                        "{}: {}",
                        std::path::Path::new(&path)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy(),
                        if mode == "--solas-taa" {
                            "Solas TAA + bloom + FXAA + final (experimental station jitter)"
                        } else if mode == "--solas-motion" {
                            "Solas motion blur + bloom + FXAA + final (camera motion; partial compatibility)"
                        } else if mode == "--solas-autofocus" {
                            "Solas autofocus + DOF + bloom + FXAA + final (partial compatibility)"
                        } else if mode == "--solas-dof" {
                            "Solas DOF + bloom + FXAA + final (fixed deck focus; partial compatibility)"
                        } else if mode == "--solas-bloom" {
                            "Solas bloom + composite14 + FXAA + final (partial compatibility)"
                        } else if mode == "--solas-post" {
                            "Solas composite14 + composite15 FXAA + final (partial compatibility)"
                        } else if volumetrics {
                            "Photon volumetric fog + shadows + SSAO + final (partial compatibility)"
                        } else if shadows {
                            "Photon opaque shadows + SSAO + final (partial compatibility)"
                        } else if ao {
                            "Photon SSAO + final GLSL (partial compatibility)"
                        } else if material_debug {
                            "Photon material roundtrip (not lighting)"
                        } else {
                            "final GLSL (partial compatibility)"
                        }
                    ),
                ),
                Err(error) => (None, format!("Shader pack: {error}")),
            }
        } else {
            (None, String::new())
        };
        if !shader_pack_status.is_empty() {
            eprintln!("{shader_pack_status}");
        }
        Self {
            shader_pack,
            shader_pack_status,
            character_size: [1., 1.],
            actors: vec![],
            sprite_time: 0.,
            atlas,
            sprite_runtime: Default::default(),
            sprite_hits: vec![],
            doors: BTreeSet::new(),
            floors: BTreeMap::new(),
            barriers: vec![],
            lighting_enabled: false,
            restrict_view: false,
            lights: vec![],
            occlusion: Default::default(),
            light_world: None,
            light_key: None,
            light_texture,
            gpu_light,
            light_params,
            light_group,
            observer: [0.; 3],
            ui,
            ui_jobs: Vec::new(),
            ui_delta: Default::default(),
            ui_scale: 1.,
            surface,
            device,
            queue,
            config,
            pipeline,
            sprite_pipeline,
            surface_pipelines,
            sprite,
            depth,
            cache: MeshCache::default(),
            gpu_chunks: BTreeMap::new(),
            last_world: None,
            camera,
            camera_group,
            actor,
            actor_capacity: 6 * std::mem::size_of::<Vertex>() as u64,
            terrain_uploads: 0,
            profile_frames: Vec::new(),
            capture: None,
        }
    }
    fn depth(device: &wgpu::Device, c: &wgpu::SurfaceConfiguration) -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d {
                    width: c.width,
                    height: c.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default())
    }
    pub fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
        self.depth = Self::depth(&self.device, &self.config);
    }
    pub fn set_ui(&mut self, ctx: &egui::Context, output: egui::FullOutput) {
        self.ui_scale = output.pixels_per_point;
        self.ui_jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
        self.ui_delta.append(output.textures_delta);
    }
    pub fn capture_next(&mut self, path: impl Into<std::path::PathBuf>) {
        self.capture = Some(path.into());
    }
    pub fn set_floors(&mut self, tiles: Vec<station_floor::FloorTile>) {
        let next: BTreeMap<_, _> = tiles.into_iter().map(|f| (f.cell, f)).collect();
        let dirty: BTreeSet<_> = self
            .floors
            .keys()
            .chain(next.keys())
            .copied()
            .filter(|c| self.floors.get(c) != next.get(c))
            .collect();
        if !dirty.is_empty() {
            self.cache.invalidate_floors(dirty);
            self.last_world = None;
            self.floors = next;
        }
    }
    pub fn set_doors(&mut self, cells: Vec<Cell>) {
        let next = cells.into_iter().collect();
        if self.doors != next {
            self.doors = next;
            self.last_world = None;
            self.cache = MeshCache::default();
        }
    }
    pub fn set_barriers(&mut self, barriers: Vec<station_barrier::Barrier>) {
        if self.barriers != barriers {
            self.barriers = barriers;
            self.light_world = None;
            self.last_world = None;
            self.cache = MeshCache::default();
        }
    }
    pub fn prepare_visibility(&mut self, cells: &Arc<Vec<Cell>>, observer: [f32; 3]) {
        self.observer = observer;
        if self.occlusion.deck != station_light::deck(observer[2])
            || !self
                .light_world
                .as_ref()
                .is_some_and(|v| Arc::ptr_eq(v, cells))
        {
            self.occlusion = station_light::Occlusion::with_barriers(
                cells,
                station_light::deck(observer[2]),
                &self.barriers,
            );
            self.light_world = Some(cells.clone());
            self.light_key = None;
        }
    }
    pub fn is_visible(&self, target: [f32; 3]) -> bool {
        !self.restrict_view || self.occlusion.entity_visible(self.observer, target)
    }
    fn update_light(&mut self) {
        let span = (18. * self.config.width as f32 / self.config.height as f32 + 2.).max(32.);
        let observer = [
            (self.observer[0] * 32.).floor() as i32,
            (self.observer[1] * 32.).floor() as i32,
            self.occlusion.deck,
        ];
        let key = LightKey {
            observer,
            span: span.to_bits(),
            lights: self.lights.clone(),
            restrict: self.restrict_view,
            enabled: self.lighting_enabled,
        };
        if self.light_key.as_ref() == Some(&key) {
            return;
        }
        let origin = [
            observer[0] as f32 / 32. - span / 2.,
            observer[1] as f32 / 32. - span / 2.,
        ];
        let params = [
            origin[0],
            origin[1],
            span,
            if self.lighting_enabled { 1. } else { 0. },
        ];
        self.queue
            .write_buffer(&self.light_params, 0, bytemuck::cast_slice(&params));
        if self.lighting_enabled {
            let profile_start = std::time::Instant::now();
            self.gpu_light.update(
                &self.device,
                &self.queue,
                &self.light_texture,
                &self.occlusion,
                self.observer,
                origin,
                span,
                &self.lights,
                self.restrict_view,
            );
            if std::env::var_os("STATION_PROFILE").is_some() {
                eprintln!(
                    "light CPU submit {:.2} ms",
                    profile_start.elapsed().as_secs_f64() * 1000.
                );
            }
        }
        self.light_key = Some(key);
    }
    pub fn hit_sprite(&self, id: station_sprite_runtime::SpriteId, point: [f32; 2]) -> bool {
        self.sprite_hits.iter().rev().any(|(owner, p, s)| {
            *owner == id && s.hit([point[0] - p[0], point[1] - p[1]], &self.atlas)
        })
    }
    pub fn reload_sprites(&mut self, path: &std::path::Path) -> Result<(), String> {
        let atlas = station_sprite::Atlas::with_directory(path)?;
        // Replacement GPU resources are created before switching the live atlas.
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("reloaded RSI atlas"),
            size: wgpu::Extent3d {
                width: station_sprite::ATLAS_SIZE as u32,
                height: station_sprite::ATLAS_SIZE as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(station_sprite::ATLAS_SIZE as u32 * 4),
                rows_per_image: None,
            },
            texture.size(),
        );
        let view = texture.create_view(&Default::default());
        let sampler = self.device.create_sampler(&Default::default());
        self.sprite = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("reloaded sprites"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        self.atlas = atlas;
        self.sprite_runtime.reset();
        self.sprite_hits.clear();
        self.last_world = None;
        self.cache.deck = None;
        Ok(())
    }
    pub fn draw(&mut self, cells: &Arc<Vec<Cell>>, player: [f32; 3], items: &[RenderItem]) {
        self.draw_with_peers(cells, player, items, &[]);
    }
    pub fn draw_with_peers(
        &mut self,
        cells: &Arc<Vec<Cell>>,
        player: [f32; 3],
        items: &[RenderItem],
        peers: &[[f32; 3]],
    ) {
        let frame_started = std::time::Instant::now();
        let profiling = std::env::var_os("STATION_PROFILE").is_some();
        let capturing = self.capture.is_some();
        self.prepare_visibility(cells, player);
        self.update_light();
        let deck = if player[2] >= 0.5 { 0 } else { -4 };
        if self.cache.deck != Some(deck)
            || !self
                .last_world
                .as_ref()
                .is_some_and(|old| Arc::ptr_eq(old, cells))
        {
            self.cache.update(cells, deck);
            self.gpu_chunks
                .retain(|key, _| self.cache.chunks.contains_key(key));
            for key in &self.cache.dirty {
                let neighbors: BTreeSet<[i32; 2]> = self.cache.chunks[key]
                    .0
                    .iter()
                    .filter(|p| p[2] > deck)
                    .map(|p| [p[0], p[1]])
                    .collect();
                let mut vertices = Vec::new();
                for face in self.cache.chunks[key].1.as_chunks::<6>().0 {
                    if face[0].p[2] <= deck as f32 + 1. {
                        let p = face[0].p;
                        let cell = [p[0] as i32, p[1] as i32, (p[2] - 1.) as i32];
                        if [0, -4].contains(&cell[2]) {
                            let tile = self.floors.get(&cell).copied().unwrap_or(
                                station_floor::FloorTile {
                                    cell,
                                    kind: Default::default(),
                                    variant: None,
                                },
                            );
                            let rect = self.atlas.frame_at(
                                "floor",
                                tile.kind.state(),
                                [0., -1.],
                                tile.frame(),
                            );
                            let mut textured = *face;
                            for (v, i) in textured.iter_mut().zip([0, 1, 2, 0, 2, 3]) {
                                let uv = [[0., 1.], [1., 1.], [1., 0.], [0., 0.]][i];
                                v.uv = std::array::from_fn(|a| {
                                    rect.min[a] + uv[a] * (rect.max[a] - rect.min[a])
                                });
                                v.color = if cell[2] < deck {
                                    [0.3, 0.3, 0.3, 1.]
                                } else {
                                    [1.; 4]
                                };
                            }
                            vertices.extend_from_slice(&textured);
                            // One-pixel trim owned by one side only, never doubled.
                            let w = 1. / 32.;
                            for (d, offset, size) in [
                                ([0, -1], [0., 0.], [1., w]),
                                ([1, 0], [1. - w, 0.], [w, 1.]),
                                ([0, 1], [0., 1. - w], [1., w]),
                                ([-1, 0], [0., 0.], [w, 1.]),
                            ] {
                                let other = [cell[0] + d[0], cell[1] + d[1], cell[2]];
                                let exists = self.cache.chunks[key].0.contains(&other);
                                let kind = self
                                    .floors
                                    .get(&other)
                                    .map_or(station_floor::FloorKind::default(), |f| f.kind);
                                if !exists || tile.kind > kind {
                                    quad(
                                        &mut vertices,
                                        [p[0] + offset[0], p[1] + offset[1], p[2] + 0.001],
                                        size,
                                        [0.14, 0.17, 0.18, 1.],
                                        false,
                                    );
                                }
                            }
                        } else {
                            vertices.extend_from_slice(face);
                        }
                        continue;
                    }
                    let p = face[0].p;
                    let cell = [p[0] as i32, p[1] as i32];
                    if self.doors.contains(&[cell[0], cell[1], deck + 1]) {
                        continue;
                    }
                    if let Some(b) = self
                        .barriers
                        .iter()
                        .find(|b| b.cell == [cell[0], cell[1], (p[2] - 1.) as i32])
                        && (!b.blocks_sight || !b.blocks_light)
                    {
                        let frame = [0.3, 0.5, 0.55, 1.];
                        for (offset, size) in [
                            ([0., 0.], [1., 0.07]),
                            ([0., 0.93], [1., 0.07]),
                            ([0., 0.], [0.07, 1.]),
                            ([0.93, 0.], [0.07, 1.]),
                        ] {
                            quad(
                                &mut vertices,
                                [p[0] + offset[0], p[1] + offset[1], p[2]],
                                size,
                                frame,
                                false,
                            );
                        }
                        quad(
                            &mut vertices,
                            [p[0] + 0.07, p[1] + 0.07, p[2]],
                            [0.86, 0.86],
                            [0.3, 0.65, 0.75, 0.22],
                            false,
                        );
                        continue;
                    }
                    for (first, second, facing) in [
                        ([0, -1], [1, 0], [0., -1.]),
                        ([1, 0], [0, 1], [1., 0.]),
                        ([0, 1], [-1, 0], [0., 1.]),
                        ([-1, 0], [0, -1], [-1., 0.]),
                    ] {
                        let has =
                            |d: [i32; 2]| neighbors.contains(&[cell[0] + d[0], cell[1] + d[1]]);
                        let state = wall_corner(
                            has(first),
                            has(second),
                            has([first[0] + second[0], first[1] + second[1]]),
                        );
                        let rect = self
                            .atlas
                            .frame("wall", &format!("wall{state}"), facing, 0.);
                        let mut corner = *face;
                        for (vertex, index) in corner.iter_mut().zip([0, 1, 2, 0, 2, 3]) {
                            let uv = [[0., 1.], [1., 1.], [1., 0.], [0., 0.]][index];
                            vertex.uv = std::array::from_fn(|i| {
                                rect.min[i] + uv[i] * (rect.max[i] - rect.min[i])
                            });
                            vertex.color = [1.; 4];
                        }
                        vertices.extend_from_slice(&corner);
                    }
                }
                if vertices.is_empty() {
                    self.gpu_chunks.remove(key);
                    continue;
                }
                let bytes = bytemuck::cast_slice(&vertices);
                let buffer = gpu_buffer::try_create(
                    &self.device,
                    "station chunk",
                    bytes.len() as u64,
                    wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                )
                .expect("terrain allocation");
                self.queue.write_buffer(&buffer, 0, bytes);
                self.gpu_chunks
                    .insert(*key, (buffer, vertices.len() as u32));
                self.terrain_uploads += 1;
            }
            self.last_world = Some(cells.clone());
        }
        if let Some(pack) = &mut self.shader_pack {
            pack.prepare_shadows(
                &self.device,
                &self.queue,
                cells,
                &self.barriers,
                deck,
                player,
            );
        }
        let aspect = self.config.width as f32 / self.config.height as f32;
        let mut matrix = projection(player, aspect);
        if let Some(pack)=&self.shader_pack {
            let jitter=pack.scene_jitter([self.config.width,self.config.height]);
            matrix[12]+=jitter[0];matrix[13]+=jitter[1];
        }
        self.queue
            .write_buffer(&self.camera, 0, bytemuck::cast_slice(&matrix));
        let mut vertices = Vec::with_capacity(6);
        if self.actors.is_empty() && self.character_size[0] > 0. {
            sprite_quad(
                &mut vertices,
                player,
                self.character_size,
                [1.; 4],
                self.atlas.fallback,
                0.32,
            );
            for p in peers {
                if self.is_visible(*p) {
                    sprite_quad(
                        &mut vertices,
                        *p,
                        self.character_size,
                        [1.; 4],
                        self.atlas.fallback,
                        0.32,
                    );
                }
            }
        }
        let alive = self
            .actors
            .iter()
            .map(|a| a.id)
            .chain(items.iter().map(|i| i.id))
            .collect();
        self.sprite_runtime.begin(self.sprite_time, &alive);
        self.sprite_hits.clear();
        let mut batches = Vec::new();
        for actor in &self.actors {
            let visible =
                self.is_visible(actor.position) && station_light::deck(actor.position[2]) == deck;
            let samples = self.sprite_runtime.sample(
                actor.id,
                &actor.layers,
                &actor.style,
                self.character_size,
                actor.facing,
                visible,
                &self.atlas,
            );
            if visible {
                batches.push(SpriteBatch {
                    id: actor.id,
                    depth: actor.style.draw_depth,
                    position: actor.position,
                    samples,
                });
            }
        }
        for item in items {
            let visible = item.position[2] - item.half <= deck as f32 + 3.
                && station_light::deck(item.position[2]) == deck
                && self.is_visible(item.position);
            let samples = self.sprite_runtime.sample(
                item.id,
                &item.layers,
                &item.style,
                item.sprite_size,
                {
                    let [x, y, z, w] = item.rotation;
                    let yaw = (2. * (w * z + x * y)).atan2(1. - 2. * (y * y + z * z));
                    [yaw.sin(), -yaw.cos()]
                },
                visible,
                &self.atlas,
            );
            if !visible {
                continue;
            }
            if !item.layers.is_empty() {
                self.sprite_hits
                    .extend(samples.iter().cloned().map(|s| (item.id, item.position, s)));
                batches.push(SpriteBatch {
                    id: item.id,
                    depth: item.style.draw_depth,
                    position: item.position,
                    samples,
                });
            } else if item.billboard {
                sprite_quad(
                    &mut vertices,
                    item.position,
                    [item.half * 2.; 2],
                    item.color,
                    self.atlas.fallback,
                    item.half,
                );
            } else {
                item_mesh(&mut vertices, item);
            }
        }
        let mesh_vertices = vertices.len() as u32;
        sort_sprites(&mut batches);
        for batch in &batches {
            append_samples(
                &mut vertices,
                &batch.samples,
                batch.position,
                deck as f32 + 1.9,
            );
        }
        if let Some(pack) = &mut self.shader_pack { pack.update_dynamic_history(&vertices); }
        let bytes = bytemuck::cast_slice(&vertices);
        if bytes.len() as u64 > self.actor_capacity {
            self.actor_capacity = (bytes.len() as u64).next_power_of_two();
            self.actor = gpu_buffer::try_create(
                &self.device,
                "entities",
                self.actor_capacity,
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            )
            .unwrap();
        }
        self.queue.write_buffer(&self.actor, 0, bytes);
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(v)
            | wgpu::CurrentSurfaceTexture::Suboptimal(v) => v,
            _ => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
        };
        let view = output.texture.create_view(&Default::default());
        let scene_view = self
            .shader_pack
            .as_mut()
            .map(|pack| pack.scene(&self.device, [self.config.width, self.config.height]));
        let surface_views = self
            .shader_pack
            .as_ref()
            .and_then(|pack| pack.scene_data_views());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut attachments = vec![Some(wgpu::RenderPassColorAttachment {
                view: scene_view.as_ref().unwrap_or(&view),
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: if self.restrict_view { 0. } else { 0.01 },
                        g: if self.restrict_view { 0. } else { 0.02 },
                        b: if self.restrict_view { 0. } else { 0.04 },
                        a: 1.,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })];
            if let Some(views) = &surface_views {
                attachments.extend(views.iter().enumerate().map(|(i, view)| {
                    Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(if i == 2 {
                                wgpu::Color::WHITE
                            } else {
                                wgpu::Color::TRANSPARENT
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })
                }));
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("station"),
                color_attachments: &attachments,
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
            pass.set_pipeline(if surface_views.is_some() {
                &self.surface_pipelines[0]
            } else {
                &self.pipeline
            });
            pass.set_bind_group(0, &self.sprite, &[]);
            pass.set_bind_group(1, &self.camera_group, &[]);
            pass.set_bind_group(2, &self.light_group, &[]);
            for (buffer, count) in self.gpu_chunks.values() {
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..*count, 0..1);
            }
            pass.set_vertex_buffer(0, self.actor.slice(..));
            pass.draw(0..mesh_vertices, 0..1);
            pass.set_pipeline(if surface_views.is_some() {
                &self.surface_pipelines[1]
            } else {
                &self.sprite_pipeline
            });
            pass.draw(mesh_vertices..vertices.len() as u32, 0..1);
        }
        if let Some(pack) = &mut self.shader_pack {
            let display = output.texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(self.config.format.remove_srgb_suffix()),
                ..Default::default()
            });
            pack.draw(&self.device, &self.queue, &mut encoder, &display);
        }
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point: self.ui_scale,
        };
        for (id, deltas) in &self.ui_delta.set {
            for delta in deltas {
                self.ui
                    .update_texture(&self.device, &self.queue, *id, delta);
            }
        }
        let callbacks = self.ui.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &self.ui_jobs,
            &screen,
        );
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("station UI"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            self.ui
                .render(&mut pass.forget_lifetime(), &self.ui_jobs, &screen);
        }
        self.queue.submit(callbacks);
        for id in &self.ui_delta.free {
            self.ui.free_texture(id);
        }
        self.ui_delta.clear();
        let capture = self.capture.take().map(|path| {
            let stride = (self.config.width * 4).div_ceil(256) * 256;
            let buffer = gpu_buffer::try_create(
                &self.device,
                "capture",
                (stride * self.config.height) as u64,
                wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            )
            .unwrap();
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &output.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(stride),
                        rows_per_image: Some(self.config.height),
                    },
                },
                wgpu::Extent3d {
                    width: self.config.width,
                    height: self.config.height,
                    depth_or_array_layers: 1,
                },
            );
            (path, buffer, stride)
        });
        self.queue.submit([encoder.finish()]);
        if let Some((path, buffer, stride)) = capture {
            if let Some((updates, reused)) = self
                .shader_pack
                .as_ref()
                .and_then(|pack| pack.shadow_cache_stats())
            {
                eprintln!("Photon shadow cache: updates={updates} reused={reused}");
            }
            let (tx, rx) = std::sync::mpsc::channel();
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
            self.device
                .poll(wgpu::PollType::wait_indefinitely())
                .unwrap();
            rx.recv().unwrap().unwrap();
            let mapped = buffer.slice(..).get_mapped_range().unwrap();
            let mut rgba = Vec::new();
            for row in mapped.chunks(stride as usize) {
                rgba.extend_from_slice(&row[..self.config.width as usize * 4]);
            }
            if matches!(
                self.config.format,
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
            ) {
                for p in rgba.as_chunks_mut::<4>().0 {
                    p.swap(0, 2);
                }
            }
            let mut encoder = png::Encoder::new(
                std::fs::File::create(path).unwrap(),
                self.config.width,
                self.config.height,
            );
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&rgba)
                .unwrap();
            drop(mapped);
            buffer.unmap();
        }
        self.queue.present(output);
        if profiling && !capturing && self.profile_frames.len() < 10000 {
            self.profile_frames
                .push(frame_started.elapsed().as_secs_f64() * 1000.);
        }
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        if self.profile_frames.is_empty() {
            return;
        }
        self.profile_frames.sort_by(f64::total_cmp);
        let n = self.profile_frames.len();
        eprintln!(
            "render CPU/wait frames={n} median={:.2}ms p95={:.2}ms max={:.2}ms (captures excluded)",
            self.profile_frames[n / 2],
            self.profile_frames[(n - 1) * 95 / 100],
            self.profile_frames[n - 1]
        );
    }
}
struct SpriteBatch {
    id: station_sprite_runtime::SpriteId,
    depth: i16,
    position: [f32; 3],
    samples: Vec<station_sprite_runtime::Sample>,
}
fn sort_sprites(batches: &mut [SpriteBatch]) {
    // DrawDepth first, then feet from north to south; identity breaks exact ties.
    // Whole entities stay together so a hand cannot interleave with another body.
    batches.sort_by(|a, b| {
        a.depth
            .cmp(&b.depth)
            .then_with(|| b.position[1].total_cmp(&a.position[1]))
            .then_with(|| a.id.cmp(&b.id))
    });
}
fn append_samples(
    vertices: &mut Vec<Vertex>,
    samples: &[station_sprite_runtime::Sample],
    position: [f32; 3],
    plane: f32,
) {
    for s in samples {
        for corner in [0, 1, 2, 0, 2, 3] {
            let mut tex = [[0., 1.], [1., 1.], [1., 0.], [0., 0.]][corner];
            for (i, v) in tex.iter_mut().enumerate() {
                if s.mirror[i] {
                    *v = 1. - *v;
                }
            }
            let uv = std::array::from_fn(|i| s.uv.min[i] + tex[i] * (s.uv.max[i] - s.uv.min[i]));
            vertices.push(Vertex {
                p: [
                    position[0] + s.corners[corner][0],
                    position[1] + s.corners[corner][1],
                    plane,
                ],
                color: s.color,
                uv,
                mask_uv: s.mask.map_or([-1.; 2], |r| {
                    std::array::from_fn(|i| r.min[i] + tex[i] * (r.max[i] - r.min[i]))
                }),
                unshaded: if s.unshaded { 1. } else { 0. },
            });
        }
    }
}
fn projection(player: [f32; 3], aspect: f32) -> [f32; 16] {
    [
        1. / (VIEW_HALF_HEIGHT * aspect),
        0.,
        0.,
        0.,
        0.,
        1. / VIEW_HALF_HEIGHT,
        0.,
        0.,
        0.,
        0.,
        -DEPTH_SCALE,
        0.,
        -player[0] / (VIEW_HALF_HEIGHT * aspect),
        -player[1] / VIEW_HALF_HEIGHT,
        DEPTH_BIAS,
        1.,
    ]
}

fn sprite_quad(
    out: &mut Vec<Vertex>,
    center: [f32; 3],
    size: [f32; 2],
    color: [f32; 4],
    rect: station_sprite::Rect,
    height: f32,
) {
    for i in [0, 1, 2, 0, 2, 3] {
        let uv = [[0., 0.], [1., 0.], [1., 1.], [0., 1.]][i];
        out.push(Vertex {
            p: [
                center[0] + (uv[0] - 0.5) * size[0],
                center[1] + (uv[1] - 0.5) * size[1],
                center[2] + height,
            ],
            color,
            mask_uv: [-1.; 2],
            unshaded: 0.,
            uv: [
                rect.min[0] + uv[0] * (rect.max[0] - rect.min[0]),
                rect.max[1] - uv[1] * (rect.max[1] - rect.min[1]),
            ],
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn floor_edit_rebuilds_boundary_chunks_but_not_distant_chunks() {
        let cells = vec![[15, 2, 0], [16, 2, 0], [40, 2, 0]];
        let mut cache = MeshCache::default();
        cache.update(&cells, 0);
        assert_eq!(cache.rebuilt, 3);
        cache.invalidate_floors([[15, 2, 0]]);
        cache.update(&cells, 0);
        assert_eq!(cache.rebuilt, 2);
        assert!(!cache.dirty.contains(&[2, 0, 0]));
        cache.update(&cells, 0);
        assert_eq!(cache.rebuilt, 0);
        let atlas = station_sprite::Atlas::bundled().unwrap();
        for kind in station_floor::FloorKind::ALL {
            assert!(atlas.contains("floor", kind.state()));
            assert_eq!(
                atlas.delays("floor", kind.state()).unwrap().len(),
                kind.variants() as usize
            );
        }
    }
    #[test]
    fn sprites_sort_by_depth_feet_and_identity_not_body_height() {
        let batch = |id, y, z, depth| SpriteBatch {
            id: [0, id, 0, 0],
            position: [0., y, z],
            depth,
            samples: vec![],
        };
        let mut b = vec![
            batch(3, 1., 1.3, 0),
            batch(2, 1., 2., 0),
            batch(1, 2., 1.1, 0),
            batch(4, 3., 1., 1),
        ];
        sort_sprites(&mut b);
        assert_eq!(b.iter().map(|b| b.id[1]).collect::<Vec<_>>(), [1, 2, 3, 4]);
        b.reverse();
        sort_sprites(&mut b);
        assert_eq!(b.iter().map(|b| b.id[1]).collect::<Vec<_>>(), [1, 2, 3, 4]);
    }

    #[test]
    fn wall_corner_covers_caps_edges_concave_and_filled_junctions() {
        assert_eq!(wall_corner(false, false, false), 0);
        assert_eq!(wall_corner(false, false, true), 0);
        assert_eq!(wall_corner(true, false, false), 1);
        assert_eq!(wall_corner(false, true, false), 2);
        assert_eq!(wall_corner(true, true, false), 5);
        assert_eq!(wall_corner(true, true, true), 6);
    }
    #[test]
    fn projection_is_orthographic_and_tracks_player() {
        let a = projection([2., 3., 1.], 2.);
        let b = projection([2., 3., -4.], 2.);
        assert_eq!(a, b);
        assert!((a[0] * 2. + a[12]).abs() < 1e-6);
        assert!((a[5] * 3. + a[13]).abs() < 1e-6);
        assert_eq!(a[15], 1.);
    }
    #[test]
    fn removing_last_chunk_releases_cache() {
        let mut c = MeshCache::default();
        c.update(&[[0, 0, 0]], 0);
        c.update(&[], 0);
        assert!(c.chunks.is_empty());
    }
    #[test]
    fn cache_rebuilds_boundary_neighbors_only() {
        let mut c = MeshCache::default();
        let cells = [[15, 0, 0], [16, 0, 0], [48, 0, 0]];
        c.update(&cells, 0);
        assert_eq!(c.rebuilt, 3);
        c.update(&cells, 0);
        assert_eq!(c.rebuilt, 0);
        c.update(&[[15, 0, 0], [16, 0, 0], [15, 0, 1], [48, 0, 0]], 0);
        assert_eq!(c.rebuilt, 2);
    }
}

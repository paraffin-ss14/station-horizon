//! Native station surface inputs. These are not Photon's packed colortex material format.
pub const FORMATS: [wgpu::TextureFormat; 3] = [
    wgpu::TextureFormat::Rgba8Unorm,
    wgpu::TextureFormat::Rgba16Float,
    wgpu::TextureFormat::R32Float,
];
pub struct SceneData {
    size: [u32; 2],
    pipelines: [wgpu::RenderPipeline; 3],
    targets: Option<[Target; 3]>,
}
struct Target {
    view: wgpu::TextureView,
    flipped: wgpu::TextureView,
    group: wgpu::BindGroup,
}
impl SceneData {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("native scene rows"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        Self {
            size: [0; 2],
            targets: None,
            pipelines: std::array::from_fn(|i| {
                super::station_shader_pack::render_pipeline(
                    device,
                    None,
                    &shader,
                    &shader,
                    "vs",
                    if i == 2 { "depth" } else { "color" },
                    FORMATS[i],
                )
            }),
        }
    }
    pub fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.size == size {
            return;
        }
        self.targets = Some(std::array::from_fn(|i| {
            let make = || {
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("native station surface"),
                        size: wgpu::Extent3d {
                            width: size[0],
                            height: size[1],
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: FORMATS[i],
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            };
            let view = make();
            let flipped = make();
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("native scene row copy"),
                layout: &self.pipelines[i].get_bind_group_layout(0),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                }],
            });
            Target {
                view,
                flipped,
                group,
            }
        }));
        self.size = size;
    }
    pub fn views(&self) -> [wgpu::TextureView; 3] {
        std::array::from_fn(|i| self.targets.as_ref().unwrap()[i].view.clone())
    }
    pub fn input(&self, name: &str) -> Option<&wgpu::TextureView> {
        let index = match name {
            "stationAlbedo" => 0,
            "stationNormal" => 1,
            "depthtex0" => 2,
            _ => return None,
        };
        Some(&self.targets.as_ref().unwrap()[index].flipped)
    }
    pub fn flip(&self, encoder: &mut wgpu::CommandEncoder) {
        for (pipeline, target) in self.pipelines.iter().zip(self.targets.as_ref().unwrap()) {
            super::station_shader_pack::run_pass(
                encoder,
                &target.flipped,
                pipeline,
                &target.group,
                3,
            );
        }
    }
}
const SHADER:&str="
@group(0) @binding(0) var source:texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {let p=array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.));return vec4f(p[i],0.,1.);}
fn pixel(p:vec4f)->vec4f {let size=textureDimensions(source);return textureLoad(source,vec2i(i32(p.x),i32(size.y)-1-i32(p.y)),0);}
@fragment fn color(@builtin(position) p:vec4f)->@location(0) vec4f {return pixel(p);}
@fragment fn depth(@builtin(position) p:vec4f)->@location(0) f32 {return pixel(p).r;}
";

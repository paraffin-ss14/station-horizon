//! Reserve Solas' foreground/hand depth range without changing scene geometry.
pub fn remap(depth: f32) -> f32 {
    0.6 + 0.4 * depth
}
pub struct Depth {
    pipeline: wgpu::RenderPipeline,
    target: Option<wgpu::TextureView>,
    size: [u32; 2],
}
impl Depth {
    pub fn new(device: &wgpu::Device, taa: bool) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Solas DOF depth adapter"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
@group(0) @binding(0) var depth: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let p=array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.));return vec4f(p[i],0.,1.);
}
@fragment fn fs(@builtin(position) p:vec4f)->@location(0) f32 {
 return 0.6 + 0.4 * textureLoad(depth,vec2i(p.xy),0).r;
}
"#
                .replace("0.6 + 0.4", if taa { "0.8 + 0.2" } else { "0.6 + 0.4" })
                .into(),
            ),
        });
        Self {
            pipeline: super::station_shader_pack::render_pipeline(
                device,
                None,
                &shader,
                &shader,
                "vs",
                "fs",
                wgpu::TextureFormat::R32Float,
            ),
            target: None,
            size: [0; 2],
        }
    }
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
        source: &wgpu::TextureView,
    ) {
        if self.size != size {
            self.target = Some(
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("Solas DOF depth"),
                        size: wgpu::Extent3d {
                            width: size[0],
                            height: size[1],
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::R32Float,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    })
                    .create_view(&Default::default()),
            );
            self.size = size;
        }
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(source),
            }],
        });
        super::station_shader_pack::run_pass(encoder, self.view(), &self.pipeline, &group, 3);
    }
    pub fn view(&self) -> &wgpu::TextureView {
        self.target.as_ref().unwrap()
    }
}

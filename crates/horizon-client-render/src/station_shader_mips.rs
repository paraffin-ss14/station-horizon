//! Linear RGBA16Float mip chain for shaderpack colour inputs.
use super::station_shader_pack::{render_pipeline, run_pass};
pub struct Mips {
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    size: [u32; 2],
    levels: Vec<wgpu::TextureView>,
    view: Option<wgpu::TextureView>,
}
fn sizes(mut size: [u32; 2]) -> Vec<[u32; 2]> {
    let mut out = vec![size];
    while size != [1, 1] {
        size = [(size[0] / 2).max(1), (size[1] / 2).max(1)];
        out.push(size);
    }
    out
}
impl Mips {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shaderpack mip generation"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        Self {
            pipeline: render_pipeline(
                device,
                None,
                &shader,
                &shader,
                "vs",
                "fs",
                wgpu::TextureFormat::Rgba16Float,
            ),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            size: [0; 2],
            levels: vec![],
            view: None,
        }
    }
    pub fn generate(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
        source: &wgpu::TextureView,
    ) {
        if self.size != size {
            let count = sizes(size).len() as u32;
            let t = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("shaderpack colour mip pyramid"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: count,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            self.view = Some(t.create_view(&Default::default()));
            self.levels = (0..count)
                .map(|i| {
                    t.create_view(&wgpu::TextureViewDescriptor {
                        base_mip_level: i,
                        mip_level_count: Some(1),
                        ..Default::default()
                    })
                })
                .collect();
            self.size = size;
        }
        for (i, level) in self.levels.iter().enumerate() {
            let input = if i == 0 { source } else { &self.levels[i - 1] };
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(input),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            run_pass(encoder, level, &self.pipeline, &group, 3);
        }
    }
    pub fn view(&self) -> &wgpu::TextureView {
        self.view.as_ref().unwrap()
    }
}
const SHADER: &str = r"
@group(0) @binding(0) var input:texture_2d<f32>;
@group(0) @binding(1) var smp:sampler;
struct Vertex {@builtin(position) position:vec4f,@location(0) uv:vec2f}
@vertex fn vs(@builtin(vertex_index) i:u32)->Vertex {
 let p=array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.));var v:Vertex;v.position=vec4f(p[i],0.,1.);v.uv=p[i]*vec2f(0.5,-0.5)+vec2f(0.5);return v;
}
@fragment fn fs(v:Vertex)->@location(0) vec4f{return textureSampleLevel(input,smp,v.uv,0.);}
";
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires GPU; run explicitly with --ignored"]
    fn gpu_mips_keep_hdr_and_alpha_across_resize() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let mut chain = Mips::new(&device);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let inspect = render_pipeline(
            &device,
            None,
            &shader,
            &shader,
            "vs",
            "fs",
            wgpu::TextureFormat::Rgba32Float,
        );
        for size in [[7, 3], [1, 8], [1, 1]] {
            let input = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let values = vec![[0x4400u16, 0x4000, 0x3800, 0x3400]; (size[0] * size[1]) as usize];
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &input,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&values),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size[0] * 8),
                    rows_per_image: Some(size[1]),
                },
                input.size(),
            );
            let output = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 256,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            chain.generate(
                &device,
                &mut encoder,
                size,
                &input.create_view(&Default::default()),
            );
            assert_eq!(chain.levels.len(), sizes(size).len());
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &inspect.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(chain.levels.last().unwrap()),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&chain.sampler),
                    },
                ],
            });
            run_pass(
                &mut encoder,
                &output.create_view(&Default::default()),
                &inspect,
                &group,
                3,
            );
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &output,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(1),
                    },
                },
                output.size(),
            );
            queue.submit([encoder.finish()]);
            let (tx, rx) = std::sync::mpsc::channel();
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            rx.recv().unwrap().unwrap();
            let mapped = buffer.slice(..).get_mapped_range().unwrap();
            let pixel: &[f32] = bytemuck::cast_slice(&mapped[..16]);
            for (actual, expected) in pixel.iter().zip([4., 2., 0.5, 0.25]) {
                assert!(
                    (actual - expected).abs() < 0.005,
                    "HDR/alpha lost for {size:?}: {pixel:?}"
                );
            }
        }
    }
    #[test]
    fn mip_sizes_cover_odd_and_narrow_windows() {
        assert_eq!(sizes([1, 1]), vec![[1, 1]]);
        assert_eq!(sizes([7, 3]), vec![[7, 3], [3, 1], [1, 1]]);
        assert_eq!(sizes([1, 8]), vec![[1, 8], [1, 4], [1, 2], [1, 1]]);
        assert_eq!(sizes([1200, 800]).len(), 11);
    }
}

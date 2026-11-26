//! Spatial reconstruction of fog only. Surface texels and UI are never blurred.
use super::super::station_shader_pack::{render_pipeline, run_pass};
use super::{SceneData, texture};
pub(super) struct FogFilter {
    pipeline: wgpu::RenderPipeline,
    targets: Vec<wgpu::TextureView>,
    bound_views: Vec<wgpu::TextureView>,
    group: Option<wgpu::BindGroup>,
}
impl FogFilter {
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            pipeline: pipeline(device, wgpu::TextureFormat::Rgba16Float),
            targets: Vec::new(),
            bound_views: Vec::new(),
            group: None,
        }
    }
    pub fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        self.bound_views.clear();
        self.group = None;
        self.targets = (0..3)
            .map(|_| texture(device, size, wgpu::TextureFormat::Rgba16Float))
            .collect();
    }
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        eval: &wgpu::RenderPipeline,
        group: &wgpu::BindGroup,
        scene: &SceneData,
        output: &wgpu::TextureView,
    ) {
        let attachments = std::array::from_fn::<_, 3, _>(|i| {
            Some(wgpu::RenderPassColorAttachment {
                view: &self.targets[i],
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Photon fog components"),
                color_attachments: &attachments,
                ..Default::default()
            });
            pass.set_pipeline(eval);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..6, 0..1);
        }
        let views = [
            self.targets[0].clone(),
            self.targets[1].clone(),
            self.targets[2].clone(),
            scene.input("depthtex0").unwrap().clone(),
            scene.input("stationNormal").unwrap().clone(),
        ];
        if self.bound_views.as_slice() != views.as_slice() {
            self.group = Some(bind(
                device,
                &self.pipeline,
                &views.iter().collect::<Vec<_>>(),
            ));
            self.bound_views = views.to_vec();
        }
        run_pass(
            encoder,
            output,
            &self.pipeline,
            self.group.as_ref().unwrap(),
            3,
        );
    }
}
fn pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("edge-aware Photon fog resolve"),
        source: wgpu::ShaderSource::Wgsl(FILTER.into()),
    });
    render_pipeline(device, None, &shader, &shader, "vs", "fs", format)
}
fn bind(
    device: &wgpu::Device,
    pipeline: &wgpu::RenderPipeline,
    views: &[&wgpu::TextureView],
) -> wgpu::BindGroup {
    let entries: Vec<_> = views
        .iter()
        .enumerate()
        .map(|(i, view)| wgpu::BindGroupEntry {
            binding: i as u32,
            resource: wgpu::BindingResource::TextureView(view),
        })
        .collect();
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Photon fog resolve inputs"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &entries,
    })
}
const FILTER: &str = r"
@group(0) @binding(0) var surface:texture_2d<f32>;
@group(0) @binding(1) var scattering:texture_2d<f32>;
@group(0) @binding(2) var transmission:texture_2d<f32>;
@group(0) @binding(3) var depth:texture_2d<f32>;
@group(0) @binding(4) var normal:texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let p=array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.));return vec4f(p[i],0.,1.);
}
@fragment fn fs(@builtin(position) pos:vec4f)->@location(0) vec4f {
 let p=vec2i(pos.xy);let base=textureLoad(surface,p,0);
 let center=textureLoad(scattering,p,0);
 if(center.a<0.5){return base;}
 let d=textureLoad(depth,p,0).r;let n=textureLoad(normal,p,0).xyz;
 let size=vec2i(textureDimensions(surface));
 const spatial=array<f32,5>(0.3678794412,0.7788007831,1.0,0.7788007831,0.3678794412);
 var sum=0.;var s=vec3f(0.);var t=vec3f(0.);
 for(var y=-2;y<=2;y++){for(var x=-2;x<=2;x++){
  let q=p+vec2i(x,y);
  if(any(q<vec2i(0))||any(q>=size)){continue;}
  let fog=textureLoad(scattering,q,0);
  if(fog.a<0.5){continue;}
  let dz=abs(textureLoad(depth,q,0).r-d);
  // Native depth changes 0.01 per world unit. Reject different surfaces outright.
  if(dz>0.0005||dot(n,textureLoad(normal,q,0).xyz)<0.95){continue;}
  let w=spatial[x+2]*spatial[y+2]*exp(-dz*4000.0);
  s+=fog.rgb*w;t+=textureLoad(transmission,q,0).rgb*w;sum+=w;
 }}
 if(sum<=0.0){return vec4f(base.rgb*textureLoad(transmission,p,0).rgb+center.rgb,base.a);}
 // Keep HDR values; never clamp the surface colour.
 return vec4f(base.rgb*(t/sum)+s/sum,base.a);
}
";

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a GPU; run explicitly with --ignored"]
    fn gpu_fog_filter_preserves_edges_visibility_texture_and_hdr() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&Default::default())).expect("GPU adapter");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).expect("GPU device");
        let pipeline = pipeline(&device, wgpu::TextureFormat::Rgba32Float);
        let extent = wgpu::Extent3d {
            width: 32,
            height: 8,
            depth_or_array_layers: 1,
        };
        let make = || {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("fog regression fixture"),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
        };
        for case in 0..4 {
            let mut inputs = vec![vec![[0f32; 4]; 256]; 5];
            for y in 0..8 {
                for x in 0..32 {
                    let i = y * 32 + x;
                    let bright = if (x + y) % 2 == 0 { 2.0 } else { 4.0 };
                    inputs[0][i] = [bright, bright, bright, 0.7];
                    let fog = if case == 0 {
                        if (x + y) % 2 == 0 { 0.2 } else { 0.8 }
                    } else if x < 16 {
                        0.25
                    } else {
                        2.0
                    };
                    inputs[1][i] = [fog, fog, fog, if case == 3 && x >= 16 { 0.0 } else { 1.0 }];
                    inputs[2][i] = [0.5, 0.25, 0.75, 1.0];
                    inputs[3][i] = [if case == 1 && x >= 16 { 0.38 } else { 0.4 }, 0., 0., 0.];
                    inputs[4][i] = if case == 2 && x >= 16 {
                        [1., 0., 0., 1.]
                    } else {
                        [0., 0., 1., 1.]
                    };
                }
            }
            let textures: Vec<_> = inputs
                .iter()
                .map(|data| {
                    let t = make();
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &t,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        bytemuck::cast_slice(data),
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(512),
                            rows_per_image: Some(8),
                        },
                        extent,
                    );
                    t
                })
                .collect();
            let views: Vec<_> = textures
                .iter()
                .map(|t| t.create_view(&Default::default()))
                .collect();
            let group = bind(&device, &pipeline, &views.iter().collect::<Vec<_>>());
            let output = make();
            let view = output.create_view(&Default::default());
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 4096,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            run_pass(&mut encoder, &view, &pipeline, &group, 3);
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
                        bytes_per_row: Some(512),
                        rows_per_image: Some(8),
                    },
                },
                extent,
            );
            queue.submit([encoder.finish()]);
            let (tx, rx) = std::sync::mpsc::channel();
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            rx.recv().unwrap().unwrap();
            let mapped = buffer.slice(..).get_mapped_range().unwrap();
            let pixels: &[[f32; 4]] = bytemuck::cast_slice(&mapped);
            let mut residual = 0.0f32;
            for y in 2..6 {
                for x in 2..30 {
                    let i = y * 32 + x;
                    for c in 0..3 {
                        let base = inputs[0][i][c];
                        let t = inputs[2][i][c];
                        assert!(pixels[i][c].is_finite());
                        if case == 0 {
                            residual += (pixels[i][c] - base * t - 0.5).powi(2);
                        } else {
                            let expected = if case == 3 && x >= 16 {
                                base
                            } else {
                                base * t + inputs[1][i][c]
                            };
                            assert!(
                                (pixels[i][c] - expected).abs() < 0.0001,
                                "case {case}, pixel {x},{y}, channel {c}: {} != {expected}",
                                pixels[i][c]
                            );
                        }
                    }
                    assert!((pixels[i][3] - 0.7).abs() < 0.0001);
                }
            }
            if case == 0 {
                assert!(
                    residual / (4. * 28. * 3.) < 0.002,
                    "fog noise was not suppressed"
                );
            }
            assert!(pixels.iter().any(|p| p[0] > 1.0), "HDR clipped");
        }
    }
}

use super::Raster;
#[test]
#[ignore = "requires GPU; run explicitly with --ignored"]
fn gpu_raster_blend_viewport_preserves_hdr_and_untouched_pixels() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let properties = horizon_shaders::Properties::parse(
        "blend.composite=ONE ONE ZERO ONE\nblend.composite.colortex7=off\nscale.composite=0.5 0.25 0.5",
        &horizon_shaders::Macros::standard(),
    );
    let raster = Raster::new(&properties, "composite", &[7, 2]);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(
            r#"
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = array<vec2<f32>, 3>(vec2(-1., -1.), vec2(3., -1.), vec2(-1., 3.));
    return vec4(p[i], 0., 1.);
}
struct Outputs { @location(0) a: vec4<f32>, @location(1) b: vec4<f32> }
@fragment fn fs() -> Outputs {
    return Outputs(vec4(4., 4., 4., 0.5), vec4(4., 4., 4., 0.5));
}
"#
            .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &raster.targets(),
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let textures: [_; 2] = std::array::from_fn(|_| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    });
    let views = textures
        .each_ref()
        .map(|t| t.create_view(&Default::default()));
    let readbacks: [_; 2] = std::array::from_fn(|_| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 1024,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    for clear in [true, false] {
        let attachments = views.each_ref().map(|view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: if clear {
                        wgpu::LoadOp::Clear(wgpu::Color {
                            r: 2.,
                            g: 2.,
                            b: 2.,
                            a: 1.,
                        })
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                },
            })
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &attachments,
            ..Default::default()
        });
        if !clear {
            let [x, y, w, h] = raster.viewport([4, 4]);
            pass.set_pipeline(&pipeline);
            pass.set_viewport(x, y, w, h, 0., 1.);
            pass.draw(0..3, 0..1);
        }
    }
    for (texture, buffer) in textures.iter().zip(&readbacks) {
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(4),
                },
            },
            wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
        );
    }
    queue.submit([encoder.finish()]);
    for (index, buffer) in readbacks.iter().enumerate() {
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let data = buffer.slice(..).get_mapped_range().unwrap();
        for y in 0..4 {
            for x in 0..4 {
                let inside = (1..3).contains(&x) && y >= 2;
                let expected_rgb: u16 = if !inside {
                    0x4000
                } else if index == 0 {
                    0x4400
                } else {
                    0x4600
                }; // half: 2, 4, 6
                let expected_alpha: u16 = if inside && index == 0 { 0x3800 } else { 0x3c00 }; // .5, 1
                for c in 0..4 {
                    let offset = y * 256 + x * 8 + c * 2;
                    assert_eq!(
                        u16::from_le_bytes([data[offset], data[offset + 1]]),
                        if c == 3 { expected_alpha } else { expected_rgb },
                        "target {index}, pixel {x},{y}, channel {c}"
                    );
                }
            }
        }
    }
}

//! GPU focus for the station's Solas DOF adapter; not a general shaderpack compute executor.
pub struct Focus {
    pipeline: wgpu::ComputePipeline,
    state: wgpu::Buffer,
    params: wgpu::Buffer,
    previous: Option<([u32; 2], [f32; 3], std::time::Instant)>,
}
fn reset_needed(
    size: [u32; 2],
    player: [f32; 3],
    old_size: [u32; 2],
    old_player: [f32; 3],
) -> bool {
    size != old_size
        || player[2].floor() != old_player[2].floor()
        || (player[0] - old_player[0]).hypot(player[1] - old_player[1]) > 16.
}
impl Focus {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Solas autofocus"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
@group(0) @binding(0) var depth: texture_2d<f32>;
@group(0) @binding(1) var<storage,read_write> focus: array<f32>;
@group(0) @binding(2) var<uniform> params: vec4f; // alpha, deck fallback, reset, unused
@compute @workgroup_size(1) fn main() {
    let size=textureDimensions(depth);
    let center=textureLoad(depth,vec2i(size/2u),0).r;
    let previous=select(focus[0],params.y,params.z>0.5);
    // Looking into empty background keeps focus. First frame/reset uses deck fallback.
    let focus_target=select(previous,center,center>=0.6 && center<0.99999);
    focus[0]=select(mix(previous,focus_target,params.x),focus_target,params.z>0.5);
}
"#
                .into(),
            ),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Solas autofocus"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            pipeline,
            state: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Solas focus history"),
                size: 4,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            params: device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            previous: None,
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        depth: &wgpu::TextureView,
        size: [u32; 2],
        player: [f32; 3],
        fallback: f32,
    ) {
        let now = std::time::Instant::now();
        let (reset, alpha) = self
            .previous
            .map(|(old_size, old_player, time)| {
                (
                    reset_needed(size, player, old_size, old_player),
                    1. - (-now.duration_since(time).as_secs_f32() / 0.15).exp(),
                )
            })
            .unwrap_or((true, 1.));
        self.previous = Some((size, player, now));
        self.dispatch(
            device,
            queue,
            encoder,
            depth,
            [alpha, fallback, f32::from(reset), 0.],
        );
    }
    fn dispatch(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        depth: &wgpu::TextureView,
        params: [f32; 4],
    ) {
        queue.write_buffer(&self.params, 0, bytemuck::cast_slice(&params));
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.state.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.params.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    pub fn copy_to(&self, encoder: &mut wgpu::CommandEncoder, uniform: &wgpu::Buffer, offset: u64) {
        encoder.copy_buffer_to_buffer(&self.state, 0, uniform, offset, 4);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resets_on_resize_deck_change_and_teleport_but_not_normal_motion() {
        assert!(!reset_needed(
            [100, 100],
            [1., 1., 0.],
            [100, 100],
            [0., 0., 0.]
        ));
        assert!(reset_needed([101, 100], [0.; 3], [100, 100], [0.; 3]));
        assert!(reset_needed([100, 100], [0., 0., 1.], [100, 100], [0.; 3]));
        assert!(reset_needed([100, 100], [17., 0., 0.], [100, 100], [0.; 3]));
    }
    #[test]
    #[ignore = "requires GPU; run explicitly with --ignored"]
    fn gpu_solas_focus_tracks_smooths_holds_and_resets() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let focus = Focus::new(&device);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 3,
                height: 3,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 16,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        for (depth, alpha, reset, expected) in [
            (0.8, 0.25, true, 0.8),
            (0.6, 0.5, false, 0.7),
            (1., 0.5, false, 0.7),
            (0.9, 0.1, true, 0.9),
            (1., 0.5, true, 0.76),
        ] {
            let mut pixels = [0.61f32; 9];
            pixels[4] = depth; // only the center controls focus
            queue.write_texture(
                texture.as_image_copy(),
                bytemuck::cast_slice(&pixels),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(12),
                    rows_per_image: Some(3),
                },
                texture.size(),
            );
            let mut encoder = device.create_command_encoder(&Default::default());
            focus.dispatch(
                &device,
                &queue,
                &mut encoder,
                &view,
                [alpha, 0.76, f32::from(reset), 0.],
            );
            // Copy into an offset, exactly as centerDepthSmooth inside a packed uniform block.
            focus.copy_to(&mut encoder, &readback, 8);
            queue.submit([encoder.finish()]);
            let (tx, rx) = std::sync::mpsc::channel();
            readback
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            rx.recv().unwrap().unwrap();
            let mapped = readback.slice(..).get_mapped_range().unwrap();
            let value = f32::from_le_bytes(mapped[8..12].try_into().unwrap());
            assert!((value - expected).abs() < 0.00001, "{value} != {expected}");
            assert_eq!(&mapped[..8], &[0; 8]);
            assert_eq!(&mapped[12..16], &[0; 4]);
            drop(mapped);
            readback.unmap();
        }
    }
}

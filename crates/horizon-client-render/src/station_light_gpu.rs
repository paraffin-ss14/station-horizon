//! GPU light-map generation. CPU grid traversal remains authoritative for entities.
use super::{Light, station_light::Occlusion};
use wgpu::util::DeviceExt;
pub const SIZE: u32 = 512;
pub struct GpuLight {
    pipeline: wgpu::ComputePipeline,
}
impl GpuLight {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("station GPU lighting"),
            source: wgpu::ShaderSource::Wgsl(include_str!("station_light_gpu.wgsl").into()),
        });
        Self {
            pipeline: device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("station light field"),
                layout: None,
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            }),
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        occlusion: &Occlusion,
        observer: [f32; 3],
        origin: [f32; 2],
        span: f32,
        lights: &[Light],
        restrict: bool,
    ) {
        // Include the full radius of lights which can affect the visible field.
        let margin = lights.iter().map(|l| l.radius).fold(1f32, f32::max).ceil() as i32 + 2;
        let base = origin.map(|v| v.floor() as i32 - margin);
        let width = (span.ceil() as i32 + margin * 2 + 2).clamp(1, 2048);
        let grid = occlusion.grid(base, width as usize);
        let mut lamps: Vec<[f32; 8]> = lights
            .iter()
            .filter(|l| super::station_light::deck(l.position[2]) == occlusion.deck)
            .filter(|l| {
                (0..2).all(|i| {
                    l.position[i] + l.radius >= origin[i]
                        && l.position[i] - l.radius <= origin[i] + span
                })
            })
            .map(|l| {
                [
                    l.position[0],
                    l.position[1],
                    l.radius,
                    l.energy,
                    l.color[0],
                    l.color[1],
                    l.color[2],
                    0.,
                ]
            })
            .collect();
        let count = lamps.len();
        if lamps.is_empty() {
            lamps.push([0.; 8]);
        }
        let mut params = Vec::new();
        params.extend_from_slice(bytemuck::cast_slice(&[
            origin[0],
            origin[1],
            span,
            0.,
            observer[0],
            observer[1],
            if restrict { 1. } else { 0. },
            count as f32,
        ]));
        params.extend_from_slice(bytemuck::cast_slice(&[base[0], base[1], width, width]));
        let buffer = |name, bytes: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(name),
                contents: bytes,
                usage,
            })
        };
        let params = buffer("light params", &params, wgpu::BufferUsages::UNIFORM);
        let walls = buffer(
            "light occupancy",
            bytemuck::cast_slice(&grid),
            wgpu::BufferUsages::STORAGE,
        );
        let lamps = buffer(
            "light sources",
            bytemuck::cast_slice(&lamps),
            wgpu::BufferUsages::STORAGE,
        );
        let view = texture.create_view(&Default::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("light compute inputs"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: walls.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: lamps.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("station lights"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(SIZE.div_ceil(8), SIZE.div_ceil(8), 1);
        }
        queue.submit([encoder.finish()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gpu_light_matches_cpu_visibility_and_attenuation() {
        pollster::block_on(async {
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance
                .request_adapter(&Default::default())
                .await
                .expect("GPU adapter for light parity");
            let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
            let gpu = GpuLight::new(&device);
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("lighting parity"),
                size: wgpu::Extent3d {
                    width: SIZE,
                    height: SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let cells: Vec<_> = (-8..8)
                .filter(|y| *y != 1)
                .map(|y| [0, y, 1])
                .chain([[1, 2, 1], [2, 2, 1], [-2, 3, 1]])
                .collect();
            let overrides =
                [(0, false, false), (2, true, false), (3, false, true)].map(|(y, sight, light)| {
                    super::super::station_barrier::Barrier {
                        cell: [0, y, 1],
                        blocks_movement: true,
                        blocks_sight: sight,
                        blocks_light: light,
                    }
                });
            let occlusion = Occlusion::with_barriers(&cells, 0, &overrides);
            let lights = vec![
                Light {
                    position: [-3., 0.5, 1.3],
                    radius: 6.,
                    energy: 4.,
                    color: [0.4, 0.7, 1.],
                },
                Light {
                    position: [3., 2., 1.3],
                    radius: 4.,
                    energy: 3.,
                    color: [1., 0.4, 0.2],
                },
            ];
            for (observer, restrict) in [
                ([-3., 0.5, 1.3], true),
                ([-1., 1., 1.3], true),
                ([3., 1.5, 1.3], false),
            ] {
                gpu.update(
                    &device,
                    &queue,
                    &texture,
                    &occlusion,
                    observer,
                    [-8., -8.],
                    16.,
                    &lights,
                    restrict,
                );
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: (SIZE * SIZE * 16) as u64,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                let mut encoder = device.create_command_encoder(&Default::default());
                encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyBufferInfo {
                        buffer: &buffer,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(SIZE * 16),
                            rows_per_image: Some(SIZE),
                        },
                    },
                    wgpu::Extent3d {
                        width: SIZE,
                        height: SIZE,
                        depth_or_array_layers: 1,
                    },
                );
                queue.submit([encoder.finish()]);
                let (tx, rx) = std::sync::mpsc::channel();
                buffer
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                rx.recv().unwrap().unwrap();
                let mapped = buffer.slice(..).get_mapped_range().unwrap();
                let values: &[[f32; 4]] = bytemuck::cast_slice(&mapped);
                for y in (0..SIZE).step_by(17) {
                    for x in (0..SIZE).step_by(17) {
                        let p = [
                            -8. + (x as f32 + 0.5) * 16. / SIZE as f32,
                            -8. + (y as f32 + 0.5) * 16. / SIZE as f32,
                        ];
                        let expected = occlusion.illumination(p, &lights);
                        let sight = if restrict {
                            let quarter = 16. / SIZE as f32 * 0.25;
                            [-1., 1.]
                                .into_iter()
                                .flat_map(|y| {
                                    [-1., 1.].map(move |x| [p[0] + x * quarter, p[1] + y * quarter])
                                })
                                .filter(|q| {
                                    occlusion.surface_visible([observer[0], observer[1]], *q)
                                })
                                .count() as f32
                                / 4.
                        } else {
                            1.
                        };
                        let actual = values[(y * SIZE + x) as usize];
                        assert_eq!(actual[3], sight, "independent sight mask at {p:?}");
                        for c in 0..3 {
                            assert!(
                                (actual[c] - expected[c]).abs() < 0.0001,
                                "{p:?}: {actual:?} != {expected:?}"
                            );
                        }
                    }
                }
                drop(mapped);
                buffer.unmap();
            }
        });
    }
}

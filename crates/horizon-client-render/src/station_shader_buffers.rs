//! Shared A/B colortex storage. Frame carry semantics follow gfx/pack_post.rs:
//! non-cleared B is copied to A, preserving B for partial writes; frames start on A.
use horizon_shaders::{BUFFERS, Buffer};
#[derive(Clone, Debug)]
pub struct Route {
    pub writes: Vec<u32>,
    flip: u16,
}
impl Route {
    pub fn new(writes: Vec<u32>, overrides: &[(u32, bool)]) -> Result<Self, String> {
        let mut mask = 0u16;
        for &b in &writes {
            if b >= BUFFERS as u32 || mask & (1 << b) != 0 {
                return Err("invalid or duplicate colortex output".into());
            }
            mask |= 1 << b;
        }
        for &(b, on) in overrides {
            if b >= BUFFERS as u32 {
                return Err("invalid colortex flip".into());
            }
            if on {
                mask |= 1 << b;
            } else {
                mask &= !(1 << b);
            }
        }
        Ok(Self { writes, flip: mask })
    }
    pub fn flipped(&self) -> u16 {
        self.flip
    }
    pub fn used(&self) -> u16 {
        self.writes.iter().fold(self.flip, |m, b| m | (1 << b))
    }
}
struct Pair {
    textures: [wgpu::Texture; 2],
    views: [wgpu::TextureView; 2],
}
pub struct Buffers {
    config: [Buffer; BUFFERS],
    used: u16,
    pairs: Vec<Option<Pair>>,
    main: u16,
    size: [u32; 2],
    scene_copy: Option<wgpu::RenderPipeline>,
    zeroes: Option<wgpu::Buffer>,
}
impl Buffers {
    pub fn new(config: [Buffer; BUFFERS], used: u16) -> Self {
        Self {
            config,
            used,
            pairs: vec![],
            main: 0,
            size: [0; 2],
            scene_copy: None,
            zeroes: None,
        }
    }
    pub fn begin_frame(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
    ) {
        let resized = self.size != size;
        if resized {
            self.zeroes = None;
            self.pairs = (0..BUFFERS)
                .map(|b| {
                    if self.used & (1 << b) == 0 {
                        None
                    } else {
                        let textures = std::array::from_fn(|_| {
                            device.create_texture(&wgpu::TextureDescriptor {
                                label: Some("station colortex A/B"),
                                size: wgpu::Extent3d {
                                    width: size[0],
                                    height: size[1],
                                    depth_or_array_layers: 1,
                                },
                                mip_level_count: 1,
                                sample_count: 1,
                                dimension: wgpu::TextureDimension::D2,
                                format: wgpu::TextureFormat::Rgba16Float,
                                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                                    | wgpu::TextureUsages::TEXTURE_BINDING
                                    | wgpu::TextureUsages::COPY_SRC
                                    | wgpu::TextureUsages::COPY_DST,
                                view_formats: &[],
                            })
                        });
                        let views =
                            std::array::from_fn(|i| textures[i].create_view(&Default::default()));
                        Some(Pair { textures, views })
                    }
                })
                .collect();
            self.size = size;
        }
        for (b, pair) in self.pairs.iter().enumerate() {
            let Some(pair) = pair else { continue };
            if resized || self.config[b].clear {
                let rgba =
                    self.config[b]
                        .clear_color
                        .unwrap_or(if b == 1 { [1.; 4] } else { [0.; 4] });
                let clear = wgpu::Color {
                    r: rgba[0] as f64,
                    g: rgba[1] as f64,
                    b: rgba[2] as f64,
                    a: rgba[3] as f64,
                };
                for view in &pair.views {
                    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("clear colortex pair"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view,
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(clear),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        ..Default::default()
                    });
                }
            } else if self.main & (1 << b) != 0 {
                encoder.copy_texture_to_texture(
                    pair.textures[1].as_image_copy(),
                    pair.textures[0].as_image_copy(),
                    pair.textures[0].size(),
                );
            }
        }
        self.main = 0;
    }
    /// Seed current colortex0 from the native scene after begin_frame. Both use GL rows.
    /// Alternate remains cleared/history-backed for partial writes and explicit pre-flips.
    pub fn seed_scene(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::TextureView,
    ) {
        if self.scene_copy.is_none() {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("station scene to colortex0"),
                source: wgpu::ShaderSource::Wgsl(SCENE_COPY.into()),
            });
            self.scene_copy = Some(super::station_shader_pack::render_pipeline(
                device,
                None,
                &shader,
                &shader,
                "vs",
                "fs",
                wgpu::TextureFormat::Rgba16Float,
            ));
        }
        let pipeline = self.scene_copy.as_ref().unwrap();
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("native scene input"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(source),
            }],
        });
        super::station_shader_pack::run_pass(encoder, self.read(0), pipeline, &group, 3);
    }
    /// Invalidate both sides; a later explicit flip cannot resurrect obsolete history.
    pub fn clear_history(&self, encoder: &mut wgpu::CommandEncoder, buffer: u32) {
        let pair = self.pairs[buffer as usize]
            .as_ref()
            .expect("allocated history");
        for view in &pair.views {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("reset shader history"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
    }
    /// Zero a region on both history sides without discarding unaffected accumulation.
    pub fn clear_history_rect(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        buffer: u32,
        rect: [u32; 4],
    ) {
        let [x, y, width, height] = rect;
        assert!(width > 0 && height > 0 && x + width <= self.size[0] && y + height <= self.size[1]);
        let stride = (self.size[0] * 8).div_ceil(256) * 256;
        let zeroes = self.zeroes.get_or_insert_with(|| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("reactive history zeroes"),
                size: stride as u64 * self.size[1] as u64,
                usage: wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            })
        });
        for texture in &self.pairs[buffer as usize]
            .as_ref()
            .expect("allocated history")
            .textures
        {
            encoder.copy_buffer_to_texture(
                wgpu::TexelCopyBufferInfo {
                    buffer: zeroes,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(stride),
                        rows_per_image: None,
                    },
                },
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x, y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    pub fn read(&self, b: u32) -> &wgpu::TextureView {
        &self.pairs[b as usize]
            .as_ref()
            .expect("allocated colortex")
            .views[usize::from(self.main & (1 << b) != 0)]
    }
    pub fn write(&self, b: u32) -> &wgpu::TextureView {
        &self.pairs[b as usize]
            .as_ref()
            .expect("allocated colortex")
            .views[usize::from(self.main & (1 << b) == 0)]
    }
    pub fn finish(&mut self, route: &Route) {
        self.main ^= route.flip;
    }
}
const SCENE_COPY: &str = r#"
@group(0) @binding(0) var scene: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i:u32) -> @builtin(position) vec4<f32> {
    let p = array<vec2<f32>,3>(vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.));
    return vec4(p[i],0.,1.);
}
@fragment fn fs(@builtin(position) p:vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(scene,vec2<i32>(p.xy),0);
}
"#;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires GPU; run explicitly with --ignored"]
    fn gpu_colortex_scene_seed_write_flip_and_frame_refresh() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let mut bank = Buffers::new(std::array::from_fn(|_| Buffer::default()), 3);
        for size in [[2, 2], [2, 2], [1, 1]] {
            let mut encoder = device.create_command_encoder(&Default::default());
            bank.begin_frame(&device, &mut encoder, size);
            paint(&mut encoder, bank.read(1), 4.);
            let source = bank.read(1).clone();
            bank.seed_scene(&device, &mut encoder, &source);
            queue.submit([encoder.finish()]);
            assert_eq!(
                read(&device, &queue, &bank, 0, false),
                0x4400,
                "native HDR scene refresh"
            );
            assert_eq!(
                read(&device, &queue, &bank, 0, true),
                0,
                "seed must not overwrite alternate"
            );
            let mut encoder = device.create_command_encoder(&Default::default());
            paint(&mut encoder, bank.write(0), 8.);
            queue.submit([encoder.finish()]);
            bank.finish(&Route::new(vec![0], &[(0, false)]).unwrap());
            assert_eq!(read(&device, &queue, &bank, 0, false), 0x4400);
            bank.finish(&Route::new(vec![], &[(0, true)]).unwrap());
            assert_eq!(
                read(&device, &queue, &bank, 0, false),
                0x4800,
                "forced flip sees new HDR result"
            );
        }
    }
    #[test]
    #[ignore = "requires GPU; run explicitly with --ignored"]
    fn gpu_colortex_history_clear_flip_and_resize() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let mut config: [Buffer; BUFFERS] = std::array::from_fn(|_| Buffer::default());
        config[1].clear_color = Some([0.25; 4]);
        config[2].clear = false;
        let mut bank = Buffers::new(config, (1 << 1) | (1 << 2));
        let route = Route::new(vec![2], &[]).unwrap();
        let mut encoder = device.create_command_encoder(&Default::default());
        bank.begin_frame(&device, &mut encoder, [2, 2]);
        paint(&mut encoder, bank.write(2), 4.);
        bank.finish(&route);
        paint(&mut encoder, bank.write(1), 2.);
        bank.finish(&Route::new(vec![1], &[]).unwrap());
        queue.submit([encoder.finish()]);
        assert_eq!(read(&device, &queue, &bank, 2, false), 0x4400);
        let mut encoder = device.create_command_encoder(&Default::default());
        bank.begin_frame(&device, &mut encoder, [2, 2]);
        queue.submit([encoder.finish()]);
        assert_eq!(
            read(&device, &queue, &bank, 2, false),
            0x4400,
            "history must move B to A"
        );
        assert_eq!(
            read(&device, &queue, &bank, 2, true),
            0x4400,
            "old B contents must survive partial writes"
        );
        assert_eq!(
            read(&device, &queue, &bank, 1, false),
            0x3400,
            "cleared main"
        );
        assert_eq!(
            read(&device, &queue, &bank, 1, true),
            0x3400,
            "cleared alternate"
        );
        let mut encoder = device.create_command_encoder(&Default::default());
        paint(&mut encoder, bank.write(2), 8.);
        bank.finish(&Route::new(vec![2], &[(2, false)]).unwrap());
        queue.submit([encoder.finish()]);
        assert_eq!(
            read(&device, &queue, &bank, 2, false),
            0x4400,
            "explicit false must retain main"
        );
        bank.finish(&Route::new(vec![], &[(2, true)]).unwrap());
        assert_eq!(
            read(&device, &queue, &bank, 2, false),
            0x4800,
            "forced flip without output"
        );
        let mut partial = device.create_command_encoder(&Default::default());
        bank.clear_history_rect(&device, &mut partial, 2, [1, 1, 1, 1]);
        queue.submit([partial.finish()]);
        for alt in [false, true] {
            assert_eq!(read_at(&device, &queue, &bank, 2, alt, [1, 1]), 0);
            assert_ne!(
                read(&device, &queue, &bank, 2, alt),
                0,
                "unaffected HDR history survives"
            );
        }
        let mut reset = device.create_command_encoder(&Default::default());
        bank.clear_history(&mut reset, 2);
        queue.submit([reset.finish()]);
        assert_eq!(read(&device, &queue, &bank, 2, false), 0);
        assert_eq!(read(&device, &queue, &bank, 2, true), 0);

        let mut encoder = device.create_command_encoder(&Default::default());
        bank.begin_frame(&device, &mut encoder, [1, 1]);
        queue.submit([encoder.finish()]);
        assert_eq!(
            read(&device, &queue, &bank, 2, false),
            0,
            "resize discards history"
        );
    }
    fn paint(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, value: f64) {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: value,
                        g: value,
                        b: value,
                        a: 1.,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
    }
    fn read(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bank: &Buffers,
        b: usize,
        alt: bool,
    ) -> u16 {
        read_at(device, queue, bank, b, alt, [0, 0])
    }
    fn read_at(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bank: &Buffers,
        b: usize,
        alt: bool,
        xy: [u32; 2],
    ) -> u16 {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 256,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        let side = usize::from(bank.main & (1 << b) != 0) ^ usize::from(alt);
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                origin: wgpu::Origin3d {
                    x: xy[0],
                    y: xy[1],
                    z: 0,
                },
                ..bank.pairs[b].as_ref().unwrap().textures[side].as_image_copy()
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(1),
                },
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
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
        u16::from_le_bytes([mapped[0], mapped[1]])
    }
    #[test]
    fn explicit_flips_override_writes_and_can_flip_other_buffers() {
        let r = Route::new(vec![1, 2], &[(1, false), (7, true)]).unwrap();
        assert_eq!(r.flip, (1 << 2) | (1 << 7));
        assert_eq!(r.used(), (1 << 1) | (1 << 2) | (1 << 7));
        let mut main = 0;
        main ^= r.flip;
        main ^= r.flip;
        assert_eq!(main, 0);
    }
    #[test]
    fn rejects_duplicate_and_invalid_outputs() {
        assert!(Route::new(vec![1, 1], &[]).is_err());
        assert!(Route::new(vec![16], &[]).is_err());
        assert!(Route::new(vec![], &[(16, true)]).is_err());
    }
}

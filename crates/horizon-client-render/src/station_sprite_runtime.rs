#[allow(dead_code)] // JSON bridge exports are exercised by rust-anything, not the game.
#[path = "../../../station/ss14-playback-rules/src/port.rs"]
mod playback_rules;
// Per-entity SpriteComponent playback and geometry. SS14 semantics, station schema.
use super::{
    SpriteLayer,
    sprite_types::SpriteStyle,
    station_sprite::{Atlas, Rect},
};
use std::collections::{BTreeMap, BTreeSet};
pub type SpriteId = [u64; 4];
#[derive(Clone, Debug, PartialEq)]
pub enum AnimationEvent {
    Frame {
        sprite: SpriteId,
        layer: String,
        frame: usize,
    },
    Finished {
        sprite: SpriteId,
        layer: String,
    },
}
#[derive(Clone, Debug)]
struct Playback {
    source: (String, String, u64),
    frame: usize,
    left: f64,
    reversed: bool,
    requested_reverse: bool,
    stopped: bool,
    last: f64,
    manual: f32,
}
#[derive(Default)]
pub struct SpriteRuntime {
    tracks: BTreeMap<(SpriteId, String), Playback>,
    pub events: Vec<AnimationEvent>,
    clock: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    pub uv: Rect,
    pub corners: [[f32; 2]; 4],
    pub color: [f32; 4],
    pub depth: i16,
    pub unshaded: bool,
    pub mask: Option<Rect>,
    pub mirror: [bool; 2],
}
impl Sample {
    pub fn pixel(&self, point: [f32; 2], atlas: &Atlas) -> Option<[f32; 4]> {
        let a = self.corners[0];
        let u = [self.corners[1][0] - a[0], self.corners[1][1] - a[1]];
        let v = [self.corners[3][0] - a[0], self.corners[3][1] - a[1]];
        let d = u[0] * v[1] - u[1] * v[0];
        if d.abs() < 0.000001 {
            return None;
        }
        let p = [point[0] - a[0], point[1] - a[1]];
        let mut uv = [
            (p[0] * v[1] - p[1] * v[0]) / d,
            1. - (u[0] * p[1] - u[1] * p[0]) / d,
        ];
        if uv.iter().any(|v| !(0.0..=1.0).contains(v)) {
            return None;
        }
        for (i, v) in uv.iter_mut().enumerate() {
            if self.mirror[i] {
                *v = 1. - *v;
            }
        }
        let pixel = |r: Rect| {
            let size = super::station_sprite::ATLAS_SIZE;
            let x = ((r.min[0] + uv[0] * (r.max[0] - r.min[0])) * size as f32).floor() as usize;
            let y = ((r.min[1] + uv[1] * (r.max[1] - r.min[1])) * size as f32).floor() as usize;
            let max_x = (r.max[0] * size as f32) as usize - 1;
            let max_y = (r.max[1] * size as f32) as usize - 1;
            let index = (y.min(max_y) * size + x.min(max_x)) * 4;
            std::array::from_fn::<_, 4, _>(|i| atlas.pixels[index + i] as f32 / 255.)
        };
        let rgba = pixel(self.uv);
        let mut rgba = std::array::from_fn(|i| rgba[i] * self.color[i]);
        rgba[3] *= self.mask.map_or(1., |r| pixel(r)[3]);
        Some(rgba)
    }
    pub fn hit(&self, point: [f32; 2], atlas: &Atlas) -> bool {
        self.pixel(point, atlas).is_some_and(|p| p[3] > 0.01)
    }
}
impl SpriteRuntime {
    pub fn begin(&mut self, time: f64, alive: &BTreeSet<SpriteId>) {
        self.events.clear();
        if time < self.clock {
            self.tracks.clear();
        }
        self.clock = time;
        self.tracks.retain(|(id, _), _| alive.contains(id));
    }
    pub fn frame(&self, id: SpriteId, key: &str) -> Option<usize> {
        self.tracks.get(&(id, key.into())).map(|t| t.frame)
    }
    pub fn finished(&self, id: SpriteId, key: &str) -> bool {
        self.tracks
            .get(&(id, key.into()))
            .is_some_and(|t| t.stopped)
    }
    pub fn restart(&mut self, id: SpriteId, key: &str) {
        self.tracks.remove(&(id, key.into()));
    }
    pub fn copy_animation(&mut self, from: SpriteId, to: SpriteId) {
        let copied: Vec<_> = self
            .tracks
            .iter()
            .filter(|((id, _), _)| *id == from)
            .map(|((_, key), v)| ((to, key.clone()), v.clone()))
            .collect();
        self.tracks.extend(copied);
    }
    pub fn reset(&mut self) {
        self.tracks.clear();
        self.events.clear();
    }
    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }
    #[allow(clippy::too_many_arguments)] // Explicit rendering inputs; no hidden global component state.
    pub fn sample(
        &mut self,
        id: SpriteId,
        layers: &[SpriteLayer],
        style: &SpriteStyle,
        size: [f32; 2],
        facing: [f32; 2],
        visible: bool,
        atlas: &Atlas,
    ) -> Vec<Sample> {
        let mut result = Vec::new();
        let mut frames = BTreeMap::new();
        let time = self.clock;
        for (index, l) in layers.iter().enumerate() {
            let key = if l.key.is_empty() {
                format!("#{index}")
            } else {
                l.key.clone()
            };
            let source = (l.rsi.clone(), l.state.clone(), l.restart);
            let delays = atlas.delays(&l.rsi, &l.state).unwrap_or(&[1.]);
            let start = if l.reversed { delays.len() - 1 } else { 0 };
            let t = self
                .tracks
                .entry((id, key.clone()))
                .or_insert_with(|| Playback {
                    source: source.clone(),
                    frame: start,
                    left: delays[start],
                    reversed: l.reversed,
                    requested_reverse: l.reversed,
                    stopped: false,
                    last: time,
                    manual: -1.,
                });
            if t.source != source || time < t.last {
                *t = Playback {
                    source,
                    frame: start,
                    left: delays[start],
                    reversed: l.reversed,
                    requested_reverse: l.reversed,
                    stopped: false,
                    last: time,
                    manual: -1.,
                };
            }
            if t.requested_reverse != l.reversed {
                t.reversed = l.reversed;
                t.requested_reverse = l.reversed;
            }
            let previous = t.frame;
            let reset_manual = t.manual != l.animation_time;
            let dt = (time - t.last).max(0.) * l.playback_speed as f64;
            t.last = time;
            let looping = l.looping && style.looping;
            if reset_manual
                || (l.synchronized && l.auto_animated && l.visible && style.visible && visible)
            {
                t.frame = start;
                t.left = delays[start];
                t.reversed = l.reversed;
                t.stopped = false;
                let seek = if l.synchronized {
                    time * l.playback_speed as f64 + l.animation_time as f64
                } else {
                    l.animation_time as f64
                };
                advance(t, delays, seek, looping, l.cycle);
                t.manual = l.animation_time;
            } else if l.auto_animated && l.visible && style.visible && visible && !t.stopped {
                let ended = advance(t, delays, dt, looping, l.cycle);
                if ended {
                    self.events.push(AnimationEvent::Finished {
                        sprite: id,
                        layer: key.clone(),
                    });
                }
            }
            if previous != t.frame {
                self.events.push(AnimationEvent::Frame {
                    sprite: id,
                    layer: key.clone(),
                    frame: t.frame,
                });
            }
            let mut dir = l.direction.or(style.direction).unwrap_or(facing);
            let angle = l.direction_offset as f32 * std::f32::consts::FRAC_PI_4;
            dir = rotate(dir, angle);
            let blank = l.rsi.is_empty() && l.state.is_empty();
            let uv = if blank {
                let size = super::station_sprite::ATLAS_SIZE as f32;
                Rect {
                    min: [32. / size, 0.],
                    max: [33. / size, 1. / size],
                }
            } else {
                atlas.frame_at(&l.rsi, &l.state, dir, t.frame)
            };
            frames.insert(key, uv);
            for alias in &l.aliases {
                frames.insert(alias.clone(), uv);
            }
            if !blank && l.visible && style.visible {
                result.push((
                    l,
                    Sample {
                        uv,
                        corners: corners(
                            l,
                            style,
                            {
                                let native = atlas.world_size(&l.rsi, &l.state);
                                [native[0] * size[0], native[1] * size[1]]
                            },
                            facing,
                        ),
                        color: std::array::from_fn(|i| l.color[i] * style.color[i]),
                        depth: l.depth.saturating_add(style.draw_depth),
                        unshaded: l.unshaded,
                        mask: None,
                        mirror: l.mirror,
                    },
                ));
            }
        }
        let keys: BTreeSet<_> = layers
            .iter()
            .enumerate()
            .map(|(i, l)| {
                if l.key.is_empty() {
                    format!("#{i}")
                } else {
                    l.key.clone()
                }
            })
            .collect();
        self.tracks
            .retain(|(owner, key), _| *owner != id || keys.contains(key));
        for (layer, sample) in &mut result {
            sample.mask = layer.mask.as_ref().and_then(|key| frames.get(key).copied());
        }
        result.sort_by_key(|(_, s)| s.depth);
        result.into_iter().map(|(_, s)| s).collect()
    }
}
// AdvanceFrameAnimation's strict '< 0' boundary and non-repeated ping-pong ends.
fn advance(t: &mut Playback, delays: &[f64], dt: f64, looping: bool, cycle: bool) -> bool {
    if delays.len() == 1 {
        return false;
    }
    let period = if cycle {
        delays.iter().sum::<f64>() * 2. - delays[0] - delays[delays.len() - 1]
    } else {
        delays.iter().sum()
    };
    let dt = if looping && dt > period {
        dt % period
    } else {
        dt
    };
    let delays: Vec<f32> = delays.iter().map(|d| *d as f32).collect();
    let mut cursor = playback_rules::Cursor {
        frame: t.frame as i32,
        left: t.left as f32,
        time: 0.,
        reversed: t.reversed,
        animated: !t.stopped,
    };
    cursor.step(&delays, dt as f32, looping, cycle);
    t.frame = cursor.frame as usize;
    t.left = cursor.left as f64;
    t.reversed = cursor.reversed;
    let ended = !cursor.animated && !t.stopped;
    t.stopped = !cursor.animated;
    ended
}

fn rotate(p: [f32; 2], a: f32) -> [f32; 2] {
    let (s, c) = a.sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}
pub fn corners(
    layer: &SpriteLayer,
    style: &SpriteStyle,
    size: [f32; 2],
    facing: [f32; 2],
) -> [[f32; 2]; 4] {
    let angle = (style.rotation
        + if style.no_rotation {
            0.
        } else {
            facing[1].atan2(facing[0]).to_degrees() + 90.
        })
    .to_radians();
    [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]].map(|p| {
        let local = rotate(
            [
                p[0] * size[0] * layer.scale[0],
                p[1] * size[1] * layer.scale[1],
            ],
            layer.rotation.to_radians(),
        );
        let p = rotate(
            [
                (local[0] + layer.offset[0]) * style.scale[0],
                (local[1] + layer.offset[1]) * style.scale[1],
            ],
            angle,
        );
        [p[0] + style.offset[0], p[1] + style.offset[1]]
    })
}
pub fn bounds(corners: &[[f32; 2]; 4]) -> ([f32; 2], [f32; 2]) {
    (
        std::array::from_fn(|i| corners.iter().map(|p| p[i]).fold(f32::INFINITY, f32::min)),
        std::array::from_fn(|i| {
            corners
                .iter()
                .map(|p| p[i])
                .fold(f32::NEG_INFINITY, f32::max)
        }),
    )
}
pub fn contains(corners: &[[f32; 2]; 4], point: [f32; 2]) -> bool {
    let mut sign = 0.;
    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        let cross = (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0]);
        if cross.abs() > 0.00001 {
            if sign != 0. && cross * sign < 0. {
                return false;
            }
            sign = cross;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn layer() -> SpriteLayer {
        serde_json::from_str(r#"{"rsi":"crate","state":"sparking","key":"effect"}"#).unwrap()
    }
    fn sample(
        runtime: &mut SpriteRuntime,
        atlas: &Atlas,
        id: SpriteId,
        l: &SpriteLayer,
        t: f64,
    ) -> Sample {
        runtime.begin(t, &BTreeSet::from([[0, 1, 0, 0], [0, 2, 0, 0]]));
        runtime
            .sample(
                id,
                std::slice::from_ref(l),
                &SpriteStyle::default(),
                [1., 1.],
                [0., -1.],
                true,
                atlas,
            )
            .remove(0)
    }
    #[test]
    fn independent_animation_pause_seek_restart_and_cleanup() {
        let atlas = Atlas::bundled().unwrap();
        let mut rt = SpriteRuntime::default();
        let mut l = layer();
        let a = [0, 1, 0, 0];
        let b = [0, 2, 0, 0];
        let first = sample(&mut rt, &atlas, a, &l, 0.).uv;
        let next = sample(&mut rt, &atlas, a, &l, 0.15).uv;
        assert_ne!(first, next);
        assert_eq!(sample(&mut rt, &atlas, b, &l, 0.15).uv, first);
        l.auto_animated = false;
        assert_eq!(sample(&mut rt, &atlas, a, &l, 0.3).uv, next);
        l.animation_time = 0.31;
        assert_ne!(sample(&mut rt, &atlas, a, &l, 0.4).uv, next);
        l.restart += 1;
        l.animation_time = 0.;
        assert_eq!(sample(&mut rt, &atlas, a, &l, 0.5).uv, first);
        rt.begin(0.6, &BTreeSet::new());
        assert_eq!(rt.track_count(), 0);
    }
    #[test]
    fn one_shot_cycle_reverse_hidden_layers_and_sync() {
        let atlas = Atlas::bundled().unwrap();
        let mut rt = SpriteRuntime::default();
        let mut l = layer();
        let id = [0, 1, 0, 0];
        l.looping = false;
        sample(&mut rt, &atlas, id, &l, 0.);
        let last = sample(&mut rt, &atlas, id, &l, 5.).uv;
        assert!(
            rt.events
                .iter()
                .any(|e| matches!(e, AnimationEvent::Finished { .. }))
        );
        assert_eq!(sample(&mut rt, &atlas, id, &l, 10.).uv, last);
        assert!(rt.events.is_empty());
        let mut playback = Playback {
            source: (String::new(), String::new(), 0),
            frame: 0,
            left: 0.1,
            reversed: false,
            requested_reverse: false,
            stopped: false,
            last: 0.,
            manual: 0.,
        };
        advance(&mut playback, &[0.1, 0.1, 0.1], 0.31, true, true);
        assert_eq!(playback.frame, 1);
        assert!(playback.reversed);
        advance(&mut playback, &[0.1, 0.1, 0.1], 0.11, true, true);
        assert_eq!(playback.frame, 0);
        l.reversed = true;
        l.restart = 1;
        assert_eq!(sample(&mut rt, &atlas, id, &l, 11.).uv, last);
        l.synchronized = true;
        l.looping = true;
        l.reversed = false;
        l.restart = 2;
        let x = sample(&mut rt, &atlas, id, &l, 12.15).uv;
        assert_eq!(sample(&mut rt, &atlas, [0, 2, 0, 0], &l, 12.15).uv, x);
    }
    #[test]
    fn named_layer_reorder_and_hide_preserve_playback() {
        let atlas = Atlas::bundled().unwrap();
        let mut rt = SpriteRuntime::default();
        let id = [0, 1, 0, 0];
        let a = layer();
        let mut b = a.clone();
        b.key = "second".into();
        b.auto_animated = false;
        let style = SpriteStyle::default();
        let alive = BTreeSet::from([id]);
        rt.begin(0., &alive);
        rt.sample(
            id,
            &[a.clone(), b.clone()],
            &style,
            [1., 1.],
            [0., -1.],
            true,
            &atlas,
        );
        rt.begin(0.15, &alive);
        let forward = rt.sample(
            id,
            &[a.clone(), b.clone()],
            &style,
            [1., 1.],
            [0., -1.],
            true,
            &atlas,
        );
        let swapped = rt.sample(
            id,
            &[b.clone(), a.clone()],
            &style,
            [1., 1.],
            [0., -1.],
            true,
            &atlas,
        );
        assert_eq!(forward[0].uv, swapped[1].uv);
        assert_eq!(forward[1].uv, swapped[0].uv);
        rt.begin(10., &alive);
        rt.sample(
            id,
            &[a.clone(), b],
            &style,
            [1., 1.],
            [0., -1.],
            false,
            &atlas,
        );
        let visible = rt.sample(id, &[a], &style, [1., 1.], [0., -1.], true, &atlas);
        assert_eq!(visible[0].uv, forward[0].uv);
        assert_eq!(rt.track_count(), 1);
    }
    #[test]
    fn transforms_masks_and_layer_order_are_resolved() {
        let atlas = Atlas::bundled().unwrap();
        let mut rt = SpriteRuntime::default();
        let mut l = layer();
        let mut mask = l.clone();
        mask.key = "mask".into();
        mask.visible = false;
        l.mask = Some("mask".into());
        l.rotation = 90.;
        l.scale = [2., 1.];
        l.mirror = [true, false];
        l.depth = 2;
        let style = SpriteStyle {
            offset: [2., 3.],
            ..Default::default()
        };
        let samples = rt.sample(
            [0; 4],
            &[l, mask],
            &style,
            [1., 1.],
            [0., -1.],
            true,
            &atlas,
        );
        assert_eq!(samples.len(), 1);
        assert!(samples[0].mask.is_some());
        assert!(samples[0].mirror[0]);
        let (min, max) = bounds(&samples[0].corners);
        assert!((min[0] - 1.5).abs() < 0.001 && (max[1] - 4.).abs() < 0.001);
        assert!(contains(&samples[0].corners, [2., 3.]));
        assert!(!contains(&samples[0].corners, [5., 5.]));
    }
}

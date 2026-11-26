//! Presentation only: authoritative snapshots remain untouched for interactions.
use station_server::{Frame, Phase, motion_rules};
use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
    time::Instant,
};
#[derive(Clone)]
struct Sample {
    tick: u64,
    peers: Vec<(u64, [f32; 3])>,
    poses: Vec<(u64, [f32; 2], bool)>,
}
#[derive(Default)]
pub struct Motion {
    radius: f32,
    clock: f64,
    epoch: Option<u64>,
    history: VecDeque<Sample>,
    position: Option<[f32; 3]>,
    last: Option<Instant>,
    cells: Option<Arc<Vec<[i32; 3]>>>,
    solid: HashSet<[i32; 3]>,
}
pub struct View {
    pub time: f64,
    pub poses: Vec<(u64, [f32; 2], bool)>,
    pub player: [f32; 3],
    pub peers: Vec<(u64, [f32; 3])>,
}
impl Motion {
    pub fn update(&mut self, f: &Frame, movement: [f32; 2], now: Instant) -> View {
        self.radius = f.character.radius;
        let poses: Vec<_> = f
            .appearances
            .iter()
            .map(|a| (a.id, a.facing, a.moving))
            .collect();
        let epoch = f.round.map(|r| r.epoch);
        let active =
            f.connected && f.local_id != 0 && f.round.is_some_and(|r| r.phase == Phase::Playing);
        if self.epoch != epoch || !active {
            self.clock = f.player.tick as f64 / 60.;
            self.history.clear();
            self.position = Some(f.player.position);
            self.last = Some(now);
            self.epoch = epoch;
        }
        if !active {
            return View {
                time: f.player.tick as f64 / 60.,
                poses,
                player: f.player.position,
                peers: f.peers.clone(),
            };
        }
        if self
            .cells
            .as_ref()
            .is_none_or(|c| !Arc::ptr_eq(c, &f.cells))
        {
            self.solid = f
                .cells
                .iter()
                .copied()
                .filter(|p| {
                    f.barriers
                        .iter()
                        .find(|b| b.cell == *p)
                        .is_none_or(|b| b.blocks_movement)
                })
                .collect();
            self.cells = Some(f.cells.clone());
        }
        let dt = self
            .last
            .replace(now)
            .map_or(0., |t| now.saturating_duration_since(t).as_secs_f32())
            .min(0.05);
        let age = f
            .received_at
            .map_or(0., |t| now.saturating_duration_since(t).as_secs_f32());
        let fresh = self.history.back().is_none_or(|s| s.tick != f.player.tick);
        if fresh {
            if self.history.back().is_some_and(|s| s.tick > f.player.tick) {
                self.clock = f.player.tick as f64 / 60.;
                self.history.clear();
                self.position = Some(f.player.position);
            }
            self.history.push_back(Sample {
                poses: poses.clone(),
                tick: f.player.tick,
                peers: f.peers.clone(),
            });
            while self.history.len() > 64 {
                self.history.pop_front();
            }
        }
        let lag = motion_rules::lag_ticks(60., (f.rtt_ms.max(0.) as f64 / 2000.).min(0.2), 1.)
            .unwrap() as f32
            / 60.;
        let delay_ticks = (lag * 60.).clamp(3., 12.);
        let target_tick = f.player.tick as f64 + (age.min(0.25) * 60.) as f64 - delay_ticks as f64;
        self.clock = self
            .clock
            .max(f.player.tick as f64 / 60. + age.min(0.25) as f64);
        // Use the same delayed timeline for remote orientation and position.
        let poses = poses
            .into_iter()
            .map(|(id, facing, moving)| {
                if id == f.local_id {
                    let norm = movement[0].hypot(movement[1]);
                    return (
                        id,
                        if norm > 0.01 {
                            movement.map(|v| v / norm)
                        } else {
                            facing
                        },
                        moving,
                    );
                }
                self.history
                    .iter()
                    .rev()
                    .filter(|s| s.tick as f64 <= target_tick)
                    .find_map(|s| s.poses.iter().find(|p| p.0 == id).copied())
                    .or_else(|| {
                        self.history
                            .front()
                            .and_then(|s| s.poses.iter().find(|p| p.0 == id).copied())
                    })
                    .unwrap_or((id, facing, moving))
            })
            .collect();
        let peers = f
            .peers
            .iter()
            .map(|&(id, current)| {
                let mut before = None;
                let mut after = None;
                for sample in &self.history {
                    if let Some((_, p)) = sample.peers.iter().find(|(uid, _)| *uid == id) {
                        if sample.tick as f64 <= target_tick {
                            before = Some((sample.tick, *p));
                        } else {
                            after = Some((sample.tick, *p));
                            break;
                        }
                    }
                }
                let p = match (before, after) {
                    (Some((a, p)), Some((b, q))) => {
                        interpolate(p, q, ((target_tick - a as f64) / (b - a) as f64) as f32)
                    }
                    (Some((_, p)), None) | (None, Some((_, p))) => p,
                    _ => current,
                };
                (id, p)
            })
            .collect();
        let mut predicted = self.position.unwrap_or(f.player.position);
        if age > 0.25 {
            // Never continue dead reckoning through a stalled connection.
            return View {
                time: self.clock,
                poses,
                player: predicted,
                peers,
            };
        }
        let movement = movement.map(|v| if v.is_finite() { v.clamp(-1., 1.) } else { 0. });
        let norm = movement[0].hypot(movement[1]).max(1.);
        for axis in 0..2 {
            let mut candidate = predicted;
            candidate[axis] += movement[axis] / norm * f.character.speed * dt;
            if !self.blocked(candidate) {
                predicted = candidate;
            }
        }
        let mut target = f.player.position;
        for axis in 0..2 {
            let mut candidate = target;
            candidate[axis] += f.player.velocity[axis] * (lag + age).min(0.25);
            if !self.blocked(candidate) {
                target = candidate;
            }
        }
        let distance = (0..3)
            .map(|i| (predicted[i] - f.player.position[i]).powi(2))
            .sum::<f32>();
        if distance > 4. || (predicted[2] - f.player.position[2]).abs() > 0.5 {
            predicted = f.player.position;
        } else {
            let alpha = 1. - (-12. * dt).exp();
            let corrected =
                std::array::from_fn(|i| predicted[i] + (target[i] - predicted[i]) * alpha);
            if !self.blocked(corrected) {
                predicted = corrected;
            }
            predicted[2] = f.player.position[2]; // vertical physics stays authoritative
        }
        self.position = Some(predicted);
        View {
            time: self.clock,
            poses,
            player: predicted,
            peers,
        }
    }
    fn blocked(&self, p: [f32; 3]) -> bool {
        let z = p[2].floor() as i32;
        for x in (p[0] - self.radius).floor() as i32..=(p[0] + self.radius).floor() as i32 {
            for y in (p[1] - self.radius).floor() as i32..=(p[1] + self.radius).floor() as i32 {
                if self.solid.contains(&[x, y, z]) {
                    return true;
                }
            }
        }
        false
    }
}
fn interpolate(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let d = (0..3).map(|i| (b[i] - a[i]).powi(2)).sum::<f32>();
    if !motion_rules::can_interpolate(d as f64, true, true, true, false).unwrap() {
        return b;
    }
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t.clamp(0., 1.))
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    fn frame(now: Instant) -> Frame {
        let s = station_server::Session::start(7);
        let mut f = s.frame();
        f.local_id = 1;
        f.round = Some(station_server::RoundSnapshot {
            phase: Phase::Playing,
            epoch: 2,
            number: 1,
            ready: false,
            elapsed: 0,
            remaining: 600,
            world_changes: 0,
        });
        f.player.position = [0.5, 0.5, 1.3];
        f.player.velocity = [0.; 3];
        f.received_at = Some(now);
        f.rtt_ms = 200.;
        f.player.tick = 100;
        f
    }
    #[test]
    fn orientation_uses_delayed_snapshot_and_animation_clock_is_continuous() {
        let t = Instant::now();
        let mut f = frame(t);
        f.appearances = vec![station_sim::appearance::ActorAppearance {
            id: 2,
            facing: [1., 0.],
            moving: true,
            layers: vec![],
            style: Default::default(),
        }];
        f.peers = vec![(2, [0., 0., 1.3])];
        let mut m = Motion::default();
        let first = m.update(&f, [0.; 2], t);
        let next = m.update(&f, [0.; 2], t + Duration::from_millis(8));
        assert!(next.time > first.time);
        f.player.tick = 112;
        f.received_at = Some(t + Duration::from_millis(200));
        f.appearances[0].facing = [0., 1.];
        f.peers[0].1 = [1., 0., 1.3];
        let delayed = m.update(&f, [0.; 2], f.received_at.unwrap());
        assert_eq!(delayed.poses[0].1, [1., 0.]);
        assert!((delayed.peers[0].1[0] - 0.5).abs() < 0.01);
        let caught_up = m.update(&f, [0.; 2], t + Duration::from_millis(310));
        assert_eq!(caught_up.poses[0].1, [0., 1.]);
        let stalled = m.update(&f, [0.; 2], t + Duration::from_secs(1));
        let still_stalled = m.update(&f, [0.; 2], t + Duration::from_secs(2));
        assert_eq!(stalled.time, still_stalled.time);
    }
    #[test]
    fn local_input_responds_before_server_then_corrects_and_stops_at_wall() {
        let t = Instant::now();
        let mut f = frame(t);
        let mut m = Motion::default();
        m.update(&f, [0.; 2], t);
        let p = m.update(&f, [1., 0.], t + Duration::from_millis(16)).player;
        assert!(
            p[0] > 0.54,
            "input must respond without a new server frame: {p:?}"
        );
        for i in 2..=60 {
            f.received_at = Some(t + Duration::from_millis(i * 16));
            f.player.tick += 1;
            m.update(&f, [0.; 2], f.received_at.unwrap());
        }
        assert!((m.position.unwrap()[0] - 0.5).abs() < 0.01);
        let mut cells = f.cells.as_ref().clone();
        cells.push([1, 0, 1]);
        f.cells = Arc::new(cells);
        for i in 61..=120 {
            f.received_at = Some(t + Duration::from_millis(i * 16));
            f.player.tick += 1;
            m.update(&f, [1., 0.], f.received_at.unwrap());
        }
        assert!(m.position.unwrap()[0] <= 0.7);
    }
    #[test]
    fn peer_interpolation_teleport_and_round_reset() {
        assert_eq!(interpolate([0.; 3], [1., 0., 0.], 0.5), [0.5, 0., 0.]);
        assert_eq!(interpolate([0.; 3], [3., 0., 0.], 0.5), [3., 0., 0.]);
        let t = Instant::now();
        let mut f = frame(t);
        let mut m = Motion::default();
        f.peers = vec![(2, [0., 0., 1.3])];
        m.update(&f, [0.; 2], t);
        f.player.tick = 112;
        f.peers[0].1 = [1., 0., 1.3];
        f.received_at = Some(t + Duration::from_millis(200));
        let v = m.update(&f, [0.; 2], f.received_at.unwrap());
        assert!((v.peers[0].1[0] - 0.5).abs() < 0.01);
        f.round.as_mut().unwrap().epoch += 1;
        f.player.tick = 1;
        f.player.position = [5., 5., 1.3];
        f.peers.clear();
        let v = m.update(&f, [0.; 2], t + Duration::from_millis(216));
        assert_eq!(v.player, f.player.position);
        assert!(v.peers.is_empty());
    }
    #[test]
    fn delayed_and_jittered_snapshots_keep_motion_bounded() {
        for rtt in [100., 200.] {
            let t = Instant::now();
            let mut f = frame(t);
            f.rtt_ms = rtt;
            f.player.velocity = [4., 0., 0.];
            let mut m = Motion::default();
            let mut last = 0.5;
            m.update(&f, [1., 0.], t);
            for step in 1..=60 {
                let now = t + Duration::from_secs_f64(step as f64 / 60.);
                if step % 3 == 0 {
                    let server_time = (step as f32 / 60. - rtt / 2000.).max(0.);
                    f.player.tick += 3;
                    f.player.position[0] = 0.5 + 4. * server_time;
                    f.received_at = Some(now);
                }
                let view = m.update(&f, [1., 0.], now);
                assert!((view.player[0] - last).abs() < 0.15);
                last = view.player[0];
            }
            assert!((last - 4.5).abs() < 0.15, "RTT {rtt}: {last}");
            assert!(last > f.player.position[0] + 0.1);
        }
    }
    #[test]
    fn lost_connection_freezes_prediction_and_does_not_change_authority() {
        let t = Instant::now();
        let f = frame(t);
        let original = f.player.position;
        let mut m = Motion::default();
        m.update(&f, [0.; 2], t);
        let p = m.update(&f, [1., 0.], t + Duration::from_millis(20)).player;
        let stalled = m
            .update(&f, [1., 0.], t + Duration::from_millis(300))
            .player;
        assert_eq!(p, stalled);
        assert_eq!(f.player.position, original);
    }
}

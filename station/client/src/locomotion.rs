//! Distance-driven presentation of static RSI characters; no simulation changes.
use horizon_station_render::SpriteStyle;
use std::collections::HashMap;

#[derive(Default)]
pub struct Locomotion {
    epoch: Option<u64>,
    tracks: HashMap<u64, Track>,
}
struct Track {
    position: [f32; 3],
    time: f64,
    phase: f32,
    weight: f32,
}
impl Locomotion {
    pub fn retain(&mut self, epoch: u64, alive: &[u64]) {
        if self.epoch != Some(epoch) {
            self.tracks.clear();
            self.epoch = Some(epoch);
        }
        self.tracks.retain(|id, _| alive.contains(id));
    }
    pub fn apply(
        &mut self,
        id: u64,
        position: [f32; 3],
        moving: bool,
        time: f64,
        style: &mut SpriteStyle,
    ) {
        let t = self.tracks.entry(id).or_insert(Track {
            position,
            time,
            phase: 0.,
            weight: 0.,
        });
        let dt = (time - t.time).clamp(0., 0.05) as f32;
        let distance = (position[0] - t.position[0]).hypot(position[1] - t.position[1]);
        let teleport = distance > 0.5 || (position[2] - t.position[2]).abs() > 0.5 || time < t.time;
        // Do not walk while stationary against a wall, floating, falling or correcting a teleport.
        let ground = horizon_station_render::station_light::deck(position[2]) as f32 + 1.3;
        let walking =
            moving && distance > 0.0005 && !teleport && (position[2] - ground).abs() < 0.08;
        if teleport {
            t.phase = 0.;
            t.weight = 0.;
        } else {
            if walking {
                t.phase =
                    (t.phase + distance * std::f32::consts::TAU / 0.9) % std::f32::consts::TAU;
            }
            let target = if walking { 1. } else { 0. };
            t.weight += (target - t.weight) * (1. - (-24. * dt).exp());
            if !walking && t.weight < 0.001 {
                t.weight = 0.;
                t.phase = 0.;
            }
        }
        // A shared transform keeps body, clothing, masks and both hands attached.
        style.offset[1] += t.phase.sin().abs() * t.weight / 32.;
        style.rotation += t.phase.sin() * t.weight * 1.5;
        t.position = position;
        t.time = time;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn walking_settles_at_walls_and_resets_on_teleport_and_round() {
        let mut l = Locomotion::default();
        l.retain(1, &[7]);
        let mut s = SpriteStyle::default();
        l.apply(7, [0., 0., 1.3], true, 0., &mut s);
        for i in 1..10 {
            s = SpriteStyle::default();
            l.apply(7, [i as f32 * 0.04, 0., 1.3], true, i as f64 / 60., &mut s);
        }
        assert!(s.offset[1] > 0. && s.offset[1] <= 1. / 32.);
        for i in 10..60 {
            s = SpriteStyle::default();
            l.apply(7, [0.36, 0., 1.3], true, i as f64 / 60., &mut s);
        }
        assert_eq!(s.offset, [0.; 2]);
        l.apply(7, [20., 0., 1.3], true, 1., &mut s);
        assert_eq!(s.rotation, 0.);
        l.retain(2, &[7]);
        assert!(l.tracks.is_empty());
        l.apply(7, [20., 0., 2.3], true, 2., &mut s);
        l.apply(7, [20.1, 0., 2.3], true, 2.02, &mut s);
        assert_eq!(s.offset, [0.; 2]);
        l.retain(2, &[]);
        assert!(l.tracks.is_empty());
    }
}

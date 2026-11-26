//! Station-only top-down occlusion; no Minecraft assets or global render state.
use std::collections::BTreeSet;
#[path = "../../../station/ss14-light-rules/src/port.rs"]
mod rules;
pub const FIELD_SIZE: usize = 256;
#[derive(Clone, Debug, PartialEq)]
pub struct Light {
    pub position: [f32; 3],
    pub radius: f32,
    pub color: [f32; 3],
    pub energy: f32,
}
pub fn deck(z: f32) -> i32 {
    if z >= 0.5 { 0 } else { -4 }
}
#[derive(Default)]
pub struct Occlusion {
    walls: BTreeSet<[i32; 2]>,
    light_walls: BTreeSet<[i32; 2]>,
    pub deck: i32,
}
impl Occlusion {
    pub fn grid(&self, origin: [i32; 2], width: usize) -> Vec<u32> {
        let mut cells = vec![0; width * width];
        for (set, bit) in [(&self.walls, 1), (&self.light_walls, 2)] {
            for &[x, y] in set {
                let (x, y) = (x - origin[0], y - origin[1]);
                if x >= 0 && y >= 0 && x < width as i32 && y < width as i32 {
                    cells[y as usize * width + x as usize] |= bit;
                }
            }
        }
        cells
    }

    pub fn new(cells: &[[i32; 3]], deck: i32) -> Self {
        Self::with_barriers(cells, deck, &[])
    }
    pub fn with_barriers(
        cells: &[[i32; 3]],
        deck: i32,
        barriers: &[super::station_barrier::Barrier],
    ) -> Self {
        let overrides: std::collections::BTreeMap<_, _> =
            barriers.iter().map(|b| (b.cell, b)).collect();
        let mut out = Self {
            deck,
            ..Default::default()
        };
        for p in cells
            .iter()
            .filter(|p| p[2] == deck + 1 || p[2] == deck + 2)
        {
            let b = overrides.get(p);
            if b.is_none_or(|b| b.blocks_sight) {
                out.walls.insert([p[0], p[1]]);
            }
            if b.is_none_or(|b| b.blocks_light) {
                out.light_walls.insert([p[0], p[1]]);
            }
        }
        out
    }
    /// Supercover grid traversal. The first wall is visible, cells beyond it are not.
    /// At an exact corner both side cells must be empty (no diagonal peeking).
    pub fn visible(&self, from: [f32; 2], to: [f32; 2]) -> bool {
        self.visible_in(&self.walls, from, to)
    }
    fn visible_in(&self, walls: &BTreeSet<[i32; 2]>, from: [f32; 2], to: [f32; 2]) -> bool {
        if !from.into_iter().chain(to).all(f32::is_finite) {
            return false;
        }
        let mut c = from.map(|v| v.floor() as i32);
        let end = to.map(|v| v.floor() as i32);
        if c == end {
            return true;
        }
        if walls.contains(&c) {
            return false;
        }
        let delta = [to[0] - from[0], to[1] - from[1]];
        let step = delta.map(|d| {
            if d > 0. {
                1
            } else if d < 0. {
                -1
            } else {
                0
            }
        });
        let dt = delta.map(|d| if d == 0. { f32::INFINITY } else { 1. / d.abs() });
        let mut t: [f32; 2] = std::array::from_fn(|i| {
            if step[i] == 0 {
                f32::INFINITY
            } else {
                ((c[i] + i32::from(step[i] > 0)) as f32 - from[i]) / delta[i]
            }
        });
        for _ in 0..1024 {
            // Endpoint on a negative-facing grid boundary belongs to the cell on
            // its positive side. Do not step past the end and miss it forever.
            if t[0].min(t[1]) >= 1.0 - 0.000001 {
                return true;
            }
            if (t[0] - t[1]).abs() < 0.000001 {
                if walls.contains(&[c[0] + step[0], c[1]])
                    || walls.contains(&[c[0], c[1] + step[1]])
                {
                    return false;
                }
                for i in 0..2 {
                    c[i] += step[i];
                    t[i] += dt[i];
                }
            } else {
                let i = usize::from(t[1] < t[0]);
                c[i] += step[i];
                t[i] += dt[i];
            }
            if c == end {
                return true;
            }
            if walls.contains(&c) {
                return false;
            }
        }
        false
    }
    /// Lighting of a wall's top sprite is sampled at its exposed, source-facing
    /// boundary. Never trace through its volume to an arbitrary interior texel:
    /// that makes neighbouring cells cut triangular holes in a continuous wall.
    pub fn surface_visible(&self, from: [f32; 2], to: [f32; 2]) -> bool {
        self.surface_in(&self.walls, from, to)
    }
    pub fn light_visible(&self, from: [f32; 2], to: [f32; 2]) -> bool {
        self.surface_in(&self.light_walls, from, to)
    }
    fn surface_in(&self, walls: &BTreeSet<[i32; 2]>, from: [f32; 2], to: [f32; 2]) -> bool {
        if !from.into_iter().chain(to).all(f32::is_finite) {
            return false;
        }
        let cell = to.map(|v| v.floor() as i32);
        if !walls.contains(&cell) {
            return self.visible_in(walls, from, to);
        }
        for axis in 0..2 {
            let low = cell[axis] as f32;
            let side = if from[axis] < low {
                -1
            } else if from[axis] > low + 1. {
                1
            } else {
                continue;
            };
            let mut adjacent = cell;
            adjacent[axis] += side;
            if walls.contains(&adjacent) {
                continue;
            }
            let mut face = to;
            face[axis] = low + if side < 0 { -0.001 } else { 1.001 };
            if self.visible_in(walls, from, face) {
                return true;
            }
        }
        false
    }
    pub fn entity_visible(&self, observer: [f32; 3], target: [f32; 3]) -> bool {
        deck(observer[2]) == deck(target[2])
            && self.visible([observer[0], observer[1]], [target[0], target[1]])
    }
    pub fn illumination(&self, p: [f32; 2], lights: &[Light]) -> [f32; 3] {
        let mut rgb = [0.12; 3];
        for l in lights {
            if deck(l.position[2]) != self.deck {
                continue;
            }
            let delta = [p[0] - l.position[0], p[1] - l.position[1]];
            let d = delta[0] * delta[0] + delta[1] * delta[1];
            if d >= l.radius * l.radius || !self.light_visible([l.position[0], l.position[1]], p) {
                continue;
            }
            let power =
                rules::attenuation(d as f64, l.radius as f64).unwrap_or(0.) as f32 * l.energy;
            for (i, v) in rgb.iter_mut().enumerate() {
                *v += l.color[i] * power;
            }
        }
        rgb
    }
    pub fn field(
        &self,
        observer: [f32; 3],
        origin: [f32; 2],
        span: f32,
        lights: &[Light],
        restrict: bool,
    ) -> Vec<[f32; 4]> {
        let field: Vec<[f32; 4]> = (0..FIELD_SIZE * FIELD_SIZE)
            .map(|i| {
                let p = [
                    origin[0]
                        + (i % FIELD_SIZE) as f32 * span / FIELD_SIZE as f32
                        + span / FIELD_SIZE as f32 / 2.,
                    origin[1]
                        + (i / FIELD_SIZE) as f32 * span / FIELD_SIZE as f32
                        + span / FIELD_SIZE as f32 / 2.,
                ];
                if restrict && !self.surface_visible([observer[0], observer[1]], p) {
                    return [0., 0., 0., 1.];
                }
                let rgb = self.illumination(p, lights);
                [rgb[0], rgb[1], rgb[2], 1.]
            })
            .collect();
        // A compact tent filter softens sub-cell shadow/FOV edges. Entity
        // visibility still uses exact grid traversal, never this visual field.
        (0..field.len())
            .map(|i| {
                let x = (i % FIELD_SIZE) as isize;
                let y = (i / FIELD_SIZE) as isize;
                let mut value = [0.; 4];
                for dy in -1isize..=1 {
                    for dx in -1isize..=1 {
                        let nx = (x + dx).clamp(0, FIELD_SIZE as isize - 1) as usize;
                        let ny = (y + dy).clamp(0, FIELD_SIZE as isize - 1) as usize;
                        let weight =
                            (if dx == 0 { 2. } else { 1. }) * (if dy == 0 { 2. } else { 1. }) / 16.;
                        for (channel, result) in value.iter_mut().enumerate() {
                            *result += field[ny * FIELD_SIZE + nx][channel] * weight;
                        }
                    }
                }
                value
            })
            .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sight_and_light_flags_are_independent_and_removed_geometry_is_open() {
        use super::super::station_barrier::Barrier;
        for sight in [true, false] {
            for light in [true, false] {
                let b = Barrier {
                    cell: [0, 0, 1],
                    blocks_movement: true,
                    blocks_sight: sight,
                    blocks_light: light,
                };
                let o = Occlusion::with_barriers(&[[0, 0, 1]], 0, std::slice::from_ref(&b));
                assert_eq!(o.visible([-1., 0.5], [2., 0.5]), !sight);
                assert_eq!(o.light_visible([-1., 0.5], [2., 0.5]), !light);
                let removed = Occlusion::with_barriers(&[], 0, &[b]);
                assert!(
                    removed.visible([-1., 0.5], [2., 0.5])
                        && removed.light_visible([-1., 0.5], [2., 0.5])
                );
            }
        }
    }
    #[test]
    fn continuous_wall_surface_has_no_cell_wedges_or_light_through_wall() {
        let cells: Vec<_> = (-8..8).map(|x| [x, 0, 1]).chain([[-1, -2, 1]]).collect();
        let wall = Occlusion::new(&cells, 0);
        let source = [-5.5, 3.5];
        for x in -7..7 {
            for dx in [0.01, 0.49, 0.99] {
                for dy in [0.01, 0.49, 0.99] {
                    assert!(wall.surface_visible(source, [x as f32 + dx, dy]));
                }
            }
        }
        assert!(!wall.surface_visible(source, [-0.5, -0.5]));
        assert!(!wall.surface_visible(source, [-0.5, -1.5]));
        assert!(!wall.entity_visible([-5.5, 3.5, 1.3], [-0.5, -0.5, 1.3]));
    }
    #[test]
    fn filtered_field_preserves_ambient_and_keeps_far_side_hidden() {
        let open = Occlusion::new(&[], 0);
        let uniform = open.field([-2., 0.5, 1.3], [-4., -4.], 8., &[], true);
        assert!(uniform.iter().all(|v| (v[0] - 0.12).abs() < 0.00001));
        let wall: Vec<_> = (-8..8).map(|y| [0, y, 1]).collect();
        let closed = Occlusion::new(&wall, 0);
        let field = closed.field([-2., 0.5, 1.3], [-4., -4.], 8., &[], true);
        assert!(field.iter().any(|v| v[0] > 0.001 && v[0] < 0.119));
        assert_eq!(field[144 * FIELD_SIZE + 192][0], 0.);
        assert!(!closed.entity_visible([-2., 0.5, 1.3], [2., 0.5, 1.3]));
    }
    #[test]
    fn unobstructed_room_has_no_ray_holes_at_grid_boundaries() {
        let cells: Vec<_> = (-8..=8)
            .flat_map(|i| [[i, -8, 1], [i, 8, 1], [-8, i, 1], [8, i, 1]])
            .collect();
        let view = Occlusion::new(&cells, 0);
        for from in [[-5., 3.], [0.5, -2.5], [5., 3.]] {
            for x in -21..21 {
                for y in -21..21 {
                    let to = [x as f32 / 3., y as f32 / 3.];
                    assert!(view.visible(from, to), "{from:?} -> {to:?}");
                }
            }
        }
    }
    #[test]
    fn doors_walls_corners_and_decks() {
        let wall = Occlusion::new(&[[1, 0, 1], [1, 0, 2]], 0);
        assert!(wall.visible([0.5, 0.5], [1.5, 0.5]));
        assert!(!wall.visible([0.5, 0.5], [2.5, 0.5]));
        assert!(!wall.visible([0.5, 0.5], [1.5, 1.5]));
        assert!(!wall.entity_visible([0.5, 0.5, 1.3], [0.5, 0.5, -2.7]));
        assert!(Occlusion::new(&[], 0).visible([0.5, 0.5], [2.5, 0.5]));
        assert!(Occlusion::new(&[[1, 0, 1]], -4).visible([0.5, 0.5], [2.5, 0.5]));
        assert!(!Occlusion::new(&[[1, 0, -3]], -4).visible([0.5, 0.5], [2.5, 0.5]));
        assert!(!Occlusion::new(&[[-1, 0, 1]], 0).visible([-1.5, 0.5], [0.5, 0.5]));
    }
    #[test]
    fn light_is_shadowed_colored_bounded_and_preserves_hdr_energy() {
        let light = Light {
            position: [0.5, 0.5, 2.],
            radius: 8.,
            color: [1., 0., 0.],
            energy: 4.,
        };
        let wall = Occlusion::new(&[[1, 0, 1]], 0);
        let center = wall.illumination([0.5, 0.5], std::slice::from_ref(&light));
        assert!(center[0] > 1.);
        assert_eq!(center[1], 0.12);
        assert_eq!(
            wall.illumination([2.5, 0.5], std::slice::from_ref(&light)),
            [0.12; 3]
        );
        assert!(
            Occlusion::new(&[], 0).illumination([2.5, 0.5], std::slice::from_ref(&light))[0] > 0.12
        );
        assert_eq!(
            wall.illumination([-10., 0.5], std::slice::from_ref(&light)),
            [0.12; 3]
        );
        assert_eq!(
            Occlusion::new(&[], -4).illumination([0.5, 0.5], &[light]),
            [0.12; 3]
        );
    }
}

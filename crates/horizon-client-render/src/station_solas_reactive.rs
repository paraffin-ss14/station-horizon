//! Conservative history rejection for changed dynamic geometry, in pack texture coordinates.
use super::{Vertex, station_solas_camera::Camera};
use glam::{Vec3, Vec4};
#[derive(Default)]
pub struct Reactive {
    previous: Vec<Vertex>,
    bounds: Option<([f32; 3], [f32; 3])>,
}
impl Reactive {
    pub fn update(&mut self, vertices: &[Vertex]) {
        for i in (0..vertices.len().max(self.previous.len())).step_by(3) {
            let old = self
                .previous
                .get(i..(i + 3).min(self.previous.len()))
                .unwrap_or(&[]);
            let new = vertices.get(i..(i + 3).min(vertices.len())).unwrap_or(&[]);
            if bytemuck::cast_slice::<_, u8>(old) == bytemuck::cast_slice::<_, u8>(new) {
                continue;
            }
            // A change at one corner affects the entire interpolated triangle.
            for v in old.iter().chain(new) {
                let (min, max) = self.bounds.get_or_insert((v.p, v.p));
                for a in 0..3 {
                    min[a] = min[a].min(v.p[a]);
                    max[a] = max[a].max(v.p[a]);
                }
            }
        }
        self.previous.clear();
        self.previous.extend_from_slice(vertices);
    }
    pub fn take_rect(&mut self, camera: &Camera, size: [u32; 2]) -> Option<[u32; 4]> {
        let (min, max) = self.bounds.take()?;
        let mut low = [f32::INFINITY; 2];
        let mut high = [f32::NEG_INFINITY; 2];
        for x in [min[0], max[0]] {
            for y in [min[1], max[1]] {
                for z in [min[2], max[2]] {
                    let world = Vec3::new(x, z, -y);
                    let clip: Vec4 =
                        camera.projection * camera.view * (world - camera.position).extend(1.);
                    for a in 0..2 {
                        // Post buffers are flipped to OpenGL texture rows; positive Y goes up.
                        let pixel = (clip[a] / clip.w * 0.5 + 0.5) * size[a] as f32;
                        low[a] = low[a].min(pixel);
                        high[a] = high[a].max(pixel);
                    }
                }
            }
        }
        // Cover Catmull-Rom's history footprint, jitter and raster rounding.
        let lo: [u32; 2] =
            std::array::from_fn(|a| (low[a].floor() - 4.).clamp(0., size[a] as f32) as u32);
        let hi: [u32; 2] =
            std::array::from_fn(|a| (high[a].ceil() + 4.).clamp(0., size[a] as f32) as u32);
        (hi[0] > lo[0] && hi[1] > lo[1]).then_some([lo[0], lo[1], hi[0] - lo[0], hi[1] - lo[1]])
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn motion_appearance_removal_and_camera_space_damage() {
        fn covers(actual: Option<[u32; 4]>, expected: [u32; 4]) {
            let [x, y, w, h] = actual.expect("changed geometry needs rejection");
            let [l, b, r, t] = expected;
            assert!(x <= l && y <= b && x + w >= r && y + h >= t);
            assert!(l - x <= 5 && b - y <= 5 && x + w - r <= 5 && y + h - t <= 5);
        }
        let mut state = Reactive::default();
        let camera = Camera::new([180, 180], [0.; 3]);
        let mut vertices = Vec::new();
        super::super::quad(&mut vertices, [0., 0., 1.], [1., 1.], [1.; 4], false);
        state.update(&vertices);
        covers(state.take_rect(&camera, [180, 180]), [90, 90, 100, 100]);
        state.update(&vertices);
        assert!(state.take_rect(&camera, [180, 180]).is_none());
        for v in &mut vertices {
            v.p[0] += 2.;
        }
        state.update(&vertices);
        covers(state.take_rect(&camera, [180, 180]), [90, 90, 120, 100]);
        vertices[0].color[0] = 0.;
        state.update(&vertices);
        assert!(state.take_rect(&camera, [180, 180]).is_some());
        state.update(&[]);
        covers(
            state.take_rect(&Camera::new([180, 180], [2., 1., 0.]), [180, 180]),
            [90, 80, 100, 90],
        );
        state.update(&[]);
        assert!(state.take_rect(&camera, [180, 180]).is_none());
    }
}

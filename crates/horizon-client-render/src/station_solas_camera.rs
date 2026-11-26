//! Station camera contract for Solas. Native (x,y,height) -> pack (x,height,-y).
//! Model-view is camera-relative, as in Minecraft shaderpacks. Depth is native 0..1.
use glam::{Mat4, Vec3, Vec4};
#[derive(Clone)]
pub struct Camera {
    pub projection: Mat4,
    pub projection_inverse: Mat4,
    pub view: Mat4,
    pub view_inverse: Mat4,
    pub position: Vec3,
}
impl Camera {
    pub fn remap_taa_depth(&mut self) {
        // Reprojection tests GL clip z > .56, so scene depth must be above .78.
        let remap = Mat4::from_cols(Vec4::X, Vec4::Y, Vec4::Z * 0.2, Vec4::new(0., 0., 0.8, 1.));
        self.projection = remap * self.projection;
        self.projection_inverse = self.projection.inverse();
    }
    pub fn jitter(&mut self, offset: [f32; 2]) {
        self.projection.w_axis.x += offset[0];
        self.projection.w_axis.y += offset[1];
        self.projection_inverse = self.projection.inverse();
    }
    pub fn remap_dof_depth(&mut self) {
        // d'=.6+.4d implies GL z'=.6+.4z. Preserve xy and homogeneous w.
        let remap = Mat4::from_cols(Vec4::X, Vec4::Y, Vec4::Z * 0.4, Vec4::new(0., 0., 0.6, 1.));
        self.projection = remap * self.projection;
        self.projection_inverse = self.projection.inverse();
    }
    pub fn new(size: [u32; 2], player: [f32; 3]) -> Self {
        let height = super::DEPTH_BIAS / super::DEPTH_SCALE;
        let half_y = super::VIEW_HALF_HEIGHT;
        let half_x = half_y * size[0] as f32 / size[1] as f32;
        // GL clip z = 2 * native depth - 1; view z = native height - camera height.
        let projection = Mat4::from_cols(
            Vec4::new(1. / half_x, 0., 0., 0.),
            Vec4::new(0., 1. / half_y, 0., 0.),
            Vec4::new(0., 0., -2. * super::DEPTH_SCALE, 0.),
            Vec4::new(
                0.,
                0.,
                2. * super::DEPTH_BIAS - 1. - 2. * super::DEPTH_SCALE * height,
                1.,
            ),
        );
        let view = Mat4::from_cols(Vec4::X, Vec4::Z, -Vec4::Y, Vec4::W);
        Self {
            projection,
            projection_inverse: projection.inverse(),
            view,
            view_inverse: view.inverse(),
            // Native renderer tracks player XY; deck selection does not move its depth plane.
            position: Vec3::new(player[0], height, -player[1]),
        }
    }
}
/// Station-owned eight-frame Halton sequence, in clip coordinates.
pub fn jitter(frame: u32, size: [u32; 2]) -> [f32; 2] {
    fn radical(mut n: u32, base: u32) -> f32 {
        let mut value = 0.;
        let mut weight = 1.;
        while n > 0 {
            weight /= base as f32;
            value += (n % base) as f32 * weight;
            n /= base;
        }
        value
    }
    let n = frame % 8 + 1;
    [
        (radical(n, 2) - 0.5) * 2. / size[0] as f32,
        (radical(n, 3) - 0.5) * 2. / size[1] as f32,
    ]
}
#[derive(Default)]
pub struct History {
    previous: Option<(Camera, [u32; 2], i32)>,
}
impl History {
    pub fn valid_temporal(&self, current: &Camera, size: [u32; 2], deck: i32) -> bool {
        self.previous
            .as_ref()
            .is_some_and(|(old, old_size, old_deck)| {
                *old_size == size
                    && *old_deck == deck
                    && old.position.distance(current.position) <= 1.
            })
    }
    pub fn advance(&mut self, current: &Camera, size: [u32; 2], deck: i32) -> Camera {
        let previous = self
            .previous
            .as_ref()
            .filter(|(old, old_size, old_deck)| {
                *old_size == size
                    && *old_deck == deck
                    && old.position.distance(current.position) <= 16.
            })
            .map(|(old, _, _)| old.clone())
            .unwrap_or_else(|| current.clone());
        self.previous = Some((current.clone(), size, deck));
        previous
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn taa_history_rejects_fast_motion_but_retains_walk_speed() {
        let mut history = History::default();
        let start = Camera::new([800, 600], [0., 0., 3.]);
        assert!(!history.valid_temporal(&start, [800, 600], 0));
        history.advance(&start, [800, 600], 0);
        assert!(history.valid_temporal(&Camera::new([800, 600], [0.25, 0., 3.]), [800, 600], 0));
        assert!(!history.valid_temporal(&Camera::new([800, 600], [8., 0., 3.]), [800, 600], 0));
        assert!(!history.valid_temporal(&start, [801, 600], 0));
        assert!(!history.valid_temporal(&start, [800, 600], -4));
    }
    #[test]
    fn taa_jitter_and_depth_match_native_projection_and_enable_reprojection() {
        for frame in 0..16 {
            let size = [1200, 800];
            let offset = jitter(frame, size);
            assert_eq!(offset, jitter(frame + 8, size));
            assert!(
                offset[0].abs() <= 1. / size[0] as f32 && offset[1].abs() <= 1. / size[1] as f32
            );
            let mut camera = Camera::new(size, [0., 0., 3.]);
            let point = Vec4::new(2., 3., -40., 1.);
            let original = camera.projection * point;
            camera.remap_taa_depth();
            camera.jitter(offset);
            let clip = camera.projection * point;
            assert!((clip.x - original.x - offset[0]).abs() < 0.00001);
            assert!((clip.y - original.y - offset[1]).abs() < 0.00001);
            assert!(
                clip.z > 0.56,
                "Solas reprojection must include camera translation"
            );
            assert!((camera.projection_inverse * clip - point).length() < 0.0001);
        }
    }
    #[test]
    fn motion_history_tracks_and_resets_without_false_velocity() {
        let mut history = History::default();
        let first = Camera::new([800, 600], [0., 0., 3.]);
        assert_eq!(
            history.advance(&first, [800, 600], 0).position,
            first.position
        );
        let moved = Camera::new([800, 600], [2., 0., 3.]);
        let previous = history.advance(&moved, [800, 600], 0);
        assert_eq!(previous.position, first.position);
        let point = Vec4::new(0., 0., -40., 1.);
        let current_clip = moved.projection * point;
        let old_clip = previous.projection
            * previous.view
            * (moved.view_inverse * point + (moved.position - previous.position).extend(0.));
        assert!(
            old_clip.x > current_clip.x,
            "rightward camera motion must move world left"
        );
        assert_eq!(
            history.advance(&moved, [801, 600], 0).position,
            moved.position
        );
        assert_eq!(
            history.advance(&first, [801, 600], -4).position,
            first.position
        );
        let teleport = Camera::new([801, 600], [100., 0., 3.]);
        assert_eq!(
            history.advance(&teleport, [801, 600], -4).position,
            teleport.position
        );
    }
    #[test]
    fn dof_depth_remap_preserves_geometry_and_background() {
        let base = Camera::new([1280, 720], [3., -8., 0.]);
        let mut adapted = Camera::new([1280, 720], [3., -8., 0.]);
        adapted.remap_dof_depth();
        for z in [-5., -40., -80.] {
            let point = Vec4::new(2., 3., z, 1.);
            let clip = base.projection * point;
            let remapped = adapted.projection * point;
            let depth = clip.z * 0.5 + 0.5;
            assert!(
                (remapped.z * 0.5 + 0.5 - super::super::station_solas_depth::remap(depth)).abs()
                    < 0.00001
            );
            assert!((adapted.projection_inverse * remapped - point).length() < 0.0001);
            assert_eq!(clip.x, remapped.x);
            assert_eq!(clip.y, remapped.y);
        }
        assert_eq!(super::super::station_solas_depth::remap(1.), 1.);
        assert!(super::super::station_solas_depth::remap(0.) > 0.56);
    }
    #[test]
    fn reconstructs_native_depth_and_world_across_camera_moves_and_resize() {
        for size in [[1200, 800], [800, 1200], [1, 1]] {
            for player in [[0., 0., 0.], [12., -9., 3.]] {
                let camera = Camera::new(size, player);
                let native = Mat4::from_cols_array(&super::super::projection(
                    player,
                    size[0] as f32 / size[1] as f32,
                ));
                for point in [
                    Vec3::new(2., 3., 0.),
                    Vec3::new(11., -7., 2.),
                    Vec3::new(-5., 6., -4.),
                ] {
                    let world = Vec3::new(point.x, point.z, -point.y);
                    let clip =
                        camera.projection * camera.view * (world - camera.position).extend(1.);
                    let expected = native * point.extend(1.);
                    assert!((clip.x - expected.x).abs() < 0.0001);
                    assert!((clip.y - expected.y).abs() < 0.0001);
                    assert!((clip.z * 0.5 + 0.5 - expected.z).abs() < 0.0001);
                    let restored = (camera.view_inverse * camera.projection_inverse * clip)
                        .truncate()
                        + camera.position;
                    assert!((restored - world).length() < 0.0001);
                }
            }
        }
    }
    #[test]
    fn world_up_faces_the_top_down_camera() {
        let camera = Camera::new([800, 600], [4., 8., 2.]);
        assert_eq!(camera.view.transform_vector3(Vec3::Y), Vec3::Z);
        assert_eq!(camera.position, Vec3::new(4., 40., -8.));
    }
}

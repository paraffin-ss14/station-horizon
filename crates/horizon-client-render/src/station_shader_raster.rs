//! Raster state shared by station post passes. Uses Horizon property semantics.
#[cfg(test)]
use horizon_shaders::{Group, Properties};
pub struct Raster {
    scale: [f32; 3],
    blends: Vec<Option<wgpu::BlendState>>,
}
impl Raster {
    #[cfg(test)]
    pub fn new(properties: &Properties, name: &str, outputs: &[u32]) -> Self {
        let default = horizon_shaders::program_blend(properties, name).flatten();
        let overrides = horizon_shaders::program_buffer_blends(properties, name);
        Self {
            scale: horizon_shaders::program_scale(properties, name, Group::Composite),
            blends: outputs
                .iter()
                .map(|b| {
                    overrides
                        .iter()
                        .find(|(id, _)| id == b)
                        .map(|(_, blend)| *blend)
                        .unwrap_or(default)
                        .map(blend_state)
                })
                .collect(),
        }
    }
    pub fn from_post(p: &horizon_shaders::PostProgram) -> Self {
        Self {
            scale: p.scale,
            blends: p
                .buffers
                .iter()
                .map(|b| {
                    p.buffer_blends
                        .iter()
                        .find(|(id, _)| id == b)
                        .map(|(_, blend)| *blend)
                        .unwrap_or(p.blend)
                        .map(blend_state)
                })
                .collect(),
        }
    }
    pub fn targets(&self) -> Vec<Option<wgpu::ColorTargetState>> {
        self.blends
            .iter()
            .map(|&blend| {
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })
            })
            .collect()
    }
    pub fn viewport(&self, size: [u32; 2]) -> [f32; 4] {
        viewport(size, self.scale)
    }
}
fn blend_state(b: horizon_shaders::Blend) -> wgpu::BlendState {
    use horizon_shaders::Factor as F;
    use wgpu::BlendFactor as W;
    let f = |f| match f {
        F::Zero => W::Zero,
        F::One => W::One,
        F::SrcColor => W::Src,
        F::OneMinusSrcColor => W::OneMinusSrc,
        F::DstColor => W::Dst,
        F::OneMinusDstColor => W::OneMinusDst,
        F::SrcAlpha => W::SrcAlpha,
        F::OneMinusSrcAlpha => W::OneMinusSrcAlpha,
        F::DstAlpha => W::DstAlpha,
        F::OneMinusDstAlpha => W::OneMinusDstAlpha,
        F::SrcAlphaSaturate => W::SrcAlphaSaturated,
    };
    let component = |src, dst| wgpu::BlendComponent {
        src_factor: f(src),
        dst_factor: f(dst),
        operation: wgpu::BlendOperation::Add,
    };
    wgpu::BlendState {
        color: component(b[0], b[1]),
        alpha: component(b[2], b[3]),
    }
}

// Texture rows in the post chain are GL rows, matching gfx/pack_post.rs.
fn viewport(size: [u32; 2], [scale, x, y]: [f32; 3]) -> [f32; 4] {
    let [w, h] = size.map(|n| n as f32);
    let (x, y) = ((x * w).floor().min(w - 1.0), (y * h).floor().min(h - 1.0));
    [
        x,
        y,
        (scale * w).floor().clamp(1.0, w - x),
        (scale * h).floor().clamp(1.0, h - y),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overrides_follow_colortex_mapping_and_off_disables_default() {
        let p = Properties::parse(
            "blend.composite=SRC_ALPHA ONE_MINUS_SRC_ALPHA ONE ZERO\nblend.composite.colortex7=off\nblend.composite.colortex2=ONE ONE ZERO ONE",
            &horizon_shaders::Macros::standard(),
        );
        let r = Raster::new(&p, "composite", &[7, 2, 1]);
        assert_eq!(r.blends[0], None);
        assert_eq!(
            r.blends[1].unwrap().color.src_factor,
            wgpu::BlendFactor::One
        );
        assert_eq!(
            r.blends[1].unwrap().color.dst_factor,
            wgpu::BlendFactor::One
        );
        assert_eq!(
            r.blends[2].unwrap().color.src_factor,
            wgpu::BlendFactor::SrcAlpha
        );
    }
    #[test]
    fn viewport_rounds_clamps_and_preserves_gl_row_order() {
        assert_eq!(viewport([7, 3], [0.5, 0.25, 0.5]), [1., 1., 3., 1.]);
        assert_eq!(viewport([8, 8], [0.5, 0.75, 0.75]), [6., 6., 2., 2.]);
        assert_eq!(viewport([1, 1], [0.01, 0.9, 0.9]), [0., 0., 1., 1.]);
    }
}

#[cfg(test)]
#[path = "station_shader_raster_test.rs"]
mod gpu_tests;

//! Cached composed SpriteComponent thumbnails, sharing world geometry/playback/masks.
use horizon_station_render::{
    station_sprite::Atlas,
    station_sprite_runtime::{Sample, SpriteRuntime, bounds},
};
use std::collections::BTreeMap;
#[derive(Default)]
pub struct Icons {
    runtime: SpriteRuntime,
    cache: BTreeMap<usize, (Vec<Sample>, egui::TextureHandle)>,
    device_cache: BTreeMap<crate::object_menu::Target, (Vec<Sample>, egui::TextureHandle)>,
    portraits: BTreeMap<usize, (Vec<Sample>, egui::TextureHandle)>,
}
impl Icons {
    pub fn clear(&mut self) {
        self.cache.clear();
        self.device_cache.clear();
        self.portraits.clear();
        self.runtime.reset();
    }
    /// Stable thumbnail cache, using the same layers and masks as the world renderer.
    pub fn portraits(
        &mut self,
        ctx: &egui::Context,
        f: &station_server::Frame,
        atlas: &Atlas,
    ) -> Vec<egui::TextureId> {
        let mut runtime = SpriteRuntime::default();
        (0..4)
            .map(|index| {
                let mut layers = f.character.layers.clone();
                let mut style = f.character.style.clone();
                if index < 3 {
                    for layer in &mut layers {
                        if layer.state == "equipped-INNERCLOTHING" {
                            layer.rsi = ["engineer", "doctor", "assistant"][index].into();
                            if index != 2 {
                                layer.color = [1.; 4];
                            }
                        }
                    }
                } else if let Some(actor) = f.appearances.iter().find(|a| a.id == f.local_id) {
                    layers = actor.layers.clone();
                    style = actor.style.clone();
                }
                let samples = runtime.sample(
                    [2, index as u64, 0, 0],
                    &crate::sprites::layers(&layers),
                    &crate::sprites::style(&style),
                    f.character.sprite_size,
                    [0., -1.],
                    true,
                    atlas,
                );
                if self
                    .portraits
                    .get(&index)
                    .is_none_or(|(old, _)| *old != samples)
                {
                    let texture = ctx.load_texture(
                        format!("crew-portrait-{index}"),
                        raster(&samples, atlas),
                        egui::TextureOptions::NEAREST,
                    );
                    self.portraits.insert(index, (samples, texture));
                }
                self.portraits[&index].1.id()
            })
            .collect()
    }
    pub fn devices(
        &mut self,
        ctx: &egui::Context,
        f: &station_server::Frame,
        atlas: &Atlas,
    ) -> Vec<(crate::object_menu::Target, crate::sprites::Icon)> {
        use crate::object_menu::Target;
        let mut runtime = SpriteRuntime::default();
        let mut objects: Vec<_> = f
            .doors
            .iter()
            .map(|d| {
                (
                    Target::Door(d.id, d.entity),
                    crate::sprites::door(
                        [3, d.id as u64, 0, 0],
                        d.cell,
                        d.glass,
                        d.horizontal,
                        d.phase,
                        d.elapsed as f32 / 60.,
                    ),
                )
            })
            .collect();
        for item in crate::sprites::power_items(&f.power) {
            let source = item.id[1];
            let id = item.id[2] as usize;
            if source == 1 || source == 2 {
                let nodes = if source == 1 {
                    &f.power.sources
                } else {
                    &f.power.controllers
                };
                objects.push((Target::Power(source == 1, id, nodes[id].cell), item));
            }
        }
        self.device_cache
            .retain(|t, _| objects.iter().any(|(key, _)| key == t));
        objects
            .into_iter()
            .map(|(target, item)| {
                let samples = runtime.sample(
                    item.id,
                    &item.layers,
                    &item.style,
                    [1., 1.],
                    [0., -1.],
                    true,
                    atlas,
                );
                if self
                    .device_cache
                    .get(&target)
                    .is_none_or(|(old, _)| *old != samples)
                {
                    let texture = ctx.load_texture(
                        format!("object-{target:?}"),
                        raster(&samples, atlas),
                        egui::TextureOptions::NEAREST,
                    );
                    self.device_cache.insert(target, (samples, texture));
                }
                (
                    target,
                    (
                        self.device_cache[&target].1.id(),
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                        [1.; 4],
                    ),
                )
            })
            .collect()
    }
    pub fn update(
        &mut self,
        ctx: &egui::Context,
        f: &station_server::Frame,
        atlas: &Atlas,
    ) -> Vec<Option<crate::sprites::Icon>> {
        let alive = f
            .items
            .iter()
            .map(|i| {
                i.entity.map_or([1, 0, i.id as u64, 0], |e| {
                    [1, e.world, e.slot as u64, e.generation as u64]
                })
            })
            .collect();
        self.runtime.begin(f.player.tick as f64 / 60., &alive);
        self.cache.retain(|id, _| *id < f.items.len());
        f.items
            .iter()
            .map(|item| {
                if item.layers.is_empty() {
                    return None;
                }
                let id = item.entity.map_or([1, 0, item.id as u64, 0], |e| {
                    [1, e.world, e.slot as u64, e.generation as u64]
                });
                let samples = self.runtime.sample(
                    id,
                    &crate::sprites::layers(&item.layers),
                    &crate::sprites::style(&item.sprite_style),
                    item.sprite_size,
                    [0., -1.],
                    true,
                    atlas,
                );
                if self
                    .cache
                    .get(&item.id)
                    .is_none_or(|(old, _)| *old != samples)
                {
                    let image = raster(&samples, atlas);
                    let texture = ctx.load_texture(
                        format!("sprite-item-{}", item.id),
                        image,
                        egui::TextureOptions::NEAREST,
                    );
                    self.cache.insert(item.id, (samples, texture));
                }
                Some((
                    self.cache[&item.id].1.id(),
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                    [1.; 4],
                ))
            })
            .collect()
    }
}
fn raster(samples: &[Sample], atlas: &Atlas) -> egui::ColorImage {
    const SIZE: usize = 32;
    let mut image =
        egui::ColorImage::new([SIZE, SIZE], vec![egui::Color32::TRANSPARENT; SIZE * SIZE]);
    if samples.is_empty() {
        return image;
    }
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    for s in samples {
        let (a, b) = bounds(&s.corners);
        for i in 0..2 {
            min[i] = min[i].min(a[i]);
            max[i] = max[i].max(b[i]);
        }
    }
    let center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
    let span = (max[0] - min[0]).max(max[1] - min[1]).max(0.001) * 1.08;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let p = [
                center[0] + ((x as f32 + 0.5) / SIZE as f32 - 0.5) * span,
                center[1] + (0.5 - (y as f32 + 0.5) / SIZE as f32) * span,
            ];
            let mut color = [0.; 4];
            for s in samples {
                if let Some(c) = s.pixel(p, atlas) {
                    for i in 0..3 {
                        color[i] = c[i] * c[3] + color[i] * (1. - c[3]);
                    }
                    color[3] = c[3] + color[3] * (1. - c[3]);
                }
            }
            let b = color.map(|v| (v.clamp(0., 1.) * 255.) as u8);
            image[(x, y)] = egui::Color32::from_rgba_premultiplied(b[0], b[1], b[2], b[3]);
        }
    }
    image
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composed_icon_respects_hidden_layers_and_masks() {
        let atlas = Atlas::bundled().unwrap();
        let mut rt = SpriteRuntime::default();
        let c = station_sim::content::Content::builtin();
        let mut mask = crate::sprites::layers(&c.character.layers).remove(0);
        mask.key = "mask".into();
        mask.rsi = "human".into();
        mask.state = "head_m".into();
        mask.visible = false;
        let mut paint = mask.clone();
        paint.key = "paint".into();
        paint.rsi = "wall".into();
        paint.state = "full".into();
        paint.visible = true;
        paint.mask = Some("mask".into());
        let layers = vec![mask, paint];
        let samples = rt.sample(
            [0; 4],
            &layers,
            &Default::default(),
            [1., 1.],
            [0., -1.],
            true,
            &atlas,
        );
        let image = raster(&samples, &atlas);
        assert!(image.pixels.iter().any(|p| p.a() > 0));
        assert!(image.pixels.iter().filter(|p| p.a() == 0).count() > 512);
    }
}

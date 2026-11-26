use crate::ui::tr;
pub fn layers(
    source: &[station_sim::sprite::SpriteLayer],
) -> Vec<horizon_station_render::SpriteLayer> {
    source
        .iter()
        .map(|l| horizon_station_render::SpriteLayer {
            rsi: l.rsi.clone(),
            state: l.state.clone(),
            key: l.key.clone(),
            aliases: l.aliases.clone(),
            visible: l.visible,
            rotation: l.rotation,
            mirror: l.mirror,
            direction_offset: l.direction_offset,
            direction: l.direction,
            auto_animated: l.auto_animated,
            looping: l.looping,
            cycle: l.cycle,
            reversed: l.reversed,
            synchronized: l.synchronized,
            animation_time: l.animation_time,
            playback_speed: l.playback_speed,
            restart: l.restart,
            unshaded: l.unshaded,
            mask: l.mask.clone(),
            depth: l.depth,
            color: l.color,
            offset: l.offset,
            scale: l.scale,
        })
        .collect()
}
pub fn style(s: &station_sim::sprite::SpriteStyle) -> horizon_station_render::SpriteStyle {
    horizon_station_render::SpriteStyle {
        visible: s.visible,
        color: s.color,
        offset: s.offset,
        scale: s.scale,
        rotation: s.rotation,
        draw_depth: s.draw_depth,
        no_rotation: s.no_rotation,
        direction: s.direction,
        looping: s.looping,
    }
}
pub fn credits(ctx: &egui::Context, open: &mut bool) {
    egui::Window::new(tr("sprites.credits"))
        .open(open)
        .default_width(650.)
        .show(ctx, |ui| {
            ui.hyperlink_to(
                "CC BY-SA 3.0",
                "https://creativecommons.org/licenses/by-sa/3.0/",
            );
            egui::ScrollArea::vertical()
                .max_height(450.)
                .show(ui, |ui| {
                    ui.label(include_str!("../../assets/ss14/ATTRIBUTION.md"));
                    ui.label(include_str!("../../assets/ss14/FLOORS-ATTRIBUTION.md"));
                    ui.label(include_str!("../../assets/ss14/AIRLOCKS-ATTRIBUTION.md"));
                    ui.label(include_str!("../../assets/ss14/POWER-ATTRIBUTION.md"));
                });
        });
}

pub type Icon = (egui::TextureId, egui::Rect, [f32; 4]);
// Original station UI art, MIT. Deliberately independent of SS14's NC slot skin.
pub fn slot_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let mut image = egui::ColorImage::new(
        [32 * 18, 32],
        vec![egui::Color32::TRANSPARENT; 32 * 18 * 32],
    );
    for slot in 0..18 {
        let mut rect = |x: usize, y: usize, w: usize, h: usize| {
            for py in y..y + h {
                for px in x..x + w {
                    image[(slot * 32 + px, py)] = egui::Color32::from_rgb(123, 157, 173);
                }
            }
        };
        // Small silhouettes, drawn on an exact pixel grid.
        match slot {
            0 => {
                rect(9, 7, 14, 11);
                rect(6, 16, 20, 4);
            }
            1 => {
                rect(5, 12, 9, 8);
                rect(18, 12, 9, 8);
                rect(14, 14, 4, 2);
            }
            2 => {
                rect(8, 7, 16, 3);
                rect(6, 9, 4, 16);
                rect(22, 9, 4, 16);
            }
            3 => {
                rect(8, 12, 16, 11);
                rect(4, 14, 4, 3);
                rect(24, 14, 4, 3);
            }
            4 => {
                rect(10, 7, 4, 16);
                rect(18, 7, 4, 16);
                rect(14, 21, 4, 4);
            }
            5 => {
                rect(10, 5, 12, 3);
                rect(7, 9, 18, 18);
                rect(11, 18, 10, 3);
            }
            6 | 7 => {
                rect(9, 7, 14, 17);
                rect(4, 9, 5, 9);
                rect(23, 9, 5, 9);
                if slot == 7 {
                    rect(9, 24, 5, 5);
                    rect(18, 24, 5, 5);
                }
            }
            8 => {
                rect(4, 13, 24, 7);
                rect(13, 10, 6, 3);
                rect(13, 20, 6, 3);
            }
            9 | 16 | 17 => {
                rect(11, 13, 12, 13);
                rect(8, 15, 3, 8);
                rect(11, 6, 3, 8);
                rect(15, 4, 3, 10);
                rect(19, 6, 3, 8);
            }
            10 => {
                rect(7, 8, 18, 17);
                rect(13, 5, 6, 3);
            }
            11 | 12 => {
                rect(7, 10, 18, 3);
                rect(9, 15, 14, 11);
            }
            13 => {
                rect(8, 5, 5, 11);
                rect(19, 5, 5, 11);
                rect(10, 18, 12, 5);
            }
            14 => {
                rect(7, 8, 7, 16);
                rect(18, 8, 7, 16);
                rect(4, 22, 10, 4);
                rect(18, 22, 10, 4);
            }
            _ => {
                rect(6, 9, 20, 15);
                rect(13, 5, 6, 4);
            }
        }
    }
    ctx.load_texture(
        "station-original-slot-icons",
        image,
        egui::TextureOptions::NEAREST,
    )
}
pub fn barriers(
    source: &[station_sim::barrier::Barrier],
) -> Vec<horizon_station_render::station_barrier::Barrier> {
    source
        .iter()
        .map(|b| horizon_station_render::station_barrier::Barrier {
            cell: b.cell,
            blocks_movement: b.blocks_movement,
            blocks_sight: b.blocks_sight,
            blocks_light: b.blocks_light,
        })
        .collect()
}

pub fn floors(
    source: &[station_sim::floor::FloorTile],
) -> Vec<horizon_station_render::station_floor::FloorTile> {
    source
        .iter()
        .map(|f| horizon_station_render::station_floor::FloorTile {
            cell: f.cell,
            variant: f.variant,
            kind: match f.kind {
                station_sim::floor::FloorKind::Steel => {
                    horizon_station_render::station_floor::FloorKind::Steel
                }
                station_sim::floor::FloorKind::Damaged => {
                    horizon_station_render::station_floor::FloorKind::Damaged
                }
                station_sim::floor::FloorKind::Tech => {
                    horizon_station_render::station_floor::FloorKind::Tech
                }
                station_sim::floor::FloorKind::Grating => {
                    horizon_station_render::station_floor::FloorKind::Grating
                }
                station_sim::floor::FloorKind::Plating => {
                    horizon_station_render::station_floor::FloorKind::Plating
                }
            },
        })
        .collect()
}
/// Server phases drive RSI playback, including snapshots received mid-animation.
pub fn door(
    id: [u64; 4],
    cell: [i32; 3],
    glass: bool,
    horizontal: bool,
    phase: station_sim::crew::DoorPhase,
    time: f32,
) -> horizon_station_render::RenderItem {
    use horizon_station_render::{RenderItem, SpriteLayer, SpriteStyle};
    RenderItem {
        id,
        position: [cell[0] as f32 + 0.5, cell[1] as f32 + 0.5, cell[2] as f32],
        rotation: [0., 0., 0., 1.],
        sprite_size: [1., 1.],
        half: 0.5,
        held: false,
        billboard: true,
        color: [1.; 4],
        style: SpriteStyle {
            draw_depth: 100,
            ..Default::default()
        },
        layers: vec![SpriteLayer {
            rsi: if glass { "airlock-glass" } else { "airlock" }.into(),
            state: phase.state().into(),
            key: "door".into(),
            aliases: vec![],
            visible: true,
            rotation: 0.,
            mirror: [false; 2],
            direction_offset: 0,
            direction: Some(if horizontal { [0., -1.] } else { [1., 0.] }),
            auto_animated: false,
            looping: false,
            cycle: false,
            reversed: false,
            synchronized: false,
            animation_time: time.clamp(0., 0.5),
            playback_speed: 1.,
            restart: 0,
            unshaded: false,
            mask: None,
            depth: 0,
            color: [1.; 4],
            offset: [0.; 2],
            scale: [1.; 2],
        }],
    }
}

/// Native prototype graphics; wiring is only visible after lifting the covering tile.
pub fn power_items(
    power: &station_sim::power::PowerSnapshot,
) -> Vec<horizon_station_render::RenderItem> {
    let mut items = Vec::new();
    let shape = |cell: [i32; 3], id: u64, source: u64, color: [f32; 4], size: [f32; 2]| {
        let mut item = door(
            [4, source, id, 0],
            [cell[0], cell[1], cell[2] + 1],
            false,
            false,
            station_sim::crew::DoorPhase::Closed,
            0.,
        );
        item.style.draw_depth = -20;
        let l = &mut item.layers[0];
        l.rsi = "station-native".into();
        l.state = "solid".into();
        l.key = "power".into();
        l.direction = None;
        l.scale = size;
        l.color = color;
        item
    };
    let connected: std::collections::BTreeSet<_> = power
        .cables
        .iter()
        .filter(|c| c.connected)
        .map(|c| c.cell)
        .collect();
    for (id, c) in power.cables.iter().enumerate().filter(|(_, c)| c.exposed) {
        let color = if c.powered {
            [1., 0.65, 0.12, 1.]
        } else {
            [0.32, 0.22, 0.16, 1.]
        };
        let mut item = shape(c.cell, id as u64, 0, color, [0.12, 0.12]);
        for [x, y] in [[-1, 0], [1, 0], [0, -1], [0, 1]] {
            if c.connected && connected.contains(&[c.cell[0] + x, c.cell[1] + y, c.cell[2]]) {
                let mut l = item.layers[0].clone();
                l.key = format!("{x}/{y}");
                l.scale = if x == 0 { [0.07, 0.5] } else { [0.5, 0.07] };
                l.offset = [x as f32 * 0.25, y as f32 * 0.25];
                item.layers.push(l);
            }
        }
        items.push(item);
    }
    for (source, nodes) in [(1, &power.sources), (2, &power.controllers)] {
        for (id, n) in nodes.iter().enumerate() {
            let mut item = shape(
                n.cell,
                id as u64,
                source,
                [0.18, 0.23, 0.26, 1.],
                [0.6, 0.6],
            );
            item.style.draw_depth = -10;
            let mut indicator = item.layers[0].clone();
            indicator.key = "indicator".into();
            indicator.depth = 1;
            indicator.scale = [0.3, 0.1];
            indicator.color = if n.powered {
                [0.15, 1., 0.3, 1.]
            } else {
                [0.9, 0.15, 0.1, 1.]
            };
            item.layers.push(indicator);
            items.push(item);
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exposed_cable_and_power_nodes_have_renderable_pixels() {
        use station_sim::power::{CableSnapshot, NodeSnapshot, PowerSnapshot};
        let mut power = PowerSnapshot {
            cables: vec![CableSnapshot {
                cell: [0, 0, 0],
                connected: true,
                powered: true,
                exposed: false,
            }],
            sources: vec![NodeSnapshot {
                cell: [0, 0, 0],
                enabled: true,
                powered: true,
                reachable: true,
            }],
            ..Default::default()
        };
        assert_eq!(power_items(&power).len(), 1);
        power.cables[0].exposed = true;
        let atlas = horizon_station_render::station_sprite::Atlas::bundled().unwrap();
        let mut runtime = horizon_station_render::station_sprite_runtime::SpriteRuntime::default();
        let items = power_items(&power);
        assert_eq!(items.len(), 2);
        for item in items {
            let samples = runtime.sample(
                item.id,
                &item.layers,
                &item.style,
                item.sprite_size,
                [0., -1.],
                true,
                &atlas,
            );
            assert!(!samples.is_empty());
            assert!(samples.iter().any(|s| s.hit([0., 0.], &atlas)));
        }
    }
    #[test]
    fn airlock_rsi_seeks_mid_animation_and_retracts_both_orientations() {
        use horizon_station_render::{
            station_sprite::Atlas, station_sprite_runtime::SpriteRuntime,
        };
        use station_sim::crew::DoorPhase;
        let atlas = Atlas::bundled().unwrap();
        for glass in [false, true] {
            for horizontal in [false, true] {
                let mut runtime = SpriteRuntime::default();
                let mut opaque = vec![];
                for time in [0., 0.32, 0.5] {
                    let item = door(
                        [3, 0, 0, 0],
                        [0, 0, 1],
                        glass,
                        horizontal,
                        DoorPhase::Opening,
                        time,
                    );
                    let samples = runtime.sample(
                        item.id,
                        &item.layers,
                        &item.style,
                        item.sprite_size,
                        [0., -1.],
                        true,
                        &atlas,
                    );
                    let count = (0..32 * 32)
                        .filter(|i| {
                            samples[0].hit(
                                [
                                    (i % 32) as f32 / 32. - 0.5 + 0.5 / 32.,
                                    (i / 32) as f32 / 32. - 0.5 + 0.5 / 32.,
                                ],
                                &atlas,
                            )
                        })
                        .count();
                    opaque.push(count);
                }
                assert!(
                    opaque[0] > opaque[1] && opaque[1] > opaque[2],
                    "{glass}/{horizontal}: {opaque:?}"
                );
            }
        }
    }
    #[test]
    fn every_builtin_visual_resolves_to_an_imported_rsi_state() {
        let atlas = horizon_station_render::station_sprite::Atlas::bundled().unwrap();
        let c = station_sim::content::Content::builtin();
        let check = |source: &[station_sim::sprite::SpriteLayer]| {
            for l in layers(source) {
                assert!(
                    (l.rsi.is_empty() && l.state.is_empty()) || atlas.contains(&l.rsi, &l.state),
                    "{}/{}",
                    l.rsi,
                    l.state
                );
            }
        };
        check(&c.character.layers);
        for p in c.prototypes.values() {
            for list in [
                &p.sprite.layers,
                &p.sprite.active_layers,
                &p.sprite.open_layers,
                &p.sprite.working_layers,
                &p.sprite.inhand_left,
                &p.sprite.inhand_right,
                &p.sprite.worn,
            ] {
                check(list);
            }
        }
    }
}

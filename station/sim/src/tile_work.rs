//! Server-authoritative tile stack use and timed surface replacement.
use super::*;
use actions::WorkTarget;
use floor::{FloorKind, FloorTile};
impl Simulation {
    pub(super) fn surface_version(&self, c: Cell) -> u64 {
        self.world.floor_versions.get(&c).copied().unwrap_or(0)
    }
    pub fn floor_kind(&self, c: Cell) -> Option<FloorKind> {
        self.world.solid(c).then(|| {
            self.world
                .floors
                .get(&c)
                .map_or(FloorKind::Steel, |f| f.kind)
        })
    }
    pub(super) fn tile_reachable(&self, c: Cell) -> bool {
        if !c.iter().all(|v| v.unsigned_abs() <= 128)
            || ![0, -4].contains(&c[2])
            || !self.world.solid(c)
            || self.world.blocks_movement([c[0], c[1], c[2] + 1])
        {
            return false;
        }
        let p = self.position();
        let end = [c[0] as f32 + 0.5, c[1] as f32 + 0.5, c[2] as f32 + 1.01];
        let d: [f32; 3] = std::array::from_fn(|i| end[i] - p[i]);
        d.iter().map(|v| v * v).sum::<f32>() <= 1.8 * 1.8
            && (1..=32).all(|n| {
                !self.world.blocks_movement(std::array::from_fn(|i| {
                    (p[i] + d[i] * n as f32 / 32.).floor() as i32
                }))
            })
    }
    fn returned_stack(&self, kind: FloorKind) -> Option<usize> {
        self.inventory
            .hands
            .iter()
            .chain(self.inventory.slots.iter())
            .flatten()
            .copied()
            .find(|&id| {
                self.items.get(id).is_some_and(|i| {
                    i.alive && i.kind == ItemKind::Tile(kind) && i.stack_count < 50
                })
            })
    }
    fn tile_prototype(&self, kind: FloorKind) -> Option<String> {
        self.content
            .prototypes
            .iter()
            .find(|(_, p)| p.kind == ItemKind::Tile(kind))
            .map(|(k, _)| k.clone())
    }
    pub(super) fn tile_work_valid(&self, target: WorkTarget, tool: usize) -> bool {
        let Some(item) = self
            .items
            .get(tool)
            .filter(|i| i.alive && self.held == Some(tool))
        else {
            return false;
        };
        match target {
            WorkTarget::LayTile(c) => {
                matches!(item.kind,ItemKind::Tile(k) if k!=FloorKind::Plating)
                    && item.stack_count > 0
                    && self.tile_reachable(c)
                    && self.floor_kind(c) == Some(FloorKind::Plating)
            }
            WorkTarget::LiftTile(c) => {
                item.kind == ItemKind::Crowbar
                    && self.tile_reachable(c)
                    && self.floor_kind(c).is_some_and(|k| {
                        k != FloorKind::Plating
                            && self.tile_prototype(k).is_some()
                            && (self.returned_stack(k).is_some() || self.items.len() < 4096)
                    })
            }
            _ => false,
        }
    }
    pub(super) fn complete_tile_work(&mut self, target: WorkTarget, tool: usize) {
        // Called only after the final validation, sequentially on the simulation thread.
        match target {
            WorkTarget::LayTile(c) => {
                let ItemKind::Tile(kind) = self.items[tool].kind else {
                    return;
                };
                if self
                    .set_floor(FloorTile {
                        cell: c,
                        kind,
                        variant: None,
                    })
                    .is_err()
                {
                    return;
                }
                self.items[tool].stack_count -= 1;
                if self.items[tool].stack_count == 0 {
                    self.despawn_entity(self.items[tool].entity);
                }
                self.inventory.message = "action.tile_laid".into();
            }
            WorkTarget::LiftTile(c) => {
                let Some(kind) = self.floor_kind(c) else {
                    return;
                };
                if let Some(id) = self.returned_stack(kind) {
                    self.items[id].stack_count += 1;
                } else {
                    let Some(key) = self.tile_prototype(kind) else {
                        return;
                    };
                    let p = [c[0] as f32 + 0.5, c[1] as f32 + 0.5, c[2] as f32 + 1.2];
                    let Ok(entity) = self.spawn_prototype(&key, p) else {
                        return;
                    };
                    let id = self.entities.get(entity).unwrap().item.unwrap();
                    self.items[id].stack_count = 1;
                }
                self.set_floor(FloorTile {
                    cell: c,
                    kind: FloorKind::Plating,
                    variant: None,
                })
                .expect("validated floor");
                self.inventory.message = "action.tile_lifted".into();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use interaction::{Command, VerbKind};
    fn equip(s: &mut Simulation, key: &str, count: u16) -> usize {
        let p = s.position();
        let e = s.spawn_prototype(key, [p[0] + 0.6, p[1], p[2]]).unwrap();
        let id = s.entities.get(e).unwrap().item.unwrap();
        s.items[id].stack_count = count;
        s.command(Command::Verb {
            target: id,
            kind: VerbKind::Pickup,
        });
        assert_eq!(s.held, Some(id));
        id
    }
    fn run(s: &mut Simulation, n: usize) {
        for _ in 0..n {
            s.step([0.; 2]);
        }
    }
    #[test]
    fn lay_consumes_on_completion_cancel_is_free_and_lift_keeps_the_deck() {
        let mut s = Simulation::new(7);
        run(&mut s, 180);
        let c = [0, 0, 0];
        s.set_floor(FloorTile {
            cell: c,
            kind: FloorKind::Plating,
            variant: None,
        })
        .unwrap();
        let tile = equip(&mut s, "tile.steel", 1);
        s.begin_work(WorkTarget::LayTile(c), tile);
        run(&mut s, 44);
        assert_eq!(s.items[tile].stack_count, 1);
        s.cancel_work(s.action().unwrap().id);
        run(&mut s, 1);
        assert_eq!(s.floor_kind(c), Some(FloorKind::Plating));
        s.begin_work(WorkTarget::LayTile(c), tile);
        run(&mut s, 45);
        assert!(!s.items[tile].alive && s.held.is_none());
        assert_eq!(s.floor_kind(c), Some(FloorKind::Steel));
        let crowbar = equip(&mut s, "crowbar", 1);
        s.begin_work(WorkTarget::LiftTile(c), crowbar);
        run(&mut s, 90);
        assert_eq!(s.floor_kind(c), Some(FloorKind::Plating));
        assert!(s.world.solid(c));
        assert!((s.position()[2] - 1.3).abs() < 0.05);
        assert_eq!(
            s.items
                .iter()
                .filter(|i| i.alive && i.kind == ItemKind::Tile(FloorKind::Steel))
                .map(|i| i.stack_count as u32)
                .sum::<u32>(),
            1
        );
        s.begin_work(WorkTarget::LiftTile(c), crowbar);
        assert!(s.action().is_none());
    }
    #[test]
    fn tile_work_rejects_obstacles_wrong_deck_and_stale_surface_versions() {
        let mut s = Simulation::new(7);
        run(&mut s, 180);
        let crowbar = equip(&mut s, "crowbar", 1);
        for c in [[100, 100, 0], [0, 0, -4]] {
            s.begin_work(WorkTarget::LiftTile(c), crowbar);
            assert!(s.action().is_none());
        }
        s.set_voxel([1, 0, 1], true);
        s.begin_work(WorkTarget::LiftTile([1, 0, 0]), crowbar);
        assert!(s.action().is_none());
        s.begin_work(WorkTarget::LiftTile([0, 0, 0]), crowbar);
        assert!(s.action().is_some());
        s.set_floor(FloorTile {
            cell: [0, 0, 0],
            kind: FloorKind::Plating,
            variant: None,
        })
        .unwrap();
        s.set_floor(FloorTile {
            cell: [0, 0, 0],
            kind: FloorKind::Steel,
            variant: None,
        })
        .unwrap();
        run(&mut s, 1);
        assert!(s.action().is_none());
        assert!(!s.items.iter().any(|i| matches!(i.kind, ItemKind::Tile(_))));
    }
    #[test]
    fn two_actors_only_return_or_consume_one_tile_for_the_same_cell() {
        let mut s = Simulation::new(7);
        s.bodies[s.items[storage::CRATE].body].set_translation(vector![6., -4., 1.4], true);
        for (id, x) in [(1, -0.3), (2, 1.3)] {
            s.add_actor(id);
            s.select_actor(id);
            s.bodies[s.player].set_translation(vector![x, 0.5, 1.3], true);
        }
        for _ in 0..90 {
            s.step_actors(&[(1, [0.; 2]), (2, [0.; 2])]);
        }
        for id in [1, 2] {
            s.select_actor(id);
            let tool = equip(&mut s, "crowbar", 1);
            s.begin_work(WorkTarget::LiftTile([0, 0, 0]), tool);
            assert!(s.action().is_some());
        }
        for _ in 0..90 {
            s.step_actors(&[(1, [0.; 2]), (2, [0.; 2])]);
        }
        assert_eq!(s.floor_kind([0, 0, 0]), Some(FloorKind::Plating));
        assert_eq!(
            s.items
                .iter()
                .filter(|i| i.alive && i.kind == ItemKind::Tile(FloorKind::Steel))
                .map(|i| i.stack_count as u32)
                .sum::<u32>(),
            1
        );
        let mut tiles = vec![];
        for id in [1, 2] {
            s.select_actor(id);
            s.command(Command::Hand(1));
            let tool = equip(&mut s, "tile.steel", 1);
            tiles.push(tool);
            s.begin_work(WorkTarget::LayTile([0, 0, 0]), tool);
            assert!(s.action().is_some());
        }
        for _ in 0..45 {
            s.step_actors(&[(1, [0.; 2]), (2, [0.; 2])]);
        }
        assert_eq!(s.floor_kind([0, 0, 0]), Some(FloorKind::Steel));
        assert_eq!(
            tiles
                .iter()
                .map(|&id| s.items[id].stack_count as u32)
                .sum::<u32>(),
            1
        );
        assert_eq!(tiles.iter().filter(|&&id| s.items[id].alive).count(), 1);
    }
}

//! Server-clock timed tool actions. Client supplies neither progress nor completion.
use super::*;
use storage::CRATE;
#[cfg(test)]
use storage::CROWBAR;
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkTarget {
    Lock,
    Cable(Cell),
    Door(usize),
    Floor(Cell),
    LiftTile(Cell),
    LayTile(Cell),
}
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)]
pub struct ActionSnapshot {
    pub id: u64,
    pub target: WorkTarget,
    pub elapsed: u64,
    pub duration: u64,
}
pub(super) struct Action {
    pub snapshot: ActionSnapshot,
    origin: [f32; 3],
    hand: usize,
    tool: usize,
    surface_version: Option<u64>,
    power_revision: Option<u64>,
}
impl Simulation {
    pub fn action(&self) -> Option<ActionSnapshot> {
        self.action.as_ref().map(|a| a.snapshot)
    }
    pub fn world_revision(&self) -> u64 {
        self.world_revision
    }
    fn work_valid(&self, target: WorkTarget, tool: usize) -> bool {
        if let WorkTarget::Cable(c) = target {
            return self.cable_work_valid(c, tool);
        }
        if let WorkTarget::Door(id) = target {
            return self.door_pry_valid(id, tool);
        }
        if matches!(target, WorkTarget::LiftTile(_) | WorkTarget::LayTile(_)) {
            return self.tile_work_valid(target, tool);
        }
        if !self
            .items
            .get(tool)
            .is_some_and(|i| i.kind == ItemKind::Crowbar)
            || self.held != Some(tool)
        {
            return false;
        }
        match target {
            WorkTarget::LiftTile(_)
            | WorkTarget::LayTile(_)
            | WorkTarget::Door(_)
            | WorkTarget::Cable(_) => unreachable!(),
            WorkTarget::Lock => self.storage.locked && self.reachable(CRATE),
            WorkTarget::Floor(c) => {
                if !(-7..=7).contains(&c[0])
                    || !(-5..=5).contains(&c[1])
                    || ![0, -4].contains(&c[2])
                    || !self.world.solid(c)
                    || self.world.solid([c[0], c[1], c[2] + 1])
                {
                    return false;
                }
                let p = self.position();
                let end = [c[0] as f32 + 0.5, c[1] as f32 + 0.5, c[2] as f32 + 1.01];
                let d = std::array::from_fn::<_, 3, _>(|i| end[i] - p[i]);
                d.iter().map(|x| x * x).sum::<f32>() <= 1.8 * 1.8
                    && (1..=32).all(|n| {
                        !self.world.blocks_movement(std::array::from_fn(|i| {
                            (p[i] + d[i] * n as f32 / 32.).floor() as i32
                        }))
                    })
            }
        }
    }
    pub(super) fn begin_work(&mut self, target: WorkTarget, tool: usize) {
        if let Some(a) = &self.action {
            if a.snapshot.target != target || a.tool != tool {
                self.inventory.message = "action.busy".into();
            }
            return;
        }
        if !self.work_valid(target, tool) {
            self.inventory.message = "action.invalid".into();
            return;
        }
        self.next_action += 1;
        self.action = Some(Action {
            snapshot: ActionSnapshot {
                id: self.next_action,
                target,
                elapsed: 0,
                duration: match target {
                    WorkTarget::Lock => 120,
                    WorkTarget::Cable(_) => 60,
                    WorkTarget::Door(_) => 120,
                    WorkTarget::Floor(_) => 150,
                    WorkTarget::LiftTile(_) => 90,
                    WorkTarget::LayTile(_) => 45,
                },
            },
            power_revision: matches!(target, WorkTarget::Cable(_) | WorkTarget::Door(_))
                .then_some(self.power.revision),
            surface_version: match target {
                WorkTarget::LiftTile(c) | WorkTarget::LayTile(c) | WorkTarget::Cable(c) => {
                    Some(self.surface_version(c))
                }
                _ => None,
            },
            origin: self.position(),
            hand: self.inventory.active,
            tool,
        });
        self.inventory.message = "action.started".into();
    }
    pub(super) fn cancel_work(&mut self, id: u64) {
        if self.action.as_ref().is_some_and(|a| a.snapshot.id == id) {
            self.action = None;
            self.inventory.message = "action.cancelled".into();
        }
    }
    pub(super) fn validate_work(&mut self) {
        if let Some(a) = &self.action {
            let moved = self
                .position()
                .iter()
                .zip(a.origin)
                .map(|(p, o)| (p - o).powi(2))
                .sum::<f32>()
                > 0.12 * 0.12;
            let changed = match a.snapshot.target {
                WorkTarget::LiftTile(c) | WorkTarget::LayTile(c) | WorkTarget::Cable(c) => {
                    a.surface_version != Some(self.surface_version(c))
                }
                _ => false,
            };
            if moved
                || changed
                || a.power_revision.is_some_and(|r| r != self.power.revision)
                || action_rules::hands_cancel(
                    true,
                    true,
                    self.inventory.hands.contains(&Some(a.tool)),
                    true,
                    a.hand as i64,
                    self.inventory.active as i64,
                )
                .unwrap_or(true)
                || !self.work_valid(a.snapshot.target, a.tool)
            {
                let id = a.snapshot.id;
                self.cancel_work(id);
            }
        }
    }
    pub(super) fn advance_work(&mut self, moving: bool) {
        if moving && let Some(a) = self.action() {
            self.cancel_work(a.id);
        }
        self.validate_work();
        let Some(a) = self.action.as_mut() else {
            return;
        };
        a.snapshot.elapsed += 1;
        if a.snapshot.elapsed < a.snapshot.duration {
            return;
        }
        // Take before effects: repeated ticks and requests cannot re-apply completion.
        let action = self.action.take().unwrap();
        let target = action.snapshot.target;
        match target {
            WorkTarget::Cable(c) => self.toggle_cable(c),
            WorkTarget::Door(id) => self.pry_door(id),
            WorkTarget::LiftTile(_) | WorkTarget::LayTile(_) => {
                self.complete_tile_work(target, action.tool)
            }
            WorkTarget::Lock => {
                self.storage.locked = false;
                self.inventory.message = "status.unlocked".into();
            }
            WorkTarget::Floor(c) => {
                self.set_voxel(c, false);
                self.inventory.message = "action.floor_done".into();
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use interaction::{Command, VerbKind};
    fn ready() -> Simulation {
        let mut s = Simulation::new(7);
        for _ in 0..180 {
            s.step([0., 0.]);
        }
        let p = s.position();
        s.bodies[s.items[CROWBAR].body].set_translation(vector![p[0] + 0.5, p[1], p[2]], true);
        s.bodies[s.items[CRATE].body].set_translation(vector![p[0], p[1] + 1., p[2]], true);
        s.command(Command::Verb {
            target: CROWBAR,
            kind: VerbKind::Pickup,
        });
        assert_eq!(s.held, Some(CROWBAR));
        s
    }
    fn start(s: &mut Simulation, target: WorkTarget) {
        s.command(Command::Work {
            target,
            tool: CROWBAR,
        });
        assert!(s.action().is_some());
    }
    #[test]
    fn no_early_unlock_and_repeated_start_does_not_restart() {
        let mut s = ready();
        start(&mut s, WorkTarget::Lock);
        let id = s.action().unwrap().id;
        for _ in 0..119 {
            s.command(Command::Work {
                target: WorkTarget::Lock,
                tool: CROWBAR,
            });
            s.step([0., 0.]);
        }
        assert!(s.storage.locked);
        assert_eq!(s.action().unwrap().elapsed, 119);
        assert_eq!(s.action().unwrap().id, id);
        s.step([0., 0.]);
        assert!(!s.storage.locked);
        assert!(s.action().is_none());
        for _ in 0..5 {
            s.command(Command::Work {
                target: WorkTarget::Lock,
                tool: CROWBAR,
            });
            s.step([0., 0.]);
        }
        assert!(s.action().is_none());
        assert_eq!(s.held, Some(CROWBAR));
        assert!(s.storage.items.is_empty());
    }
    #[test]
    fn cancel_at_last_tick_and_stale_cancel_cannot_cancel_new_work() {
        let mut s = ready();
        start(&mut s, WorkTarget::Lock);
        let first = s.action().unwrap().id;
        for _ in 0..119 {
            s.step([0., 0.]);
        }
        s.command(Command::CancelWork(first));
        s.step([0., 0.]);
        assert!(s.storage.locked);
        start(&mut s, WorkTarget::Lock);
        let second = s.action().unwrap().id;
        assert_ne!(first, second);
        s.command(Command::CancelWork(first));
        assert_eq!(s.action().unwrap().id, second);
        s.command(Command::Work {
            target: WorkTarget::Floor([0, 0, 0]),
            tool: CROWBAR,
        });
        assert_eq!(s.action().unwrap().id, second);
    }
    #[test]
    fn changing_hand_dropping_throwing_and_moving_cancel() {
        for cmd in [
            Command::Swap,
            Command::Verb {
                target: CROWBAR,
                kind: VerbKind::Drop,
            },
            Command::Verb {
                target: CROWBAR,
                kind: VerbKind::Throw,
            },
            Command::Slot(11),
        ] {
            let mut s = ready();
            start(&mut s, WorkTarget::Lock);
            s.command(cmd);
            assert!(s.action().is_none());
            assert!(s.storage.locked);
        }
        let mut s = ready();
        start(&mut s, WorkTarget::Lock);
        s.command(Command::Swap);
        s.command(Command::Swap);
        assert!(s.action().is_none());
        start(&mut s, WorkTarget::Lock);
        s.throw_item();
        s.pickup();
        assert!(s.action().is_none());
        if s.held != Some(CROWBAR) {
            s.drop_item();
            s.command(Command::Verb {
                target: CROWBAR,
                kind: VerbKind::Pickup,
            });
        }
        start(&mut s, WorkTarget::Lock);
        s.step([1., 0.]);
        assert!(s.action().is_none());
        assert!(s.storage.locked);
    }
    #[test]
    fn obstacle_target_loss_and_external_movement_cancel() {
        let mut s = ready();
        start(&mut s, WorkTarget::Lock);
        s.set_voxel([0, 1, 1], true);
        s.step([0., 0.]);
        assert!(s.action().is_none());
        assert!(s.storage.locked);
        s.set_voxel([0, 1, 1], false);
        start(&mut s, WorkTarget::Lock);
        s.bodies[s.items[CRATE].body].set_translation(vector![6., 5., 1.3], true);
        s.step([0., 0.]);
        assert!(s.action().is_none());
        start(&mut s, WorkTarget::Floor([0, 0, 0]));
        s.bodies[s.player].set_translation(vector![0.8, 0.5, 1.3], true);
        s.step([0., 0.]);
        assert!(s.action().is_none());
        assert!(s.world.solid([0, 0, 0]));
    }
    #[test]
    fn floor_completion_updates_collision_and_is_applied_once() {
        let mut s = ready();
        let revision = s.world_revision();
        start(&mut s, WorkTarget::Floor([0, 0, 0]));
        for _ in 0..149 {
            s.step([0., 0.]);
        }
        assert!(s.world.solid([0, 0, 0]));
        assert_eq!(s.world_revision(), revision);
        s.step([0., 0.]);
        assert!(!s.world.solid([0, 0, 0]));
        assert_eq!(s.world_revision(), revision + 1);
        assert!(s.action().is_none());
        for _ in 0..240 {
            s.command(Command::Work {
                target: WorkTarget::Floor([0, 0, 0]),
                tool: CROWBAR,
            });
            s.step([0., 0.]);
        }
        assert_eq!(s.world_revision(), revision + 1);
        assert!((s.position()[2] + 2.7).abs() < 0.05);
    }
    #[test]
    fn missing_floor_wrong_tool_and_boundary_are_rejected() {
        let mut s = ready();
        for c in [[i32::MAX, 0, 0], [0, 0, 1], [8, 0, 0], [0, 0, -4]] {
            s.command(Command::Work {
                target: WorkTarget::Floor(c),
                tool: CROWBAR,
            });
            assert!(s.action().is_none());
        }
        s.command(Command::Work {
            target: WorkTarget::Lock,
            tool: 0,
        });
        assert!(s.action().is_none());
        start(&mut s, WorkTarget::Floor([0, 0, 0]));
        s.set_voxel([0, 0, 0], false);
        s.set_voxel([0, 0, 0], true);
        s.step([0., 0.]);
        assert!(s.action().is_none());
    }
}

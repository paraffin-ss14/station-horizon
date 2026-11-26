//! Station professions, physical ID cards and shared access-controlled doors.
use super::*;
#[path = "../../ss14-access-rules/src/port.rs"]
mod access;
#[derive(
    serde::Serialize,
    serde::Deserialize,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
)]
pub enum Job {
    Engineer,
    Doctor,
    #[default]
    Assistant,
}
impl Job {
    pub const ALL: [Self; 3] = [Self::Engineer, Self::Doctor, Self::Assistant];
    pub fn index(self) -> usize {
        match self {
            Self::Engineer => 0,
            Self::Doctor => 1,
            Self::Assistant => 2,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Engineer => "job.engineer",
            Self::Doctor => "job.doctor",
            Self::Assistant => "job.assistant",
        }
    }
}
/// RSI opening/closing takes 0.49 seconds; physics runs at 60 Hz.
pub const DOOR_TICKS: u16 = 30;
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoorPhase {
    Closed,
    Opening,
    Open,
    Closing,
}
impl DoorPhase {
    pub fn moving(self) -> bool {
        matches!(self, Self::Opening | Self::Closing)
    }
    pub fn state(self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::Opening => "opening",
            Self::Open => "open",
            Self::Closing => "closing",
        }
    }
}
pub(super) struct Door {
    pub(super) entity: entity::EntityId,
    pub(super) alive: bool,
    name: String,
    pub(super) cell: Cell,
    access: u8,
    open: bool,
    phase: DoorPhase,
    elapsed: u16,
    glass: bool,
    horizontal: bool,
}
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct DoorSnapshot {
    pub entity: entity::EntityId,
    pub name: String,
    pub id: usize,
    pub cell: Cell,
    pub open: bool,
    pub powered: bool,
    pub phase: DoorPhase,
    pub elapsed: u16,
    pub glass: bool,
    pub horizontal: bool,
    pub allowed: bool,
    pub reachable: bool,
}
impl Simulation {
    pub fn prepare_departments(&mut self) {
        if !self.doors.is_empty() {
            return;
        }
        let data = self.content.clone();
        let edits: Vec<_> = data
            .map
            .departments
            .iter()
            .flat_map(|b| b.cells().map(|c| (c, true)))
            .collect();
        self.edit_voxels(&edits);
        for d in &data.map.doors {
            let entity = self.entities.insert(entity::Entity {
                prototype: "door".into(),
                position: d.cell.map(|v| v as f32),
                sprite: None,
                body: None,
                item: None,
                actor: None,
                door: Some(self.doors.len()),
                container_capacity: None,
                access: Some(d.access),
            });
            self.doors.push(Door {
                entity,
                alive: true,
                name: d.name.clone(),
                cell: d.cell,
                access: d.access,
                open: false,
                phase: DoorPhase::Closed,
                elapsed: 0,
                glass: d.glass,
                horizontal: d.horizontal,
            });
            self.edit_voxels(&[
                (d.cell, true),
                ([d.cell[0], d.cell[1], d.cell[2] + 1], true),
            ]);
        }
        self.bodies[self.items[storage::CRATE].body]
            .set_translation(Vector::from(data.map.crate_round_position), true);
        self.world_revision = 0; // This is the new round's template, not a player edit.
    }
    pub fn outfit_actor(&mut self, id: u64, job: Job, late: bool) {
        if self.outfitted.contains(&id) || !self.select_actor(id) {
            return;
        }
        self.outfitted.insert(id);
        let mut p = self.content.map.spawns[if late { "arrival" } else { job.key() }];
        if late || job == Job::Assistant {
            p[0] += (id % 4) as f32;
        }
        self.bodies[self.player].set_translation(Vector::from(p), true);
        self.bodies[self.player].set_linvel(vector![0., 0., 0.], true);
        let kit = self.content.kits[job.key()].clone();
        for item in kit {
            let entity = self
                .spawn_prototype(&item.prototype, self.position())
                .expect("validated kit");
            let id = self.entities.get(entity).unwrap().item.unwrap();
            self.inventory.slots[item.slot] = Some(id);
            self.bodies[self.items[id].body].set_enabled(false);
        }
    }
    fn access_tags(&self) -> u8 {
        [self.inventory.slots[10], self.held]
            .into_iter()
            .flatten()
            .filter_map(|id| self.items.get(id))
            .fold(0, |tags, item| {
                tags | match item.kind {
                    ItemKind::IdCard(access) => access,
                    _ => 0,
                }
            })
    }
    fn door_reachable(&self, door: &Door) -> bool {
        let p = self.position();
        let q = [
            door.cell[0] as f32 + 0.5,
            door.cell[1] as f32 + 0.5,
            door.cell[2] as f32 + self.content.character.radius,
        ];
        let d: [f32; 3] = std::array::from_fn(|i| q[i] - p[i]);
        d.iter().map(|v| v * v).sum::<f32>() <= 1.8 * 1.8
            && (1..32).all(|i| {
                let c = std::array::from_fn(|a| (p[a] + d[a] * i as f32 / 32.).floor() as i32);
                (c[0] == door.cell[0] && c[1] == door.cell[1]) || !self.world.blocks_movement(c)
            })
    }
    pub fn door_snapshots(&self) -> Vec<DoorSnapshot> {
        self.doors
            .iter()
            .enumerate()
            .filter(|(_, d)| d.alive)
            .map(|(id, d)| DoorSnapshot {
                entity: d.entity,
                name: d.name.clone(),
                id,
                cell: d.cell,
                open: d.open,
                powered: self.device_powered([d.cell[0], d.cell[1], d.cell[2] - 1]),
                phase: d.phase,
                elapsed: d.elapsed,
                glass: d.glass,
                horizontal: d.horizontal,
                reachable: self.door_reachable(d),
                allowed: access::tags_allowed(self.access_tags() as i64, 0, d.access as i64, 0, 1)
                    .unwrap(),
            })
            .collect()
    }
    pub(super) fn toggle_door(&mut self, id: usize) {
        let Some(state) = self.door_snapshots().into_iter().find(|d| d.id == id) else {
            self.inventory.message = "error.action".into();
            return;
        };
        if !state.reachable {
            self.inventory.message = "error.reach".into();
            return;
        }
        if !state.powered {
            self.inventory.message = "power.no_power".into();
            return;
        }
        if !state.allowed {
            self.inventory.message = "error.access".into();
            return;
        }
        if state.phase.moving() {
            return;
        }
        if state.open && self.door_occupied(state.cell) {
            self.inventory.message = "error.door_blocked".into();
            return;
        }
        self.doors[id].phase = if state.open {
            DoorPhase::Closing
        } else {
            DoorPhase::Opening
        };
        self.doors[id].elapsed = 0;
    }
    pub(super) fn door_pry_valid(&self, id: usize, tool: usize) -> bool {
        self.held == Some(tool)
            && self
                .items
                .get(tool)
                .is_some_and(|i| i.alive && i.kind == ItemKind::Crowbar)
            && self
                .door_snapshots()
                .iter()
                .find(|d| d.id == id)
                .is_some_and(|d| {
                    d.reachable
                        && !d.phase.moving()
                        && !d.open
                        && power::rules::pry_allowed(false, d.powered, false).unwrap_or(false)
                })
    }
    pub(super) fn pry_door(&mut self, id: usize) {
        self.doors[id].phase = DoorPhase::Opening;
        self.doors[id].elapsed = 0;
    }
    fn door_occupied(&self, c: Cell) -> bool {
        self.bodies.iter().any(|(_, b)| {
            b.is_enabled()
                && b.colliders().iter().any(|h| {
                    let collider = &self.colliders[*h];
                    let pose =
                        b.position() * collider.position_wrt_parent().copied().unwrap_or_default();
                    let bounds = collider.shape().compute_aabb(&pose);
                    (0..3).all(|axis| {
                        bounds.maxs[axis] > c[axis] as f32 - 0.02
                            && bounds.mins[axis]
                                < c[axis] as f32 + if axis == 2 { 2.02 } else { 1.02 }
                    })
                })
        })
    }
    pub(super) fn tick_doors(&mut self) {
        for id in 0..self.doors.len() {
            if !self.doors[id].alive || !self.doors[id].phase.moving() {
                continue;
            }
            let c = self.doors[id].cell;
            if self.doors[id].phase == DoorPhase::Closing && self.door_occupied(c) {
                // Reverse from the same approximate visual position; passage stays free.
                self.doors[id].phase = DoorPhase::Opening;
                self.doors[id].elapsed = DOOR_TICKS - self.doors[id].elapsed;
            }
            let door = &mut self.doors[id];
            door.elapsed = (door.elapsed + 1).min(DOOR_TICKS);
            if door.elapsed == DOOR_TICKS {
                let open = door.phase == DoorPhase::Opening;
                door.phase = if open {
                    DoorPhase::Open
                } else {
                    DoorPhase::Closed
                };
                door.elapsed = 0;
                if door.open != open {
                    door.open = open;
                    self.edit_voxels(&[(c, !open), ([c[0], c[1], c[2] + 1], !open)]);
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use interaction::Command;
    fn crew() -> Simulation {
        let mut s = Simulation::new(7);
        s.prepare_departments();
        s.add_actor(1);
        s.add_actor(2);
        s.outfit_actor(1, Job::Engineer, false);
        s.outfit_actor(2, Job::Doctor, false);
        for _ in 0..120 {
            s.step_actors(&[(1, [0.; 2]), (2, [0.; 2])]);
        }
        s
    }
    #[test]
    fn roles_get_distinct_gear_and_doors_use_actual_cards() {
        let mut s = crew();
        s.select_actor(1);
        assert_eq!(
            s.items[s.inventory.slots[11].unwrap()].kind,
            ItemKind::Crowbar
        );
        let card = s.inventory.slots[10].unwrap();
        let count = s.items.len();
        s.outfit_actor(1, Job::Doctor, false);
        assert_eq!(s.items.len(), count);
        assert!(s.door_snapshots()[0].allowed);
        assert!(!s.door_snapshots()[1].allowed);
        s.command(Command::Door(0));
        for _ in 0..DOOR_TICKS {
            s.tick_doors();
        }
        assert!(s.doors[0].open);
        assert!(!s.world.solid([-3, 2, 1]));
        s.command(Command::Door(0));
        for _ in 0..DOOR_TICKS {
            s.tick_doors();
        }
        assert!(!s.doors[0].open);
        s.command(Command::Slot(10));
        s.drop_item();
        s.command(Command::Door(0));
        for _ in 0..DOOR_TICKS {
            s.tick_doors();
        }
        assert!(!s.doors[0].open);
        assert_eq!(s.inventory.message, "error.access");
        s.select_actor(2);
        assert_eq!(
            s.items[s.inventory.slots[11].unwrap()].kind,
            ItemKind::Medkit
        );
        s.bodies[s.player].set_translation(vector![-4., 2.5, 1.3], true);
        s.command(Command::Door(0));
        for _ in 0..DOOR_TICKS {
            s.tick_doors();
        }
        assert!(!s.doors[0].open);
        let p = s.position();
        s.bodies[s.items[card].body].set_translation(vector![p[0] + 0.5, p[1], p[2]], true);
        s.command(Command::Verb {
            target: card,
            kind: interaction::VerbKind::Pickup,
        });
        assert_eq!(s.held, Some(card));
        s.command(Command::Door(0));
        for _ in 0..DOOR_TICKS {
            s.tick_doors();
        }
        assert!(s.doors[0].open);
        s.bodies[s.player].set_translation(vector![-2.5, 2.5, 1.3], true);
        s.command(Command::Door(0));
        for _ in 0..DOOR_TICKS {
            s.tick_doors();
        }
        assert!(s.doors[0].open);
        assert_eq!(s.inventory.message, "error.door_blocked");
    }
    #[test]
    fn closed_door_blocks_physics_and_open_door_allows_passage() {
        let mut s = crew();
        for _ in 0..60 {
            s.step_actors(&[(1, [1., 0.]), (2, [0.; 2])]);
        }
        s.select_actor(1);
        assert!(s.position()[0] < -3.25);
        s.command(Command::Door(0));
        for _ in 0..DOOR_TICKS {
            s.tick_doors();
        }
        assert!(s.doors[0].open);
        for _ in 0..30 {
            s.step_actors(&[(1, [1., 0.]), (2, [0.; 2])]);
        }
        s.select_actor(1);
        assert!(s.position()[0] > -2.);
    }
    #[test]
    fn animation_controls_collision_and_obstruction_reverses_closing() {
        let mut s = crew();
        s.select_actor(1);
        let cell = s.doors[0].cell;
        let revision = s.world_revision;
        s.command(Command::Door(0));
        assert_eq!(s.doors[0].phase, DoorPhase::Opening);
        for _ in 0..DOOR_TICKS - 1 {
            s.tick_doors();
        }
        assert!(s.world.blocks_movement(cell));
        assert_eq!(s.world_revision, revision);
        s.tick_doors();
        assert!(!s.world.blocks_movement(cell));
        assert_eq!(s.world_revision, revision + 1);
        s.command(Command::Door(0));
        for _ in 0..DOOR_TICKS / 2 {
            s.tick_doors();
        }
        assert!(s.doors[0].open);
        // An item enters after closing starts. Its collider, not only its centre, counts.
        let body = s.items[storage::CRATE].body;
        s.bodies[body].set_enabled(true);
        s.bodies[body].set_translation(vector![-3.1, 2.5, 1.5], true);
        assert!(s.door_occupied(cell));
        s.tick_doors();
        assert_eq!(s.doors[0].phase, DoorPhase::Opening);
        for _ in 0..DOOR_TICKS {
            s.tick_doors();
        }
        assert_eq!(s.doors[0].phase, DoorPhase::Open);
        assert_eq!(s.world_revision, revision + 1);
        s.bodies[body].set_enabled(false);
        s.command(Command::Door(0));
        for _ in 0..DOOR_TICKS - 1 {
            s.tick_doors();
        }
        assert!(!s.world.blocks_movement(cell));
        s.tick_doors();
        assert!(s.world.blocks_movement(cell));
        assert_eq!(s.world_revision, revision + 2);
    }
    #[test]
    fn glass_airlock_keeps_physics_but_transmits_sight_and_light() {
        let mut c = (*content::Content::builtin()).clone();
        c.map.doors[0].glass = true;
        let mut s = Simulation::with_content(7, Arc::new(c));
        s.prepare_departments();
        let cell = s.doors[0].cell;
        assert!(s.world.blocks_movement(cell));
        let barrier = &s.world.barriers[&cell];
        assert!(!barrier.blocks_sight && !barrier.blocks_light);
        assert!(s.door_snapshots()[0].glass);
        s.bodies[s.player].set_translation(vector![-4., 2.5, 1.3], true);
        for _ in 0..60 {
            s.step([1., 0.]);
        }
        assert!(s.position()[0] < -3.25);
    }
    #[test]
    fn late_arrival_has_id_and_outfitting_cannot_duplicate_items() {
        let mut s = crew();
        s.add_actor(3);
        s.outfit_actor(3, Job::Assistant, true);
        assert_eq!(s.position()[1], -3.5);
        assert!(s.inventory.slots[10].is_some());
        let count = s.items.len();
        s.outfit_actor(3, Job::Engineer, true);
        assert_eq!(s.items.len(), count);
        assert!(!s.door_snapshots()[0].allowed);
    }
}

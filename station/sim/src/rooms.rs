//! Incremental connected air regions for the two station decks. No gas simulation.
use super::*;
use serde::{Deserialize, Serialize};
#[path = "../../ss14-rooms-rules/src/port.rs"]
mod rules;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Room {
    pub id: u64,
    pub cells: Vec<Cell>,
    /// Direct openings to vacuum; the whole connected region is exposed.
    pub breaches: Vec<Cell>,
    pub space: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RoomsSnapshot {
    pub revision: u64,
    pub rebuilt_cells: usize,
    pub rooms: Vec<Room>,
}
impl RoomsSnapshot {
    pub fn at(&self, cell: Cell) -> Option<&Room> {
        self.rooms
            .iter()
            .find(|r| r.cells.binary_search(&cell).is_ok())
    }
}
#[derive(Default)]
pub(super) struct Rooms {
    domain: BTreeSet<Cell>,
    bounds: BTreeMap<i32, ([i32; 2], [i32; 2])>,
    owner: BTreeMap<Cell, u64>,
    components: BTreeMap<u64, Room>,
    next_id: u64,
    snapshot: Arc<RoomsSnapshot>,
}
/// Both voxel layers must seal the column. Glass transmits light, not air.
fn sealed(world: &World, c: Cell) -> bool {
    world.blocks_movement([c[0], c[1], c[2] + 1]) && world.blocks_movement([c[0], c[1], c[2] + 2])
}
fn cardinal(c: Cell) -> [Cell; 4] {
    [
        [c[0] - 1, c[1], c[2]],
        [c[0] + 1, c[1], c[2]],
        [c[0], c[1] - 1, c[2]],
        [c[0], c[1] + 1, c[2]],
    ]
}
impl Rooms {
    fn expand(&mut self, floors: impl Iterator<Item = Cell>) -> BTreeSet<Cell> {
        let mut changed = BTreeSet::new();
        for c in floors.filter(|c| [0, -4].contains(&c[2])) {
            let b = self
                .bounds
                .entry(c[2])
                .or_insert(([c[0], c[1]], [c[0], c[1]]));
            for (axis, v) in c.iter().enumerate().take(2) {
                b.0[axis] = b.0[axis].min(*v);
                b.1[axis] = b.1[axis].max(*v);
            }
        }
        for (&z, &(min, max)) in &self.bounds {
            for x in min[0]..=max[0] {
                for y in min[1]..=max[1] {
                    if self.domain.insert([x, y, z]) {
                        changed.insert([x, y, z]);
                    }
                }
            }
        }
        changed
    }
    fn neighbors(&self, world: &World, c: Cell) -> Vec<Cell> {
        let mut n = cardinal(c).to_vec();
        if !world.solid(c) {
            n.push([c[0], c[1], c[2] - 4]);
        }
        let above = [c[0], c[1], c[2] + 4];
        if self.domain.contains(&above) && !world.solid(above) {
            n.push(above);
        }
        n
    }
    pub(super) fn initialize(&mut self, world: &World) {
        let all = self.expand(world.cells());
        self.rebuild(world, all);
    }
    pub(super) fn update(&mut self, world: &World, edits: &[Cell]) {
        let mut seeds = BTreeSet::new();
        for p in edits {
            for deck in [0, -4] {
                if (deck..=deck + 2).contains(&p[2]) {
                    seeds.insert([p[0], p[1], deck]);
                }
            }
        }
        if seeds.is_empty() {
            return;
        }
        // Most wall edits do not change the domain. Only actual floor additions can expand it.
        if edits
            .iter()
            .any(|c| [0, -4].contains(&c[2]) && world.solid(*c) && !self.domain.contains(c))
        {
            seeds.extend(self.expand(edits.iter().copied().filter(|c| world.solid(*c))));
        }
        let affected: BTreeSet<_> = seeds
            .iter()
            .flat_map(|c| std::iter::once(*c).chain(self.neighbors(world, *c)))
            .filter_map(|c| self.owner.get(&c).copied())
            .collect();
        for id in affected {
            if let Some(room) = self.components.remove(&id) {
                for c in room.cells {
                    self.owner.remove(&c);
                    seeds.insert(c);
                }
            }
        }
        self.rebuild(world, seeds);
    }
    fn rebuild(&mut self, world: &World, mut pending: BTreeSet<Cell>) {
        let mut rebuilt = 0;
        while let Some(seed) = pending.pop_first() {
            if !self.domain.contains(&seed) || sealed(world, seed) || self.owner.contains_key(&seed)
            {
                continue;
            }
            self.next_id += 1;
            let id = self.next_id;
            let mut queue = vec![seed];
            let mut cells = BTreeSet::new();
            let mut breaches = BTreeSet::new();
            while let Some(c) = queue.pop() {
                if self.owner.contains_key(&c) || sealed(world, c) {
                    continue;
                }
                self.owner.insert(c, id);
                pending.remove(&c);
                cells.insert(c);
                rebuilt += 1;
                for n in self.neighbors(world, c) {
                    if !rules::edge_open(sealed(world, c), sealed(world, n)).unwrap_or(false) {
                        continue;
                    }
                    if !self.domain.contains(&n) {
                        if rules::default_space(false, false).unwrap_or(false) {
                            breaches.insert(c);
                        }
                    } else if !self.owner.contains_key(&n) {
                        queue.push(n);
                    }
                }
            }
            self.components.insert(
                id,
                Room {
                    id,
                    cells: cells.into_iter().collect(),
                    space: !breaches.is_empty(),
                    breaches: breaches.into_iter().collect(),
                },
            );
        }
        self.snapshot = Arc::new(RoomsSnapshot {
            revision: self.snapshot.revision + 1,
            rebuilt_cells: rebuilt,
            rooms: self.components.values().cloned().collect(),
        });
    }
}
impl Simulation {
    pub fn rooms(&self) -> Arc<RoomsSnapshot> {
        self.rooms.snapshot.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        content::{Content, DoorDef, Fill},
        floor::{FloorKind, FloorTile},
        interaction::Command,
    };
    fn fixture(glass: bool) -> Arc<Content> {
        let mut c = (*Content::builtin()).clone();
        c.map.asteroid = false;
        c.map.power = None;
        c.map.departments.clear();
        c.map.floors.clear();
        c.map.barriers.clear();
        c.map.base.clear();
        for deck in [0, -4] {
            c.map.base.push(Fill {
                min: [-4, -3, deck],
                max: [4, 3, deck],
            });
            for y in [-3, 3] {
                c.map.base.push(Fill {
                    min: [-4, y, deck + 1],
                    max: [4, y, deck + 2],
                });
            }
            for x in [-4, 4] {
                c.map.base.push(Fill {
                    min: [x, -2, deck + 1],
                    max: [x, 2, deck + 2],
                });
            }
        }
        for y in [-2, -1, 1, 2] {
            c.map.base.push(Fill {
                min: [0, y, 1],
                max: [0, y, 2],
            });
        }
        c.map.doors = vec![DoorDef {
            cell: [0, 0, 1],
            access: 0,
            name: "door.title".into(),
            glass,
            horizontal: false,
        }];
        c.map.crate_round_position = [2., 2., 1.4];
        Arc::new(c)
    }
    fn simulation(glass: bool) -> Simulation {
        let mut s = Simulation::with_content(7, fixture(glass));
        s.prepare_departments();
        s.bodies[s.player].set_translation(vector![-0.5, 0.5, 1.3], true);
        s
    }
    #[test]
    fn door_phases_merge_split_and_glass_remains_airtight() {
        for glass in [false, true] {
            let mut s = simulation(glass);
            let closed = s.rooms();
            let left = closed.at([-2, 0, 0]).unwrap();
            let right = closed.at([2, 0, 0]).unwrap();
            assert_ne!(left.id, right.id);
            assert!(!left.space && !right.space);
            s.command(Command::Door(0));
            for _ in 0..crate::crew::DOOR_TICKS - 1 {
                s.tick_doors();
            }
            assert!(Arc::ptr_eq(&closed, &s.rooms()));
            s.tick_doors();
            let open = s.rooms();
            assert_eq!(
                open.at([-2, 0, 0]).unwrap().id,
                open.at([2, 0, 0]).unwrap().id
            );
            s.command(Command::Door(0));
            for _ in 0..crate::crew::DOOR_TICKS {
                s.tick_doors();
            }
            let closed = s.rooms();
            assert_ne!(
                closed.at([-2, 0, 0]).unwrap().id,
                closed.at([2, 0, 0]).unwrap().id
            );
        }
    }
    #[test]
    fn wall_breach_repair_rebuilds_only_touching_component() {
        let mut s = simulation(false);
        let initial = s.rooms();
        let right = initial.at([2, 0, 0]).unwrap().id;
        let lower = initial.at([-2, 0, -4]).unwrap().id;
        s.set_voxel([-4, 0, 2], false);
        let breached = s.rooms();
        assert!(breached.at([-2, 0, 0]).unwrap().space);
        assert!(
            breached
                .at([-2, 0, 0])
                .unwrap()
                .breaches
                .contains(&[-4, 0, 0])
        );
        assert_eq!(breached.at([2, 0, 0]).unwrap().id, right);
        assert_eq!(breached.at([-2, 0, -4]).unwrap().id, lower);
        assert_eq!(breached.rebuilt_cells, 16);
        s.set_voxel([-4, 0, 2], true);
        assert!(!s.rooms().at([-2, 0, 0]).unwrap().space);
        assert_eq!(s.rooms().at([2, 0, 0]).unwrap().id, right);
    }
    #[test]
    fn surface_removal_is_not_a_breach_and_holes_connect_decks_before_space() {
        let mut s = simulation(false);
        let initial = s.rooms();
        s.set_floor(FloorTile {
            cell: [-2, 0, 0],
            kind: FloorKind::Plating,
            variant: None,
        })
        .unwrap();
        assert!(Arc::ptr_eq(&initial, &s.rooms()));
        s.set_voxel([-2, 0, 0], false);
        let linked = s.rooms();
        assert_eq!(
            linked.at([-2, 0, 0]).unwrap().id,
            linked.at([-2, 0, -4]).unwrap().id
        );
        assert!(!linked.at([-2, 0, 0]).unwrap().space);
        s.set_voxel([-2, 0, -4], false);
        assert!(s.rooms().at([-2, 0, 0]).unwrap().space);
        s.set_voxel([-2, 0, 0], true);
        let split = s.rooms();
        assert!(!split.at([-2, 0, 0]).unwrap().space);
        assert!(split.at([-2, 0, -4]).unwrap().space);
        s.set_voxel([-2, 0, -4], true);
        assert!(!s.rooms().at([-2, 0, -4]).unwrap().space);
        let fresh = simulation(false);
        assert!(!fresh.rooms().at([-2, 0, 0]).unwrap().space);
    }
    #[test]
    fn incremental_graph_matches_full_rebuild_after_mixed_edits() {
        let mut s = simulation(false);
        for (cell, solid) in [
            ([-4, 0, 1], false),
            ([0, 0, 1], false),
            ([0, 0, 2], false),
            ([-2, 0, 0], false),
            ([-2, 0, -4], false),
            ([0, 0, 1], true),
            ([0, 0, 2], true),
            ([-4, 0, 1], true),
            ([-2, 0, 0], true),
            ([5, 0, 0], true),
        ] {
            s.set_voxel(cell, solid);
            let mut full = Rooms {
                domain: s.rooms.domain.clone(),
                bounds: s.rooms.bounds.clone(),
                ..Default::default()
            };
            full.rebuild(&s.world, full.domain.clone());
            let shape = |snap: &RoomsSnapshot| {
                let mut r: Vec<_> = snap
                    .rooms
                    .iter()
                    .map(|r| (r.cells.clone(), r.breaches.clone(), r.space))
                    .collect();
                r.sort();
                r
            };
            assert_eq!(
                shape(&s.rooms()),
                shape(&full.snapshot),
                "edit {cell:?}={solid}"
            );
        }
    }
}

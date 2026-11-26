//! Binary power graph; SS14 guards verified through rust-anything.
use super::*;
use serde::{Deserialize, Serialize};
#[path = "../../ss14-power-rules/src/port.rs"]
pub(crate) mod rules;
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PowerDef {
    pub cables: Vec<Cell>,
    pub sources: Vec<PowerNode>,
    pub controllers: Vec<PowerNode>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerNode {
    pub cell: Cell,
    pub enabled: bool,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct NodeSnapshot {
    pub cell: Cell,
    pub enabled: bool,
    pub powered: bool,
    pub reachable: bool,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CableSnapshot {
    pub cell: Cell,
    pub connected: bool,
    pub powered: bool,
    pub exposed: bool,
}
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct PowerSnapshot {
    pub revision: u64,
    pub cables: Vec<CableSnapshot>,
    pub sources: Vec<NodeSnapshot>,
    pub controllers: Vec<NodeSnapshot>,
}
#[derive(Default)]
pub(super) struct Power {
    pub definition: Option<PowerDef>,
    pub(super) disconnected: BTreeSet<Cell>,
    pub(super) live: BTreeSet<Cell>,
    pub dirty: bool,
    pub revision: u64,
}
impl PowerDef {
    pub fn validate(&self) -> Result<(), String> {
        let valid = |c: Cell| {
            c[0].unsigned_abs() <= 128 && c[1].unsigned_abs() <= 128 && [0, -4].contains(&c[2])
        };
        if self.cables.len() > 4096
            || self.sources.len() > 32
            || self.controllers.len() > 64
            || !self.cables.iter().copied().all(valid)
            || !self
                .sources
                .iter()
                .chain(&self.controllers)
                .all(|n| valid(n.cell))
            || self.cables.iter().collect::<BTreeSet<_>>().len() != self.cables.len()
            || self
                .sources
                .iter()
                .map(|n| n.cell)
                .collect::<BTreeSet<_>>()
                .len()
                != self.sources.len()
            || self
                .controllers
                .iter()
                .map(|n| n.cell)
                .collect::<BTreeSet<_>>()
                .len()
                != self.controllers.len()
        {
            return Err("invalid power network".into());
        }
        Ok(())
    }
}
impl Simulation {
    pub(super) fn refresh_power(&mut self) {
        if !self.power.dirty {
            return;
        }
        self.power.dirty = false;
        self.power.live.clear();
        let Some(def) = &self.power.definition else {
            return;
        };
        let cables: BTreeSet<_> = def
            .cables
            .iter()
            .copied()
            .filter(|c| !self.power.disconnected.contains(c) && self.world.solid(*c))
            .collect();
        let mut pending: Vec<_> = def
            .sources
            .iter()
            .filter(|n| n.enabled && cables.contains(&n.cell))
            .map(|n| n.cell)
            .collect();
        while let Some(c) = pending.pop() {
            if !self.power.live.insert(c) {
                continue;
            }
            for [x, y] in [[-1, 0], [1, 0], [0, -1], [0, 1]] {
                let next = [c[0] + x, c[1] + y, c[2]];
                if rules::cable_reachable(true, true, cables.contains(&next), false, false)
                    .unwrap_or(false)
                    && !self.power.live.contains(&next)
                {
                    pending.push(next);
                }
            }
        }
    }
    /// A receiver belongs to the nearest controller within eight tiles, on its deck.
    /// Assignment is independent of power, so outages cannot steal a nearby APC's supply.
    pub fn device_powered(&self, cell: Cell) -> bool {
        let Some(def) = &self.power.definition else {
            return true;
        };
        let nearest = def
            .controllers
            .iter()
            .enumerate()
            .filter_map(|(id, n)| {
                let d = (n.cell[0] - cell[0]).pow(2) + (n.cell[1] - cell[1]).pow(2);
                (n.cell[2] == cell[2] && d <= 64).then_some((d, id, n))
            })
            .min_by_key(|(d, id, _)| (*d, *id));
        let supplied =
            nearest.is_some_and(|(_, _, n)| n.enabled && self.power.live.contains(&n.cell));
        rules::receiver_powered(false, true, true, supplied).unwrap_or(false)
    }
    pub fn powered_lamps(&self) -> Vec<content::Lamp> {
        self.content
            .map
            .lamps
            .iter()
            .cloned()
            .map(|mut lamp| {
                let cell = [
                    lamp.position[0].floor() as i32,
                    lamp.position[1].floor() as i32,
                    if lamp.position[2] >= 0.5 { 0 } else { -4 },
                ];
                if !self.device_powered(cell) {
                    lamp.energy = 0.;
                }
                lamp
            })
            .collect()
    }
    pub fn power_snapshot(&self) -> PowerSnapshot {
        let Some(def) = &self.power.definition else {
            return PowerSnapshot::default();
        };
        let nodes = |ns: &[PowerNode]| {
            ns.iter()
                .map(|n| NodeSnapshot {
                    cell: n.cell,
                    enabled: n.enabled,
                    powered: n.enabled && self.power.live.contains(&n.cell),
                    reachable: self.tile_reachable(n.cell),
                })
                .collect()
        };
        PowerSnapshot {
            revision: self.power.revision,
            cables: def
                .cables
                .iter()
                .map(|c| CableSnapshot {
                    cell: *c,
                    connected: !self.power.disconnected.contains(c) && self.world.solid(*c),
                    powered: self.power.live.contains(c),
                    exposed: self.floor_kind(*c) == Some(floor::FloorKind::Plating),
                })
                .collect(),
            sources: nodes(&def.sources),
            controllers: nodes(&def.controllers),
        }
    }
    pub(super) fn power_switch(&mut self, source: bool, id: usize) {
        let node = self.power.definition.as_ref().and_then(|d| {
            if source {
                d.sources.get(id)
            } else {
                d.controllers.get(id)
            }
        });
        let Some(n) = node else {
            self.inventory.message = "error.action".into();
            return;
        };
        if !self.tile_reachable(n.cell) {
            self.inventory.message = "error.reach".into();
            return;
        }
        let def = self.power.definition.as_mut().unwrap();
        let n = if source {
            &mut def.sources[id]
        } else {
            &mut def.controllers[id]
        };
        n.enabled = !n.enabled;
        self.power.revision += 1;
        self.power.dirty = true;
        self.refresh_power();
    }
    pub(super) fn cable_work_valid(&self, c: Cell, tool: usize) -> bool {
        self.held == Some(tool)
            && self
                .items
                .get(tool)
                .is_some_and(|i| i.alive && i.kind == ItemKind::Wirecutters)
            && self.floor_kind(c) == Some(floor::FloorKind::Plating)
            && self.tile_reachable(c)
            && self
                .power
                .definition
                .as_ref()
                .is_some_and(|d| d.cables.contains(&c))
    }
    pub(super) fn toggle_cable(&mut self, c: Cell) {
        if !self.power.disconnected.remove(&c) {
            self.power.disconnected.insert(c);
        }
        self.power.revision += 1;
        self.power.dirty = true;
        self.refresh_power();
        self.inventory.message = "power.cable_done".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actions::WorkTarget,
        crew::{DoorPhase, Job},
        floor::{FloorKind, FloorTile},
        interaction::Command,
    };
    fn fixture() -> Simulation {
        let mut c = (*content::Content::builtin()).clone();
        c.map.crate_round_position = [6., -4., 1.4];
        c.map.power = Some(PowerDef {
            cables: vec![[-6, 2, 0], [-5, 2, 0], [-4, 2, 0], [5, 2, 0]],
            sources: vec![
                PowerNode {
                    cell: [-6, 2, 0],
                    enabled: true,
                },
                PowerNode {
                    cell: [5, 2, 0],
                    enabled: true,
                },
            ],
            controllers: vec![
                PowerNode {
                    cell: [-4, 2, 0],
                    enabled: true,
                },
                PowerNode {
                    cell: [5, 2, 0],
                    enabled: true,
                },
            ],
        });
        c.kits
            .get_mut("job.engineer")
            .unwrap()
            .push(content::KitItem {
                prototype: "wirecutters".into(),
                slot: 8,
            });
        let mut s = Simulation::with_content(7, Arc::new(c));
        s.prepare_departments();
        s.outfit_actor(0, Job::Engineer, false);
        for _ in 0..120 {
            s.step([0.; 2]);
        }
        s
    }
    fn run(s: &mut Simulation, n: usize) {
        for _ in 0..n {
            s.step([0.; 2]);
        }
    }
    #[test]
    fn cable_outage_pry_and_repair_preserve_other_room_and_deck() {
        let mut s = fixture();
        let c = [-5, 2, 0];
        s.command(Command::Slot(8));
        let cutters = s.held.unwrap();
        s.begin_work(WorkTarget::Cable(c), cutters);
        assert!(s.action().is_none());
        s.set_floor(FloorTile {
            cell: c,
            kind: FloorKind::Plating,
            variant: None,
        })
        .unwrap();
        s.begin_work(WorkTarget::Cable(c), cutters);
        run(&mut s, 59);
        assert!(s.door_snapshots()[0].powered);
        run(&mut s, 1);
        assert!(!s.door_snapshots()[0].powered);
        assert!(s.door_snapshots()[1].powered);
        assert!(s.world.solid(c));
        assert!(
            s.power_snapshot()
                .cables
                .iter()
                .any(|n| n.cell == c && !n.connected)
        );
        s.command(Command::Door(0));
        assert_eq!(s.inventory.message, "power.no_power");
        s.command(Command::Slot(8));
        s.command(Command::Slot(11));
        let crowbar = s.held.unwrap();
        s.begin_work(WorkTarget::Door(0), crowbar);
        run(&mut s, 60);
        // Power restoration interrupts the mechanical action.
        s.toggle_cable(c);
        run(&mut s, 1);
        assert!(s.action().is_none());
        assert_eq!(s.door_snapshots()[0].phase, DoorPhase::Closed);
        s.toggle_cable(c);
        s.begin_work(WorkTarget::Door(0), crowbar);
        run(&mut s, 150);
        assert!(s.door_snapshots()[0].open);
        assert!(!s.world.blocks_movement([-3, 2, 1]));
        s.command(Command::Slot(11));
        s.command(Command::Slot(8));
        s.begin_work(WorkTarget::Cable(c), cutters);
        run(&mut s, 60);
        assert!(s.door_snapshots()[0].powered);
        s.command(Command::Slot(8)); // Clear the held tool from the doorway before closing.
        s.command(Command::Door(0));
        run(&mut s, 30);
        assert!(
            !s.door_snapshots()[0].open,
            "{:?} {} {:?}",
            s.door_snapshots()[0],
            s.inventory.message,
            s.position()
        );
    }
    #[test]
    fn topology_handles_alternate_routes_decks_missing_floor_and_restart() {
        let mut s = fixture();
        s.toggle_cable([-5, 2, 0]);
        assert!(!s.device_powered([-4, 2, 0]));
        s.power
            .definition
            .as_mut()
            .unwrap()
            .cables
            .extend([[-6, 3, 0], [-5, 3, 0], [-4, 3, 0]]);
        s.power.dirty = true;
        s.refresh_power();
        assert!(s.device_powered([-4, 2, 0]));
        s.set_voxel([-5, 3, 0], false);
        s.refresh_power();
        assert!(!s.device_powered([-4, 2, 0]));
        assert!(!s.device_powered([-4, 2, -4]));
        assert!(s.device_powered([5, 2, 0]));
        let fresh = Simulation::with_content(7, s.content());
        assert!(fresh.device_powered([-4, 2, 0]));
    }
    #[test]
    fn competing_cable_work_and_floor_cover_cancel_without_double_toggle() {
        let mut s = fixture();
        let c = [-5, 2, 0];
        s.command(Command::Slot(8));
        let tool = s.held.unwrap();
        s.set_floor(FloorTile {
            cell: c,
            kind: FloorKind::Plating,
            variant: None,
        })
        .unwrap();
        s.begin_work(WorkTarget::Cable(c), tool);
        run(&mut s, 20);
        s.toggle_cable(c);
        run(&mut s, 80);
        assert!(s.action().is_none());
        assert!(!s.device_powered([-4, 2, 0]));
        s.begin_work(WorkTarget::Cable(c), tool);
        run(&mut s, 20);
        s.set_floor(FloorTile {
            cell: c,
            kind: FloorKind::Steel,
            variant: None,
        })
        .unwrap();
        run(&mut s, 80);
        assert!(!s.device_powered([-4, 2, 0]));
    }
}

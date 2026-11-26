//! Offline map-template editing. Never mutates a running server or round.
use crate::{
    Cell, World,
    content::{Content, DoorDef, Fill, Lamp, Placement},
};
use std::{collections::BTreeSet, path::Path, sync::Arc};
#[path = "../../ss14-editor-rules/src/port.rs"]
mod placement_rules;

#[derive(Clone)]
pub enum Tool {
    Floor,
    Cable,
    EraseCable,
    PowerSource,
    PowerController,
    ErasePowerNode,
    PaintFloor {
        kind: crate::floor::FloorKind,
        variant: Option<u8>,
    },
    Wall,
    Window,
    EraseFloor,
    EraseWall,
    Door {
        glass: bool,
        horizontal: bool,
        access: u8,
        name: String,
    },
    Item {
        prototype: String,
        key: String,
    },
    EraseItem,
    Lamp {
        radius: f32,
        color: [f32; 3],
        energy: f32,
    },
    EraseLamp,
    Spawn(String),
}
pub struct Editor {
    content: Content,
    undo: Vec<Content>,
    redo: Vec<Content>,
    saved: String,
    pub revision: u64,
}
fn encoded(c: &Content) -> Result<String, String> {
    let text = serde_json::to_string(c).map_err(|e| e.to_string())?;
    if text.len() > 1024 * 1024 {
        return Err("editor.too_large".into());
    }
    Content::parse(&text)?;
    Ok(text)
}
fn boxes(cells: BTreeSet<Cell>) -> Vec<Fill> {
    let mut result: Vec<Fill> = Vec::new();
    for c in cells {
        if let Some(last) = result.last_mut()
            && last.max[0] == c[0]
            && last.max[1] == c[1]
            && last.max[2] + 1 == c[2]
        {
            last.max = c;
        } else {
            result.push(Fill { min: c, max: c });
        }
    }
    result
}
impl Editor {
    pub fn new(content: Arc<Content>) -> Self {
        Self {
            saved: encoded(&content).expect("validated content"),
            content: (*content).clone(),
            undo: vec![],
            redo: vec![],
            revision: 0,
        }
    }
    pub fn content(&self) -> &Content {
        &self.content
    }
    pub fn dirty(&self) -> bool {
        serde_json::to_string(&self.content).is_ok_and(|v| v != self.saved)
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn cells(&self) -> Arc<Vec<Cell>> {
        let mut w = World::from_template(7, &self.content.map);
        for b in &self.content.map.departments {
            for c in b.cells() {
                w.set(c, true);
            }
        }
        for d in &self.content.map.doors {
            w.set(d.cell, true);
            w.set([d.cell[0], d.cell[1], d.cell[2] + 1], true);
        }
        Arc::new(w.cells().collect())
    }
    pub fn undo(&mut self) {
        if let Some(c) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.content, c));
            self.revision += 1;
        }
    }
    pub fn redo(&mut self) {
        if let Some(c) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.content, c));
            self.revision += 1;
        }
    }
    pub fn load(&mut self, path: impl AsRef<Path>) -> Result<(), String> {
        let c = Content::load(path)?;
        let revision = self.revision + 1;
        *self = Self::new(c);
        self.revision = revision;
        Ok(())
    }
    pub fn save(&mut self, path: impl AsRef<Path>) -> Result<(), String> {
        use std::io::Write;
        let text = encoded(&self.content)?;
        let path = path.as_ref();
        // Exclusive temporary file; never truncate the destination on validation/write failure.
        let mut temp_name = path.as_os_str().to_os_string();
        temp_name.push(format!(".{}.tmp", std::process::id()));
        let temp = std::path::PathBuf::from(temp_name);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        let result = (|| {
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temp, path)
        })();
        if let Err(e) = result {
            let _ = std::fs::remove_file(&temp);
            return Err(e.to_string());
        }
        self.saved = text;
        Ok(())
    }
    /// Integral tile centers; SS14's zero-width rectangle remains empty.
    /// A normal single click uses `apply`, not a degenerate rectangle.
    pub fn rectangle(&mut self, start: Cell, end: Cell, tool: Tool) -> Result<(), String> {
        if start[2] != end[2] {
            return Err("editor.bounds".into());
        }
        check_cell(start)?;
        check_cell(end)?;
        if !matches!(
            tool,
            Tool::Floor
                | Tool::PaintFloor { .. }
                | Tool::Wall
                | Tool::Window
                | Tool::EraseFloor
                | Tool::EraseWall
        ) {
            return Err("editor.geometry_only".into());
        }
        let dx = end[0] - start[0];
        let dy = end[1] - start[1];
        let nx = placement_rules::axis_count(i64::from(dx))? as i32;
        let ny = placement_rules::axis_count(i64::from(dy))? as i32;
        if nx * ny > 4096 {
            return Err("editor.too_large".into());
        }
        let cells: Vec<_> = (0..nx)
            .flat_map(|x| {
                (0..ny).map(move |y| {
                    [
                        start[0] + x * dx.signum(),
                        start[1] + y * dy.signum(),
                        start[2],
                    ]
                })
            })
            .collect();
        self.apply(&cells, tool)
    }
    pub fn apply(&mut self, cells: &[Cell], tool: Tool) -> Result<(), String> {
        if cells.len() > 4096 {
            return Err("editor.too_large".into());
        }
        for &c in cells {
            check_cell(c)?;
        }
        if cells.is_empty() {
            return Ok(());
        }
        let mut next = self.content.clone();
        let mut geometry: BTreeSet<Cell> = World::from_template(7, &next.map).cells().collect();
        for b in &next.map.departments {
            geometry.extend(b.cells());
        }
        for &c in cells {
            let upper = [c[0], c[1], c[2] + 1];
            let top = [c[0], c[1], c[2] + 2];
            let pos = [c[0] as f32 + 0.5, c[1] as f32 + 0.5, c[2] as f32 + 3.];
            match &tool {
                Tool::Cable
                | Tool::EraseCable
                | Tool::PowerSource
                | Tool::PowerController
                | Tool::ErasePowerNode => {
                    if !geometry.contains(&c) {
                        return Err("editor.floor_missing".into());
                    }
                    let power = next.map.power.get_or_insert_with(Default::default);
                    match tool {
                        Tool::Cable => {
                            if !power.cables.contains(&c) {
                                power.cables.push(c);
                            }
                        }
                        Tool::EraseCable => power.cables.retain(|v| *v != c),
                        Tool::PowerSource | Tool::PowerController => {
                            let nodes = if matches!(tool, Tool::PowerSource) {
                                &mut power.sources
                            } else {
                                &mut power.controllers
                            };
                            if !nodes.iter().any(|n| n.cell == c) {
                                nodes.push(crate::power::PowerNode {
                                    cell: c,
                                    enabled: true,
                                });
                            }
                        }
                        Tool::ErasePowerNode => {
                            power.sources.retain(|n| n.cell != c);
                            power.controllers.retain(|n| n.cell != c);
                        }
                        _ => unreachable!(),
                    }
                }
                Tool::Floor => {
                    geometry.insert(c);
                }
                Tool::PaintFloor { kind, variant } => {
                    let tile = crate::floor::FloorTile {
                        cell: c,
                        kind: *kind,
                        variant: *variant,
                    };
                    if !tile.valid() || !geometry.contains(&c) {
                        return Err("editor.floor_missing".into());
                    }
                    next.map.floors.retain(|f| f.cell != c);
                    next.map.floors.push(tile);
                }
                Tool::EraseFloor => {
                    next.map.floors.retain(|f| f.cell != c);
                    geometry.remove(&c);
                }
                Tool::Wall | Tool::Window | Tool::EraseWall | Tool::Door { .. } => {
                    next.map
                        .barriers
                        .retain(|b| b.cell != upper && b.cell != top);
                    next.map.doors.retain(|d| d.cell != upper);
                    geometry.remove(&upper);
                    geometry.remove(&top);
                    match &tool {
                        Tool::Wall | Tool::Window => {
                            geometry.insert(upper);
                            geometry.insert(top);
                            if matches!(tool, Tool::Window) {
                                for cell in [upper, top] {
                                    next.map.barriers.push(crate::barrier::Barrier {
                                        cell,
                                        blocks_movement: true,
                                        blocks_sight: false,
                                        blocks_light: false,
                                    });
                                }
                            }
                        }
                        Tool::Door {
                            access,
                            name,
                            glass,
                            horizontal,
                        } => next.map.doors.push(DoorDef {
                            glass: *glass,
                            horizontal: *horizontal,
                            cell: upper,
                            access: *access,
                            name: name.clone(),
                        }),
                        _ => {}
                    }
                }
                Tool::Item { prototype, key } => {
                    if !next.prototypes.contains_key(prototype) {
                        return Err("editor.prototype_missing".into());
                    }
                    let key = if key.is_empty() {
                        (0..=512)
                            .map(|i| format!("placed.{i}"))
                            .find(|k| !next.map.items.iter().any(|p| &p.key == k))
                            .ok_or("editor.too_large")?
                    } else {
                        key.clone()
                    };
                    if let Some(p) = next.map.items.iter_mut().find(|p| p.key == key) {
                        p.position = pos; // Moving a required alias preserves its prototype and kind.
                    } else {
                        next.map.items.push(Placement {
                            key: key.clone(),
                            prototype: prototype.clone(),
                            position: pos,
                        });
                    }
                    if key == "crate" {
                        next.map.crate_round_position = pos;
                    }
                }
                Tool::EraseItem => {
                    let hit = |p: &Placement| {
                        let p = if p.key == "crate" {
                            next.map.crate_round_position
                        } else {
                            p.position
                        };
                        p[0].floor() as i32 == c[0]
                            && p[1].floor() as i32 == c[1]
                            && (p[2] >= 0.5) == (c[2] == 0)
                    };
                    if next.map.items.iter().any(|p| {
                        hit(p)
                            && ["flashlight", "crate", "helmet", "crowbar"]
                                .contains(&p.key.as_str())
                    }) {
                        return Err("editor.required_item".into());
                    }
                    next.map.items.retain(|p| !hit(p));
                }
                Tool::Lamp {
                    radius,
                    color,
                    energy,
                } => {
                    next.map.lamps.retain(|l| {
                        !(l.position[0].floor() as i32 == c[0]
                            && l.position[1].floor() as i32 == c[1]
                            && (l.position[2] >= 0.5) == (c[2] == 0))
                    });
                    next.map.lamps.push(Lamp {
                        position: pos,
                        radius: *radius,
                        color: *color,
                        energy: *energy,
                    });
                }
                Tool::EraseLamp => next.map.lamps.retain(|l| {
                    !(l.position[0].floor() as i32 == c[0]
                        && l.position[1].floor() as i32 == c[1]
                        && (l.position[2] >= 0.5) == (c[2] == 0))
                }),
                Tool::Spawn(key) => {
                    let p = next.map.spawns.get_mut(key).ok_or("editor.spawn_missing")?;
                    *p = pos;
                }
            }
        }
        if matches!(
            tool,
            Tool::Floor
                | Tool::Wall
                | Tool::Window
                | Tool::EraseFloor
                | Tool::EraseWall
                | Tool::Door { .. }
        ) {
            next.map.base = boxes(geometry);
            next.map.departments.clear();
            next.map.asteroid = false;
        }
        if encoded(&next)? == encoded(&self.content)? {
            return Ok(());
        }
        self.undo.push(std::mem::replace(&mut self.content, next));
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.revision += 1;
        Ok(())
    }
}
fn check_cell(c: Cell) -> Result<(), String> {
    if c[0].unsigned_abs() > 64 || c[1].unsigned_abs() > 64 || ![0, -4].contains(&c[2]) {
        Err("editor.bounds".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rooms_follow_wall_edit_undo_redo_and_serialized_template() {
        let mut e =
            Editor::new(Content::parse(include_str!("../../content/rooms-lab.json")).unwrap());
        let exposed = |e: &Editor| {
            let mut sim = crate::Simulation::with_content(
                7,
                Content::parse(&serde_json::to_string(e.content()).unwrap()).unwrap(),
            );
            sim.prepare_departments();
            sim.rooms().at([-2, 0, 0]).unwrap().space
        };
        assert!(!exposed(&e));
        e.apply(&[[-4, 0, 0]], Tool::EraseWall).unwrap();
        assert!(exposed(&e));
        e.undo();
        assert!(!exposed(&e));
        e.redo();
        assert!(exposed(&e));
        e.apply(&[[-4, 0, 0]], Tool::Window).unwrap();
        assert!(!exposed(&e));
    }
    #[test]
    fn power_tools_roundtrip_undo_and_feed_receivers() {
        let mut e = Editor::new(Content::builtin());
        e.apply(&[[0, 0, 0], [1, 0, 0], [2, 0, 0]], Tool::Cable)
            .unwrap();
        e.apply(&[[0, 0, 0]], Tool::PowerSource).unwrap();
        e.apply(&[[2, 0, 0]], Tool::PowerController).unwrap();
        let build = |e: &Editor| {
            crate::Simulation::with_content(
                7,
                Content::parse(&serde_json::to_string(e.content()).unwrap()).unwrap(),
            )
        };
        assert!(build(&e).device_powered([3, 0, 0]));
        e.apply(&[[1, 0, 0]], Tool::EraseCable).unwrap();
        assert!(!build(&e).device_powered([3, 0, 0]));
        e.undo();
        assert!(build(&e).device_powered([3, 0, 0]));
        e.redo();
        assert!(!build(&e).device_powered([3, 0, 0]));
    }
    #[test]
    fn painted_rectangle_roundtrips_without_geometry_changes_and_undoes() {
        let mut e = Editor::new(Content::builtin());
        let cells = e.cells();
        e.rectangle(
            [-2, -2, 0],
            [1, 1, 0],
            Tool::PaintFloor {
                kind: crate::floor::FloorKind::Damaged,
                variant: None,
            },
        )
        .unwrap();
        assert_eq!(e.cells(), cells);
        assert_eq!(e.content.map.floors.len(), 16);
        let parsed = Content::parse(&encoded(e.content()).unwrap()).unwrap();
        assert_eq!(parsed.map.floors, e.content.map.floors);
        let frames: Vec<_> = e.content.map.floors.iter().map(|t| t.frame()).collect();
        e.undo();
        assert!(e.content.map.floors.is_empty());
        e.redo();
        assert_eq!(
            frames,
            e.content
                .map
                .floors
                .iter()
                .map(|t| t.frame())
                .collect::<Vec<_>>()
        );
        e.apply(&[[0, 0, 0]], Tool::EraseFloor).unwrap();
        assert!(!e.content.map.floors.iter().any(|t| t.cell == [0, 0, 0]));
        assert!(
            e.apply(
                &[[0, 0, 0]],
                Tool::PaintFloor {
                    kind: crate::floor::FloorKind::Steel,
                    variant: None
                }
            )
            .is_err()
        );
    }
    #[test]
    fn windows_save_restore_and_replace_with_normal_walls() {
        let mut e = Editor::new(Content::test_station());
        e.apply(&[[0, -4, 0]], Tool::Window).unwrap();
        let saved = encoded(e.content()).unwrap();
        assert!(
            Content::parse(&saved)
                .unwrap()
                .map
                .barriers
                .iter()
                .any(|b| b.cell == [0, -4, 1] && !b.blocks_sight)
        );
        e.apply(&[[0, -4, 0]], Tool::Wall).unwrap();
        assert!(
            !e.content()
                .map
                .barriers
                .iter()
                .any(|b| b.cell == [0, -4, 1])
        );
        e.undo();
        assert_eq!(encoded(e.content()).unwrap(), saved);
    }
    #[test]
    fn lamps_roundtrip_replace_undo_delete_and_reject_invalid_values() {
        let mut e = Editor::new(Content::builtin());
        let baseline = e.content().map.lamps.len();
        let cell = [2, -2, -4];
        e.apply(
            &[cell],
            Tool::Lamp {
                radius: 8.,
                color: [0.2, 0.4, 1.],
                energy: 3.,
            },
        )
        .unwrap();
        assert_eq!(e.content().map.lamps.len(), baseline + 1);
        e.apply(
            &[cell],
            Tool::Lamp {
                radius: 4.,
                color: [1., 0., 0.],
                energy: 2.,
            },
        )
        .unwrap();
        assert_eq!(e.content().map.lamps.len(), baseline + 1);
        e.undo();
        assert_eq!(e.content().map.lamps.last().unwrap().radius, 8.);
        let before = encoded(e.content()).unwrap();
        assert!(
            e.apply(
                &[cell],
                Tool::Lamp {
                    radius: f32::NAN,
                    color: [1.; 3],
                    energy: 2.
                }
            )
            .is_err()
        );
        assert_eq!(encoded(e.content()).unwrap(), before);
        let path = std::env::temp_dir().join(format!("station-lights-{}.json", std::process::id()));
        e.save(&path).unwrap();
        let c = Content::load(&path).unwrap();
        assert_eq!(c.map.lamps.last().unwrap().position, [2.5, -1.5, -1.]);
        let sim = crate::Simulation::with_content(7, c);
        assert_eq!(sim.content().map.lamps.len(), baseline + 1);
        e.apply(&[cell], Tool::EraseLamp).unwrap();
        assert_eq!(e.content().map.lamps.len(), baseline);
        e.undo();
        assert_eq!(e.content().map.lamps.len(), baseline + 1);
        std::fs::remove_file(path).unwrap();
        let mut legacy: serde_json::Value =
            serde_json::from_str(include_str!("../../content/station.json")).unwrap();
        legacy["map"].as_object_mut().unwrap().remove("lamps");
        assert!(
            Content::parse(&legacy.to_string())
                .unwrap()
                .map
                .lamps
                .is_empty()
        );
    }
    #[test]
    fn rectangle_is_one_transaction_and_redo_is_invalidated() {
        let mut e = Editor::new(Content::builtin());
        e.rectangle([22, 2, 0], [20, 0, 0], Tool::Floor).unwrap();
        assert!(e.dirty());
        assert!(e.cells().contains(&[21, 1, 0]));
        e.undo();
        assert!(!e.dirty());
        assert!(!e.cells().contains(&[21, 1, 0]));
        e.redo();
        assert!(e.cells().contains(&[21, 1, 0]));
        e.undo();
        e.apply(&[[24, 0, 0]], Tool::Wall).unwrap();
        assert!(!e.can_redo());
        let revision = e.revision;
        e.rectangle([20, 0, 0], [23, 0, 0], Tool::Floor).unwrap();
        assert_eq!(revision, e.revision);
        assert!(e.rectangle([-64, -64, 0], [64, 64, 0], Tool::Wall).is_err());
        assert_eq!(revision, e.revision);
    }
    #[test]
    fn invalid_edit_is_atomic_and_required_aliases_are_movable() {
        let mut e = Editor::new(Content::builtin());
        assert!(e.apply(&[[0, 0, 0], [65, 0, 0]], Tool::Wall).is_err());
        assert!(!e.dirty());
        assert!(e.apply(&[[3, -3, 0]], Tool::EraseItem).is_err());
        assert!(!e.dirty());
        e.apply(
            &[[2, -3, -4]],
            Tool::Item {
                prototype: "crate".into(),
                key: "crate".into(),
            },
        )
        .unwrap();
        assert_eq!(e.content.map.crate_round_position, [2.5, -2.5, -1.]);
        e.apply(
            &[[1, -3, 0]],
            Tool::Item {
                prototype: "medkit".into(),
                key: String::new(),
            },
        )
        .unwrap();
        assert_eq!(e.content.map.items.len(), 5);
        e.apply(&[[1, -3, 0]], Tool::EraseItem).unwrap();
        assert_eq!(e.content.map.items.len(), 4);
    }
    #[test]
    fn saved_template_overwrites_loads_and_builds_fresh_physics() {
        let path = std::env::temp_dir().join(format!("station-editor-{}.json", std::process::id()));
        let mut e = Editor::new(Content::builtin());
        e.apply(&[[-3, 2, 0]], Tool::EraseWall).unwrap();
        e.apply(
            &[[0, 2, -4]],
            Tool::Door {
                glass: true,
                horizontal: true,
                access: 2,
                name: "job.doctor".into(),
            },
        )
        .unwrap();
        e.apply(&[[1, -2, -4]], Tool::Spawn("job.doctor".into()))
            .unwrap();
        e.save(&path).unwrap();
        e.apply(&[[0, -2, -4]], Tool::Wall).unwrap();
        e.save(&path).unwrap();
        assert!(!e.dirty());
        let content = Content::load(&path).unwrap();
        for _ in 0..2 {
            let mut sim = crate::Simulation::with_content(7, content.clone());
            sim.prepare_departments();
            assert!(!sim.world().solid([-3, 2, 1]));
            assert!(sim.world().solid([0, -2, -3]));
            assert!(sim.door_snapshots().iter().any(|d| d.cell == [0, 2, -3]));
            sim.outfit_actor(0, crate::crew::Job::Doctor, false);
            assert_eq!(sim.position(), [1.5, -1.5, -1.]);
            for _ in 0..120 {
                sim.step([0.; 2]);
            }
            assert!((sim.position()[2] + 2.7).abs() < 0.1);
            let door = sim
                .door_snapshots()
                .iter()
                .find(|d| d.cell == [0, 2, -3])
                .unwrap()
                .id;
            sim.bodies[sim.player]
                .set_translation(rapier3d::prelude::Vector::from([1.5, 2.5, -2.7]), true);
            sim.command(crate::interaction::Command::Door(door));
            for _ in 0..crate::crew::DOOR_TICKS {
                sim.tick_doors();
            }
            assert!(
                sim.door_snapshots()
                    .iter()
                    .find(|d| d.id == door)
                    .unwrap()
                    .open
            );
            sim.bodies[sim.player]
                .set_translation(rapier3d::prelude::Vector::from([0.5, 2.5, -2.7]), true);
            sim.command(crate::interaction::Command::Door(door));
            for _ in 0..crate::crew::DOOR_TICKS {
                sim.tick_doors();
            }
            assert!(
                sim.door_snapshots()
                    .iter()
                    .find(|d| d.id == door)
                    .unwrap()
                    .open
            );
            assert_eq!(sim.inventory.message, "error.door_blocked");
        }
        e.apply(&[[0, 0, 0]], Tool::Wall).unwrap();
        e.load(&path).unwrap();
        assert!(!e.dirty());
        assert!(e.load(path.with_extension("missing")).is_err());
        assert!(!e.dirty());
        std::fs::remove_file(path).unwrap();
    }
}

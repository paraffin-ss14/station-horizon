//! Validated, immutable round content. No save data or executable scripts.
use super::*;
use serde::Deserialize;
use std::sync::{Arc, OnceLock};
#[derive(serde::Serialize, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prototype {
    #[serde(default = "one_item")]
    pub stack_count: u16,
    #[serde(default)]
    pub sprite: sprite::SpriteVisual,
    #[serde(default = "default_sprite_size")]
    pub sprite_size: [f32; 2],
    pub kind: ItemKind,
    pub name: String,
    pub description: String,
    pub half: f32,
    pub mass: f32,
    pub clothing: bool,
    pub slots: i64,
    pub size: i64,
    pub color: [f32; 4],
    pub billboard: bool,
    pub capacity: usize,
}
#[derive(serde::Serialize, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fill {
    pub min: Cell,
    pub max: Cell,
}
impl Fill {
    pub fn cells(&self) -> impl Iterator<Item = Cell> + '_ {
        (self.min[0]..=self.max[0]).flat_map(move |x| {
            (self.min[1]..=self.max[1])
                .flat_map(move |y| (self.min[2]..=self.max[2]).map(move |z| [x, y, z]))
        })
    }
}
#[derive(serde::Serialize, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub key: String,
    pub prototype: String,
    pub position: [f32; 3],
}
#[derive(serde::Serialize, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DoorDef {
    #[serde(default)]
    pub glass: bool,
    #[serde(default)]
    pub horizontal: bool,
    pub cell: Cell,
    pub access: u8,
    pub name: String,
}
#[derive(serde::Serialize, Clone, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Lamp {
    pub position: [f32; 3],
    pub radius: f32,
    pub color: [f32; 3],
    pub energy: f32,
}
#[derive(serde::Serialize, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Map {
    #[serde(default)]
    pub power: Option<crate::power::PowerDef>,
    #[serde(default)]
    pub floors: Vec<floor::FloorTile>,
    #[serde(default)]
    pub barriers: Vec<barrier::Barrier>,
    #[serde(default)]
    pub lamps: Vec<Lamp>,
    pub base: Vec<Fill>,
    pub departments: Vec<Fill>,
    pub asteroid: bool,
    pub items: Vec<Placement>,
    pub crate_round_position: [f32; 3],
    pub doors: Vec<DoorDef>,
    pub spawns: BTreeMap<String, [f32; 3]>,
}
#[derive(serde::Serialize, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KitItem {
    pub prototype: String,
    pub slot: usize,
}
#[derive(serde::Serialize, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Character {
    #[serde(default)]
    pub style: sprite::SpriteStyle,
    #[serde(default)]
    pub layers: Vec<sprite::SpriteLayer>,
    pub radius: f32,
    pub speed: f32,
    pub sprite_size: [f32; 2],
}
#[derive(serde::Serialize, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Content {
    pub version: u32,
    pub prototypes: BTreeMap<String, Prototype>,
    pub map: Map,
    pub kits: BTreeMap<String, Vec<KitItem>>,
    pub character: Character,
}
fn one_item() -> u16 {
    1
}
fn default_sprite_size() -> [f32; 2] {
    [1.; 2]
}
impl Content {
    pub fn test_station() -> Arc<Self> {
        Self::parse(include_str!("../../content/test-station.json")).expect("test station content")
    }
    pub fn builtin() -> Arc<Self> {
        static DATA: OnceLock<Arc<Content>> = OnceLock::new();
        DATA.get_or_init(|| {
            Self::parse(include_str!("../../content/station.json"))
                .expect("bundled station content")
        })
        .clone()
    }
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Arc<Self>, String> {
        if std::fs::metadata(path.as_ref())
            .map_err(|e| e.to_string())?
            .len()
            > 1024 * 1024
        {
            return Err("content exceeds 1 MiB".into());
        }
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        if bytes.len() > 1024 * 1024 {
            return Err("content exceeds 1 MiB".into());
        }
        Self::parse(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)
    }
    pub fn parse(text: &str) -> Result<Arc<Self>, String> {
        let c: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        c.validate()?;
        Ok(Arc::new(c))
    }
    fn validate(&self) -> Result<(), String> {
        if let Some(power) = &self.map.power {
            power.validate()?;
        }
        let check = |ok: bool, msg: &str| if ok { Ok(()) } else { Err(msg.to_owned()) };
        check(self.version == 1, "unsupported content version")?;
        check(
            self.prototypes.len() <= 256
                && self.map.items.len() <= 512
                && self.map.doors.len() <= 64,
            "too many entities",
        )?;
        check(
            self.character.layers.len() <= 16
                && self.character.layers.iter().all(sprite::SpriteLayer::valid),
            "invalid character sprite layers",
        )?;
        sprite::SpriteComponent {
            style: self.character.style.clone(),
            layers: self.character.layers.clone(),
        }
        .validate()?;
        for (key, p) in &self.prototypes {
            check(
                (1..=50).contains(&p.stack_count)
                    && (matches!(p.kind, ItemKind::Tile(_)) || p.stack_count == 1),
                "invalid item stack",
            )?;
            check(
                !matches!(p.kind, ItemKind::Tile(floor::FloorKind::Plating)),
                "plating is a structural base, not a floor tile",
            )?;
            check(
                p.sprite.style.valid() && p.sprite.rules.len() <= 64,
                "invalid sprite style or rules",
            )?;
            for rule in &p.sprite.rules {
                check(
                    !rule.key.is_empty() && rule.key.len() <= 64 && rule.value.len() <= 128,
                    "invalid appearance condition",
                )?;
                let mut component = sprite::SpriteComponent {
                    style: p.sprite.style.clone(),
                    layers: p.sprite.layers.clone(),
                };
                component.appearance(
                    &std::collections::BTreeMap::from([(rule.key.clone(), rule.value.clone())]),
                    std::slice::from_ref(rule),
                )?;
            }
            for layers in [
                &p.sprite.layers,
                &p.sprite.active_layers,
                &p.sprite.open_layers,
                &p.sprite.working_layers,
                &p.sprite.inhand_left,
                &p.sprite.inhand_right,
                &p.sprite.worn,
            ] {
                check(
                    layers.len() <= 64 && layers.iter().all(sprite::SpriteLayer::valid),
                    "invalid prototype sprite layers",
                )?;
                sprite::SpriteComponent {
                    style: p.sprite.style.clone(),
                    layers: layers.clone(),
                }
                .validate()?;
            }
            check(
                p.sprite_size
                    .iter()
                    .all(|v| v.is_finite() && (0.1..=3.).contains(v)),
                "invalid item sprite size",
            )?;
            check(
                !key.is_empty()
                    && key.len() <= 80
                    && p.name.len() <= 256
                    && p.description.len() <= 2048,
                "invalid prototype text",
            )?;
            check(
                p.half.is_finite()
                    && (0.01..=1.).contains(&p.half)
                    && p.mass.is_finite()
                    && (0.01..=100.).contains(&p.mass),
                "invalid item physics",
            )?;
            check(
                p.color
                    .iter()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
                    && (0..=16).contains(&p.size)
                    && (0..=65535).contains(&p.slots)
                    && p.capacity <= 64,
                "invalid item components",
            )?;
        }
        check(
            self.map.barriers.len() <= 4096,
            "too many barrier overrides",
        )?;
        check(self.map.floors.len() <= 32768, "too many floor surfaces")?;
        let mut floor_cells = BTreeSet::new();
        for tile in &self.map.floors {
            check(
                tile.valid() && floor_cells.insert(tile.cell),
                "invalid/duplicate floor surface",
            )?;
        }
        let mut barrier_cells = BTreeSet::new();
        for b in &self.map.barriers {
            check(
                b.cell.iter().all(|c| c.unsigned_abs() <= 128) && barrier_cells.insert(b.cell),
                "invalid/duplicate barrier cell",
            )?;
        }
        let mut total = 0_i64;
        for b in self.map.base.iter().chain(&self.map.departments) {
            check(
                (0..3).all(|i| b.min[i] >= -128 && b.max[i] <= 128 && b.min[i] <= b.max[i]),
                "invalid map box",
            )?;
            total += (0..3)
                .map(|i| i64::from(b.max[i] - b.min[i] + 1))
                .product::<i64>();
        }
        check(total <= 200_000, "map exceeds voxel budget")?;
        let position = |p: [f32; 3]| p.iter().all(|v| v.is_finite() && v.abs() <= 128.);
        let mut keys = BTreeSet::new();
        for item in &self.map.items {
            check(keys.insert(item.key.as_str()), "duplicate map entity key")?;
            check(
                self.prototypes.contains_key(&item.prototype) && position(item.position),
                "invalid map item",
            )?;
        }
        for key in ["flashlight", "crate", "helmet", "crowbar"] {
            check(keys.contains(key), "missing compatibility map alias")?;
        }
        let crate_item = self.map.items.iter().find(|i| i.key == "crate").unwrap();
        let crate_proto = &self.prototypes[&crate_item.prototype];
        check(
            crate_proto.kind == ItemKind::Crate && crate_proto.capacity > 0,
            "crate alias requires a container prototype",
        )?;
        for key in [
            "local",
            "arrival",
            "job.engineer",
            "job.doctor",
            "job.assistant",
        ] {
            check(
                self.map.spawns.get(key).is_some_and(|p| position(*p)),
                "missing/invalid spawn",
            )?;
        }
        check(
            position(self.map.crate_round_position),
            "invalid crate position",
        )?;
        check(self.map.lamps.len() <= 32, "too many lamps")?;
        for l in &self.map.lamps {
            check(
                position(l.position)
                    && l.radius.is_finite()
                    && (1.0..=16.0).contains(&l.radius)
                    && l.energy.is_finite()
                    && (0.0..=8.0).contains(&l.energy)
                    && l.color
                        .iter()
                        .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
                "invalid lamp",
            )?;
        }
        let mut doors = BTreeSet::new();
        for d in &self.map.doors {
            check(
                d.cell.iter().all(|c| c.unsigned_abs() <= 127)
                    && doors.insert(d.cell)
                    && d.name.len() <= 256,
                "invalid door",
            )?;
        }
        for job in crew::Job::ALL {
            let kit = self.kits.get(job.key()).ok_or("missing role kit")?;
            let mut slots = BTreeSet::new();
            for item in kit {
                check(
                    item.slot < 16 && slots.insert(item.slot),
                    "invalid or duplicate kit slot",
                )?;
                let p = self
                    .prototypes
                    .get(&item.prototype)
                    .ok_or("unknown kit prototype")?;
                check(
                    ss14_rules::can_fit(
                        interaction::SLOTS[item.slot].flag,
                        p.slots,
                        p.clothing,
                        true,
                        p.size,
                        1,
                    )
                    .unwrap_or(false),
                    "kit item does not fit slot",
                )?;
            }
        }
        check(
            self.character.style.valid()
                && self.character.radius.is_finite()
                && (0.1..=0.5).contains(&self.character.radius)
                && self.character.speed.is_finite()
                && (0.1..=10.).contains(&self.character.speed)
                && self
                    .character
                    .sprite_size
                    .iter()
                    .all(|v| v.is_finite() && (0.1..=3.).contains(v)),
            "invalid character",
        )?;
        Ok(())
    }
}
impl Map {
    pub fn effective_barriers(&self) -> Vec<barrier::Barrier> {
        let mut barriers: BTreeMap<_, _> =
            self.barriers.iter().cloned().map(|b| (b.cell, b)).collect();
        for d in &self.doors {
            for cell in [d.cell, [d.cell[0], d.cell[1], d.cell[2] + 1]] {
                barriers.insert(
                    cell,
                    barrier::Barrier {
                        cell,
                        blocks_movement: true,
                        blocks_sight: !d.glass,
                        blocks_light: !d.glass,
                    },
                );
            }
        }
        barriers.into_values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn data() -> serde_json::Value {
        serde_json::from_str(include_str!("../../content/station.json")).unwrap()
    }
    #[test]
    fn invalid_definitions_are_rejected_before_world_creation() {
        for mutate in [
            |v: &mut serde_json::Value| v["prototypes"]["flashlight"]["mass"] = (-1).into(),
            |v: &mut serde_json::Value| v["map"]["items"][0]["prototype"] = "missing".into(),
            |v: &mut serde_json::Value| v["map"]["doors"][0]["cell"][0] = i32::MIN.into(),
            |v: &mut serde_json::Value| v["kits"]["job.engineer"][1]["slot"] = 100.into(),
            |v: &mut serde_json::Value| v["map"]["base"][0]["max"][0] = 100000.into(),
        ] {
            let mut v = data();
            mutate(&mut v);
            assert!(Content::parse(&v.to_string()).is_err());
        }
    }
    #[test]
    fn custom_prototype_map_and_character_drive_simulation_without_code_changes() {
        let mut v = data();
        let mut p = v["prototypes"]["flashlight"].clone();
        p["kind"] = "Generic".into();
        p["name"] = "Signal marker".into();
        p["half"] = 0.42.into();
        p["color"] = serde_json::json!([0.1, 0.8, 0.9, 1.]);
        v["prototypes"]["marker"] = p;
        v["map"]["items"].as_array_mut().unwrap().push(
            serde_json::json!({"key":"marker-a","prototype":"marker","position":[4.5,-2.5,3.]}),
        );
        v["map"]["items"].as_array_mut().unwrap().reverse();
        v["map"]["base"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"min":[0,0,1],"max":[0,0,1]}));
        v["character"]["speed"] = 2.into();
        let mut s = Simulation::with_content(7, Content::parse(&v.to_string()).unwrap());
        assert_eq!(s.items()[0].name, "item.flashlight");
        assert_eq!(s.items()[4].name, "Signal marker");
        assert_eq!(s.items()[4].half, 0.42);
        assert!(s.world().solid([0, 0, 1]));
        s.step([0., 1.]);
        assert!((s.velocity()[1] - 2.).abs() < 0.01);
        let before = s.entities().len();
        assert!(s.spawn_prototype("absent", [0.; 3]).is_err());
        assert_eq!(s.entities().len(), before);
    }
}

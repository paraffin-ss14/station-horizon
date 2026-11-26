pub mod appearance;
#[path = "../../../crates/horizon-client-render/src/station_barrier.rs"]
pub mod barrier;
pub mod content;
pub mod editor;
pub mod entity;
#[path = "../../../crates/horizon-client-render/src/station_floor.rs"]
pub mod floor;
#[path = "../../../crates/horizon-client-render/src/station_sprite_types.rs"]
pub mod sprite;
use std::sync::Arc;
pub mod actors;
pub mod crew;
pub mod power;
pub mod rooms;
mod wire;
// Standalone Z-up simulation. No window, Minecraft resources or wire protocol.
#[path = "../../ss14-action-rules/src/port.rs"]
pub mod action_rules;
pub mod actions;
pub mod interaction;
#[path = "../../ss14-rules/src/port.rs"]
pub mod ss14_rules;
pub mod storage;
#[path = "../../ss14-storage-rules/src/port.rs"]
pub mod storage_rules;
mod tile_work;
use horizon_core::{noise::PerlinNoise, random::LegacyRandom};
use interaction::InventorySnapshot;
use rapier3d::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

pub type Cell = [i32; 3];
pub const CHUNK: i32 = 16;
pub const DT: f32 = 1.0 / 60.0;

#[derive(Default, Clone, Debug, PartialEq)]
pub struct World {
    chunks: BTreeMap<Cell, Box<[bool; 4096]>>,
    barriers: BTreeMap<Cell, barrier::Barrier>,
    floors: BTreeMap<Cell, floor::FloorTile>,
    floor_versions: BTreeMap<Cell, u64>,
}
impl World {
    fn address(p: Cell) -> (Cell, usize) {
        let c = p.map(|v| v.div_euclid(CHUNK));
        let [x, y, z] = p.map(|v| v.rem_euclid(CHUNK) as usize);
        (c, x + 16 * (y + 16 * z))
    }
    pub fn solid(&self, p: Cell) -> bool {
        let (c, i) = Self::address(p);
        self.chunks.get(&c).is_some_and(|chunk| chunk[i])
    }
    pub fn blocks_movement(&self, p: Cell) -> bool {
        self.solid(p) && self.barriers.get(&p).is_none_or(|b| b.blocks_movement)
    }
    pub fn set(&mut self, p: Cell, solid: bool) {
        if self.solid(p) != solid {
            *self.floor_versions.entry(p).or_default() += 1;
        }
        if !solid {
            self.floors.remove(&p);
        }
        let (c, i) = Self::address(p);
        if solid {
            self.chunks
                .entry(c)
                .or_insert_with(|| Box::new([false; 4096]))[i] = true;
        } else if let Some(chunk) = self.chunks.get_mut(&c) {
            chunk[i] = false;
            if !chunk.iter().any(|v| *v) {
                self.chunks.remove(&c);
            }
        }
    }
    pub fn cells(&self) -> impl Iterator<Item = Cell> + '_ {
        self.chunks.iter().flat_map(|(c, chunk)| {
            chunk
                .iter()
                .enumerate()
                .filter(|(_, v)| **v)
                .map(move |(i, _)| {
                    [
                        c[0] * 16 + (i % 16) as i32,
                        c[1] * 16 + (i / 16 % 16) as i32,
                        c[2] * 16 + (i / 256) as i32,
                    ]
                })
        })
    }
    pub fn station(seed: i64) -> Self {
        Self::from_template(seed, &content::Content::builtin().map)
    }
    fn from_template(seed: i64, map: &content::Map) -> Self {
        let mut w = Self {
            floors: map.floors.iter().copied().map(|f| (f.cell, f)).collect(),
            barriers: map
                .effective_barriers()
                .into_iter()
                .map(|b| (b.cell, b))
                .collect(),
            ..Default::default()
        };
        for b in &map.base {
            for cell in b.cells() {
                w.set(cell, true);
            }
        }
        if !map.asteroid {
            return w;
        }
        // Actual Horizon Perlin sampler supplies the asteroid density field.
        let noise = PerlinNoise::new(&mut LegacyRandom::new(seed));
        for x in 11_i32..=19 {
            for y in -4..=4 {
                for z in -3..=3 {
                    let d = ((x - 15).pow(2) + y * y + z * z) as f32;
                    if d < 13.0 + noise.get3(x as f64 * 0.4, y as f64 * 0.4, z as f64 * 0.4) * 10.0
                    {
                        w.set([x, y, z], true);
                    }
                }
            }
        }
        w
    }
}

/// Pure presentation contract: no pitch/yaw, XY right/up, orthographic at every Z.
#[derive(Clone, Copy)]
pub struct TopCamera {
    pub center: [f32; 2],
    pub vertical_span: f32,
}
impl TopCamera {
    pub fn project(&self, p: [f32; 3], aspect: f32) -> [f32; 2] {
        [
            (p[0] - self.center[0]) * 2.0 / (self.vertical_span * aspect),
            (p[1] - self.center[1]) * 2.0 / self.vertical_span,
        ]
    }
    pub fn billboard(center: [f32; 3], size: [f32; 2]) -> [[f32; 3]; 4] {
        let [x, y, z] = center;
        let [w, h] = size.map(|v| v * 0.5);
        [
            [x - w, y - h, z],
            [x + w, y - h, z],
            [x + w, y + h, z],
            [x - w, y + h, z],
        ]
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct ItemSnapshot {
    pub kind: ItemKind,
    pub stack_count: u16,
    pub sprite_style: sprite::SpriteStyle,
    pub layers: Vec<sprite::SpriteLayer>,
    pub sprite_size: [f32; 2],
    pub held_layer: bool,
    pub id: usize,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub half: f32,
    pub held: bool,
    pub visible: bool,
    pub name: String,
    pub description: String,
    pub entity: Option<entity::EntityId>,
    pub color: [f32; 4],
    pub billboard: bool,
    pub activated: bool,
}
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Flashlight,
    Crate,
    Helmet,
    Crowbar,
    Wirecutters,
    IdCard(u8),
    Medkit,
    Generic,
    Tile(floor::FloorKind),
}
struct Item {
    entity: entity::EntityId,
    prototype: String,
    alive: bool,
    kind: ItemKind,
    stack_count: u16,
    activated: bool,
    body: RigidBodyHandle,
    half: f32,
}
pub struct Simulation {
    rooms: rooms::Rooms,
    power: power::Power,
    content: Arc<content::Content>,
    entities: entity::Registry,
    outfitted: BTreeSet<u64>,
    doors: Vec<crew::Door>,
    world: World,
    pipeline: PhysicsPipeline,
    islands: IslandManager,
    broad: BroadPhaseMultiSap,
    narrow: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    joints: ImpulseJointSet,
    multijoints: MultibodyJointSet,
    ccd: CCDSolver,
    terrain: BTreeMap<Cell, Vec<ColliderHandle>>,
    pub rebuilt_chunks: usize,
    items: Vec<Item>,
    held: Option<usize>,
    inventory: InventorySnapshot,
    storage: storage::Storage,
    action: Option<actions::Action>,
    next_action: u64,
    world_revision: u64,
    facing: [f32; 2],
    previous_gravity: bool,
    player: RigidBodyHandle,
    active_actor: u64,
    actors: BTreeMap<u64, actors::Actor>,
    pub tick: u64,
    pub gravity: bool,
}
impl Simulation {
    pub fn content(&self) -> Arc<content::Content> {
        self.content.clone()
    }
    pub fn new(seed: i64) -> Self {
        Self::with_content(seed, content::Content::builtin())
    }
    pub fn with_content(seed: i64, content: Arc<content::Content>) -> Self {
        let mut bodies = RigidBodySet::new();
        let player = bodies.insert(
            RigidBodyBuilder::dynamic()
                .translation(Vector::from(content.map.spawns["local"]))
                .lock_rotations()
                .ccd_enabled(true),
        );
        let mut colliders = ColliderSet::new();
        colliders.insert_with_parent(
            ColliderBuilder::ball(content.character.radius).friction(0.0),
            player,
            &mut bodies,
        );
        let mut s = Self {
            rooms: Default::default(),
            power: power::Power {
                definition: content.map.power.clone(),
                dirty: true,
                ..Default::default()
            },
            entities: entity::Registry::new(),
            content: content.clone(),
            outfitted: BTreeSet::new(),
            doors: Vec::new(),
            world: World::from_template(seed, &content.map),
            pipeline: PhysicsPipeline::new(),
            islands: IslandManager::new(),
            broad: BroadPhaseMultiSap::new(),
            narrow: NarrowPhase::new(),
            bodies,
            colliders,
            joints: ImpulseJointSet::new(),
            multijoints: MultibodyJointSet::new(),
            ccd: CCDSolver::new(),
            terrain: BTreeMap::new(),
            rebuilt_chunks: 0,
            items: Vec::new(),
            held: None,
            inventory: InventorySnapshot::default(),
            storage: storage::Storage::default(),
            action: None,
            next_action: 0,
            world_revision: 0,
            facing: [1., 0.],
            previous_gravity: true,
            player,
            active_actor: 0,
            actors: BTreeMap::new(),
            tick: 0,
            gravity: true,
        };
        let chunks: Vec<_> = s.world.chunks.keys().copied().collect();
        for chunk in chunks {
            s.rebuild_chunk(chunk);
        }
        s.register_actor(0, player);
        // Preserve legacy aliases for current gameplay adapters; all new references use entity IDs.
        for key in ["flashlight", "crate", "helmet", "crowbar"] {
            let placement = content.map.items.iter().find(|i| i.key == key).unwrap();
            s.spawn_prototype(&placement.prototype, placement.position)
                .expect("validated placement");
        }
        for p in &content.map.items {
            if !["flashlight", "crate", "helmet", "crowbar"].contains(&p.key.as_str()) {
                s.spawn_prototype(&p.prototype, p.position)
                    .expect("validated placement");
            }
        }
        s.refresh_power();
        s.rooms.initialize(&s.world);
        s
    }
    /// Merge consecutive X voxels into cuboids; terrain has no per-voxel rigid bodies.
    fn rebuild_chunk(&mut self, chunk: Cell) {
        self.rebuilt_chunks += 1;
        for h in self.terrain.remove(&chunk).unwrap_or_default() {
            self.colliders
                .remove(h, &mut self.islands, &mut self.bodies, true);
        }
        let mut rows: BTreeMap<(i32, i32), Vec<i32>> = BTreeMap::new();
        if let Some(data) = self.world.chunks.get(&chunk) {
            for (i, &solid) in data.iter().enumerate() {
                if solid {
                    let x = chunk[0] * 16 + (i % 16) as i32;
                    let y = chunk[1] * 16 + (i / 16 % 16) as i32;
                    let z = chunk[2] * 16 + (i / 256) as i32;
                    if self.world.blocks_movement([x, y, z]) {
                        rows.entry((y, z)).or_default().push(x);
                    }
                }
            }
        }
        for ((y, z), mut xs) in rows {
            xs.sort_unstable();
            let mut i = 0;
            while i < xs.len() {
                let start = xs[i];
                let mut end = start;
                i += 1;
                while i < xs.len() && xs[i] == end + 1 {
                    end = xs[i];
                    i += 1;
                }
                let width = (end - start + 1) as f32;
                let h = self.colliders.insert(
                    ColliderBuilder::cuboid(width / 2.0, 0.5, 0.5).translation(vector![
                        start as f32 + width / 2.0,
                        y as f32 + 0.5,
                        z as f32 + 0.5
                    ]),
                );
                self.terrain.entry(chunk).or_default().push(h);
            }
        }
        for (_, body) in self.bodies.iter_mut() {
            let p = body.translation();
            if (0..3).all(|i| {
                p[i] >= chunk[i] as f32 * 16. - 1. && p[i] <= (chunk[i] + 1) as f32 * 16. + 1.
            }) {
                body.wake_up(true);
            }
        }
    }
    /// Authoritative surface edit for server systems. No collider rebuild is needed.
    pub fn set_floor(&mut self, tile: floor::FloorTile) -> Result<bool, String> {
        if !tile.valid() || !self.world.solid(tile.cell) {
            return Err("invalid floor surface".into());
        }
        if self.world.floors.get(&tile.cell) == Some(&tile) {
            return Ok(false);
        }
        self.world.floors.insert(tile.cell, tile);
        *self.world.floor_versions.entry(tile.cell).or_default() += 1;
        self.world_revision += 1;
        Ok(true)
    }
    pub fn floors(&self) -> Vec<floor::FloorTile> {
        self.world
            .floors
            .values()
            .filter(|f| self.world.solid(f.cell))
            .copied()
            .collect()
    }
    pub fn set_voxel(&mut self, p: Cell, solid: bool) {
        self.edit_voxels(&[(p, solid)]);
    }
    pub fn edit_voxels(&mut self, edits: &[(Cell, bool)]) {
        let mut dirty = BTreeSet::new();
        let mut changed = Vec::new();
        for &(p, solid) in edits {
            if self.world.solid(p) != solid {
                self.world.set(p, solid);
                changed.push(p);
                dirty.insert(p.map(|v| v.div_euclid(16)));
            }
        }
        if !changed.is_empty() {
            self.rooms.update(&self.world, &changed);
        }
        self.rebuilt_chunks = 0;
        if !dirty.is_empty() {
            self.world_revision += 1;
            self.power.dirty = true;
        }
        for chunk in dirty {
            self.rebuild_chunk(chunk);
        }
        self.validate_work();
    }
    pub fn spawn_prototype(&mut self, key: &str, p: [f32; 3]) -> Result<entity::EntityId, String> {
        if self.items.len() >= 4096 || !p.iter().all(|v| v.is_finite() && v.abs() <= 128.) {
            return Err("invalid entity spawn".into());
        }
        let proto = self
            .content
            .prototypes
            .get(key)
            .ok_or("unknown prototype")?
            .clone();
        let (half, mass) = (proto.half, proto.mass);
        let body = self.bodies.insert(
            RigidBodyBuilder::dynamic()
                .translation(vector![p[0], p[1], p[2]])
                .ccd_enabled(true),
        );
        self.colliders.insert_with_parent(
            ColliderBuilder::cuboid(half, half, half)
                .mass(mass)
                .friction(0.6)
                .restitution(0.45),
            body,
            &mut self.bodies,
        );
        let entity = self.entities.insert(entity::Entity {
            prototype: key.into(),
            position: p,
            sprite: Some(entity::SpriteComponent {
                size: [half * 2.; 2],
                color: proto.color,
                billboard: proto.billboard,
            }),
            body: Some(body),
            item: Some(self.items.len()),
            actor: None,
            door: None,
            container_capacity: (proto.capacity > 0).then_some(proto.capacity),
            access: if let ItemKind::IdCard(a) = proto.kind {
                Some(a)
            } else {
                None
            },
        });
        self.items.push(Item {
            entity,
            prototype: key.into(),
            alive: true,
            kind: proto.kind,
            stack_count: proto.stack_count,
            body,
            half,
            activated: false,
        });
        Ok(entity)
    }
    pub fn items(&self) -> Vec<ItemSnapshot> {
        self.items
            .iter()
            .enumerate()
            .map(|(id, item)| {
                let proto = &self.content.prototypes[&item.prototype];
                if !item.alive {
                    return ItemSnapshot {
                        kind: item.kind,
                        stack_count: 0,
                        sprite_style: Default::default(),
                        layers: vec![],
                        sprite_size: [1.; 2],
                        held_layer: false,
                        id,
                        entity: None,
                        name: proto.name.clone(),
                        description: proto.description.clone(),
                        position: [0.; 3],
                        rotation: [0., 0., 0., 1.],
                        half: item.half,
                        held: false,
                        visible: false,
                        activated: false,
                        color: proto.color,
                        billboard: proto.billboard,
                    };
                }
                let body = &self.bodies[item.body];
                let p = body.translation();
                let q = body.rotation().quaternion();
                ItemSnapshot {
                    kind: item.kind,
                    stack_count: item.stack_count,
                    sprite_style: self.content.prototypes[&item.prototype]
                        .sprite
                        .style
                        .clone(),
                    layers: self.item_layers(id),
                    sprite_size: proto.sprite_size,
                    held_layer: (self.held == Some(id) || self.held_by_other(id))
                        && (!proto.sprite.inhand_left.is_empty()
                            || !proto.sprite.inhand_right.is_empty()),
                    id,
                    position: [p.x, p.y, p.z],
                    rotation: [q.i, q.j, q.k, q.w],
                    half: item.half,
                    held: self.held == Some(id) || self.held_by_other(id),
                    visible: !self.stowed_by_other(id)
                        && !self.storage.items.contains(&id)
                        && !self.inventory.slots.contains(&Some(id))
                        && (!self.inventory.hands.contains(&Some(id)) || self.held == Some(id)),
                    name: proto.name.clone(),
                    description: proto.description.clone(),
                    entity: Some(item.entity),
                    color: proto.color,
                    billboard: proto.billboard,
                    activated: item.activated,
                }
            })
            .collect()
    }
    /// Pick the nearest reachable item. Holding is a bounded spring, so walls remain solid.
    pub fn pickup(&mut self) {
        if self.held.is_some() {
            return;
        }
        let p = self.position();
        let mut best = 1.6f32;
        let mut chosen = None;
        for (id, item) in self.items.iter().enumerate() {
            let q = self.bodies[item.body].translation();
            let delta = [q.x - p[0], q.y - p[1], q.z - p[2]];
            let d = (delta.iter().map(|v| v * v).sum::<f32>()).sqrt();
            if d < best && self.reachable(id) && !self.contained(id) {
                best = d;
                chosen = Some(id);
            }
        }
        self.held = chosen;
        self.inventory.hands[self.inventory.active] = chosen;
    }
    pub fn drop_item(&mut self) {
        self.inventory.hands[self.inventory.active] = None;
        self.held = None;
        self.validate_work();
    }
    pub fn throw_item(&mut self) {
        self.inventory.hands[self.inventory.active] = None;
        if let Some(id) = self.held.take() {
            let body = &mut self.bodies[self.items[id].body];
            body.apply_impulse(vector![self.facing[0] * 3., self.facing[1] * 3., 0.5], true);
        }
        self.validate_work();
    }
    pub fn world(&self) -> &World {
        &self.world
    }
    pub fn position(&self) -> [f32; 3] {
        let p = self.bodies[self.player].translation();
        [p.x, p.y, p.z]
    }
    pub fn velocity(&self) -> [f32; 3] {
        let v = self.bodies[self.player].linvel();
        [v.x, v.y, v.z]
    }
    pub fn impulse(&mut self, impulse: [f32; 3]) {
        self.bodies[self.player].apply_impulse(vector![impulse[0], impulse[1], impulse[2]], true);
    }
    pub fn step(&mut self, movement: [f32; 2]) {
        self.prepare_actor(movement);
        self.physics_step();
        self.advance_work(movement[0].hypot(movement[1]) > 0.01);
    }
    fn prepare_actor(&mut self, movement: [f32; 2]) {
        let movement = movement.map(|v| if v.is_finite() { v.clamp(-1., 1.) } else { 0. });
        let norm = movement[0].hypot(movement[1]);
        if norm > 0.01 {
            self.facing = movement.map(|v| v / norm);
        }
        let length = norm.max(1.);
        if self.gravity != self.previous_gravity {
            for (_, body) in self.bodies.iter_mut() {
                body.wake_up(true);
            }
            self.previous_gravity = self.gravity;
        }
        if let Some(id) = self.held {
            let player = self.position();
            let target = vector![
                player[0] + self.facing[0] * 0.9,
                player[1] + self.facing[1] * 0.9,
                player[2] + 0.5
            ];
            let body = &mut self.bodies[self.items[id].body];
            if (body.translation() - vector![player[0], player[1], player[2]]).norm() > 2.5 {
                self.held = None;
                self.inventory.hands[self.inventory.active] = None;
            } else {
                let acceleration = (target - body.translation()) * 35. - body.linvel() * 9.
                    + vector![0., 0., if self.gravity { 9.81 } else { 0. }];
                let acceleration = acceleration.cap_magnitude(45.);
                body.apply_impulse(acceleration * body.mass() * DT, true);
            }
        }
        let z = self.bodies[self.player].linvel().z;
        self.bodies[self.player].set_linvel(
            vector![
                movement[0] / length * self.content.character.speed,
                movement[1] / length * self.content.character.speed,
                z
            ],
            true,
        );
    }
    fn physics_step(&mut self) {
        self.refresh_power();
        self.tick_doors();
        let g = if self.gravity { -9.81 } else { 0.0 };
        self.pipeline.step(
            &vector![0.0, 0.0, g],
            &IntegrationParameters {
                dt: DT,
                ..Default::default()
            },
            &mut self.islands,
            &mut self.broad,
            &mut self.narrow,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.joints,
            &mut self.multijoints,
            &mut self.ccd,
            None,
            &(),
            &(),
        );
        self.tick += 1;
    }
    pub fn open_floor(&mut self) {
        let p = self.position();
        let mut edits = Vec::new();
        for x in -1..=1 {
            for y in -1..=1 {
                edits.push(([p[0].floor() as i32 + x, p[1].floor() as i32 + y, 0], false));
            }
        }
        self.edit_voxels(&edits);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn floor_surface_edit_preserves_physics_and_removal_clears_surface() {
        let mut sim = Simulation::new(7);
        let before = sim.terrain.clone();
        let revision = sim.world_revision;
        let tile = floor::FloorTile {
            cell: [0, 0, 0],
            kind: floor::FloorKind::Damaged,
            variant: Some(3),
        };
        assert_eq!(sim.set_floor(tile), Ok(true));
        assert_eq!(sim.world_revision, revision + 1);
        assert_eq!(sim.terrain, before);
        assert_eq!(sim.set_floor(tile), Ok(false));
        assert!(
            sim.set_floor(floor::FloorTile {
                variant: Some(4),
                ..tile
            })
            .is_err()
        );
        sim.set_voxel(tile.cell, false);
        assert!(sim.floors().is_empty());
        assert!(sim.set_floor(tile).is_err());
    }
    #[test]
    fn window_collision_and_passable_barrier_are_independent_of_occupancy() {
        for blocking in [true, false] {
            let mut c = (*content::Content::test_station()).clone();
            for b in &mut c.map.barriers {
                b.blocks_movement = blocking;
            }
            let mut sim = Simulation::with_content(7, Arc::new(c));
            sim.bodies[sim.player].set_translation(vector![-1.5, -7.5, 1.3], true);
            for _ in 0..60 {
                sim.step([-1., 0.]);
            }
            assert!(sim.world.solid([-3, -8, 1]));
            if blocking {
                assert!(sim.position()[0] >= -1.71);
            } else {
                assert!(sim.position()[0] < -3.5);
            }
        }
    }
    #[test]
    fn negative_chunk_edit_roundtrip() {
        let mut w = World::default();
        for p in [[-1, -16, -17], [16, 0, 0], [0, 0, 0]] {
            w.set(p, true);
            assert!(w.solid(p));
        }
        assert_eq!(w.cells().count(), 3);
        w.set([-1, -16, -17], false);
        assert!(!w.solid([-1, -16, -17]));
        assert!(!w.solid([-1, -16, -16]));
    }
    #[test]
    fn horizon_generation_is_seeded() {
        assert_eq!(World::station(7), World::station(7));
        assert_ne!(World::station(7), World::station(8));
    }
    #[test]
    fn top_projection_has_no_height_parallax() {
        let c = TopCamera {
            center: [2.0, 3.0],
            vertical_span: 20.0,
        };
        assert_eq!(
            c.project([4.0, 7.0, 0.0], 2.0),
            c.project([4.0, 7.0, 100.0], 2.0)
        );
        assert_eq!(c.project([2.0, 3.0, -4.0], 2.0), [0.0, 0.0]);
        let q = TopCamera::billboard([2.0, 3.0, 4.0], [1.0, 2.0]);
        assert!(q.iter().all(|v| v[2] == 4.0));
        assert_eq!(q[2], [2.5, 4.0, 4.0]);
    }
    #[test]
    fn voxel_edits_update_collision_geometry() {
        let mut s = Simulation::new(7);
        s.set_voxel([0, 0, 1], true);
        for _ in 0..240 {
            s.step([0.0, 0.0]);
        }
        assert!((s.position()[2] - 2.3).abs() < 0.05);
        s.set_voxel([0, 0, 1], false);
        for _ in 0..240 {
            s.step([0.0, 0.0]);
        }
        assert!((s.position()[2] - 1.3).abs() < 0.05);
    }
    #[test]
    fn floor_removal_falls_to_lower_deck() {
        let mut s = Simulation::new(7);
        for _ in 0..240 {
            s.step([0.0, 0.0]);
        }
        assert!((s.position()[2] - 1.3).abs() < 0.05, "{:?}", s.position());
        s.open_floor();
        for _ in 0..240 {
            s.step([0.0, 0.0]);
        }
        assert!((s.position()[2] + 2.7).abs() < 0.05, "{:?}", s.position());
    }
    #[test]
    fn wall_stops_player_and_zero_gravity_preserves_z_motion() {
        let mut s = Simulation::new(7);
        for _ in 0..240 {
            s.step([1.0, 0.0]);
        }
        assert!(s.position()[0] < 7.75, "{:?}", s.position());
        s.gravity = false;
        s.impulse([0.0, 0.0, 0.1]);
        let z = s.position()[2];
        for _ in 0..60 {
            s.step([0.0, 0.0]);
        }
        assert!(s.position()[2] > z + 0.1);
    }
}

#[cfg(test)]
mod item_tests {
    use super::*;
    fn settle(s: &mut Simulation) {
        for _ in 0..240 {
            s.step([0., 0.]);
        }
    }
    #[test]
    fn edit_preserves_other_chunk_handles_and_noop_does_nothing() {
        let mut s = Simulation::new(7);
        let other = s.terrain[&[-1, 0, 0]].clone();
        let lower = s.terrain[&[0, 0, -1]].clone();
        s.set_voxel([0, 0, 0], false);
        assert_eq!(s.rebuilt_chunks, 1);
        assert_eq!(other, s.terrain[&[-1, 0, 0]]);
        assert_eq!(lower, s.terrain[&[0, 0, -1]]);
        s.set_voxel([0, 0, 0], false);
        assert_eq!(s.rebuilt_chunks, 0);
        s.edit_voxels(&[([15, 0, 10], true), ([16, 0, 10], true)]);
        assert_eq!(s.rebuilt_chunks, 2);
    }
    #[test]
    fn sleeping_crate_falls_through_hole_and_lands() {
        let mut s = Simulation::new(7);
        settle(&mut s);
        let id = s.items[0].body;
        assert!((s.bodies[id].translation().z - 1.28).abs() < 0.05);
        s.bodies[id].sleep();
        let lower = s.terrain[&[0, 0, -1]].clone();
        s.open_floor();
        assert_eq!(s.rebuilt_chunks, 4);
        assert_eq!(lower, s.terrain[&[0, 0, -1]]);
        settle(&mut s);
        assert!(
            (s.bodies[id].translation().z + 2.72).abs() < 0.07,
            "{:?}",
            s.items()
        );
    }
    #[test]
    fn pickup_drop_throw_and_occlusion() {
        let mut s = Simulation::new(7);
        settle(&mut s);
        s.pickup();
        assert_eq!(s.held, Some(0));
        for _ in 0..60 {
            s.step([0., 0.]);
        }
        assert!(s.items()[0].held);
        s.drop_item();
        assert!(s.held.is_none());
        settle(&mut s);
        s.pickup();
        assert_eq!(s.held, Some(0));
        let id = s.items[0].body;
        let vx = s.bodies[id].linvel().x;
        s.throw_item();
        assert!(s.held.is_none());
        assert!(s.bodies[id].linvel().x > vx + 2.);
        s.set_voxel([1, 0, 1], true);
        s.bodies[id].set_translation(vector![1.8, 0.5, 1.3], true);
        s.pickup();
        assert!(s.held.is_none());
    }
    #[test]
    fn zero_gravity_preserves_momentum_and_bounces_at_wall() {
        let mut s = Simulation::new(7);
        s.gravity = false;
        let id = s.items[0].body;
        s.bodies[id].set_translation(vector![4., 0.5, 1.7], true);
        s.bodies[id].set_linvel(vector![2., 0., 0.], true);
        for _ in 0..30 {
            s.step([0., 0.]);
        }
        assert!((s.bodies[id].linvel().x - 2.).abs() < 0.01);
        assert!((s.bodies[id].translation().z - 1.7).abs() < 0.01);
        let mut bounced = false;
        for _ in 0..180 {
            s.step([0., 0.]);
            if s.bodies[id].linvel().x < -0.1 {
                bounced = true;
                break;
            }
        }
        assert!(bounced);
        assert!(s.bodies[id].translation().x < 8.);
    }
}

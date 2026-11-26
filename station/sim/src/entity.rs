//! Generational entity registry. Component snapshots are independent of vector indices.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
#[derive(
    serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord,
)]
pub struct EntityId {
    pub world: u64,
    pub slot: u32,
    pub generation: u32,
}
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct SpriteComponent {
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub billboard: bool,
}
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct EntitySnapshot {
    pub id: EntityId,
    pub prototype: String,
    pub position: [f32; 3],
    pub sprite: Option<SpriteComponent>,
    pub item: Option<usize>,
    pub actor: Option<u64>,
    pub door: Option<usize>,
    pub container_capacity: Option<usize>,
    pub access: Option<u8>,
    pub physical: bool,
}
pub(super) struct Entity {
    pub prototype: String,
    pub position: [f32; 3],
    pub sprite: Option<SpriteComponent>,
    pub body: Option<RigidBodyHandle>,
    pub item: Option<usize>,
    pub actor: Option<u64>,
    pub door: Option<usize>,
    pub container_capacity: Option<usize>,
    pub access: Option<u8>,
}
struct Slot {
    generation: u32,
    value: Option<Entity>,
}
pub(super) struct Registry {
    world: u64,
    slots: Vec<Slot>,
    free: Vec<usize>,
}
impl Registry {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            world: NEXT.fetch_add(1, Ordering::Relaxed),
            slots: vec![],
            free: vec![],
        }
    }
    pub fn insert(&mut self, value: Entity) -> EntityId {
        let slot = if let Some(slot) = self.free.pop() {
            self.slots[slot].value = Some(value);
            slot
        } else {
            self.slots.push(Slot {
                generation: 0,
                value: Some(value),
            });
            self.slots.len() - 1
        };
        EntityId {
            world: self.world,
            slot: slot as u32,
            generation: self.slots[slot].generation,
        }
    }
    pub fn get(&self, id: EntityId) -> Option<&Entity> {
        if id.world != self.world {
            return None;
        }
        self.slots
            .get(id.slot as usize)
            .filter(|s| s.generation == id.generation)?
            .value
            .as_ref()
    }
    pub fn remove(&mut self, id: EntityId) -> Option<Entity> {
        self.get(id)?;
        let slot = &mut self.slots[id.slot as usize];
        let value = slot.value.take();
        if let Some(next) = slot.generation.checked_add(1) {
            slot.generation = next;
            self.free.push(id.slot as usize);
        }
        value
    }
    pub fn iter(&self) -> impl Iterator<Item = (EntityId, &Entity)> {
        self.slots.iter().enumerate().filter_map(|(i, s)| {
            s.value.as_ref().map(|e| {
                (
                    EntityId {
                        world: self.world,
                        slot: i as u32,
                        generation: s.generation,
                    },
                    e,
                )
            })
        })
    }
}
impl Simulation {
    pub fn entities(&self) -> Vec<EntitySnapshot> {
        self.entities
            .iter()
            .map(|(id, e)| EntitySnapshot {
                id,
                prototype: e.prototype.clone(),
                position: e
                    .body
                    .and_then(|h| self.bodies.get(h))
                    .map_or(e.position, |b| {
                        let p = b.translation();
                        [p.x, p.y, p.z]
                    }),
                sprite: e.sprite.clone(),
                item: e.item,
                actor: e.actor,
                door: e.door,
                container_capacity: e.container_capacity,
                access: e.access,
                physical: e.body.is_some(),
            })
            .collect()
    }
    pub fn contains_entity(&self, id: EntityId) -> bool {
        self.entities.get(id).is_some()
    }
    pub fn despawn_entity(&mut self, id: EntityId) -> bool {
        let Some(e) = self.entities.get(id) else {
            return false;
        };
        let (item, actor, door) = (e.item, e.actor, e.door);
        if let Some(actor) = actor {
            if actor == 0 {
                return false;
            } // local player lifetime is the round lifetime
            self.remove_actor(actor);
            return true;
        }
        let active = self.active_actor;
        if let Some(item) = item {
            self.items[item].alive = false;
            self.select_actor(0);
            let actors: Vec<_> = self
                .actor_positions()
                .into_iter()
                .map(|(id, _)| id)
                .collect();
            for actor in actors {
                self.select_actor(actor);
                if self.held == Some(item) {
                    self.held = None;
                }
                for slot in self
                    .inventory
                    .hands
                    .iter_mut()
                    .chain(self.inventory.slots.iter_mut())
                {
                    if *slot == Some(item) {
                        *slot = None;
                    }
                }
                self.validate_work();
            }
            self.storage.items.retain(|i| *i != item);
            if item == storage::CRATE {
                let contents = std::mem::take(&mut self.storage.items);
                for i in contents {
                    self.release_body(i);
                }
            }
            self.items[item].alive = false;
            self.bodies.remove(
                self.items[item].body,
                &mut self.islands,
                &mut self.colliders,
                &mut self.joints,
                &mut self.multijoints,
                true,
            );
            self.select_actor(0);
        }
        if let Some(door) = door {
            let c = self.doors[door].cell;
            self.doors[door].alive = false;
            self.edit_voxels(&[(c, false), ([c[0], c[1], c[2] + 1], false)]);
        }
        self.select_actor(active);
        self.entities.remove(id).is_some()
    }
    pub(super) fn register_actor(&mut self, id: u64, body: RigidBodyHandle) {
        self.entities.insert(Entity {
            prototype: "character".into(),
            position: [0.; 3],
            sprite: Some(SpriteComponent {
                size: self.content.character.sprite_size,
                color: [1.; 4],
                billboard: true,
            }),
            body: Some(body),
            item: None,
            actor: Some(id),
            door: None,
            container_capacity: None,
            access: None,
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use interaction::{Command, VerbKind};
    #[test]
    fn deleted_ids_never_resolve_after_slot_reuse_or_new_world() {
        let mut s = Simulation::new(7);
        let old = s.items()[0].entity.unwrap();
        let bodies = s.bodies.len();
        let colliders = s.colliders.len();
        assert!(s.despawn_entity(old));
        assert!(!s.despawn_entity(old));
        assert_eq!(s.bodies.len(), bodies - 1);
        assert_eq!(s.colliders.len(), colliders - 1);
        assert!(!s.items()[0].visible);
        assert!(s.items()[0].entity.is_none());
        let new = s.spawn_prototype("flashlight", [1.5, 0.5, 3.]).unwrap();
        assert_eq!(old.slot, new.slot);
        assert_ne!(old.generation, new.generation);
        assert!(!s.contains_entity(old));
        s.command(Command::Verb {
            target: 0,
            kind: VerbKind::Pickup,
        });
        assert_eq!(s.held, None);
        assert!(!Simulation::new(7).contains_entity(new));
    }
    #[test]
    fn deletion_cleans_inventory_actions_storage_doors_and_actors() {
        let mut s = Simulation::new(7);
        s.prepare_departments();
        s.add_actor(1);
        s.outfit_actor(1, crew::Job::Engineer, false);
        for _ in 0..90 {
            s.step_actors(&[(1, [0.; 2])]);
        }
        s.select_actor(1);
        s.command(Command::Slot(11));
        let tool = s.held.unwrap();
        s.command(Command::Work {
            tool,
            target: actions::WorkTarget::Floor([-4, 2, 0]),
        });
        assert!(s.action().is_some());
        assert!(s.despawn_entity(s.items[tool].entity));
        assert!(s.action().is_none());
        assert_eq!(s.inventory.hands, [None; 2]);
        let card = s.inventory.slots[10].unwrap();
        assert!(s.despawn_entity(s.items[card].entity));
        assert_eq!(s.inventory.slots[10], None);
        s.storage.items.push(0);
        s.bodies[s.items[0].body].set_enabled(false);
        assert!(s.despawn_entity(s.items[storage::CRATE].entity));
        assert!(s.storage.items.is_empty());
        assert!(s.bodies[s.items[0].body].is_enabled());
        let door = s.door_snapshots()[0].entity;
        assert!(s.despawn_entity(door));
        assert!(!s.world.solid([-3, 2, 1]));
        assert!(s.door_snapshots().iter().all(|d| d.entity != door));
        let actor = s
            .entities()
            .into_iter()
            .find(|e| e.actor == Some(1))
            .unwrap()
            .id;
        assert!(s.despawn_entity(actor));
        assert!(!s.contains_entity(actor));
        s.add_actor(1);
        s.outfit_actor(1, crew::Job::Doctor, false);
        assert!(s.inventory.slots[10].is_some());
    }
}

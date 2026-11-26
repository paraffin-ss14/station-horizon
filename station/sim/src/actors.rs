use super::*;
pub(super) struct Actor {
    body: RigidBodyHandle,
    held: Option<usize>,
    inventory: InventorySnapshot,
    action: Option<actions::Action>,
    facing: [f32; 2],
}
impl Simulation {
    pub fn actor_appearances(&self) -> Vec<appearance::ActorAppearance> {
        let mut result = vec![self.actor_appearance(
            self.active_actor,
            &self.inventory,
            self.facing,
            self.player,
        )];
        result.extend(
            self.actors
                .iter()
                .filter(|(_, a)| self.bodies[a.body].is_enabled())
                .map(|(&id, a)| self.actor_appearance(id, &a.inventory, a.facing, a.body)),
        );
        result
    }
    pub(super) fn any_actor_unlocking(&self) -> bool {
        self.actors.values().any(|a| {
            a.action
                .as_ref()
                .is_some_and(|a| a.snapshot.target == actions::WorkTarget::Lock)
        })
    }

    pub fn add_actor(&mut self, id: u64) {
        if id == self.active_actor || self.actors.contains_key(&id) {
            return;
        }
        let mut spawn = self.content.map.spawns["job.assistant"];
        spawn[0] += (id % 4) as f32;
        let body = self.bodies.insert(
            RigidBodyBuilder::dynamic()
                .translation(Vector::from(spawn))
                .lock_rotations()
                .ccd_enabled(true),
        );
        self.colliders.insert_with_parent(
            ColliderBuilder::ball(self.content.character.radius).friction(0.),
            body,
            &mut self.bodies,
        );
        self.register_actor(id, body);
        self.actors.insert(
            id,
            Actor {
                body,
                held: None,
                inventory: InventorySnapshot::default(),
                action: None,
                facing: [1., 0.],
            },
        );
    }
    pub fn select_actor(&mut self, id: u64) -> bool {
        if id == self.active_actor {
            return true;
        }
        let Some(mut actor) = self.actors.remove(&id) else {
            return false;
        };
        std::mem::swap(&mut self.player, &mut actor.body);
        std::mem::swap(&mut self.held, &mut actor.held);
        std::mem::swap(&mut self.inventory, &mut actor.inventory);
        std::mem::swap(&mut self.action, &mut actor.action);
        std::mem::swap(&mut self.facing, &mut actor.facing);
        self.actors.insert(self.active_actor, actor);
        self.active_actor = id;
        true
    }
    pub fn remove_actor(&mut self, id: u64) {
        if !self.select_actor(id) {
            return;
        }
        self.outfitted.remove(&id);
        let items: Vec<_> = self
            .inventory
            .hands
            .iter()
            .chain(self.inventory.slots.iter())
            .flatten()
            .copied()
            .collect();
        self.held = None;
        self.inventory = InventorySnapshot::default();
        self.action = None;
        for item in items {
            self.release_body(item);
        }
        self.select_actor(0);
        let entity = self
            .entities
            .iter()
            .find_map(|(eid, e)| (e.actor == Some(id)).then_some(eid));
        if let Some(entity) = entity {
            self.entities.remove(entity);
        }
        if let Some(actor) = self.actors.remove(&id) {
            self.bodies.remove(
                actor.body,
                &mut self.islands,
                &mut self.colliders,
                &mut self.joints,
                &mut self.multijoints,
                true,
            );
        }
    }
    pub(super) fn stowed_by_other(&self, id: usize) -> bool {
        self.actors.values().any(|a| {
            a.held != Some(id)
                && (a.inventory.hands.contains(&Some(id)) || a.inventory.slots.contains(&Some(id)))
        })
    }
    pub(super) fn held_by_other(&self, id: usize) -> bool {
        self.actors.values().any(|a| a.held == Some(id))
    }
    pub(super) fn owned_by_other(&self, id: usize) -> bool {
        self.actors
            .values()
            .any(|a| a.inventory.hands.contains(&Some(id)) || a.inventory.slots.contains(&Some(id)))
    }
    pub fn actor_positions(&self) -> Vec<(u64, [f32; 3])> {
        let mut result = vec![(self.active_actor, self.position())];
        result.extend(self.actors.iter().map(|(&id, a)| {
            let p = self.bodies[a.body].translation();
            (id, [p.x, p.y, p.z])
        }));
        result
    }
    pub fn step_actors(&mut self, inputs: &[(u64, [f32; 2])]) {
        // Actor 0 is a disabled local placeholder in network sessions.
        self.select_actor(0);
        self.bodies[self.player].set_enabled(false);
        for &(id, movement) in inputs {
            if self.select_actor(id) {
                self.prepare_actor(movement);
            }
        }
        self.physics_step();
        for &(id, movement) in inputs {
            if self.select_actor(id) {
                self.advance_work(movement[0].hypot(movement[1]) > 0.01);
            }
        }
        self.select_actor(0);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use interaction::{Command, VerbKind};
    #[test]
    fn actors_share_items_but_not_inventory_and_disconnect_releases() {
        let mut s = Simulation::new(7);
        s.add_actor(1);
        s.add_actor(2);
        for _ in 0..100 {
            s.step_actors(&[(1, [0.; 2]), (2, [0.; 2])]);
        }
        s.select_actor(1);
        s.command(Command::Verb {
            target: 0,
            kind: VerbKind::Pickup,
        });
        assert_eq!(s.inventory().hands[0], Some(0));
        s.select_actor(2);
        s.command(Command::Verb {
            target: 0,
            kind: VerbKind::Pickup,
        });
        assert_eq!(s.inventory().hands[0], None);
        assert!(!s.reachable(0));
        s.remove_actor(1);
        assert!(!s.actor_positions().iter().any(|(id, _)| *id == 1));
        s.select_actor(2);
        assert!(!s.contained(0));
        assert!(s.items()[0].visible);
    }
    #[test]
    fn shared_physics_advances_once_and_independent_movement() {
        let mut s = Simulation::new(7);
        s.add_actor(1);
        s.add_actor(2);
        for _ in 0..60 {
            s.step_actors(&[(1, [-1., 0.]), (2, [1., 0.])]);
        }
        assert_eq!(s.tick, 60);
        s.select_actor(1);
        assert!(s.position()[0] < 0.);
        s.select_actor(2);
        assert!(s.position()[0] > 4.);
        s.set_voxel([0, 0, 0], false);
        s.select_actor(1);
        assert!(!s.world().solid([0, 0, 0]));
    }
}

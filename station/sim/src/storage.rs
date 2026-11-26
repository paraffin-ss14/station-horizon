//! Authoritative prototype storage. SS14 guards are shared with the oracle bridge.
use super::*;
use interaction::{Verb, VerbKind};
pub const CRATE: usize = 1;
pub const CROWBAR: usize = 3;
pub const CAPACITY: usize = 2;
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct StorageSnapshot {
    pub entity: usize,
    pub locked: bool,
    pub open: bool,
    pub accessible: bool,
    pub items: Vec<usize>,
    pub capacity: usize,
}
pub struct Storage {
    pub locked: bool,
    pub open: bool,
    pub items: Vec<usize>,
}
impl Default for Storage {
    fn default() -> Self {
        Self {
            locked: true,
            open: false,
            items: Vec::new(),
        }
    }
}
impl Simulation {
    pub fn storage(&self) -> StorageSnapshot {
        let accessible = self.reachable(CRATE);
        StorageSnapshot {
            entity: CRATE,
            locked: self.storage.locked,
            open: self.storage.open,
            accessible,
            items: if accessible && self.storage.open {
                self.storage.items.clone()
            } else {
                Vec::new()
            },
            capacity: self.content.prototypes[&self.items[CRATE].prototype].capacity,
        }
    }
    pub(super) fn storage_access(&self) -> Result<(), &'static str> {
        if !self.reachable(CRATE) {
            return Err("error.reach");
        }
        if self.storage.locked {
            return Err("error.locked");
        }
        if !self.storage.open {
            return Err("error.closed");
        }
        Ok(())
    }
    fn store_check(&self, item: usize) -> Result<(), &'static str> {
        self.storage_access()?;
        if self.held != Some(item) {
            return Err("error.stale_hand");
        }
        let proto = &self.content.prototypes[&self.items[item].prototype];
        if !storage_rules::storage_size_fits(proto.size, 2, proto.capacity > 0, 2).unwrap_or(false)
        {
            return Err("error.does_not_fit");
        }
        if self.storage.items.len()
            >= self.content.prototypes[&self.items[CRATE].prototype].capacity
        {
            return Err("error.full");
        }
        Ok(())
    }
    fn use_check(&self, target: usize, used: usize) -> Result<(), &'static str> {
        if self.held != Some(used) {
            return Err("error.stale_hand");
        }
        if !self.reachable(target) {
            return Err("error.reach");
        }
        if target != CRATE || target == used {
            return Err("error.interact_using_unsupported");
        }
        if self.storage.locked {
            if storage_rules::bypass_ready(
                false,
                self.items
                    .get(used)
                    .is_some_and(|i| i.kind == ItemKind::Crowbar),
                true,
                true,
            )
            .unwrap_or(false)
            {
                Ok(())
            } else {
                Err("error.tool")
            }
        } else {
            self.store_check(used)
        }
    }
    pub(super) fn storage_verbs(&self, target: usize, out: &mut Vec<Verb>) {
        if target == CRATE {
            let result = if !self.reachable(target) {
                Err("error.reach")
            } else if self.storage.locked {
                Err("error.locked")
            } else {
                Ok(())
            };
            out.push(Verb {
                kind: if self.storage.open {
                    VerbKind::Close
                } else {
                    VerbKind::Open
                },
                label: if self.storage.open {
                    "verb.close_storage"
                } else {
                    "verb.open_storage"
                },
                category: "category.interaction",
                enabled: result.is_ok(),
                reason: result.err().unwrap_or(""),
                priority: 3,
            });
        }
        if let Some(used) = self.held.filter(|used| *used != target) {
            let result = self.use_check(target, used);
            out.push(Verb {
                kind: if target == CRATE && self.storage.locked {
                    VerbKind::Pry(used)
                } else {
                    VerbKind::UseOn(used)
                },
                label: if target == CRATE && self.storage.locked {
                    "verb.pry"
                } else {
                    "verb.use_on"
                },
                category: "category.interaction",
                enabled: result.is_ok(),
                reason: result.err().unwrap_or(""),
                priority: 5,
            });
        }
    }
    pub(super) fn use_on(&mut self, target: usize, used: usize) {
        if let Err(reason) = self.use_check(target, used) {
            self.inventory.message = (reason).into();
            return;
        }
        if self.storage.locked {
            self.begin_work(actions::WorkTarget::Lock, used);
        } else {
            self.store_item(target, used);
        }
    }
    pub(super) fn store_item(&mut self, container: usize, item: usize) {
        if container != CRATE {
            self.inventory.message = "error.action".into();
            return;
        }
        if let Err(reason) = self.store_check(item) {
            self.inventory.message = (reason).into();
            return;
        }
        self.held = None;
        self.inventory.hands[self.inventory.active] = None;
        self.bodies[self.items[item].body].set_enabled(false);
        self.storage.items.push(item);
        self.inventory.message = "status.stored".into();
    }
    pub(super) fn take_item(&mut self, container: usize, item: usize) {
        if container != CRATE {
            self.inventory.message = "error.action".into();
            return;
        }
        if let Err(reason) = self.storage_access() {
            self.inventory.message = (reason).into();
            return;
        }
        if self.held.is_some() {
            self.inventory.message = "error.hand_full".into();
            return;
        }
        let Some(index) = self.storage.items.iter().position(|id| *id == item) else {
            self.inventory.message = "error.action".into();
            return;
        };
        self.storage.items.remove(index);
        self.held = Some(item);
        self.inventory.hands[self.inventory.active] = Some(item);
        self.release_body(item);
        self.inventory.message = "status.taken".into();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use interaction::Command;
    fn near() -> Simulation {
        let mut s = Simulation::new(7);
        for _ in 0..180 {
            s.step([0., 0.]);
        }
        let p = s.position();
        s.bodies[s.items[CRATE].body].set_translation(vector![p[0] + 1., p[1], p[2]], true);
        for id in [0, 2, CROWBAR] {
            s.bodies[s.items[id].body].set_translation(vector![p[0], p[1] + 0.6, p[2]], true);
        }
        s
    }
    fn verb(s: &mut Simulation, target: usize, kind: VerbKind) {
        s.command(Command::Verb { target, kind });
    }
    fn unlock(s: &mut Simulation) {
        verb(s, CROWBAR, VerbKind::Pickup);
        verb(s, CRATE, VerbKind::Pry(CROWBAR));
        for _ in 0..120 {
            s.step([0., 0.]);
        }
        assert!(!s.storage.locked);
        verb(s, CRATE, VerbKind::Open);
        assert!(s.storage.open);
    }
    fn assert_unique(s: &Simulation) {
        let mut ids = std::collections::BTreeSet::new();
        for id in s
            .inventory
            .hands
            .iter()
            .chain(&s.inventory.slots)
            .flatten()
            .copied()
            .chain(s.storage.items.iter().copied())
        {
            assert!(ids.insert(id), "duplicate {id}");
        }
        for id in &s.storage.items {
            assert!(!s.bodies[s.items[*id].body].is_enabled());
            assert!(!s.items()[*id].visible);
        }
        assert_eq!(s.held, s.inventory.hands[s.inventory.active]);
    }
    #[test]
    fn unlock_store_take_use_and_return() {
        let mut s = near();
        verb(&mut s, CRATE, VerbKind::Open);
        assert!(!s.storage.open);
        unlock(&mut s);
        s.command(Command::Store {
            container: CRATE,
            item: CROWBAR,
        });
        assert_unique(&s);
        s.command(Command::Take {
            container: CRATE,
            item: CROWBAR,
        });
        assert_eq!(s.held, Some(CROWBAR));
        assert!(s.bodies[s.items[CROWBAR].body].is_enabled());
        verb(&mut s, CRATE, VerbKind::UseOn(CROWBAR));
        assert_eq!(s.storage.items, vec![CROWBAR]);
        assert_unique(&s);
        verb(&mut s, CRATE, VerbKind::Close);
        assert!(s.storage().items.is_empty());
        s.command(Command::Take {
            container: CRATE,
            item: CROWBAR,
        });
        assert_eq!(s.held, None);
        verb(&mut s, CRATE, VerbKind::Open);
        s.command(Command::Take {
            container: CRATE,
            item: CROWBAR,
        });
        assert_eq!(s.held, Some(CROWBAR));
        assert_unique(&s);
    }
    #[test]
    fn full_and_replayed_requests_preserve_all_items() {
        let mut s = near();
        unlock(&mut s);
        for item in [CROWBAR, 0] {
            if item == 0 {
                verb(&mut s, 0, VerbKind::Pickup);
            }
            s.command(Command::Store {
                container: CRATE,
                item,
            });
        }
        verb(&mut s, 2, VerbKind::Pickup);
        s.command(Command::Store {
            container: CRATE,
            item: 2,
        });
        assert_eq!(s.inventory.message, "error.full");
        assert_eq!(s.held, Some(2));
        assert_eq!(s.storage.items, vec![CROWBAR, 0]);
        s.command(Command::Take {
            container: CRATE,
            item: 0,
        });
        assert_eq!(s.held, Some(2));
        s.command(Command::Store {
            container: CRATE,
            item: 0,
        });
        assert_eq!(s.held, Some(2));
        assert_unique(&s);
        s.drop_item();
        s.command(Command::Take {
            container: CRATE,
            item: 0,
        });
        s.command(Command::Take {
            container: CRATE,
            item: 0,
        });
        assert_eq!(s.held, Some(0));
        assert_eq!(s.storage.items, vec![CROWBAR]);
        assert_unique(&s);
    }
    #[test]
    fn wrong_tool_wall_distance_and_stale_hand_rejected() {
        let mut s = near();
        verb(&mut s, 0, VerbKind::Pickup);
        verb(&mut s, CRATE, VerbKind::UseOn(0));
        assert!(s.storage.locked);
        s.drop_item();
        unlock(&mut s);
        s.command(Command::Store {
            container: CRATE,
            item: CROWBAR,
        });
        let p = s.position();
        s.world.set([1, 0, 1], true);
        s.command(Command::Take {
            container: CRATE,
            item: CROWBAR,
        });
        assert_eq!(s.held, None);
        assert!(s.storage().items.is_empty());
        s.world.set([1, 0, 1], false);
        s.bodies[s.items[CRATE].body].set_translation(vector![p[0] + 10., p[1], p[2]], true);
        s.command(Command::Take {
            container: CRATE,
            item: CROWBAR,
        });
        assert_eq!(s.held, None);
        assert_unique(&s);
        s.bodies[s.items[CRATE].body].set_translation(vector![p[0] + 1., p[1], p[2]], true);
        s.command(Command::Take {
            container: CRATE,
            item: CROWBAR,
        });
        s.command(Command::Swap);
        verb(&mut s, CRATE, VerbKind::UseOn(CROWBAR));
        assert!(s.storage.items.is_empty());
        assert_eq!(s.inventory.hands, [Some(CROWBAR), None]);
        assert_unique(&s);
    }
    #[test]
    fn hidden_items_cannot_be_picked_and_crate_cannot_contain_itself() {
        let mut s = near();
        unlock(&mut s);
        s.command(Command::Store {
            container: CRATE,
            item: CROWBAR,
        });
        verb(&mut s, CROWBAR, VerbKind::Pickup);
        assert_eq!(s.held, None);
        s.pickup();
        assert_ne!(s.held, Some(CROWBAR));
        s.drop_item();
        verb(&mut s, CRATE, VerbKind::Pickup);
        s.command(Command::Store {
            container: CRATE,
            item: CRATE,
        });
        assert_eq!(s.held, Some(CRATE));
        assert_eq!(s.storage.items, vec![CROWBAR]);
        assert_unique(&s);
        s.command(Command::Take {
            container: 999,
            item: 999,
        });
        assert_unique(&s);
    }
}

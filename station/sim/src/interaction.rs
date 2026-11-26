//! SS14-inspired interaction adapters; pure upstream branches are in ss14_rules.
//! Copyright (c) 2017-2026 Space Wizards Federation; adapted under MIT.
use super::*;
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerbKind {
    Examine,
    Pickup,
    Drop,
    Throw,
    Activate,
    Open,
    Close,
    UseOn(usize),
    Pry(usize),
    Equip(usize),
}
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Transfer {
        item: usize,
        from: usize,
        to: usize,
    },
    Door(usize),
    PowerSwitch {
        source: bool,
        id: usize,
    },
    Verb {
        target: usize,
        kind: VerbKind,
    },
    Slot(usize),
    Hand(usize),
    Swap,
    Work {
        target: actions::WorkTarget,
        tool: usize,
    },
    CancelWork(u64),
    Store {
        container: usize,
        item: usize,
    },
    Take {
        container: usize,
        item: usize,
    },
}
#[derive(serde::Serialize, Clone, Debug)]
pub struct Verb {
    pub kind: VerbKind,
    pub label: &'static str,
    pub category: &'static str,
    pub enabled: bool,
    pub reason: &'static str,
    pub priority: i64,
}
impl<'de> serde::Deserialize<'de> for Verb {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Owned {
            pub kind: VerbKind,
            pub label: String,
            pub category: String,
            pub enabled: bool,
            pub reason: String,
            pub priority: i64,
        }
        let v = Owned::deserialize(d)?;
        Ok(Self {
            kind: v.kind,
            label: crate::wire::lookup(&v.label)
                .ok_or_else(|| serde::de::Error::custom("unknown key"))?,
            category: crate::wire::lookup(&v.category)
                .ok_or_else(|| serde::de::Error::custom("unknown key"))?,
            enabled: v.enabled,
            reason: crate::wire::lookup(&v.reason)
                .ok_or_else(|| serde::de::Error::custom("unknown key"))?,
            priority: v.priority,
        })
    }
}

#[derive(Clone, Copy)]
pub struct Slot {
    pub name: &'static str,
    pub flag: i64,
    pub depends: Option<usize>,
}
pub const SLOTS: [Slot; 16] = [
    Slot {
        name: "slot.head",
        flag: 2,
        depends: None,
    },
    Slot {
        name: "slot.eyes",
        flag: 4,
        depends: None,
    },
    Slot {
        name: "slot.ears",
        flag: 8,
        depends: None,
    },
    Slot {
        name: "slot.mask",
        flag: 16,
        depends: None,
    },
    Slot {
        name: "slot.neck",
        flag: 128,
        depends: None,
    },
    Slot {
        name: "slot.back",
        flag: 256,
        depends: None,
    },
    Slot {
        name: "slot.outer",
        flag: 32,
        depends: None,
    },
    Slot {
        name: "slot.uniform",
        flag: 64,
        depends: None,
    },
    Slot {
        name: "slot.belt",
        flag: 512,
        depends: None,
    },
    Slot {
        name: "slot.gloves",
        flag: 1024,
        depends: None,
    },
    Slot {
        name: "slot.id",
        flag: 2048,
        depends: None,
    },
    Slot {
        name: "slot.left_pocket",
        flag: 4096,
        depends: None,
    },
    Slot {
        name: "slot.right_pocket",
        flag: 4096,
        depends: None,
    },
    Slot {
        name: "slot.legs",
        flag: 8192,
        depends: None,
    },
    Slot {
        name: "slot.shoes",
        flag: 16384,
        depends: None,
    },
    Slot {
        name: "slot.suit_storage",
        flag: 32768,
        depends: Some(6),
    },
];
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct InventorySnapshot {
    pub hands: [Option<usize>; 2],
    pub active: usize,
    pub slots: [Option<usize>; 16],
    pub message: String,
    pub fit_masks: Vec<u16>,
}
impl InventorySnapshot {
    /// Slots 0..16, hands 16..18. Never swaps or displaces an occupied destination.
    pub fn at(&self, place: usize) -> Option<usize> {
        if place < 16 {
            self.slots[place]
        } else {
            self.hands.get(place - 16).copied().flatten()
        }
    }
    pub fn transfer_allowed(&self, item: usize, from: usize, to: usize) -> bool {
        from < 18
            && to < 18
            && from != to
            && self.at(from) == Some(item)
            && self.at(to).is_none()
            && !SLOTS
                .iter()
                .enumerate()
                .any(|(i, s)| s.depends == Some(from) && self.slots[i].is_some())
            && (to >= 16
                || (SLOTS[to].depends != Some(from)
                    && self.fit_masks.get(item).is_some_and(|m| m & (1 << to) != 0)))
    }
}
impl Simulation {
    pub fn inventory(&self) -> InventorySnapshot {
        let mut snapshot = self.inventory.clone();
        snapshot.fit_masks = (0..self.items.len())
            .map(|id| {
                (0..16).fold(0, |mask, slot| {
                    mask | if self.fits(id, slot) { 1 << slot } else { 0 }
                })
            })
            .collect();
        snapshot
    }
    pub(super) fn contained(&self, id: usize) -> bool {
        self.owned_by_other(id)
            || self.inventory.hands.contains(&Some(id))
            || self.inventory.slots.contains(&Some(id))
            || self.storage.items.contains(&id)
    }
    pub fn reachable(&self, id: usize) -> bool {
        if !self.items.get(id).is_some_and(|i| i.alive) || self.owned_by_other(id) {
            return false;
        }
        let Some(item) = self.items.get(id) else {
            return false;
        };
        if self.storage.items.contains(&id) {
            return self.storage_access().is_ok();
        }
        if self.contained(id) {
            return true;
        }
        let p = self.position();
        let q = self.bodies[item.body].translation();
        let d = [q.x - p[0], q.y - p[1], q.z - p[2]];
        d.iter().map(|v| v * v).sum::<f32>() <= 1.6 * 1.6
            && (1..=32).all(|i| {
                let t = i as f32 / 32.;
                !self
                    .world
                    .blocks_movement(std::array::from_fn(|a| (p[a] + d[a] * t).floor() as i32))
            })
    }
    fn fits(&self, id: usize, slot: usize) -> bool {
        let Some(def) = SLOTS.get(slot) else {
            return false;
        };
        let Some(item) = self.items.get(id) else {
            return false;
        };
        let p = &self.content.prototypes[&item.prototype];
        let (clothing, flags, size) = (p.clothing, p.slots, p.size);
        def.depends
            .is_none_or(|d| self.inventory.slots[d].is_some())
            && ss14_rules::can_fit(def.flag, flags, clothing, true, size, 1).unwrap_or(false)
    }
    pub fn verbs(&self, id: usize) -> Vec<Verb> {
        if !self.items.get(id).is_some_and(|i| i.alive) {
            return vec![];
        }
        let reach = self.reachable(id);
        let active = self.held == Some(id);
        let mut verbs = vec![Verb {
            kind: VerbKind::Examine,
            label: "verb.examine",
            category: "category.other",
            enabled: reach,
            reason: "error.reach",
            priority: 0,
        }];
        if !self.contained(id) {
            verbs.push(Verb {
                kind: VerbKind::Pickup,
                label: "verb.pickup",
                category: "category.interaction",
                enabled: reach && self.held.is_none(),
                reason: if !reach {
                    "error.reach"
                } else {
                    "error.hand_full"
                },
                priority: 4,
            });
        }
        if active {
            verbs.push(Verb {
                kind: VerbKind::Drop,
                label: "verb.drop",
                category: "category.interaction",
                enabled: true,
                reason: "",
                priority: 4,
            });
            verbs.push(Verb {
                kind: VerbKind::Throw,
                label: "verb.throw",
                category: "category.interaction",
                enabled: true,
                reason: "",
                priority: 4,
            });
            for (slot, def) in SLOTS.iter().enumerate() {
                if self.fits(id, slot) {
                    verbs.push(Verb {
                        kind: VerbKind::Equip(slot),
                        label: def.name,
                        category: "category.equipment",
                        enabled: self.inventory.slots[slot].is_none(),
                        reason: "error.occupied",
                        priority: 1,
                    });
                }
            }
        }
        if self.items[id].kind == ItemKind::Flashlight {
            verbs.push(Verb {
                kind: VerbKind::Activate,
                label: if self.items[id].activated {
                    "verb.deactivate"
                } else {
                    "verb.activate"
                },
                category: "category.interaction",
                enabled: reach,
                reason: "error.reach",
                priority: 1,
            });
        }
        self.storage_verbs(id, &mut verbs);
        verbs.sort_by(|a, b| {
            ss14_rules::priority(0, 0, a.priority, b.priority)
                .unwrap_or(0)
                .cmp(&0)
        });
        verbs
    }
    pub fn command(&mut self, command: Command) {
        self.inventory.message = "".into();
        match command {
            Command::Transfer { item, from, to } => self.transfer(item, from, to),
            Command::PowerSwitch { source, id } => self.power_switch(source, id),
            Command::Door(id) => self.toggle_door(id),
            Command::Work { target, tool } => self.begin_work(target, tool),
            Command::CancelWork(id) => self.cancel_work(id),
            Command::Store { container, item } => self.store_item(container, item),
            Command::Take { container, item } => self.take_item(container, item),
            Command::Swap => {
                let hand = ss14_rules::next_hand(self.inventory.active as i64, 2, false)
                    .unwrap_or(0) as usize;
                self.select_hand(hand);
            }
            Command::Hand(hand) => self.select_hand(hand),
            Command::Slot(slot) => self.use_slot(slot),
            Command::Verb { target, kind } => {
                let Some(verb) = self.verbs(target).into_iter().find(|v| v.kind == kind) else {
                    self.inventory.message = "error.action".into();
                    return;
                };
                if !verb.enabled {
                    self.inventory.message = (verb.reason).into();
                    return;
                }
                match kind {
                    VerbKind::Examine => {
                        self.inventory.message = self.content.prototypes
                            [&self.items[target].prototype]
                            .description
                            .clone();
                    }
                    VerbKind::Pickup => {
                        self.held = Some(target);
                        self.inventory.hands[self.inventory.active] = Some(target);
                    }
                    VerbKind::Drop => self.drop_item(),
                    VerbKind::Throw => self.throw_item(),
                    VerbKind::Activate => {
                        self.items[target].activated = !self.items[target].activated;
                        self.inventory.message = (if self.items[target].activated {
                            "status.active"
                        } else {
                            "status.inactive"
                        })
                        .into();
                    }
                    VerbKind::Equip(slot) => self.use_slot(slot),
                    VerbKind::Open => self.storage.open = true,
                    VerbKind::Close => self.storage.open = false,
                    VerbKind::UseOn(used) => self.use_on(target, used),
                    VerbKind::Pry(used) => self.begin_work(actions::WorkTarget::Lock, used),
                }
            }
        }
        self.validate_work();
    }
    fn transfer(&mut self, item: usize, from: usize, to: usize) {
        if !self.items.get(item).is_some_and(|i| i.alive)
            || !self.inventory().transfer_allowed(item, from, to)
        {
            self.inventory.message = "error.action".into();
            return;
        }
        if from < 16 {
            self.inventory.slots[from] = None;
        } else {
            self.inventory.hands[from - 16] = None;
        }
        if to < 16 {
            self.inventory.slots[to] = Some(item);
        } else {
            self.inventory.hands[to - 16] = Some(item);
        }
        self.held = self.inventory.hands[self.inventory.active];
        if self.held == Some(item) {
            self.release_body(item);
        } else {
            self.bodies[self.items[item].body].set_enabled(false);
        }
    }
    fn select_hand(&mut self, hand: usize) {
        if hand >= 2 {
            self.inventory.message = "error.action".into();
            return;
        }
        if hand == self.inventory.active {
            return;
        }
        if let Some(id) = self.held {
            self.bodies[self.items[id].body].set_enabled(false);
        }
        self.inventory.active = hand;
        self.held = self.inventory.hands[hand];
        if let Some(id) = self.held {
            self.release_body(id);
        }
    }
    pub(super) fn release_body(&mut self, id: usize) {
        let p = self.position();
        let body = &mut self.bodies[self.items[id].body];
        body.set_translation(vector![p[0], p[1], p[2] + 0.8], true);
        body.set_linvel(vector![0., 0., 0.], true);
        body.set_angvel(vector![0., 0., 0.], true);
        body.set_enabled(true);
    }
    fn use_slot(&mut self, slot: usize) {
        if slot >= SLOTS.len() {
            self.inventory.message = "error.action".into();
            return;
        }
        let item = self.inventory.slots[slot];
        match ss14_rules::slot_action(self.held.is_some(), item.is_some()).unwrap_or(0) {
            3 => {
                self.use_on(item.unwrap(), self.held.unwrap());
            }
            2 => {
                let id = item.unwrap();
                self.inventory.slots[slot] = None;
                self.inventory.hands[self.inventory.active] = Some(id);
                self.held = Some(id);
                self.release_body(id);
                for (child, def) in SLOTS.iter().enumerate() {
                    if def.depends == Some(slot)
                        && let Some(id) = self.inventory.slots[child].take()
                    {
                        self.release_body(id);
                    }
                }
            }
            1 => {
                let id = self.held.unwrap();
                if !self.fits(id, slot) {
                    self.inventory.message = "error.does_not_fit".into();
                    return;
                }
                self.held = None;
                self.inventory.hands[self.inventory.active] = None;
                self.inventory.slots[slot] = Some(id);
                self.bodies[self.items[id].body].set_enabled(false);
            }
            _ => {}
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drag_transfers_are_atomic_reject_stale_and_keep_active_hand() {
        let mut s = settled();
        pick(&mut s, 0);
        s.command(Command::Transfer {
            item: 0,
            from: 16,
            to: 0,
        });
        assert_eq!(s.inventory.hands[0], Some(0)); // flashlight is not headwear
        s.command(Command::Transfer {
            item: 0,
            from: 16,
            to: 11,
        });
        assert_eq!(s.inventory.slots[11], Some(0));
        assert_eq!(s.held, None);
        s.command(Command::Transfer {
            item: 0,
            from: 16,
            to: 12,
        }); // stale source
        assert_eq!(s.inventory.slots[11], Some(0));
        assert_eq!(s.inventory.slots[12], None);
        s.command(Command::Transfer {
            item: 0,
            from: 11,
            to: 17,
        });
        assert_eq!(s.inventory.hands, [None, Some(0)]);
        assert_eq!(s.held, None);
        s.command(Command::Transfer {
            item: 0,
            from: 17,
            to: 16,
        });
        assert_eq!(s.held, Some(0));
        s.command(Command::Transfer {
            item: 0,
            from: 16,
            to: usize::MAX,
        });
        assert_eq!(s.held, Some(0));
        s.command(Command::Transfer {
            item: 0,
            from: 16,
            to: 11,
        });
        s.command(Command::Transfer {
            item: 0,
            from: 11,
            to: 12,
        });
        assert_eq!(s.inventory.slots[11], None);
        assert_eq!(s.inventory.slots[12], Some(0));
    }
    #[test]
    fn drag_rejects_occupied_slots_and_another_actors_item() {
        let mut s = settled();
        pick(&mut s, 0);
        s.command(Command::Transfer {
            item: 0,
            from: 16,
            to: 11,
        });
        s.inventory.hands[0] = Some(2);
        let before = s.inventory.clone();
        s.command(Command::Transfer {
            item: 2,
            from: 16,
            to: 11,
        });
        assert_eq!(s.inventory.hands, before.hands);
        assert_eq!(s.inventory.slots, before.slots);
        s.add_actor(99);
        assert!(s.select_actor(99));
        s.command(Command::Transfer {
            item: 0,
            from: 11,
            to: 16,
        });
        assert_eq!(s.inventory.hands, [None, None]);
    }
    fn settled() -> Simulation {
        let mut s = Simulation::new(7);
        for _ in 0..180 {
            s.step([0., 0.]);
        }
        s
    }
    fn pick(s: &mut Simulation, id: usize) {
        s.command(Command::Verb {
            target: id,
            kind: VerbKind::Pickup,
        });
    }
    #[test]
    fn pockets_preserve_identity_and_disable_physics() {
        let mut s = settled();
        pick(&mut s, 0);
        assert_eq!(s.held, Some(0));
        s.command(Command::Slot(11));
        assert_eq!(s.inventory.slots[11], Some(0));
        assert_eq!(s.held, None);
        assert!(!s.items()[0].visible);
        assert!(!s.bodies[s.items[0].body].is_enabled());
        for _ in 0..60 {
            s.step([0., 0.]);
        }
        s.command(Command::Slot(11));
        assert_eq!(s.held, Some(0));
        assert_eq!(s.inventory.slots[11], None);
        assert!(s.bodies[s.items[0].body].is_enabled());
        s.command(Command::Verb {
            target: 0,
            kind: VerbKind::Throw,
        });
        assert_eq!(s.inventory.hands, [None, None]);
        assert!(s.items()[0].visible);
    }
    #[test]
    fn hand_switch_retains_items_without_duplication() {
        let mut s = settled();
        pick(&mut s, 0);
        s.command(Command::Swap);
        assert_eq!(s.inventory.hands, [Some(0), None]);
        assert_eq!(s.held, None);
        assert!(!s.items()[0].visible);
        pick(&mut s, 0);
        assert_eq!(s.held, None);
        s.command(Command::Swap);
        assert_eq!(s.held, Some(0));
        assert!(s.items()[0].visible);
        s.command(Command::Hand(999));
        assert_eq!(s.inventory.active, 0);
    }
    #[test]
    fn rejects_stale_out_of_range_and_invalid_slots() {
        let mut s = settled();
        pick(&mut s, 1);
        assert_eq!(s.held, None);
        assert_eq!(s.inventory.message, "error.reach");
        pick(&mut s, 0);
        s.command(Command::Slot(0));
        assert_eq!(s.held, Some(0));
        assert_eq!(s.inventory.message, "error.does_not_fit");
        s.command(Command::Slot(999));
        assert_eq!(s.held, Some(0));
        s.command(Command::Verb {
            target: 999,
            kind: VerbKind::Examine,
        });
        assert_eq!(s.inventory.message, "error.action");
        assert!(!s.fits(1, 11));
        assert!(s.fits(2, 0));
        assert!(!s.fits(2, 1));
    }
    #[test]
    fn occupied_slot_keeps_both_items_and_wall_blocks_actions() {
        let mut s = settled();
        pick(&mut s, 0);
        s.command(Command::Slot(11));
        let p = s.position();
        s.bodies[s.items[2].body].set_translation(vector![p[0] + 0.5, p[1], p[2]], true);
        pick(&mut s, 2);
        assert_eq!(s.held, Some(2));
        s.command(Command::Slot(11));
        assert_eq!(s.inventory.slots[11], Some(0));
        assert_eq!(s.held, Some(2));
        s.command(Command::Slot(0));
        assert_eq!(s.inventory.slots[0], Some(2));
        assert_eq!(s.held, None);
        s.bodies[s.items[1].body].set_translation(vector![p[0] + 1.2, p[1], p[2]], true);
        s.world.set([1, 0, 1], true);
        assert!(!s.reachable(1));
        s.command(Command::Verb {
            target: 1,
            kind: VerbKind::Pickup,
        });
        assert_eq!(s.held, None);
    }
}

//! Authoritative equipment layers. Clients receive visuals, never invent gear.
use super::*;
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ActorAppearance {
    pub id: u64,
    pub facing: [f32; 2],
    pub moving: bool,
    pub layers: Vec<sprite::SpriteLayer>,
    pub style: sprite::SpriteStyle,
}
impl Simulation {
    pub(super) fn item_layers(&self, id: usize) -> Vec<sprite::SpriteLayer> {
        let item = &self.items[id];
        let v = &self.content.prototypes[&item.prototype].sprite;
        let mut layers = if id == storage::CRATE && self.storage.open && !v.open_layers.is_empty() {
            v.open_layers.clone()
        } else if item.activated && !v.active_layers.is_empty() {
            v.active_layers.clone()
        } else {
            v.layers.clone()
        };
        if id == storage::CRATE
            && (self
                .action()
                .is_some_and(|a| a.target == actions::WorkTarget::Lock)
                || self.any_actor_unlocking())
        {
            layers.extend(v.working_layers.clone());
        }
        let data = std::collections::BTreeMap::from([
            ("activated".into(), item.activated.to_string()),
            (
                "open".into(),
                (id == storage::CRATE && self.storage.open).to_string(),
            ),
            (
                "held".into(),
                (self.held == Some(id) || self.held_by_other(id)).to_string(),
            ),
        ]);
        let mut component = sprite::SpriteComponent {
            style: v.style.clone(),
            layers,
        };
        // Definitions are validated on load. Preserve the original visual if a
        // rule targets a layer absent from this particular variant.
        let _ = component.appearance(&data, &v.rules);
        component.layers
    }
    pub(super) fn actor_appearance(
        &self,
        id: u64,
        inventory: &InventorySnapshot,
        facing: [f32; 2],
        body: RigidBodyHandle,
    ) -> ActorAppearance {
        let mut layers = self.content.character.layers.clone();
        // Back/underclothes first, helmet last. Order is independent of equip time.
        for slot in [5, 7, 13, 14, 6, 8, 9, 4, 3, 2, 1, 0] {
            if let Some(item) = inventory.slots[slot]
                .and_then(|i| self.items.get(i))
                .filter(|i| i.alive)
            {
                layers.extend(keyed(
                    self.content.prototypes[&item.prototype].sprite.worn.clone(),
                    &format!("slot{slot}"),
                ));
            }
        }
        for (hand, item) in inventory.hands.iter().enumerate() {
            if let Some(item) = item.and_then(|i| self.items.get(i)).filter(|i| i.alive) {
                let v = &self.content.prototypes[&item.prototype].sprite;
                layers.extend(keyed(
                    if hand == 0 {
                        v.inhand_left.clone()
                    } else {
                        v.inhand_right.clone()
                    },
                    &format!("hand{hand}"),
                ));
            }
        }
        let v = self.bodies[body].linvel();
        ActorAppearance {
            style: self.content.character.style.clone(),
            id,
            facing,
            moving: v.x.hypot(v.y) > 0.05,
            layers,
        }
    }
}
fn keyed(mut layers: Vec<sprite::SpriteLayer>, prefix: &str) -> Vec<sprite::SpriteLayer> {
    for (index, l) in layers.iter_mut().enumerate() {
        l.key = format!(
            "{prefix}_{}",
            if l.key.is_empty() {
                index.to_string()
            } else {
                l.key.clone()
            }
        );
        for alias in &mut l.aliases {
            *alias = format!("{prefix}_{alias}");
        }
        if let Some(mask) = &mut l.mask {
            *mask = format!("{prefix}_{mask}");
        }
    }
    layers
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn equipment_hands_and_remote_actor_layers_follow_authority() {
        let mut s = Simulation::new(7);
        s.prepare_departments();
        s.add_actor(1);
        s.outfit_actor(1, crew::Job::Doctor, false);
        s.select_actor(1);
        let appearance = |s: &Simulation| {
            s.actor_appearances()
                .into_iter()
                .find(|a| a.id == 1)
                .unwrap()
        };
        assert!(
            appearance(&s)
                .layers
                .iter()
                .any(|l| l.rsi == "doctor" && l.state == "equipped-INNERCLOTHING")
        );
        s.command(interaction::Command::Slot(11));
        assert!(
            appearance(&s)
                .layers
                .iter()
                .any(|l| l.rsi == "medkit" && l.state == "firstaid-inhand-left")
        );
        s.command(interaction::Command::Hand(1));
        assert!(appearance(&s).layers.iter().any(|l| l.rsi == "medkit"));
        s.command(interaction::Command::Hand(0));
        s.drop_item();
        assert!(!appearance(&s).layers.iter().any(|l| l.rsi == "medkit"));
        s.command(interaction::Command::Slot(7));
        assert!(
            !appearance(&s)
                .layers
                .iter()
                .any(|l| l.rsi == "doctor" && l.state == "equipped-INNERCLOTHING")
        );
        s.drop_item();
        let position = s.position();
        s.bodies[s.items[2].body].set_translation(Vector::from(position), true);
        s.command(interaction::Command::Verb {
            target: 2,
            kind: interaction::VerbKind::Pickup,
        });
        s.command(interaction::Command::Verb {
            target: 2,
            kind: interaction::VerbKind::Equip(0),
        });
        assert!(
            appearance(&s)
                .layers
                .iter()
                .any(|l| l.rsi == "helmet" && l.state == "equipped-HELMET")
        );
        s.select_actor(0);
        assert!(s.actor_appearances().iter().any(|a| a.id == 1));
        s.remove_actor(1);
        assert!(!s.actor_appearances().iter().any(|a| a.id == 1));
    }
}

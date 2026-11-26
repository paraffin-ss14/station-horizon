//! Serializable station appearance data; renderer and simulation share the schema.
use serde::{Deserialize, Serialize};
fn yes() -> bool {
    true
}
fn speed() -> f32 {
    1.
}
fn white() -> [f32; 4] {
    [1.; 4]
}
fn unit() -> [f32; 2] {
    [1.; 2]
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SpriteLayer {
    #[serde(default)]
    pub rsi: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default)]
    pub rotation: f32,
    #[serde(default)]
    pub mirror: [bool; 2],
    #[serde(default)]
    pub direction_offset: i8,
    #[serde(default)]
    pub direction: Option<[f32; 2]>,
    #[serde(default = "yes")]
    pub auto_animated: bool,
    #[serde(default = "yes")]
    pub looping: bool,
    #[serde(default)]
    pub cycle: bool,
    #[serde(default)]
    pub reversed: bool,
    #[serde(default)]
    pub synchronized: bool,
    #[serde(default)]
    pub animation_time: f32,
    #[serde(default = "speed")]
    pub playback_speed: f32,
    #[serde(default)]
    pub restart: u64,
    #[serde(default)]
    pub unshaded: bool,
    #[serde(default)]
    pub mask: Option<String>,
    #[serde(default)]
    pub depth: i16,

    #[serde(default = "white")]
    pub color: [f32; 4],
    #[serde(default)]
    pub offset: [f32; 2],
    #[serde(default = "unit")]
    pub scale: [f32; 2],
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SpriteVisual {
    #[serde(default)]
    pub rules: Vec<AppearanceRule>,
    #[serde(default)]
    pub layers: Vec<SpriteLayer>,
    #[serde(default)]
    pub style: SpriteStyle,
    #[serde(default)]
    pub active_layers: Vec<SpriteLayer>,
    #[serde(default)]
    pub open_layers: Vec<SpriteLayer>,
    #[serde(default)]
    pub working_layers: Vec<SpriteLayer>,
    #[serde(default)]
    pub inhand_left: Vec<SpriteLayer>,
    #[serde(default)]
    pub inhand_right: Vec<SpriteLayer>,
    #[serde(default)]
    pub worn: Vec<SpriteLayer>,
}
impl SpriteLayer {
    pub fn valid(&self) -> bool {
        fn name(v: &str) -> bool {
            !v.is_empty()
                && v.len() <= 96
                && v.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        }
        self.aliases.len() <= 16
            && self.aliases.iter().all(|a| name(a))
            && (self.key.is_empty() || name(&self.key))
            && self.mask.as_ref().is_none_or(|m| name(m))
            && self.rotation.is_finite()
            && self.rotation.abs() <= 36000.
            && (-8..=8).contains(&self.direction_offset)
            && self
                .direction
                .is_none_or(|d| d.iter().all(|v| v.is_finite()))
            && self.animation_time.is_finite()
            && (0.0..=3600.).contains(&self.animation_time)
            && self.playback_speed.is_finite()
            && (0.0..=16.).contains(&self.playback_speed)
            && ((self.rsi.is_empty() && self.state.is_empty())
                || (name(&self.rsi) && name(&self.state)))
            && self
                .color
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            && self.offset.iter().all(|v| v.is_finite() && v.abs() <= 2.)
            && self
                .scale
                .iter()
                .all(|v| v.is_finite() && (0.1..=4.).contains(v))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct SpriteStyle {
    pub visible: bool,
    pub color: [f32; 4],
    pub offset: [f32; 2],
    pub scale: [f32; 2],
    pub rotation: f32,
    pub draw_depth: i16,
    pub no_rotation: bool,
    pub direction: Option<[f32; 2]>,
    pub looping: bool,
}
impl Default for SpriteStyle {
    fn default() -> Self {
        Self {
            visible: true,
            color: [1.; 4],
            offset: [0.; 2],
            scale: [1.; 2],
            rotation: 0.,
            draw_depth: 0,
            no_rotation: true,
            direction: None,
            looping: true,
        }
    }
}
impl SpriteStyle {
    pub fn valid(&self) -> bool {
        self.color
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            && self.offset.iter().all(|v| v.is_finite() && v.abs() <= 8.)
            && self
                .scale
                .iter()
                .all(|v| v.is_finite() && (0.01..=8.).contains(v))
            && self.rotation.is_finite()
            && self.rotation.abs() <= 36000.
            && self
                .direction
                .is_none_or(|d| d.iter().all(|v| v.is_finite()))
    }
}
/// Keyed layer operations used by appearance systems and editors. Mutations are atomic.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SpriteComponent {
    pub style: SpriteStyle,
    pub layers: Vec<SpriteLayer>,
}
impl SpriteComponent {
    pub fn index(&self, key: &str) -> Option<usize> {
        self.layers
            .iter()
            .position(|l| l.key == key || l.aliases.iter().any(|a| a == key))
    }
    pub fn insert(&mut self, at: usize, layer: SpriteLayer) -> Result<(), String> {
        if at > self.layers.len()
            || self.layers.len() >= 64
            || !layer.valid()
            || (!layer.key.is_empty() && self.index(&layer.key).is_some())
        {
            return Err("invalid or duplicate sprite layer".into());
        }
        let mut candidate = self.clone();
        candidate.layers.insert(at, layer);
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
    pub fn remove(&mut self, key: &str) -> Option<SpriteLayer> {
        let index = self.index(key)?;
        let removed = self.layers.remove(index);
        for layer in &mut self.layers {
            if layer
                .mask
                .as_ref()
                .is_some_and(|m| m == &removed.key || removed.aliases.contains(m))
            {
                layer.mask = None;
            }
        }
        Some(removed)
    }
    pub fn set(&mut self, key: &str, layer: SpriteLayer) -> Result<(), String> {
        let i = self.index(key).ok_or("missing sprite layer")?;
        if !layer.valid() || layer.key != self.layers[i].key {
            return Err("invalid sprite layer replacement".into());
        }
        let mut candidate = self.clone();
        candidate.layers[i] = layer;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
    pub fn move_layer(&mut self, key: &str, target: usize) -> Result<(), String> {
        let i = self.index(key).ok_or("missing sprite layer")?;
        if target >= self.layers.len() {
            return Err("layer index out of bounds".into());
        }
        let layer = self.layers.remove(i);
        self.layers.insert(target, layer);
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        if !self.style.valid() || self.layers.len() > 64 || self.layers.iter().any(|l| !l.valid()) {
            return Err("invalid sprite component".into());
        }
        let mut keys = std::collections::BTreeSet::new();
        for l in &self.layers {
            for key in std::iter::once(&l.key)
                .chain(&l.aliases)
                .filter(|s| !s.is_empty())
            {
                if !keys.insert(key) {
                    return Err("duplicate sprite key or alias".into());
                }
            }
        }
        for l in &self.layers {
            if l.mask.as_ref().is_some_and(|k| {
                self.index(k)
                    .is_none_or(|i| std::ptr::eq(&self.layers[i], l))
            }) {
                return Err("missing or self-referencing mask layer".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct LayerPatch {
    pub state: Option<String>,
    pub rsi: Option<String>,
    pub visible: Option<bool>,
    pub color: Option<[f32; 4]>,
    pub offset: Option<[f32; 2]>,
    pub scale: Option<[f32; 2]>,
    pub rotation: Option<f32>,
    pub auto_animated: Option<bool>,
    pub looping: Option<bool>,
    pub unshaded: Option<bool>,
    pub restart: Option<u64>,
}
impl LayerPatch {
    fn apply(&self, l: &mut SpriteLayer) {
        if let Some(v) = &self.state {
            l.state = v.clone();
        }
        if let Some(v) = &self.rsi {
            l.rsi = v.clone();
        }
        if let Some(v) = self.visible {
            l.visible = v;
        }
        if let Some(v) = self.color {
            l.color = v;
        }
        if let Some(v) = self.offset {
            l.offset = v;
        }
        if let Some(v) = self.scale {
            l.scale = v;
        }
        if let Some(v) = self.rotation {
            l.rotation = v;
        }
        if let Some(v) = self.auto_animated {
            l.auto_animated = v;
        }
        if let Some(v) = self.looping {
            l.looping = v;
        }
        if let Some(v) = self.unshaded {
            l.unshaded = v;
        }
        if let Some(v) = self.restart {
            l.restart = v;
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppearanceRule {
    pub key: String,
    pub value: String,
    pub layer: String,
    pub patch: LayerPatch,
}
impl SpriteComponent {
    pub fn appearance(
        &mut self,
        data: &std::collections::BTreeMap<String, String>,
        rules: &[AppearanceRule],
    ) -> Result<(), String> {
        let mut candidate = self.clone();
        for r in rules {
            if data.get(&r.key) == Some(&r.value) {
                let i = candidate
                    .index(&r.layer)
                    .ok_or("appearance targets missing layer")?;
                r.patch.apply(&mut candidate.layers[i]);
            }
        }
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blank_layers_aliases_and_mask_removal_stay_valid() {
        let blank: SpriteLayer =
            serde_json::from_str(r#"{"key":"placeholder","aliases":["effect"]}"#).unwrap();
        let mut sprite = SpriteComponent::default();
        sprite.insert(0, blank.clone()).unwrap();
        assert_eq!(sprite.index("effect"), Some(0));
        let mut layer = blank;
        layer.key = "paint".into();
        layer.aliases.clear();
        layer.rsi = "wall".into();
        layer.state = "full".into();
        layer.mask = Some("effect".into());
        sprite.insert(1, layer).unwrap();
        sprite.remove("effect").unwrap();
        assert!(sprite.layers[0].mask.is_none());
        sprite.validate().unwrap();
    }
    #[test]
    fn keyed_edits_and_appearance_are_atomic() {
        let layer: SpriteLayer =
            serde_json::from_str(r#"{"rsi":"flashlight","state":"flashlight","key":"lamp"}"#)
                .unwrap();
        let mut sprite = SpriteComponent::default();
        sprite.insert(0, layer.clone()).unwrap();
        assert!(sprite.insert(1, layer.clone()).is_err());
        let rule = AppearanceRule {
            key: "on".into(),
            value: "true".into(),
            layer: "lamp".into(),
            patch: LayerPatch {
                unshaded: Some(true),
                ..Default::default()
            },
        };
        sprite
            .appearance(
                &std::collections::BTreeMap::from([("on".into(), "true".into())]),
                &[rule],
            )
            .unwrap();
        assert!(sprite.layers[0].unshaded);
        let mut bad = layer;
        bad.scale = [f32::NAN, 1.];
        assert!(sprite.set("lamp", bad).is_err());
        assert!(sprite.layers[0].unshaded);
        assert!(sprite.move_layer("lamp", 1).is_err());
        assert!(sprite.remove("lamp").is_some());
        assert!(sprite.layers.is_empty());
    }
}

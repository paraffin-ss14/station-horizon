//! Serializable tile surfaces; independent of voxel collision and lighting.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum FloorKind {
    #[default]
    Steel,
    Damaged,
    Tech,
    Grating,
    Plating,
}
impl FloorKind {
    pub const ALL: [Self; 5] = [
        Self::Steel,
        Self::Damaged,
        Self::Tech,
        Self::Grating,
        Self::Plating,
    ];
    pub fn state(self) -> &'static str {
        match self {
            Self::Steel => "steel",
            Self::Damaged => "damaged",
            Self::Tech => "tech",
            Self::Grating => "grating",
            Self::Plating => "plating",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Steel => "floor.steel",
            Self::Damaged => "floor.damaged",
            Self::Tech => "floor.tech",
            Self::Grating => "floor.grating",
            Self::Plating => "floor.plating",
        }
    }
    pub fn variants(self) -> u8 {
        if self == Self::Damaged { 4 } else { 1 }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FloorTile {
    pub cell: [i32; 3],
    pub kind: FloorKind,
    #[serde(default)]
    pub variant: Option<u8>,
}
impl FloorTile {
    pub fn valid(self) -> bool {
        self.cell.iter().all(|c| c.unsigned_abs() <= 128)
            && [0, -4].contains(&self.cell[2])
            && self.variant.is_none_or(|v| v < self.kind.variants())
    }
    pub fn frame(self) -> usize {
        self.variant.unwrap_or_else(|| {
            let [x, y, z] = self.cell.map(|v| v as u32);
            let mut h = x.wrapping_mul(0x9e3779b9)
                ^ y.wrapping_mul(0x85ebca6b)
                ^ z.wrapping_mul(0xc2b2ae35);
            h ^= h >> 16;
            h = h.wrapping_mul(0x7feb352d);
            h ^= h >> 15;
            (h % self.kind.variants() as u32) as u8
        }) as usize
    }
}

//! Shared data only: occupancy, collision, sight and light are independent.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Barrier {
    pub cell: [i32; 3],
    pub blocks_movement: bool,
    pub blocks_sight: bool,
    pub blocks_light: bool,
}

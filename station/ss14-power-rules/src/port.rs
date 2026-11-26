//! Extracted SS14 branches. Copyright (c) 2017-2026 Space Wizards Federation. MIT.
//! ECS/grid enumeration and load comparison stay in the station adapter.
#![forbid(unsafe_code)]
pub fn receiver_powered(
    disabled: bool,
    needs_power: bool,
    positive_load: bool,
    enough_power: bool,
) -> Result<bool, String> {
    Ok(!disabled && (!needs_power || (positive_load && enough_power)))
}
pub fn cable_reachable(
    anchored: bool,
    has_grid: bool,
    cable: bool,
    same_tile_device: bool,
    blocked_direction: bool,
) -> Result<bool, String> {
    Ok(anchored && has_grid && !blocked_direction && (cable || same_tile_device))
}
pub fn pry_allowed(cancelled: bool, powered: bool, pry_powered: bool) -> Result<bool, String> {
    Ok(!cancelled && (!powered || pry_powered))
}

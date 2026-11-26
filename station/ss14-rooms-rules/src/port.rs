//! Extracted SS14 atmosphere guards. MIT, Space Wizards Federation.
#![forbid(unsafe_code)]
pub fn edge_open(own_blocked: bool, adjacent_blocked: bool) -> Result<bool, String> {
    Ok(!own_blocked && !adjacent_blocked)
}
pub fn default_space(has_map: bool, map_space: bool) -> Result<bool, String> {
    Ok(!has_map || map_space)
}

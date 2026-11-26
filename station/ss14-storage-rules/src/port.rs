//! Adapted SS14 guards. Copyright (c) 2017-2026 Space Wizards Federation. MIT.
//! See ../ss14-port/SS14-LICENSE.txt. ECS events, grid packing and DoAfter excluded.
#![forbid(unsafe_code)]
pub fn bypass_ready(
    same_user: bool,
    has_quality: bool,
    locked: bool,
    can_force: bool,
) -> Result<bool, String> {
    Ok(!same_user && has_quality && locked && can_force)
}
pub fn storage_size_fits(
    size: i64,
    max_size: i64,
    is_storage: bool,
    nested_max: i64,
) -> Result<bool, String> {
    Ok((size as i32) <= (max_size as i32)
        && (!is_storage || (nested_max as i32) < (max_size as i32)))
}

//! SS14 hand-cancellation guards only. Copyright (c) 2017-2026 Space Wizards Federation.
//! MIT, see ../ss14-port/SS14-LICENSE.txt. Not the complete DoAfter system.
#![forbid(unsafe_code)]
pub fn hands_cancel(
    break_on_drop: bool,
    initial_item: bool,
    holding: bool,
    break_on_change: bool,
    initial_hand: i64,
    active_hand: i64,
) -> Result<bool, String> {
    Ok((break_on_drop && initial_item && !holding)
        || (break_on_change && (active_hand as i32) != (initial_hand as i32)))
}

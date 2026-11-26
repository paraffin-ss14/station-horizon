//! Copyright (c) 2019 Space Station 14 Contributors. MIT: ../ss14-port/ROBUST-LICENSE.txt.
#![forbid(unsafe_code)]
pub fn can_interpolate(
    distance_squared: f64,
    active: bool,
    same_parent: bool,
    valid_parent: bool,
    deleted: bool,
) -> Result<bool, String> {
    let distance = distance_squared as f32;
    Ok(active
        && same_parent
        && valid_parent
        && !deleted
        && distance > 0.001_f32 * 0.001_f32
        && distance < 4.0)
}
pub fn discard_input(sequence: i64, acknowledged: i64) -> Result<bool, String> {
    Ok(sequence as u32 <= acknowledged as u32)
}
pub fn lag_ticks(tick_rate: f64, ping_seconds: f64, time_scale: f64) -> Result<f64, String> {
    Ok((tick_rate * ping_seconds / time_scale).ceil())
}

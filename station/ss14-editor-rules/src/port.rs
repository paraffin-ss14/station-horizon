//! MIT: Space Station 14 Contributors; ../ss14-port/ROBUST-LICENSE.txt.
#![forbid(unsafe_code)]
pub fn axis_count(delta: i64) -> Result<f64, String> {
    let difference = delta as f32;
    let sign = if difference == 0.0 {
        0.0
    } else {
        difference.signum()
    };
    Ok((difference + 0.5 * sign).abs().ceil() as f64)
}
#[cfg(test)]
mod tests {
    #[test]
    fn endpoints_and_degenerate_axis() {
        assert_eq!(super::axis_count(-3), Ok(4.0));
        assert_eq!(super::axis_count(3), Ok(4.0));
        assert_eq!(super::axis_count(0), Ok(0.0));
    }
}

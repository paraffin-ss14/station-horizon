//! Copyright (c) 2019 Space Station 14 Contributors. MIT: ../ss14-port/ROBUST-LICENSE.txt.
#![forbid(unsafe_code)]
pub fn attenuation(squared_distance: f64, radius: f64) -> Result<f64, String> {
    let s = (((squared_distance as f32 + 1.0).sqrt()) / radius as f32).clamp(0.0, 1.0);
    let s2 = s * s;
    let curve = s + (s2 - s);
    Ok((((1.0 - s2) * (1.0 - s2)) / (1.0 + curve)).clamp(0.0, 1.0) as f64)
}
#[cfg(test)]
mod tests {
    #[test]
    fn cutoff_and_center() {
        assert_eq!(super::attenuation(16.0, 4.0), Ok(0.0));
        assert!((super::attenuation(0.0, 4.0).unwrap() - 0.8272059).abs() < 0.000001);
    }
}

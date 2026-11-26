//! Validated post-effect options. Does not expose unsupported feature switches.
use std::collections::BTreeMap;
const ALLOWED: &[&str] = &[
    "TONEMAP_BRIGHTNESS",
    "TONEMAP_CONTRAST",
    "TONEMAP_WHITE_THRESHOLD",
    "TONEMAP_HIGHLIGHTS",
    "TONEMAP_SHADOWS",
    "SATURATION",
    "VIBRANCE",
    "BLOOM_STRENGTH_OVERWORLD",
    "BLOOM_CONTRAST",
    "DOF_STRENGTH",
    "MOTION_BLUR_STRENGTH",
    "CHROMATIC_ABERRATION",
    "CHROMATIC_ABERRATION_STRENGTH",
];
fn parse(
    text: &str,
    available: &BTreeMap<String, horizon_shaders::PackOption>,
) -> Result<BTreeMap<String, String>, String> {
    let values: BTreeMap<String, serde_json::Value> =
        serde_json::from_str(text).map_err(|e| format!("Solas settings JSON: {e}"))?;
    let mut chosen = BTreeMap::new();
    for (name, value) in values {
        if !ALLOWED.contains(&name.as_str()) {
            return Err(format!("unsupported Solas setting {name}"));
        }
        let option = available
            .get(&name)
            .ok_or_else(|| format!("shaderpack has no option {name}"))?;
        let canonical = if option.switch {
            let flag = value
                .as_bool()
                .ok_or_else(|| format!("{name} requires true or false"))?
                .to_string();
            option.values.iter().find(|v| **v == flag)
        } else {
            let number = value
                .as_f64()
                .ok_or_else(|| format!("{name} requires a number"))?;
            option
                .values
                .iter()
                .find(|v| v.parse::<f64>().ok() == Some(number))
        }
        .ok_or_else(|| {
            format!(
                "invalid {name}={value}; allowed: {}",
                option.values.join(", ")
            )
        })?;
        chosen.insert(name, canonical.clone());
    }
    Ok(chosen)
}
pub fn load(pack: &horizon_shaders::Pack) -> Result<BTreeMap<String, String>, String> {
    let args: Vec<_> = std::env::args().collect();
    let flags: Vec<_> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == "--solas-settings")
        .collect();
    if flags.is_empty() {
        return Ok(BTreeMap::new());
    }
    if flags.len() != 1 {
        return Err("--solas-settings must appear once".into());
    }
    let path = args
        .get(flags[0].0 + 1)
        .filter(|p| !p.starts_with("--"))
        .ok_or("--solas-settings requires a JSON path")?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("Solas settings {path}: {e}"))?;
    let chosen = parse(&text, &horizon_shaders::options(pack))?;
    eprintln!("Solas settings: {} options from {path}", chosen.len());
    Ok(chosen)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn available() -> BTreeMap<String, horizon_shaders::PackOption> {
        BTreeMap::from([(
            "SATURATION".into(),
            horizon_shaders::PackOption {
                name: "SATURATION".into(),
                default: "1.00".into(),
                values: vec!["0.00".into(), "1.00".into(), "2.00".into()],
                switch: false,
                tested: true,
            },
        )])
    }
    #[test]
    fn supported_switch_requires_boolean_and_does_not_unlock_other_features() {
        let mut options = available();
        options.insert(
            "CHROMATIC_ABERRATION".into(),
            horizon_shaders::PackOption {
                name: "CHROMATIC_ABERRATION".into(),
                default: "false".into(),
                values: vec!["false".into(), "true".into()],
                switch: true,
                tested: true,
            },
        );
        for (value, expected) in [("true", "true"), ("false", "false")] {
            let text = format!("{{\"CHROMATIC_ABERRATION\":{value}}}");
            assert_eq!(
                parse(&text, &options).unwrap()["CHROMATIC_ABERRATION"],
                expected
            );
        }
        assert!(parse(r#"{"CHROMATIC_ABERRATION":1}"#, &options).is_err());
        assert!(parse(r#"{"SATURATION":true}"#, &options).is_err());
        assert!(parse(r#"{"TAA":true}"#, &options).is_err());
    }
    #[test]
    fn uses_pack_spelling_and_accepts_numeric_json() {
        assert_eq!(
            parse(r#"{"SATURATION":1}"#, &available()).unwrap()["SATURATION"],
            "1.00"
        );
        assert!(parse("{}", &available()).unwrap().is_empty());
    }
    #[test]
    fn rejects_unknown_unsupported_missing_and_out_of_range_options() {
        for json in [
            r#"{"TAA":1}"#,
            r#"{"SATURATION":3}"#,
            r#"{"SATURATION":1.1}"#,
            r#"{"SATURATION":"1.00"}"#,
            r#"{"SATURATION":null}"#,
            r#"{"SATURATION":NaN}"#,
            r#"{"DOF_STRENGTH":3}"#,
        ] {
            assert!(parse(json, &available()).is_err(), "{json}");
        }
    }
}

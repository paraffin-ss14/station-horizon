//! Copyright (c) 2019 Space Station 14 Contributors. MIT: ../ss14-port/ROBUST-LICENSE.txt.
#![forbid(unsafe_code)]
use serde_json::json;
pub fn fold(data: String) -> Result<String, String> {
    let Ok(delays) = serde_json::from_str::<Vec<Vec<f32>>>(&data) else {
        return Ok("invalid".into());
    };
    let n = delays.len();
    if ![1, 4, 8].contains(&n)
        || delays.iter().any(|d| {
            d.is_empty()
                || d.len() > 64
                || d.iter().any(|v| !v.is_finite() || *v < 0.001 || *v > 60.)
        })
    {
        return Ok("invalid".into());
    }
    if n == 1 {
        return Ok(json!({"delays":delays[0].iter().map(|v|v.to_bits()).collect::<Vec<_>>(),"indices":[(0..delays[0].len()).collect::<Vec<_>>()]}).to_string());
    }
    let mut work: Vec<Vec<i32>> = delays
        .iter()
        .map(|d| d.iter().map(|v| (v * 1000.) as i32).collect())
        .collect();
    let lengths: Vec<i32> = work.iter().map(|d| d.iter().sum()).collect();
    let maximum = lengths.iter().copied().max().unwrap_or(0);
    let mut bases = vec![0; n];
    for d in 0..n {
        let last = work[d].len() - 1;
        work[d][last] += maximum - lengths[d];
        if d > 0 {
            bases[d] = bases[d - 1] + delays[d - 1].len();
        }
    }
    let mut offsets = vec![0; n];
    let mut output = vec![];
    let mut indices = vec![vec![]; n];
    loop {
        let minimum = (0..n).map(|d| work[d][offsets[d]]).min().unwrap_or(0);
        output.push((minimum as f32 / 1000.).to_bits());
        for d in 0..n {
            indices[d].push(bases[d] + offsets[d]);
        }
        let mut done = false;
        for d in 0..n {
            work[d][offsets[d]] -= minimum;
            if work[d][offsets[d]] == 0 {
                offsets[d] += 1;
            }
            if offsets[d] == work[d].len() {
                done = true;
                break;
            }
        }
        if done {
            break;
        }
    }
    Ok(json!({"delays":output,"indices":indices}).to_string())
}
#[cfg(test)]
mod tests {
    #[test]
    fn asynchronous_directions_and_bad_input() {
        let text = super::fold("[[0.1,0.2],[0.3],[0.2,0.1],[0.3]]".into()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["indices"][0], serde_json::json!([0, 1, 1]));
        assert_eq!(value["indices"][1], serde_json::json!([2, 2, 2]));
        assert_eq!(super::fold("[[0]]".into()).unwrap(), "invalid");
    }
}

//! SpriteComponent.Layer.AdvanceFrameAnimation, RobustToolbox. MIT; see ../ss14-port/ROBUST-LICENSE.txt.
#![forbid(unsafe_code)]
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug)]
pub struct Cursor {
    pub frame: i32,
    pub left: f32,
    pub time: f32,
    pub reversed: bool,
    pub animated: bool,
}
impl Cursor {
    pub fn step(&mut self, delays: &[f32], delta: f32, looping: bool, cycle: bool) {
        self.time += delta;
        self.left -= delta;
        let count = delays.len() as i32;
        while self.left < 0. {
            if self.reversed {
                self.frame -= 1;
                if self.frame < 0 {
                    if !looping {
                        self.frame = 0;
                        self.left = 0.;
                        self.animated = false;
                        return;
                    }
                    if cycle {
                        self.frame = 1;
                        self.reversed = false;
                    } else {
                        self.frame = count - 1;
                    }
                    self.time = -self.left;
                }
            } else {
                self.frame += 1;
                if self.frame >= count {
                    if !looping {
                        self.frame = count - 1;
                        self.left = 0.;
                        self.animated = false;
                        return;
                    }
                    if cycle {
                        self.frame = count - 2;
                        self.reversed = true;
                    } else {
                        self.frame = 0;
                    }
                    self.time = -self.left;
                }
            }
            self.left += delays[self.frame as usize];
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Input {
    delays: Vec<f32>,
    delta: f32,
    frame: i32,
    left: f32,
    reversed: bool,
    r#loop: bool,
    cycle: bool,
}
#[derive(Serialize)]
struct Output {
    frame: i32,
    left: i32,
    time: i32,
    reversed: bool,
    animated: bool,
}
pub fn advance(data: String) -> Result<String, String> {
    let Ok(x) = serde_json::from_str::<Input>(&data) else {
        return Ok("invalid".into());
    };
    if x.delays.len() < 2
        || x.delays.len() > 64
        || x.delays
            .iter()
            .any(|v| !v.is_finite() || *v < 0.001 || *v > 60.)
        || !x.delta.is_finite()
        || !(0.0..=60.).contains(&x.delta)
        || x.frame < 0
        || x.frame as usize >= x.delays.len()
        || !x.left.is_finite()
        || x.left.abs() > 60.
    {
        return Ok("invalid".into());
    }
    let mut c = Cursor {
        frame: x.frame,
        left: x.left,
        time: 0.,
        reversed: x.reversed,
        animated: true,
    };
    c.step(&x.delays, x.delta, x.r#loop, x.cycle);
    serde_json::to_string(&Output {
        frame: c.frame,
        left: c.left.to_bits() as i32,
        time: c.time.to_bits() as i32,
        reversed: c.reversed,
        animated: c.animated,
    })
    .map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_boundary_and_ping_pong() {
        let mut c = Cursor {
            frame: 0,
            left: 0.1,
            time: 0.,
            reversed: false,
            animated: true,
        };
        c.step(&[0.1, 0.2, 0.3], 0.1, true, true);
        assert_eq!(c.frame, 0);
        c.step(&[0.1, 0.2, 0.3], 0.51, true, true);
        assert_eq!(c.frame, 1);
        assert!(c.reversed);
    }
}

//! RSI metadata/PNG loader, folded animation playback and one GPU atlas.
use serde::Deserialize;
use std::{collections::BTreeMap, io::Cursor};
#[path = "station_sprite_assets.rs"]
mod assets;
#[path = "../../../station/ss14-sprite-rules/src/port.rs"]
mod rules;
pub const ATLAS_SIZE: usize = 2048;
/// RobustToolbox EyeManager.PixelsPerMeter: one 32px frame spans one tile.
pub const PIXELS_PER_METER: f32 = 32.;
pub fn frame_world_size(pixels: [usize; 2]) -> [f32; 2] {
    pixels.map(|p| p as f32 / PIXELS_PER_METER)
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: [f32; 2],
    pub max: [f32; 2],
}
#[derive(Deserialize)]
struct Meta {
    version: u32,
    size: Size,
    states: Vec<StateMeta>,
}
#[derive(Deserialize)]
struct Size {
    x: usize,
    y: usize,
}
fn one() -> usize {
    1
}
#[derive(Deserialize)]
struct StateMeta {
    name: String,
    #[serde(default = "one")]
    directions: usize,
    #[serde(default)]
    delays: Vec<Vec<f32>>,
}
#[derive(Deserialize)]
struct Folded {
    delays: Vec<u32>,
    indices: Vec<Vec<usize>>,
}
struct State {
    pixels: [usize; 2],
    frames: Vec<Rect>,
    delays: Vec<f64>,
    indices: Vec<Vec<usize>>,
    total: f64,
}
pub struct Atlas {
    pub pixels: Vec<u8>,
    states: BTreeMap<(String, String), State>,
    pub fallback: Rect,
}
impl Atlas {
    pub fn bundled() -> Result<Self, String> {
        Self::load(
            assets::FILES
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_vec()))
                .collect(),
        )
    }
    pub fn with_directory(path: &std::path::Path) -> Result<Self, String> {
        let mut files: BTreeMap<String, Vec<u8>> = assets::FILES
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_vec()))
            .collect();
        let mut total = 0;
        for (index, entry) in std::fs::read_dir(path)
            .map_err(|e| e.to_string())?
            .enumerate()
        {
            if index >= 64 {
                return Err("sprite pack exceeds folder limit".into());
            }
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !entry.file_type().map_err(|e| e.to_string())?.is_dir() || !name.ends_with(".rsi") {
                continue;
            }
            if !safe(&name[..name.len() - 4]) {
                return Err("invalid RSI folder name".into());
            }
            for (index, file) in std::fs::read_dir(entry.path())
                .map_err(|e| e.to_string())?
                .enumerate()
            {
                if index >= 513 {
                    return Err("RSI exceeds file limit".into());
                }
                let file = file.map_err(|e| e.to_string())?;
                let file_name = file.file_name().to_string_lossy().into_owned();
                if file_name != "meta.json" && !file_name.ends_with(".png") {
                    continue;
                }
                let length = file.metadata().map_err(|e| e.to_string())?.len();
                total += length;
                if length > 8 * 1024 * 1024 || total > 32 * 1024 * 1024 {
                    return Err("sprite pack exceeds byte limit".into());
                }
                files.insert(
                    format!("{name}/{file_name}"),
                    std::fs::read(file.path()).map_err(|e| e.to_string())?,
                );
            }
        }
        Self::load(files)
    }
    fn load(files: BTreeMap<String, Vec<u8>>) -> Result<Self, String> {
        let fallback = Rect {
            min: [0., 0.],
            max: [32. / ATLAS_SIZE as f32; 2],
        };
        let mut atlas = Self {
            pixels: vec![0; ATLAS_SIZE * ATLAS_SIZE * 4],
            states: BTreeMap::new(),
            fallback,
        };
        for y in 0..32 {
            for x in 0..32 {
                let i = (y * ATLAS_SIZE + x) * 4;
                atlas.pixels[i..i + 4].copy_from_slice(if (x / 4 + y / 4) % 2 == 0 {
                    &[240, 30, 180, 255]
                } else {
                    &[30, 10, 30, 255]
                });
            }
        }
        // Native, tintable geometry uses a reserved white pixel, independent of RSI packs.
        atlas.pixels[32 * 4..33 * 4].copy_from_slice(&[255; 4]);
        atlas.states.insert(
            ("station-native".into(), "solid".into()),
            State {
                pixels: [32, 32],
                frames: vec![Rect {
                    min: [32.5 / ATLAS_SIZE as f32, 0.5 / ATLAS_SIZE as f32],
                    max: [32.5 / ATLAS_SIZE as f32, 0.5 / ATLAS_SIZE as f32],
                }],
                delays: vec![1.],
                indices: vec![vec![0]],
                total: 1.,
            },
        );
        let (mut x, mut y, mut row) = (34, 1, 32);
        for (file, bytes) in files.iter().filter(|(k, _)| k.ends_with("/meta.json")) {
            let folder = file.strip_suffix("/meta.json").ok_or("metadata path")?;
            let alias = folder.strip_suffix(".rsi").ok_or("metadata folder")?;
            let meta: Meta = serde_json::from_slice(bytes).map_err(|e| format!("{file}: {e}"))?;
            if meta.version != 1
                || meta.states.len() > 512
                || meta.size.x == 0
                || meta.size.y == 0
                || meta.size.x > 128
                || meta.size.y > 128
            {
                return Err(format!("{file}: unsupported dimensions/version"));
            }
            for state in meta.states {
                if !safe(&state.name) || ![1, 4, 8].contains(&state.directions) {
                    return Err(format!("{file}: invalid state"));
                }
                let delays = if state.delays.is_empty() {
                    vec![vec![1.]; state.directions]
                } else {
                    state.delays
                };
                if delays.len() != state.directions {
                    return Err(format!("{file}: direction/delay count mismatch"));
                }
                let folded =
                    rules::fold(serde_json::to_string(&delays).map_err(|e| e.to_string())?)?;
                let folded: Folded = serde_json::from_str(&folded)
                    .map_err(|_| format!("{file}: invalid frame delays"))?;
                let name = format!("{folder}/{}.png", state.name);
                let png = files.get(&name).ok_or_else(|| format!("missing {name}"))?;
                let mut decoder = png::Decoder::new(Cursor::new(png));
                decoder.set_transformations(
                    png::Transformations::EXPAND | png::Transformations::STRIP_16,
                );
                let mut reader = decoder.read_info().map_err(|e| format!("{name}: {e}"))?;
                let length = reader.output_buffer_size().ok_or("PNG size overflow")?;
                if length > 32 * 1024 * 1024 {
                    return Err("PNG decoded size exceeds limit".into());
                }
                let mut raw = vec![0; length];
                let info = reader.next_frame(&mut raw).map_err(|e| e.to_string())?;
                let width = info.width as usize;
                let height = info.height as usize;
                let count: usize = delays.iter().map(Vec::len).sum();
                if !width.is_multiple_of(meta.size.x)
                    || !height.is_multiple_of(meta.size.y)
                    || width / meta.size.x * (height / meta.size.y) < count
                {
                    return Err(format!("{name}: truncated frame sheet"));
                }
                let channels = match info.color_type {
                    png::ColorType::Rgba => 4,
                    png::ColorType::Rgb => 3,
                    png::ColorType::Grayscale => 1,
                    png::ColorType::GrayscaleAlpha => 2,
                    _ => return Err("unsupported PNG format".into()),
                };
                let mut frames = vec![];
                for frame in 0..count {
                    if x + meta.size.x >= ATLAS_SIZE {
                        x = 1;
                        y += row + 2;
                        row = 0;
                    }
                    if y + meta.size.y >= ATLAS_SIZE {
                        return Err("sprite atlas full".into());
                    }
                    row = row.max(meta.size.y);
                    let sx = frame % (width / meta.size.x) * meta.size.x;
                    let sy = frame / (width / meta.size.x) * meta.size.y;
                    // Extrude edge texels into a one-pixel gutter. Fractional camera
                    // jitter can round a boundary UV outside the frame; transparent
                    // packing gaps must not turn opaque tile edges into cracks.
                    for yy in 0..meta.size.y + 2 {
                        for xx in 0..meta.size.x + 2 {
                            let source_x = xx.saturating_sub(1).min(meta.size.x - 1);
                            let source_y = yy.saturating_sub(1).min(meta.size.y - 1);
                            let i = ((sy + source_y) * width + sx + source_x) * channels;
                            let p = &raw[i..i + channels];
                            let rgba = match channels {
                                4 => [p[0], p[1], p[2], p[3]],
                                3 => [p[0], p[1], p[2], 255],
                                2 => [p[0], p[0], p[0], p[1]],
                                _ => [p[0], p[0], p[0], 255],
                            };
                            let o = ((y + yy - 1) * ATLAS_SIZE + x + xx - 1) * 4;
                            atlas.pixels[o..o + 4].copy_from_slice(&rgba);
                        }
                    }
                    frames.push(Rect {
                        min: [x as f32 / ATLAS_SIZE as f32, y as f32 / ATLAS_SIZE as f32],
                        max: [
                            (x + meta.size.x) as f32 / ATLAS_SIZE as f32,
                            (y + meta.size.y) as f32 / ATLAS_SIZE as f32,
                        ],
                    });
                    x += meta.size.x + 2;
                }
                let times: Vec<f64> = folded
                    .delays
                    .iter()
                    .map(|v| f32::from_bits(*v) as f64)
                    .collect();
                let total = times.iter().sum();
                if atlas
                    .states
                    .insert(
                        (alias.to_string(), state.name),
                        State {
                            pixels: [meta.size.x, meta.size.y],
                            frames,
                            delays: times,
                            indices: folded.indices,
                            total,
                        },
                    )
                    .is_some()
                {
                    return Err("duplicate RSI state".into());
                }
            }
        }
        Ok(atlas)
    }
    pub fn world_size(&self, rsi: &str, state: &str) -> [f32; 2] {
        self.states
            .get(&(rsi.into(), state.into()))
            .map_or([1.; 2], |s| frame_world_size(s.pixels))
    }
    pub fn delays(&self, rsi: &str, state: &str) -> Option<&[f64]> {
        self.states
            .get(&(rsi.into(), state.into()))
            .map(|s| s.delays.as_slice())
    }
    pub fn frame_at(&self, rsi: &str, state: &str, facing: [f32; 2], frame: usize) -> Rect {
        let Some(s) = self.states.get(&(rsi.into(), state.into())) else {
            return self.fallback;
        };
        let dir = direction(facing, s.indices.len());
        s.frames[s.indices[dir][frame.min(s.delays.len() - 1)]]
    }
    pub fn contains(&self, rsi: &str, state: &str) -> bool {
        self.states.contains_key(&(rsi.into(), state.into()))
    }
    pub fn frame(&self, rsi: &str, state: &str, facing: [f32; 2], time: f64) -> Rect {
        let Some(s) = self.states.get(&(rsi.into(), state.into())) else {
            return self.fallback;
        };
        let dir = direction(facing, s.indices.len());
        let mut t = if time.is_finite() {
            time.max(0.) % s.total
        } else {
            0.
        };
        let mut frame = 0;
        for (i, &delay) in s.delays.iter().enumerate() {
            frame = i;
            if t < delay {
                break;
            }
            t -= delay;
        }
        s.frames[s.indices[dir][frame]]
    }
    pub fn state_count(&self) -> usize {
        self.states.len()
    }
}
fn safe(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 96
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
pub fn direction([x, y]: [f32; 2], count: usize) -> usize {
    if count == 1 {
        return 0;
    }
    if !x.is_finite() || !y.is_finite() || x == 0. && y == 0. {
        return 0;
    }
    if count == 8 && x.abs() > y.abs() * 0.41421357 && y.abs() > x.abs() * 0.41421357 {
        return match (x > 0., y > 0.) {
            (true, false) => 4,
            (false, false) => 5,
            (true, true) => 6,
            (false, true) => 7,
        };
    }
    if x.abs() > y.abs() {
        if x > 0. { 2 } else { 3 }
    } else if y > 0. {
        1
    } else {
        0
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atlas_gutters_repeat_each_frame_edge_including_alpha() {
        let atlas = Atlas::bundled().unwrap();
        let pixel = |x: usize, y: usize| {
            let i = (y * ATLAS_SIZE + x) * 4;
            &atlas.pixels[i..i + 4]
        };
        for ((rsi, _), state) in &atlas.states {
            if rsi == "station-native" {
                continue;
            }
            for rect in &state.frames {
                let [x, y] = rect.min.map(|v| (v * ATLAS_SIZE as f32) as usize);
                let [right, bottom] = rect.max.map(|v| (v * ATLAS_SIZE as f32) as usize);
                for yy in y - 1..=bottom {
                    for xx in x - 1..=right {
                        if xx < x || xx == right || yy < y || yy == bottom {
                            assert_eq!(
                                pixel(xx, yy),
                                pixel(xx.clamp(x, right - 1), yy.clamp(y, bottom - 1))
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn rsi_pixels_define_world_scale_without_cropping() {
        assert_eq!(frame_world_size([32, 32]), [1., 1.]);
        assert_eq!(frame_world_size([64, 32]), [2., 1.]);
        assert_eq!(frame_world_size([16, 64]), [0.5, 2.]);
        let atlas = Atlas::bundled().unwrap();
        assert_eq!(atlas.world_size("crowbar", "icon"), [1., 1.]);
        assert_eq!(atlas.world_size("human", "head_m"), [1., 1.]);
    }
    #[test]
    fn bundled_states_directions_and_animation() {
        let a = Atlas::bundled().unwrap();
        assert!(a.state_count() > 50);
        let south = a.frame("human", "head_m", [0., -1.], 0.);
        let north = a.frame("human", "head_m", [0., 1.], 0.);
        assert_ne!(south, north);
        let first = a.frame("crate", "sparking", [0., -1.], 0.);
        assert_ne!(first, a.frame("crate", "sparking", [0., -1.], 0.15));
        assert_eq!(first, a.frame("crate", "sparking", [0., -1.], 0.61));
        assert_eq!(a.frame("missing", "missing", [0., 0.], 0.), a.fallback);
        assert_eq!(
            (0..8)
                .map(|i| direction(
                    [
                        [0., -1.],
                        [0., 1.],
                        [1., 0.],
                        [-1., 0.],
                        [1., -1.],
                        [-1., -1.],
                        [1., 1.],
                        [-1., 1.]
                    ][i],
                    8
                ))
                .collect::<Vec<_>>(),
            (0..8).collect::<Vec<_>>()
        );
    }
    #[test]
    fn rejects_bad_metadata_and_missing_sheets() {
        let files =
            |meta: &str| BTreeMap::from([("x.rsi/meta.json".into(), meta.as_bytes().to_vec())]);
        assert!(Atlas::load(files(r#"{"version":1,"size":{"x":0,"y":32},"states":[]}"#)).is_err());
        assert!(
            Atlas::load(files(
                r#"{"version":1,"size":{"x":32,"y":32},"states":[{"name":"../../bad"}]}"#
            ))
            .is_err()
        );
        assert!(
            Atlas::load(files(
                r#"{"version":1,"size":{"x":32,"y":32},"states":[{"name":"a"}]}"#
            ))
            .is_err()
        );
    }
}

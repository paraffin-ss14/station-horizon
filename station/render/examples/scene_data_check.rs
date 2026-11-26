//! Check the actual debug GPU capture, including surface/background and untouched UI.
use std::{fs::File, io::BufReader, path::Path};
fn read(path: &Path) -> (u32, u32, Vec<[u8; 3]>) {
    let mut reader = png::Decoder::new(BufReader::new(File::open(path).unwrap()))
        .read_info()
        .unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    let stride = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        _ => panic!("expected RGB capture"),
    };
    if stride == 4 {
        assert!(
            bytes[..info.buffer_size()]
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[3] == 255),
            "capture must be opaque: {}",
            path.display()
        );
    }
    (
        info.width,
        info.height,
        bytes[..info.buffer_size()]
            .chunks_exact(stride)
            .map(|p| [p[0], p[1], p[2]])
            .collect(),
    )
}
fn main() {
    let dir = std::env::args().nth(1).expect("evidence directory");
    let (w, h, scene) = read(&Path::new(&dir).join("scene-data.png"));
    let (bw, bh, baseline) = read(&Path::new(&dir).join("baseline.png"));
    assert_eq!((w, h), (bw, bh));
    assert_eq!((w, h), (1200, 800), "fixture camera size changed");
    let at = |x: usize, y: usize| scene[y * w as usize + x];
    for y in 0..h as usize {
        for x in 0..270 {
            assert_eq!(
                at(x, y),
                baseline[y * w as usize + x],
                "UI changed at {x},{y}"
            );
        }
    }
    let mut normal_pixels = 0;
    for y in 120..620 {
        for x in 430..780 {
            let [r, g, b] = at(x, y);
            if (125..=130).contains(&r) && (125..=130).contains(&g) && b > 250 {
                normal_pixels += 1;
            }
        }
    }
    assert!(
        normal_pixels > 10000,
        "normal target missing or wrong orientation"
    );
    let mut depths = std::collections::BTreeSet::new();
    for y in 100..660 {
        for x in 825..970 {
            let [r, g, b] = at(x, y);
            if r == g && g == b && r > 0 && r < 254 {
                depths.insert(r);
            }
        }
    }
    assert!(
        depths.len() > 1,
        "depth target contains no surface variation"
    );
    assert_eq!(at(1100, 700), [0; 3], "background must remain clear");
    let mut colors = std::collections::BTreeSet::new();
    for y in 320..600 {
        for x in 290..370 {
            colors.insert(at(x, y));
        }
    }
    assert!(
        colors.len() > 16,
        "albedo does not contain sprite texture detail"
    );
    let (mw, mh, material) = read(&Path::new(&dir).join("photon-material.png"));
    assert_eq!((w, h), (mw, mh));
    for y in 0..h as usize {
        for x in 0..270 {
            assert_eq!(
                material[y * w as usize + x],
                baseline[y * w as usize + x],
                "material pass changed UI"
            );
        }
    }
    for (label, x0, x1, tolerance) in [
        ("albedo", 285, 390, 1.5),
        ("normal", 430, 780, 1.5),
        ("depth", 825, 970, 0.1),
    ] {
        let mut error = 0u64;
        let mut count = 0u64;
        for y in 120..650 {
            for x in x0..x1 {
                let a = at(x, y);
                let b = material[y * w as usize + x];
                for c in 0..3 {
                    error += a[c].abs_diff(b[c]) as u64;
                    count += 1;
                }
            }
        }
        let mean = error as f64 / count as f64;
        assert!(
            mean < tolerance,
            "Photon {label} roundtrip error {mean} exceeds {tolerance}"
        );
        println!("Photon {label} mean absolute byte error: {mean:.4}");
    }
    let (aw, ah, ao) = read(&Path::new(&dir).join("photon-ao.png"));
    let (_, _, photon) = read(&Path::new(&dir).join("photon.png"));
    assert_eq!((aw, ah), (w, h));
    for y in 0..h as usize {
        for x in 0..270 {
            assert_eq!(
                ao[y * w as usize + x],
                photon[y * w as usize + x],
                "AO changed UI"
            );
        }
    }
    let mut darker = 0;
    let mut delta = 0i64;
    let mut samples = 0;
    for y in 100..620 {
        for x in 300..940 {
            let i = y * w as usize + x;
            let d = photon[i].iter().map(|&c| c as i64).sum::<i64>()
                - ao[i].iter().map(|&c| c as i64).sum::<i64>();
            if d > 9 {
                darker += 1;
            }
            delta += d;
            samples += 3;
        }
    }
    let darkening = delta as f64 / samples as f64;
    assert!(
        darker > 1000 && (0.1..30.).contains(&darkening),
        "SSAO missing or corrupt: {darker} pixels, {darkening} mean"
    );
    for y in 700..780 {
        for x in 1050..1150 {
            let i = y * w as usize + x;
            for c in 0..3 {
                assert!(
                    ao[i][c].abs_diff(photon[i][c]) <= 2,
                    "AO affected empty background"
                );
            }
        }
    }
    println!(
        "Photon SSAO: {darker} darkened pixels, mean change {darkening:.3}, UI/background preserved."
    );
    let (sw, sh, shadows) = read(&Path::new(&dir).join("photon-shadows.png"));
    assert_eq!((sw, sh), (w, h));
    let mut shaded = 0usize;
    let mut unchanged = 0usize;
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = y * w as usize + x;
            if x < 270 {
                assert_eq!(shadows[i], ao[i], "shadow pass changed UI");
            }
            if (100..620).contains(&y) && (300..940).contains(&x) {
                let d: i64 = (0..3).map(|c| ao[i][c] as i64 - shadows[i][c] as i64).sum();
                if d > 15 {
                    shaded += 1;
                }
                if d.abs() <= 3 {
                    unchanged += 1;
                }
            }
            if y >= 700 && x >= 1050 {
                for c in 0..3 {
                    assert!(
                        shadows[i][c].abs_diff(ao[i][c]) <= 2,
                        "shadow pass changed background"
                    );
                }
            }
        }
    }
    assert!(
        shaded > 1000 && unchanged > 1000,
        "missing shadows or uniformly darkened scene: {shaded} shaded, {unchanged} unchanged"
    );
    println!(
        "Photon shadows: {shaded} darkened pixels, {unchanged} unchanged; UI/background preserved."
    );
    let (vw, vh, volume) = read(&Path::new(&dir).join("photon-volumetrics.png"));
    assert_eq!((vw, vh), (w, h));
    let mut fog_pixels = 0usize;
    let mut total_change = 0u64;
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = y * w as usize + x;
            if x < 270 {
                assert_eq!(volume[i], shadows[i], "volumetric fog changed UI");
            }
            if y >= 700 && x >= 1050 {
                for c in 0..3 {
                    assert!(
                        volume[i][c].abs_diff(shadows[i][c]) <= 2,
                        "fog changed empty background"
                    );
                }
            }
            if (100..620).contains(&y) && (300..940).contains(&x) {
                let change: u64 = (0..3)
                    .map(|c| volume[i][c].abs_diff(shadows[i][c]) as u64)
                    .sum();
                if change > 9 {
                    fog_pixels += 1;
                }
                total_change += change;
            }
        }
    }
    let mean_change = total_change as f64 / (520. * 640. * 3.);
    assert!(
        fog_pixels > 1000 && (0.1..45.).contains(&mean_change),
        "fog missing or overexposed: {fog_pixels} pixels, {mean_change} mean"
    );
    println!(
        "Photon volumetric fog: {fog_pixels} changed pixels, mean RGB change {mean_change:.3}; UI/background preserved."
    );
    let (iw, ih, identity) = read(&Path::new(&dir).join("final-slot.png"));
    assert_eq!((iw, ih), (w, h));
    let mut identity_error = 0u64;
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = y * w as usize + x;
            if x < 270 {
                assert_eq!(identity[i], baseline[i], "generic final changed UI");
            }
            for c in 0..3 {
                identity_error += identity[i][c].abs_diff(baseline[i][c]) as u64;
            }
        }
    }
    let identity_mean = identity_error as f64 / (w as f64 * h as f64 * 3.);
    assert!(
        identity_mean < 0.5,
        "explicit colortex7 sRGB roundtrip changed color/orientation: {identity_mean}"
    );
    println!("Generic final colortex7 + sRGB roundtrip mean RGB error: {identity_mean:.4}");
    if std::env::args().any(|a| a == "--solas") {
        let (sw, sh, solas) = read(&Path::new(&dir).join("solas.png"));
        assert_eq!((sw, sh), (w, h));
        let mut difference = 0u64;
        let mut changed = 0;
        for y in 0..h as usize {
            for x in 0..w as usize {
                let i = y * w as usize + x;
                if x < 270 {
                    assert_eq!(solas[i], baseline[i], "Solas changed UI");
                }
                let d: u64 = (0..3)
                    .map(|c| solas[i][c].abs_diff(baseline[i][c]) as u64)
                    .sum();
                difference += d;
                if d > 3 {
                    changed += 1;
                }
            }
        }
        let mean = difference as f64 / (w as f64 * h as f64 * 3.);
        assert!(
            changed > 1000 && (0.001..10.).contains(&mean),
            "Solas final missing or wrong color encoding: {changed}, {mean}"
        );
        println!(
            "Solas original final: {changed} changed pixels, mean RGB change {mean:.4}; UI and opacity preserved."
        );
    }
    if std::env::args().any(|a| a == "--solas") {
        let (pw, ph, post) = read(&Path::new(&dir).join("solas-post.png"));
        assert_eq!((pw, ph), (w, h));
        let (_, _, final_only) = read(&Path::new(&dir).join("solas.png"));
        let mut difference = 0u64;
        let mut samples = 0u64;
        let mut levels = std::collections::BTreeSet::new();
        for y in 0..h as usize {
            for x in 0..w as usize {
                let i = y * w as usize + x;
                if x < 270 {
                    assert_eq!(post[i], baseline[i], "Solas post changed UI");
                }
                if (100..620).contains(&y) && (300..940).contains(&x) {
                    levels.insert(post[i]);
                    for c in 0..3 {
                        difference += post[i][c].abs_diff(final_only[i][c]) as u64;
                        samples += 1;
                    }
                }
            }
        }
        let mean = difference as f64 / samples as f64;
        assert!(
            (1.0..100.).contains(&mean) && levels.len() > 1000,
            "Solas post chain missing/blank: mean {mean}, colors {}",
            levels.len()
        );
        println!(
            "Solas composite14/15: mean RGB change {mean:.3}, {} surface colors; UI and opacity preserved.",
            levels.len()
        );
    }
    if std::env::args().any(|a| a == "--solas") {
        let (bw, bh, bloom) = read(&Path::new(&dir).join("solas-bloom.png"));
        assert_eq!((bw, bh), (w, h));
        let (_, _, post) = read(&Path::new(&dir).join("solas-post.png"));
        let mut changed = 0usize;
        let mut error = 0u64;
        for y in 0..h as usize {
            for x in 0..w as usize {
                let i = y * w as usize + x;
                if x < 270 {
                    assert_eq!(bloom[i], baseline[i], "Solas bloom changed UI");
                }
                if (100..620).contains(&y) && (300..940).contains(&x) {
                    let delta: u64 = (0..3)
                        .map(|c| bloom[i][c].abs_diff(post[i][c]) as u64)
                        .sum();
                    error += delta;
                    if delta > 6 {
                        changed += 1;
                    }
                }
            }
        }
        let mean = error as f64 / (520. * 640. * 3.);
        assert!(
            changed > 1000 && (0.1..25.).contains(&mean),
            "Solas bloom missing or corrupt: {changed} changed pixels, mean {mean}"
        );
        println!(
            "Solas bloom: {changed} changed pixels, mean RGB change {mean:.3}; UI and opacity preserved."
        );
    }
    if std::env::args().any(|a| a == "--solas") {
        let (dw, dh, dof) = read(&Path::new(&dir).join("solas-dof.png"));
        let (_, _, bloom) = read(&Path::new(&dir).join("solas-bloom.png"));
        assert_eq!((dw, dh), (w, h));
        let mut changed = 0;
        let mut error = 0u64;
        for (i, (a, b)) in dof.iter().zip(&bloom).enumerate() {
            if i % (w as usize) < 270 {
                assert_eq!(a, b, "Solas DOF changed UI");
            }
            let delta = (0..3).map(|c| a[c].abs_diff(b[c]) as u64).sum::<u64>();
            error += delta;
            changed += usize::from(delta > 0);
        }
        let mean = error as f64 / (w as f64 * h as f64 * 3.);
        assert!(
            changed > 100 && (0.001..2.).contains(&mean),
            "Solas DOF missing or corrupt: {changed} pixels, mean {mean}"
        );
        println!(
            "Solas original DOF: {changed} changed pixels, mean RGB change {mean:.4}; UI and opacity preserved."
        );
    }
    if std::env::args().any(|a| a == "--solas") {
        let (aw, ah, autofocus) = read(&Path::new(&dir).join("solas-autofocus.png"));
        let (_, _, fixed) = read(&Path::new(&dir).join("solas-dof.png"));
        assert_eq!((aw, ah), (w, h));
        let mut error = 0u64;
        for (i, (a, b)) in autofocus.iter().zip(&fixed).enumerate() {
            if i % (w as usize) < 270 {
                assert_eq!(a, b, "Autofocus changed UI");
            }
            error += (0..3).map(|c| a[c].abs_diff(b[c]) as u64).sum::<u64>();
        }
        let mean = error as f64 / (w as f64 * h as f64 * 3.);
        assert!(mean < 2., "Autofocus damaged scene: {mean}");
        println!(
            "Solas autofocus: mean RGB change vs fixed focus {mean:.4}; UI and opacity preserved."
        );
    }
    if std::env::args().any(|a| a == "--solas") {
        let (sw, sh, soft) = read(&Path::new(&dir).join("solas-soft.png"));
        let (_, _, bloom) = read(&Path::new(&dir).join("solas-bloom.png"));
        assert_eq!((sw, sh), (w, h));
        let mut changed = 0;
        let mut error = 0u64;
        for (i, (a, b)) in soft.iter().zip(&bloom).enumerate() {
            if i % (w as usize) < 270 {
                assert_eq!(a, b, "Solas settings changed UI");
            }
            let delta = (0..3).map(|c| a[c].abs_diff(b[c]) as u64).sum::<u64>();
            changed += usize::from(delta > 6);
            error += delta;
        }
        let mean = error as f64 / (w as f64 * h as f64 * 3.);
        assert!(
            changed > 1000 && (0.1..30.).contains(&mean),
            "Solas settings missing or corrupt: {changed} pixels, mean {mean}"
        );
        println!(
            "Solas soft preset: {changed} changed pixels, mean RGB change {mean:.4}; UI and opacity preserved."
        );
    }
    if std::env::args().any(|a| a == "--solas") {
        let (_, _, baseline_pan) = read(&Path::new(&dir).join("solas-bloom-pan.png"));
        let (_, _, bloom) = read(&Path::new(&dir).join("solas-bloom.png"));
        assert_eq!(
            baseline_pan, bloom,
            "scripted motion must end on the same unblurred scene"
        );
        let (mw, mh, moving) = read(&Path::new(&dir).join("solas-motion-pan.png"));
        let (_, _, stationary) = read(&Path::new(&dir).join("solas-motion.png"));
        assert_eq!((mw, mh), (w, h));
        let mut error = 0u64;
        let mut changed = 0;
        for (i, (a, b)) in moving.iter().zip(&stationary).enumerate() {
            if i % (w as usize) < 270 {
                assert_eq!(a, b, "motion blur changed UI");
            }
            let delta = (0..3).map(|c| a[c].abs_diff(b[c]) as u64).sum::<u64>();
            error += delta;
            changed += usize::from(delta > 6);
        }
        let mean = error as f64 / (w as f64 * h as f64 * 3.);
        assert!(
            changed > 1000 && (0.01..20.).contains(&mean),
            "Solas motion missing or corrupt: {changed} pixels, mean {mean}"
        );
        println!(
            "Solas original motion blur: {changed} changed pixels, mean RGB change {mean:.4}; unblurred control and UI preserved."
        );
    }
    if std::env::args().any(|a| a == "--solas") {
        let (cw, ch, chromatic) = read(&Path::new(&dir).join("solas-chromatic.png"));
        let (_, _, off) = read(&Path::new(&dir).join("solas-chromatic-off.png"));
        let (_, _, bloom) = read(&Path::new(&dir).join("solas-bloom.png"));
        assert_eq!((cw, ch), (w, h));
        assert_eq!(
            off, bloom,
            "disabling chromatic aberration must restore default final exactly"
        );
        let mut changed = 0;
        let mut error = 0u64;
        for (i, (a, b)) in chromatic.iter().zip(&bloom).enumerate() {
            if i % (w as usize) < 270 {
                assert_eq!(a, b, "chromatic aberration changed UI");
            }
            let delta = (0..3).map(|c| a[c].abs_diff(b[c]) as u64).sum::<u64>();
            changed += usize::from(delta > 6);
            error += delta;
        }
        let mean = error as f64 / (w as f64 * h as f64 * 3.);
        assert!(
            changed > 1000 && (0.01..20.).contains(&mean),
            "Solas chromatic aberration missing or corrupt: {changed}, {mean}"
        );
        println!(
            "Solas original chromatic aberration: {changed} changed pixels, mean RGB change {mean:.4}; off restores exact default, UI and opacity preserved."
        );
    }
    if std::env::args().any(|a| a == "--solas") {
        let (_, _, bloom) = read(&Path::new(&dir).join("solas-bloom.png"));
        for name in [
            "solas-taa",
            "solas-taa-pan",
            "solas-taa-cut",
            "solas-taa-object",
        ] {
            let (tw, th, taa) = read(&Path::new(&dir).join(format!("{name}.png")));
            assert_eq!((tw, th), (w, h));
            if name == "solas-taa-object" {
                let (_, _, still) = read(&Path::new(&dir).join("solas-taa.png"));
                let (_, _, reset) = read(&Path::new(&dir).join("solas-taa-cut.png"));
                for y in 0..h { for x in 0..w {
                    let i = (y*w+x) as usize;
                    if !(680..960).contains(&x) || !(440..700).contains(&y) {
                        assert_eq!(taa[i], still[i], "object rejection discarded unrelated history at {x},{y}");
                    }
                    // Old and new sprite positions must match a frame without history.
                    if (545..565).contains(&y) && ((720..750).contains(&x) || (812..835).contains(&x)) {
                        assert_eq!(taa[i], reset[i], "stale object history at {x},{y}");
                    }
                }}
            }
            // Opaque neutral floor patches crossed by the former atlas-boundary cracks.
            // Jitter must not expose the blue clear color between adjacent tiles.
            for [left, top, right, bottom] in [[450, 120, 500, 250], [530, 550, 570, 610]] {
                for y in top..bottom {
                    for x in left..right {
                        let pixel = taa[(y * w + x) as usize];
                        assert!(
                            i16::from(pixel[2]) - i16::from(pixel[0]) <= 15,
                            "{name}: blue background leaked through opaque floor at {x},{y}: {pixel:?}"
                        );
                    }
                }
            }
            let mut changed = 0;
            let mut error = 0u64;
            for (i, (a, b)) in taa.iter().zip(&bloom).enumerate() {
                if i % (w as usize) < 270 {
                    assert_eq!(a, b, "TAA jitter changed UI");
                }
                let delta = (0..3).map(|c| a[c].abs_diff(b[c]) as u64).sum::<u64>();
                changed += usize::from(delta > 0);
                error += delta;
            }
            let mean = error as f64 / (w as f64 * h as f64 * 3.);
            assert!(
                changed > 100 && (0.001..15.).contains(&mean),
                "{name} missing or corrupt: {changed}, {mean}"
            );
            println!(
                "{name}: {changed} changed pixels, mean RGB change {mean:.4}; UI and opacity preserved."
            );
        }
    }
    println!(
        "GPU surface capture checked: {normal_pixels} normal pixels, {} depth levels, {} albedo colors; UI identical.",
        depths.len(),
        colors.len()
    );
}

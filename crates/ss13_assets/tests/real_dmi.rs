//! Checks against the real source repo (`SS13_SOURCE`, default `../BandaStation`).
//! Ignored by default: `cargo test -p ss13_assets --release -- --ignored`.

use std::io::Cursor;
use std::path::PathBuf;

use ss13_assets::dmi::{Dmi, DmiMeta};
use ss13_assets::import::{ATLAS_DIR, MANIFEST_FILE, find_dmi_files, import_dmi};
use ss13_assets::manifest::AssetManifest;

fn source() -> PathBuf {
    std::env::var("SS13_SOURCE")
        .unwrap_or_else(|_| env!("SS13_SOURCE").into())
        .into()
}

fn read(rel: &str) -> Dmi {
    Dmi::read(&std::fs::read(source().join(rel)).unwrap()).unwrap()
}

/// The Description text, read independently of `ss13_assets::dmi`.
fn raw_description(bytes: &[u8]) -> String {
    let reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let chunk = reader.info().compressed_latin1_text.iter();
    chunk
        .filter(|c| c.keyword == "Description")
        .map(|c| c.get_text().unwrap())
        .next()
        .unwrap()
}

/// Naive per-state `(dirs, frames, delay list as written)` straight from the text.
fn naive_states(text: &str) -> Vec<(u8, u32, Vec<f32>)> {
    let mut out: Vec<(u8, u32, Vec<f32>)> = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with("state = ") {
            out.push((1, 1, Vec::new()));
        } else if let Some(v) = l.strip_prefix("dirs = ") {
            out.last_mut().unwrap().0 = v.parse().unwrap();
        } else if let Some(v) = l.strip_prefix("frames = ") {
            out.last_mut().unwrap().1 = v.parse().unwrap();
        } else if let Some(v) = l.strip_prefix("delay = ") {
            out.last_mut().unwrap().2 = v.split(',').map(|d| d.parse().unwrap()).collect();
        }
    }
    out
}

#[test]
#[ignore = "needs the source repo"]
fn every_dmi_parses_and_matches_its_description() {
    let src = source();
    let files = find_dmi_files(&src).unwrap();
    assert!(files.len() > 1000, "found only {} .dmi files", files.len());
    let mut failures = Vec::new();
    let mut states = 0;
    for rel in &files {
        let bytes = std::fs::read(src.join(rel)).unwrap();
        let dmi = match Dmi::read(&bytes) {
            Ok(d) => d,
            Err(e) => {
                failures.push(format!("{rel}: {e}"));
                continue;
            }
        };
        let naive = naive_states(&raw_description(&bytes));
        let DmiMeta { states: parsed, .. } = &dmi.meta;
        assert_eq!(parsed.len(), naive.len(), "{rel}: state count");
        for (s, (dirs, frames, delays)) in parsed.iter().zip(&naive) {
            assert_eq!((s.dirs, s.frames), (*dirs, *frames), "{rel} {:?}", s.name);
            if s.frames > 1 {
                assert_eq!(s.delays.len() as u32, s.frames, "{rel} {:?}", s.name);
                let n = delays.len().min(s.delays.len());
                assert_eq!(s.delays[..n], delays[..n], "{rel} {:?}", s.name);
                assert!(s.delays[n..].iter().all(|&d| d == 1.0));
            } else {
                assert!(s.delays.is_empty());
            }
        }
        states += parsed.len();
    }
    println!("{} files, {states} states", files.len());
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
#[ignore = "needs the source repo"]
fn known_states_match_description() {
    // state = "opening": dirs = 1, frames = 6, delay = 1,1,1,1,1,1, loop = 1
    let doors = read("icons/obj/doors/airlocks/station/public.dmi");
    let opening = doors
        .meta
        .states
        .iter()
        .find(|s| s.name == "opening")
        .unwrap();
    assert_eq!(
        (opening.dirs, opening.frames, opening.loop_count),
        (1, 6, 1)
    );
    assert_eq!(opening.delays, [1.0; 6]);

    // state = "chicken_brown": dirs = 4, frames = 3, delay = 2,2,2, movement = 1
    let animal = read("icons/mob/simple/animal.dmi");
    let chicken = &animal.meta.states[1];
    assert_eq!(chicken.name, "chicken_brown");
    assert_eq!((chicken.dirs, chicken.frames), (4, 3));
    assert_eq!(chicken.delays, [2.0; 3]);
    assert!(chicken.movement);

    // 96x96 sheet; state = "smoke": dirs = 4, frames = 3, delay = 2,1,1, rewind = 1
    let big = read("icons/effects/96x96.dmi");
    assert_eq!((big.meta.width, big.meta.height), (96, 96));
    let smoke = big.meta.states.iter().find(|s| s.name == "smoke").unwrap();
    assert_eq!((smoke.dirs, smoke.frames, smoke.rewind), (4, 3, true));
    assert_eq!(smoke.delays, [2.0, 1.0, 1.0]);
    assert_eq!(big.icon_rect(smoke.icon_index(2, 3))[2..], [96, 96]);

    // state = "erasing": frames = 22 but 23 delays are written; the extra one is dropped.
    let chronos = read("icons/obj/chronos.dmi");
    let erasing = chronos
        .meta
        .states
        .iter()
        .find(|s| s.name == "erasing")
        .unwrap();
    assert_eq!(erasing.delays.len(), 22);
}

#[test]
#[ignore = "needs the source repo; slow in debug, run with --release"]
fn full_import_writes_a_consistent_manifest() {
    let out = std::env::temp_dir().join(format!("ss13-import-test-{}", std::process::id()));
    let report = import_dmi(&source(), &out, None, None).unwrap();
    assert!(report.failures.is_empty(), "{:#?}", report.failures);

    let manifest: AssetManifest =
        postcard::from_bytes(&std::fs::read(out.join(MANIFEST_FILE)).unwrap()).unwrap();
    assert_eq!(manifest.states.len(), report.states);
    assert_eq!(manifest.atlases.len(), report.atlases);
    let mut sizes = Vec::new();
    let mut atlases = Vec::new();
    for (i, a) in manifest.atlases.iter().enumerate() {
        assert_eq!(
            a,
            &format!("{ATLAS_DIR}/{i}.{}", ss13_assets::page::EXTENSION)
        );
        let page = ss13_assets::page::decode(&std::fs::read(out.join(a)).unwrap()).unwrap();
        sizes.push((page.width, page.height));
        atlases.push((page.width, page.rgba));
    }
    for s in &manifest.states {
        let (w, h) = sizes[s.atlas as usize];
        assert_eq!(s.rects.len(), s.delays.len().max(1) * usize::from(s.dirs));
        for r in &s.rects {
            assert!(
                r[0] + r[2] <= w && r[1] + r[3] <= h,
                "{} {:?}",
                s.dmi,
                s.state
            );
        }
    }
    // Pixel check: sampled states' atlas rects hold exactly the source icons.
    for s in manifest.states.iter().step_by(499) {
        let dmi = read(&s.dmi);
        let src = dmi.meta.states.iter().find(|d| d.name == s.state).unwrap();
        let (aw, px) = &atlases[s.atlas as usize];
        for (k, r) in s.rects.iter().enumerate() {
            let icon = dmi.icon_pixels(src.first_icon + k as u32);
            let row = r[2] as usize * 4;
            for y in 0..r[3] as usize {
                let start = ((r[1] as usize + y) * *aw as usize + r[0] as usize) * 4;
                assert_eq!(
                    px[start..start + row],
                    icon[y * row..(y + 1) * row],
                    "{} {:?}",
                    s.dmi,
                    s.state
                );
            }
        }
    }
    std::fs::remove_dir_all(&out).unwrap();
}

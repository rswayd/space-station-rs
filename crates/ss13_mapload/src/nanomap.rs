//! BandaStation's station map images (`/datum/asset/simple/nanomaps`,
//! `modular_bandastation/nanomap/code/nanomap_assets.dm`), the backdrop of the camera console's
//! and the crew monitor's maps. `ss13-mapload convert` copies them to
//! `<assets>/nanomap/<asset key>` (the key tgui's `resolveAsset` gets, built from the map json's
//! `map_name`), with the attribution files of their source directories under
//! `nanomap/licenses/<source path>/` and a `SOURCES.txt` (file, source path, source commit).

use std::path::Path;

use anyhow::Context as _;
use ss13_assets::license::Attributions;

use crate::sound::CopyReport;

/// `<assets>/nanomap/`; the client names it too (`ss13_game::nanomap::NANOMAP_DIR`).
pub const NANOMAP_DIR: &str = "nanomap";

/// `/datum/asset/simple/nanomaps` `assets`: asset key -> file in the source.
pub const NANOMAPS: [(&str, &str); 15] = [
    (
        "Cyberiad_nanomap_z1.png",
        "icons/_nanomaps/Cyberiad_nanomap_z1.png",
    ),
    (
        "Cyberiad_nanomap_z2.png",
        "icons/_nanomaps/Cyberiad_nanomap_z2.png",
    ),
    (
        "Delta Station_nanomap_z1.png",
        "icons/_nanomaps/Delta Station_nanomap_z1.png",
    ),
    (
        "Ice Box Station_nanomap_z1.png",
        "icons/_nanomaps/Ice Box Station_nanomap_z1.png",
    ),
    (
        "Ice Box Station_nanomap_z2.png",
        "icons/_nanomaps/Ice Box Station_nanomap_z2.png",
    ),
    (
        "Ice Box Station_nanomap_z3.png",
        "icons/_nanomaps/Ice Box Station_nanomap_z3.png",
    ),
    (
        "MetaStation_nanomap_z1.png",
        "icons/_nanomaps/MetaStation_nanomap_z1.png",
    ),
    (
        "Tramstation_nanomap_z1.png",
        "icons/_nanomaps/Tramstation_nanomap_z1.png",
    ),
    (
        "Tramstation_nanomap_z2.png",
        "icons/_nanomaps/Tramstation_nanomap_z2.png",
    ),
    (
        "Lavaland_nanomap_z1.png",
        "icons/_nanomaps/Lavaland_nanomap_z1.png",
    ),
    (
        "Kilo Station_nanomap_z1.png",
        "icons/_nanomaps/KiloStation_nanomap_z1.png",
    ),
    (
        "NebulaStation_nanomap_z1.png",
        "icons/_nanomaps/NebulaStation_nanomap_z1.png",
    ),
    (
        "NebulaStation_nanomap_z2.png",
        "icons/_nanomaps/NebulaStation_nanomap_z2.png",
    ),
    (
        "Wawastation_nanomap_z1.png",
        "icons/_nanomaps/Wawastation_nanomap_z1.png",
    ),
    (
        "Wawastation_nanomap_z2.png",
        "icons/_nanomaps/Wawastation_nanomap_z2.png",
    ),
];

/// The assets' file of an asset key: `<key without .png>.qoi`.
pub fn qoi_name(key: &str) -> String {
    format!(
        "{}.{}",
        key.strip_suffix(".png").unwrap_or(key),
        ss13_assets::page::EXTENSION
    )
}

/// Converts [`NANOMAPS`] from `source` to `<assets>/nanomap/<key>.qoi` and writes `SOURCES.txt`.
pub fn copy_nanomaps(source: &Path, assets: &Path, commit: &str) -> anyhow::Result<CopyReport> {
    let out = assets.join(NANOMAP_DIR);
    std::fs::create_dir_all(&out)?;
    let mut report = CopyReport::default();
    let mut sources = format!(
        "# Station map images (nanomaps) copied by `ss13-mapload convert` from BandaStation \
         (tgstation fork), commit {commit}.\n# BandaStation's NanoMap bot renders them from the \
         station maps (`NanoMap Auto-Update` commits). License: CC BY-SA 3.0 like the icons, \
         unless an attribution/license file under licenses/ says otherwise.\n\
         # file\tsource path\n"
    );
    let mut attributions = Attributions::default();
    for (key, rel) in NANOMAPS {
        let from = source.join(rel);
        if !from.is_file() {
            report.missing.push(rel.to_owned());
            continue;
        }
        // QOI like the atlas pages (`ss13_assets::page`): a 2040 px map decodes about 3x faster
        // than its PNG, which matters on the browser's main thread.
        let png = std::fs::read(&from).with_context(|| format!("reading {rel}"))?;
        let pixels =
            ss13_assets::dmi::read_png_rgba8(&png).with_context(|| format!("decoding {rel}"))?;
        let qoi = ss13_assets::page::encode(pixels.width, pixels.height, &pixels.rgba)
            .with_context(|| format!("encoding {rel}"))?;
        std::fs::write(out.join(qoi_name(key)), &qoi)?;
        report.bytes += qoi.len() as u64;
        report.copied += 1;
        sources.push_str(&format!("{}\t{rel}\n", qoi_name(key)));
        attributions.add(Path::new(rel), rel);
    }
    report.attributions = attributions.copy(source, &out.join("licenses"))?;
    std::fs::write(out.join("SOURCES.txt"), sources)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_under_the_asset_key() {
        let dir = std::env::temp_dir().join(format!("ss13-nanomap-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (source, assets) = (dir.join("src"), dir.join("assets"));
        std::fs::create_dir_all(source.join("icons/_nanomaps")).unwrap();
        std::fs::write(source.join("icons/license.txt"), "icons").unwrap();
        // A 2x1 RGBA PNG: one red pixel, one transparent.
        let rgba = [255, 0, 0, 255, 0, 0, 0, 0];
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, 2, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&rgba).unwrap();
        }
        std::fs::write(
            source.join("icons/_nanomaps/KiloStation_nanomap_z1.png"),
            &png,
        )
        .unwrap();
        let report = copy_nanomaps(&source, &assets, "abc").unwrap();
        assert_eq!((report.copied, report.attributions), (1, 1));
        assert_eq!(report.missing.len(), NANOMAPS.len() - 1);
        let out = assets.join(NANOMAP_DIR);
        // The map json's `map_name` is "Kilo Station"; the file has no space.
        // Stored as QOI with the same pixels.
        let qoi = std::fs::read(out.join("Kilo Station_nanomap_z1.qoi")).unwrap();
        let page = ss13_assets::page::decode(&qoi).unwrap();
        assert_eq!((page.width, page.height, page.rgba), (2, 1, rgba.to_vec()));
        assert!(!out.join("Kilo Station_nanomap_z1.png").exists());
        assert!(out.join("licenses/icons/license.txt").is_file());
        let sources = std::fs::read_to_string(out.join("SOURCES.txt")).unwrap();
        assert!(sources.contains("commit abc"));
        assert!(
            sources.contains(
                "Kilo Station_nanomap_z1.qoi\ticons/_nanomaps/KiloStation_nanomap_z1.png"
            )
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

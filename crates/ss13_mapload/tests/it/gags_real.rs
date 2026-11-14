//! Acceptance: every converted GAGS type draws its rendered icon (`ss13_assets::gags`), and the
//! renders match tg's own: the `icons/map_icons/` previews are GAGS renders of each type's
//! `post_init_icon_state` (`SSgreyscale_previews`, through the same iconforge code).
//!
//! Needs `SS13_ASSETS` built by the full pipeline (`ss13-dm-extract`, `ss13-import --types`,
//! `ss13-mapload convert`). Run with `--nocapture` for the numbers.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_assets::gags::PREFIX;
use ss13_assets::manifest::{AssetManifest, IconStateInfo};
use ss13_mapload::convert::DmTypes;
use ss13_mapload::item::DmItem;
use ss13_mapload::{DmAtom, DmProtoPlugin, IconResolution, Template, read_manifest};
use ss13_proto::{ProtoComponents, Prototypes};

struct Atlases<'a> {
    root: &'a Path,
    pages: HashMap<u32, (u32, Vec<u8>)>,
}

impl Atlases<'_> {
    /// RGBA8 rows of frame `i` of `s`.
    fn frame(&mut self, manifest: &AssetManifest, s: &IconStateInfo, i: usize) -> Vec<u8> {
        let (width, rgba) = self.pages.entry(s.atlas).or_insert_with(|| {
            let path = self.root.join(&manifest.atlases[s.atlas as usize]);
            let page = ss13_assets::page::decode(&std::fs::read(&path).unwrap()).unwrap();
            (page.width, page.rgba)
        });
        let [x, y, w, h] = s.rects[i];
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for row in y..y + h {
            let start = ((row * *width + x) * 4) as usize;
            out.extend_from_slice(&rgba[start..start + (w * 4) as usize]);
        }
        out
    }
}

/// Pixels equal (both transparent counts as equal), and within 2 per channel.
fn compare(a: &[u8], b: &[u8]) -> (usize, usize, usize) {
    let (mut exact, mut close) = (0, 0);
    for (p, q) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0) {
        if p == q || (p[3] == 0 && q[3] == 0) {
            exact += 1;
            close += 1;
        } else if p.iter().zip(q).all(|(x, y)| x.abs_diff(*y) <= 2) {
            close += 1;
        }
    }
    (exact, close, a.len() / 4)
}

#[test]
#[ignore = "needs generated assets (SS13_ASSETS)"]
fn gags_types_resolve_and_match_tg_previews() {
    let assets = std::env::var_os("SS13_ASSETS")
        .map(PathBuf::from)
        .expect("SS13_ASSETS");
    let types = DmTypes::read(&assets.join("dm_types.json")).unwrap();
    let manifest = read_manifest(&assets).unwrap();
    let index = manifest.index();
    let ron_path = assets.join("prototypes/dm_types.ron");
    let ron = std::fs::read_to_string(&ron_path).unwrap();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, DmProtoPlugin));
    let world = app.world();
    let protos = Prototypes::from_sources(
        &[(ron_path, ron)],
        world.resource::<ProtoComponents>(),
        world.resource::<AppTypeRegistry>(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let gags_keys: std::collections::HashSet<&str> = manifest
        .states
        .iter()
        .filter(|s| s.dmi.starts_with(PREFIX))
        .map(|s| s.dmi.as_str())
        .collect();

    let mut atlases = Atlases {
        root: &assets,
        pages: HashMap::new(),
    };
    let mut ids: Vec<&str> = protos.ids().collect();
    ids.sort_unstable();
    let (mut gags_types, mut unresolved, mut missing_worn) = (0, Vec::new(), Vec::new());
    let mut were_placeholders = Vec::new();
    let mut default_state = 0;
    let mut placeholders = Vec::new();
    let (mut compared, mut exact, mut close, mut total) = (0, 0, 0, 0);
    let mut shape_mismatch = Vec::new();
    let mut worst: BTreeMap<String, f64> = BTreeMap::new();
    let mut named: BTreeMap<&str, f64> = BTreeMap::new();
    for id in ids {
        let proto = protos.get(id).unwrap();
        let Some(atom) = proto.get::<DmAtom>() else {
            continue;
        };
        if let Some(item) = proto.get::<DmItem>() {
            for file in [&item.worn_icon, &item.lefthand_file, &item.righthand_file] {
                if file.starts_with(PREFIX) && !gags_keys.contains(file.as_str()) {
                    missing_worn.push(format!("{id}: {file}"));
                }
            }
        }
        if !atom.icon.starts_with(PREFIX) {
            // Without a config the map preview stays (`load::map_icon_preview`); its default
            // state is tg's "ERROR" placeholder.
            if atom.icon.starts_with("icons/map_icons/")
                && Template::new(proto.clone(), &index).icon != IconResolution::Exact
            {
                placeholders.push(id.to_owned());
            }
            continue;
        }
        gags_types += 1;
        let template = Template::new(proto.clone(), &index);
        // `DefaultState`: tg's canisters keep their preview `icon_state`
        // (`post_init_icon_state = ""`), so BYOND draws the render's default state.
        match template.icon {
            IconResolution::Exact => {}
            IconResolution::DefaultState => default_state += 1,
            _ => {
                unresolved.push(format!("{id}: {}:{:?}", atom.icon, atom.icon_state));
                continue;
            }
        }
        // What the source names: the map preview file and state.
        let source_icon = types
            .resolved(id, "icon")
            .and_then(|v| v.get("resource"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if !source_icon.starts_with("icons/map_icons/") {
            continue;
        }
        let source_state = types
            .resolved(id, "icon_state")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        // Before GAGS: the exact state, else the nearest ancestor's preview, else "ERROR".
        let had_preview = index.contains_key(&(source_icon, source_state))
            || std::iter::successors(Some(id), |p| {
                p.rfind('/').filter(|&i| i > 0).map(|i| &p[..i])
            })
            .any(|p| index.contains_key(&(source_icon, p)));
        if !had_preview {
            were_placeholders.push(id.to_owned());
        }
        // tg previews the type's own render under its path (absent for `NO_NEW_GAGS_PREVIEW_1`).
        let Some(&preview) = index.get(&(source_icon, id)) else {
            continue;
        };
        let ours = &manifest.states[template.appearance.as_ref().unwrap().icon_state.0 as usize];
        let theirs = &manifest.states[preview.0 as usize];
        if (ours.width, ours.height, ours.rects.len())
            != (theirs.width, theirs.height, theirs.rects.len())
        {
            shape_mismatch.push(format!(
                "{id}: ours {}x{} x{} vs preview {}x{} x{}",
                ours.width,
                ours.height,
                ours.rects.len(),
                theirs.width,
                theirs.height,
                theirs.rects.len()
            ));
            continue;
        }
        compared += 1;
        let (mut e, mut c, mut t) = (0, 0, 0);
        for i in 0..ours.rects.len() {
            let a = atlases.frame(&manifest, ours, i);
            let b = atlases.frame(&manifest, theirs, i);
            let (de, dc, dt) = compare(&a, &b);
            (e, c, t) = (e + de, c + dc, t + dt);
        }
        (exact, close, total) = (exact + e, close + c, total + t);
        let ratio = e as f64 / t as f64;
        if ratio < 1.0 {
            worst.insert(id.to_owned(), ratio);
        }
        for name in [
            "/obj/item/clothing/under/color/grey",
            "/obj/item/modular_computer/pda/crew/medical",
            "/obj/item/clothing/head/beret",
            "/obj/item/clothing/shoes/sneakers/brown",
        ] {
            if id == name {
                named.insert(name, ratio);
            }
        }
    }
    println!(
        "{gags_types} prototypes draw a GAGS icon ({} rendered icons), {default_state} its default \
         state",
        gags_keys.len()
    );
    println!(
        "{} of them showed the map_icons ERROR placeholder before: {were_placeholders:?}",
        were_placeholders.len()
    );
    println!(
        "compared with tg previews: {compared} types, {total} px: exact {:.4}%, within 2: {:.4}%",
        100.0 * exact as f64 / total as f64,
        100.0 * close as f64 / total as f64
    );
    println!("named: {named:?}");
    println!("not exact ({}): {worst:?}", worst.len());
    println!(
        "shape mismatches ({}): {shape_mismatch:?}",
        shape_mismatch.len()
    );
    assert!(
        placeholders.is_empty(),
        "map_icons placeholders: {placeholders:?}"
    );
    assert!(
        unresolved.is_empty(),
        "GAGS icons without their state: {unresolved:?}"
    );
    assert!(
        missing_worn.is_empty(),
        "worn/in-hand GAGS icons not rendered: {missing_worn:?}"
    );
    assert!(
        gags_types > 200 && compared > 100,
        "{gags_types} {compared}"
    );
    assert!(close as f64 / total as f64 > 0.99, "previews differ");
    assert_eq!(named.len(), 4, "{named:?}");
    assert!(named.values().all(|&r| r > 0.99), "{named:?}");
}

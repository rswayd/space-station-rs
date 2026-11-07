//! Loading the wizard den template and adopting its items.

use super::*;

/// The types the den places that have no prototype (all of them without prototypes).
pub(super) fn missing_types(world: &World, den: &std::path::Path) -> Vec<String> {
    let map = match DmmMap::from_file(den) {
        Ok(map) => map,
        Err(e) => return vec![format!("{}: {e}", den.display())],
    };
    let Some(protos) = world.get_resource::<ss13_proto::Prototypes>() else {
        return vec!["(no prototypes)".to_owned()];
    };
    let mut missing: Vec<String> = map
        .prefabs
        .iter()
        .flat_map(|p| p.objects.iter())
        .map(|o| o.path.clone())
        .filter(|p| protos.get(p).is_none())
        .collect();
    missing.sort();
    missing.dedup();
    missing
}

pub(super) fn load_names(world: &mut World) {
    let seed = world
        .get_resource::<ss13_mapload::MapSeed>()
        .map_or(0, |s| s.0);
    world.insert_resource(WizardRng(Rng::new(seed ^ 0x7769_7a61_7264)));
    let source = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.source.clone());
    let read = |file: &str| -> Vec<String> {
        source
            .as_ref()
            .and_then(|s| ss13_data::read(&s.join("strings/names").join(file)).ok())
            .map(|bytes| {
                String::from_utf8_lossy(&bytes)
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    world.insert_resource(WizardNames {
        first: read("wizardfirst.txt"),
        second: read("wizardsecond.txt"),
    });
}

/// `GLOB.wizardstart`: the den's start landmarks.
pub(super) fn find_den(world: &mut World) {
    let Some(path) = world.get_resource::<DenTemplate>().map(|d| d.0.clone()) else {
        return;
    };
    let Some(first_z) = world
        .get_resource::<MapLoadReport>()
        .and_then(|r| r.maps.iter().find(|m| m.path == path).map(|m| m.first_z))
    else {
        warn!("wizard den {} was not loaded", path.display());
        return;
    };
    let Ok(map) = DmmMap::from_file(&path) else {
        return;
    };
    let marks: Vec<bool> = map
        .prefabs
        .iter()
        .map(|p| p.objects.iter().any(|o| o.path == WIZARD_START))
        .collect();
    let mut spawns = Vec::new();
    let (sx, sy, sz) = map.size;
    for mz in 1..=sz {
        for y in 1..=sy {
            for x in 1..=sx {
                let Some(i) = map.index(x, y, mz).map(|i| i as usize) else {
                    continue;
                };
                if marks.get(i).copied().unwrap_or(false) {
                    spawns.push(TilePos::new(x as i32, y as i32, first_z + (mz - 1) as u16));
                }
            }
        }
    }
    info!("wizard den at z{first_z}: {} start tiles", spawns.len());
    world.insert_resource(WizardDen {
        z: Some(first_z),
        spawns,
        next: 0,
    });
}

/// Items spawned from the map, bought or summoned get their state.
pub(super) fn adopt_items(
    mut commands: Commands,
    added: Query<(Entity, &DmItem), Added<DmItem>>,
    books: Query<(), With<Spellbook>>,
    scrolls: Query<(), With<TeleportScroll>>,
    contracts: Query<(), With<ApprenticeContract>>,
) {
    for (e, item) in &added {
        if item.path == SPELLBOOK && books.get(e).is_err() {
            commands.entity(e).insert(Spellbook::default());
        } else if is_under(&item.path, SCROLL) && scrolls.get(e).is_err() {
            let uses = if is_under(&item.path, APPRENTICE_SCROLL) {
                APPRENTICE_SCROLL_USES
            } else {
                SCROLL_USES
            };
            commands.entity(e).insert(TeleportScroll { uses });
        } else if item.path == CONTRACT && contracts.get(e).is_err() {
            commands.entity(e).insert(ApprenticeContract::default());
        }
    }
}

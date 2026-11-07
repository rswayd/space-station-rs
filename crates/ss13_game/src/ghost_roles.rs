//! Map mob spawners (server), after tg `/obj/effect/mob_spawn` (`code/modules/mob_spawn/`):
//! the placed human ghost roles and corpses of [`ss13_mapload::mob_spawn`].
//!
//! - A ghost role spawner (`/obj/effect/mob_spawn/ghost_role/human/*`: sleepers, cryopods, ash
//!   walker eggs) becomes a [`GhostRoleSpawner`] of the spawners menu (`crate::ghost_verbs`);
//!   a ghost's click on it opens that menu (tg `attack_ghost` asks "Become ...?").
//! - Taking it ([`create_human`]): a human of the type's species with a random look, its
//!   `mob_name` (else the player's character name), its outfit, its faction; the role's texts go
//!   to the player (tg `special`'s boxed message).
//! - A corpse (`/obj/effect/mob_spawn/corpse/human/*`) is made at map init (tg `CORPSE_INSTANT`):
//!   the same human, dead, with the type's brute, burn and oxy damage, husked when `husk`; the
//!   spawner goes.
// shortcut: per-type `special()`/`allow_spawn()`/`post_equip()` overrides are not run (ash walker
// tribe team and egg growth, Oldstation access, battlecruiser team, the bartender's ID, hermit
// mood) nor job bans and the "recently died" check; `outfit_override`, hair and skin vars,
// `corpse_description` and `conceal_presence` (sensors, PDA) are not applied; every outfit gets
// the grey backpack, as jobs do. Non-human spawners (spiders, golems, venus traps, drones) are
// their mobs' groups.

use bevy::prelude::*;
use ss13_mapload::jobs::OutfitSlot;
use ss13_mapload::mob_spawn::{DmMobSpawn, MOB_SPAWN_FILE, MobSpawnData, MobSpawnDef};
use ss13_mapload::{MapLoadPlugin, Template, read_manifest};
use ss13_shared::TilePos;
use ss13_world::{MapInitLate, Mapped};

use crate::ghost_verbs::GhostRoleSpawner;
use crate::health::{DamageType, Health, Stat};
use crate::inventory::Inventory;
use crate::outfit::OutfitTemplates;

/// [`MOB_SPAWN_FILE`], read on the first map init.
#[derive(Resource, Default, Debug)]
pub struct MobSpawns(pub MobSpawnData);

pub struct GhostRolesPlugin;

impl Plugin for GhostRolesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(MapInitLate, init_mob_spawns);
    }
}

/// The spawner type's vars.
pub fn def(world: &World, path: &str) -> Option<MobSpawnDef> {
    world.get_resource::<MobSpawns>()?.0.get(path).cloned()
}

/// Loads [`MobSpawns`] and adds its outfits' items to the outfit templates.
fn load(world: &mut World) {
    let Some(assets) = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.assets.clone())
    else {
        world.init_resource::<MobSpawns>();
        return;
    };
    let data = MobSpawnData::read(&assets.join(MOB_SPAWN_FILE)).unwrap_or_else(|e| {
        warn!("mob spawners: {e:#}");
        MobSpawnData::default()
    });
    let manifest = read_manifest(&assets).unwrap_or_default();
    let index = manifest.index();
    let mut added = Vec::new();
    {
        let have = world.get_resource::<OutfitTemplates>();
        let protos = world.get_resource::<ss13_proto::Prototypes>();
        for path in data.item_types() {
            if have.is_some_and(|t| t.0.contains_key(path)) {
                continue;
            }
            if let Some(proto) = protos.and_then(|p| p.get(path).cloned()) {
                added.push((path.to_owned(), Template::new(proto, &index)));
            }
        }
    }
    if !world.contains_resource::<OutfitTemplates>() {
        world.init_resource::<OutfitTemplates>();
    }
    world.resource_mut::<OutfitTemplates>().0.extend(added);
    info!("{} mob spawner types", data.spawns.len());
    world.insert_resource(MobSpawns(data));
}

/// The placed spawners of this map pass: roles into the menu, corpses made.
fn init_mob_spawns(world: &mut World) {
    if !world.contains_resource::<MobSpawns>() {
        load(world);
    }
    let mut q = world.query_filtered::<(Entity, &DmMobSpawn, &TilePos), With<Mapped>>();
    let found: Vec<(Entity, String, TilePos)> = q
        .iter(world)
        .map(|(e, s, &p)| (e, s.path.clone(), p))
        .collect();
    let mut corpses = 0;
    for (spawner, path, pos) in found {
        let Some(def) = def(world, &path) else {
            warn!("{path}: no mob spawner data (rerun `ss13-mapload convert`)");
            continue;
        };
        if def.corpse {
            let name = def.mob_name.clone().unwrap_or_else(|| def.name.clone());
            create_corpse(world, &def, pos, name);
            world.despawn(spawner);
            corpses += 1;
        } else if world.get::<GhostRoleSpawner>(spawner).is_none() {
            world.entity_mut(spawner).insert(GhostRoleSpawner {
                name: def.name.clone(),
                desc: role_text(&def),
                uses: def.uses,
                def: Some(path),
            });
        }
    }
    if corpses > 0 {
        info!("{corpses} corpses made");
    }
}

/// The menu text: `you_are_text`, `flavour_text`, `important_text` (tg's spawners menu shows
/// all three).
pub fn role_text(def: &MobSpawnDef) -> String {
    [&def.you_are_text, &def.flavour_text, &def.important_text]
        .into_iter()
        .filter(|t| !t.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
}

/// tg `mob_spawn/create` of a human: species, a random look, `name`, the outfit, the faction.
pub fn create_human(world: &mut World, def: &MobSpawnDef, pos: TilePos, name: String) -> Entity {
    let seed = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(0, u32::MAX);
    let mut look = crate::human::AppearancePrefs::random(u64::from(seed)).body();
    if let Some(species) = def.mob_species.as_deref().and_then(|p| {
        crate::species::SPECIES
            .iter()
            .find(|d| d.path == p)
            .map(|d| d.id)
    }) {
        look.species = species;
    }
    let mob = crate::server::spawn_body(world, pos, name, look);
    let mut inventory = match &def.outfit {
        Some(outfit) => world
            .try_resource_scope(|world, templates: Mut<OutfitTemplates>| {
                crate::outfit::equip_outfit(world, mob, outfit, &templates.0)
            })
            .unwrap_or_default(),
        None => Inventory::default(),
    };
    crate::species::equip_species_outfit(world, mob, &mut inventory);
    if let Some(outfit) = def
        .outfit
        .as_ref()
        .filter(|o| o.item(OutfitSlot::Back).is_some())
    {
        inventory.back = crate::storage::spawn_backpack(world, mob);
        if let Some(pack) = inventory.back {
            crate::outfit::fill_backpack(world, pack, outfit);
        }
    }
    world.entity_mut(mob).insert(inventory);
    crate::inventory::refresh_look(world, mob);
    if !def.faction.is_empty() {
        world
            .entity_mut(mob)
            .insert(crate::mobs::Faction(def.faction.clone()));
    }
    mob
}

/// tg `corpse/special`: `death()`, then the damage; `husk` drains it.
pub fn create_corpse(world: &mut World, def: &MobSpawnDef, pos: TilePos, name: String) -> Entity {
    let mob = create_human(world, def, pos, name);
    if let Some(mut health) = world.get::<Health>(mob).cloned() {
        health.adjust(DamageType::Oxy, def.oxy_damage);
        health.adjust(DamageType::Brute, def.brute_damage);
        health.adjust(DamageType::Burn, def.burn_damage);
        world.entity_mut(mob).insert(health);
    }
    world.entity_mut(mob).insert(Stat::Dead);
    if def.husk {
        crate::death::become_husk(world, mob);
    }
    mob
}

//! Conjuring (server) after tg `spell_types/conjure/` (`/datum/action/cooldown/spell/conjure`)
//! and `spell_types/conjure_item/` (`/datum/action/cooldown/spell/conjure_item`).
//!
//! - **Conjure** ([`ConjureDef`], `cast`): `summon_amount` things of a random `summon_type` on
//!   random tiles within `summon_radius` (none of them twice; dense turfs skipped with
//!   `summon_respects_density`); `summon_lifespan` removes them later. Cult floors and walls
//!   change the turf. Summon Simians adds a monkey per level and makes the summons and the
//!   caster (a minute) `FACTION_MONKEY`; at max level it needs no robe and grants Gorilla Form.
//!   The traps' charges are one each.
//! - **Conjure item** ([`ItemDef`], `cast`): the item into an empty hand of the caster,
//!   `delete_old` replacing what the spell conjured before; `requires_hands` needs hands. The
//!   spell packet of Thrown Lightning shocks (80) what it hits and is gone.
// shortcut: the traps do not spare their caster's mind (no `immune_minds`, `crate::traps`), the
// simian's gear, the justicebot's emag and laser, the chair's buckling and the mime's
// `SPELL_REQUIRES_MIME_VOW` are not ported; an occupied active hand is not emptied first (the
// other hand is used); `create_summon_timer` is unused by the ported spells.

use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::SpatialIndex;

use super::effects::{pick, put_in_hands};
use super::{MAGIC_RESISTANCE, Spell, Spells, can_block_magic, chat_to, now};
use crate::inventory::Inventory;
use crate::traps::{DeleteAt, Trap};

const FACTION_MONKEY: &str = "monkey";
/// `simian/cast`: the caster's minute among the monkeys.
const SIMIAN_ALLY_SECS: f64 = 60.0;
pub const SPELL_PACKET: &str = "/obj/item/spellpacket/lightningbolt";
/// `spellpacket/throw_impact`: `electrocute_act(80)`.
const PACKET_SHOCK: f32 = 80.0;

/// `/datum/action/cooldown/spell/conjure` vars.
#[derive(Clone, Copy, Debug)]
pub struct ConjureDef {
    pub spell: Spell,
    pub types: &'static [&'static str],
    pub radius: u32,
    pub amount: u32,
    /// Seconds; 0: for good.
    pub lifespan: f64,
    pub respects_density: bool,
}

const fn conj(
    spell: Spell,
    types: &'static [&'static str],
    radius: u32,
    amount: u32,
) -> ConjureDef {
    ConjureDef {
        spell,
        types,
        radius,
        amount,
        lifespan: 0.0,
        respects_density: false,
    }
}

pub const CONJURES: [ConjureDef; 20] = [
    conj(Spell::Bees, &["/mob/living/basic/bee/toxin"], 3, 9),
    conj(Spell::Carp, &["/mob/living/basic/carp"], 1, 1),
    conj(Spell::Cheese, &["/obj/item/food/cheese/wheel"], 1, 9),
    conj(
        Spell::ConstructShell,
        &["/obj/structure/constructshell"],
        0,
        1,
    ),
    conj(
        Spell::ConstructShellLesser,
        &["/obj/structure/constructshell"],
        0,
        1,
    ),
    conj(Spell::Creature, &["/mob/living/basic/creature"], 3, 10),
    conj(Spell::CultFloor, &["/turf/open/floor/engine/cult"], 0, 1),
    conj(
        Spell::CultWall,
        &["/turf/closed/wall/mineral/cult/artificer"],
        0,
        1,
    ),
    conj(
        Spell::EdSwarm,
        &["/mob/living/basic/bot/secbot/ed209"],
        3,
        10,
    ),
    ConjureDef {
        lifespan: 25.0,
        ..conj(Spell::InvisibleChair, &["/obj/structure/chair/mime"], 0, 1)
    },
    ConjureDef {
        lifespan: 30.0,
        ..conj(Spell::InvisibleWall, &["/obj/effect/forcefield/mime"], 0, 1)
    },
    conj(Spell::LinkWorlds, &["/obj/structure/spawner/nether"], 1, 1),
    conj(Spell::Presents, &["/obj/item/gift"], 3, 5),
    // "Listed twice so it's twice as likely".
    conj(
        Spell::Simian,
        &[
            "/mob/living/basic/gorilla/lesser",
            "/mob/living/carbon/human/species/monkey/angry",
            "/mob/living/carbon/human/species/monkey/angry",
        ],
        2,
        4,
    ),
    conj(Spell::Soulstone, &["/obj/item/soulstone"], 0, 1),
    conj(Spell::SoulstoneCult, &["/obj/item/soulstone"], 0, 1),
    conj(
        Spell::SoulstoneNoncult,
        &["/obj/item/soulstone/anybody"],
        0,
        1,
    ),
    conj(
        Spell::SoulstonePurified,
        &["/obj/item/soulstone/anybody/purified"],
        0,
        1,
    ),
    conj(
        Spell::SoulstoneMystic,
        &["/obj/item/soulstone/mystic"],
        0,
        1,
    ),
    ConjureDef {
        lifespan: 300.0,
        respects_density: true,
        ..conj(
            Spell::TheTraps,
            &[
                "/obj/structure/trap/stun",
                "/obj/structure/trap/fire",
                "/obj/structure/trap/chill",
                "/obj/structure/trap/damage",
            ],
            3,
            5,
        )
    },
];

/// `/datum/action/cooldown/spell/conjure_item` vars.
#[derive(Clone, Copy, Debug)]
pub struct ItemDef {
    pub spell: Spell,
    pub item: &'static str,
    pub delete_old: bool,
    pub requires_hands: bool,
    /// `invisible_box`'s `box_lifespan`, seconds; 0: for good.
    pub lifespan: f64,
}

pub const CONJURE_ITEMS: [ItemDef; 5] = [
    ItemDef {
        spell: Spell::InfiniteGuns,
        item: "/obj/item/gun/ballistic/rifle/enchanted",
        delete_old: false,
        requires_hands: true,
        lifespan: 0.0,
    },
    ItemDef {
        spell: Spell::ArcaneBarrage,
        item: "/obj/item/gun/magic/wand/arcane_barrage",
        delete_old: false,
        requires_hands: true,
        lifespan: 0.0,
    },
    ItemDef {
        spell: Spell::InvisibleBox,
        item: "/obj/item/storage/box/mime",
        delete_old: false,
        requires_hands: false,
        lifespan: 50.0,
    },
    ItemDef {
        spell: Spell::SpellPacket,
        item: SPELL_PACKET,
        delete_old: true,
        requires_hands: true,
        lifespan: 0.0,
    },
    ItemDef {
        spell: Spell::Snowball,
        item: "/obj/item/toy/snowball",
        delete_old: true,
        requires_hands: true,
        lifespan: 0.0,
    },
];

/// Every path a conjuring spell spawns (for `ss13_mapload` runtime types).
pub fn runtime_paths() -> Vec<&'static str> {
    CONJURES
        .iter()
        .flat_map(|d| d.types.iter().copied())
        .filter(|p| !p.starts_with("/turf"))
        .chain(CONJURE_ITEMS.iter().map(|d| d.item))
        .collect()
}

fn conjure_def(spell: Spell) -> Option<&'static ConjureDef> {
    CONJURES.iter().find(|d| d.spell == spell)
}

fn item_def(spell: Spell) -> Option<&'static ItemDef> {
    CONJURE_ITEMS.iter().find(|d| d.spell == spell)
}

pub(super) fn handles(spell: Spell) -> bool {
    conjure_def(spell).is_some() || item_def(spell).is_some()
}

/// `cast()` of a conjure or conjure-item spell.
pub(super) fn cast(world: &mut World, mob: Entity, spell: Spell, level: u8, pos: TilePos) -> bool {
    if let Some(def) = conjure_def(spell) {
        conjure(world, mob, def, level, pos);
        return true;
    }
    item_def(spell).is_some_and(|def| conjure_item(world, mob, def, pos))
}

/// The tiles of `range(radius, pos)`.
pub fn summon_tiles(world: &World, pos: TilePos, radius: u32, skip_dense: bool) -> Vec<TilePos> {
    let index = world.resource::<SpatialIndex>();
    let r = radius as i32;
    let mut tiles = Vec::new();
    for x in (pos.x - r)..=(pos.x + r) {
        for y in (pos.y - r)..=(pos.y + r) {
            let tile = TilePos::new(x, y, pos.z);
            if index.in_bounds(tile) && (!skip_dense || index.dense_on_tile(tile).is_empty()) {
                tiles.push(tile);
            }
        }
    }
    tiles
}

fn conjure(world: &mut World, mob: Entity, def: &ConjureDef, level: u8, pos: TilePos) {
    let mut tiles = summon_tiles(world, pos, def.radius, def.respects_density);
    // `simian/level_spell`: `summon_amount++`.
    let amount = if def.spell == Spell::Simian {
        def.amount + u32::from(level.saturating_sub(1))
    } else {
        def.amount
    };
    let t = now(world);
    for _ in 0..amount {
        let (Some(tile), Some(path)) = (pick(world, &tiles), pick(world, def.types)) else {
            break;
        };
        tiles.retain(|&x| x != tile);
        if path.starts_with("/turf") {
            crate::construction::change_turf(world, tile, path, Vec::new(), true);
            crate::construction::tile_changed(world, tile);
            return;
        }
        let spawned = if path.starts_with("/mob") {
            crate::mobs::spawn::spawn_mob(world, path, tile)
        } else {
            crate::construction::spawn_obj(world, path, tile)
        };
        let Some(e) = spawned else {
            warn!("no prototype for {path}: nothing conjured at {tile:?}");
            continue;
        };
        if def.lifespan > 0.0 {
            world.entity_mut(e).insert(DeleteAt(t + def.lifespan));
        }
        post_summon(world, mob, def.spell, e);
    }
    if def.spell == Spell::Simian {
        ally_with_monkeys(world, mob, t);
    }
}

/// A conjured trap's single charge, set once `crate::traps` has adopted it.
#[derive(Component, Clone, Copy, Debug)]
struct SummonedTrap;

/// The caster's temporary `FACTION_MONKEY`.
#[derive(Component, Clone, Copy, Debug)]
struct MonkeyAlly {
    until: f64,
}

/// `post_summon` of the spell.
fn post_summon(world: &mut World, _mob: Entity, spell: Spell, e: Entity) {
    match spell {
        Spell::TheTraps => {
            world.entity_mut(e).insert(SummonedTrap);
        }
        Spell::Simian => add_faction(world, e, FACTION_MONKEY),
        Spell::EdSwarm => {
            world.entity_mut(e).insert(Name::new("Wizard's Justicebot"));
        }
        _ => {}
    }
}

fn add_faction(world: &mut World, e: Entity, faction: &str) {
    let mut factions = world
        .get::<crate::mobs::Faction>(e)
        .cloned()
        .unwrap_or(crate::mobs::Faction(Vec::new()));
    if !factions.0.iter().any(|f| f == faction) {
        factions.0.push(faction.to_owned());
        world.entity_mut(e).insert(factions);
    }
}

/// `simian/cast`: a minute of `FACTION_MONKEY`, unless the caster already is one.
fn ally_with_monkeys(world: &mut World, mob: Entity, t: f64) {
    let already = world
        .get::<crate::mobs::Faction>(mob)
        .is_some_and(|f| f.0.iter().any(|x| x == FACTION_MONKEY));
    if already {
        return;
    }
    add_faction(world, mob, FACTION_MONKEY);
    world.entity_mut(mob).insert(MonkeyAlly {
        until: t + SIMIAN_ALLY_SECS,
    });
}

/// `simian/level_spell` at max level: robeless, and the gorilla transformation.
pub(super) fn simian_mastery(world: &mut World, mob: Entity) {
    if let Some(mut spells) = world.get_mut::<Spells>(mob)
        && let Some(known) = spells.get_mut(Spell::Simian)
    {
        known.robeless = true;
    }
    super::learn(world, mob, Spell::GorillaForm);
    chat_to(
        world,
        mob,
        Span::Notice,
        "Your simian power has reached maximum capacity! You can now cast this spell naked, and \
         have additionally been granted a gorilla transformation spell!",
    );
}

/// `conjure_item/cast`.
fn conjure_item(world: &mut World, mob: Entity, def: &ItemDef, pos: TilePos) -> bool {
    if def.requires_hands && world.get::<Inventory>(mob).is_none() {
        chat_to(world, mob, Span::Warning, "no free hands!");
        return false;
    }
    if def.delete_old {
        delete_old_items(world, mob, def);
    }
    let Some(created) = crate::construction::spawn_obj(world, def.item, pos) else {
        warn!("no prototype for {}: nothing conjured", def.item);
        return false;
    };
    world.flush();
    if def.lifespan > 0.0 {
        let t = now(world);
        world.entity_mut(created).insert(DeleteAt(t + def.lifespan));
    }
    // `put_in_hands(del_on_fail = delete_on_failure)`.
    if !put_in_hands(world, mob, created) {
        world.despawn(created);
        return true;
    }
    if def.delete_old
        && let Some(mut spells) = world.get_mut::<Spells>(mob)
        && let Some(known) = spells.get_mut(def.spell)
    {
        known.conjured.push(created);
    }
    true
}

/// `QDEL_LAZYLIST(item_refs)`: only what still is the conjured item (a reused entity is not).
fn delete_old_items(world: &mut World, mob: Entity, def: &ItemDef) {
    let old = world
        .get_mut::<Spells>(mob)
        .and_then(|mut s| {
            s.get_mut(def.spell)
                .map(|k| std::mem::take(&mut k.conjured))
        })
        .unwrap_or_default();
    for item in old {
        if crate::construction::path_of(world, item).is_some_and(|p| p == def.item) {
            world.despawn(item);
        }
    }
}

/// `/obj/item/spellpacket/lightningbolt/throw_impact`: true when `item` is a spent packet.
pub fn packet_thrown(world: &mut World, item: Entity, hit: Option<Entity>) -> bool {
    if crate::construction::path_of(world, item).as_deref() != Some(SPELL_PACKET) {
        return false;
    }
    if let Some(hit) = hit
        && world.get::<crate::health::Health>(hit).is_some()
        && !can_block_magic(world, hit, MAGIC_RESISTANCE)
    {
        crate::wires::electrocute_act(world, hit, PACKET_SHOCK, item, 1.0);
    }
    world.despawn(item);
    true
}

/// The conjured traps' charge and the end of the simian's alliance.
pub(super) fn tick(world: &mut World) {
    let traps: Vec<Entity> = world
        .query_filtered::<Entity, (With<SummonedTrap>, With<Trap>)>()
        .iter(world)
        .collect();
    for e in traps {
        world.entity_mut(e).remove::<SummonedTrap>();
        if let Some(mut trap) = world.get_mut::<Trap>(e) {
            trap.charges = 1;
        }
    }
    let t = now(world);
    let allies: Vec<(Entity, MonkeyAlly)> = world
        .query::<(Entity, &MonkeyAlly)>()
        .iter(world)
        .map(|(e, a)| (e, *a))
        .collect();
    for (mob, ally) in allies {
        if t < ally.until {
            continue;
        }
        world.entity_mut(mob).remove::<MonkeyAlly>();
        if let Some(mut factions) = world.get_mut::<crate::mobs::Faction>(mob) {
            factions.0.retain(|f| f != FACTION_MONKEY);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_conjure_is_castable_and_unique() {
        for def in CONJURES.iter() {
            assert!(handles(def.spell), "{:?}", def.spell);
            assert!(!def.types.is_empty());
        }
        let mut spells: Vec<Spell> = CONJURES.iter().map(|d| d.spell).collect();
        spells.sort();
        spells.dedup();
        assert_eq!(spells.len(), CONJURES.len());
        assert!(handles(Spell::Snowball));
        assert!(!handles(Spell::Fireball));
    }

    #[test]
    fn runtime_paths_cover_summons_but_no_turfs() {
        let paths = runtime_paths();
        assert!(paths.contains(&"/mob/living/basic/bee/toxin"));
        assert!(paths.contains(&SPELL_PACKET));
        assert!(paths.iter().all(|p| !p.starts_with("/turf")));
    }
}

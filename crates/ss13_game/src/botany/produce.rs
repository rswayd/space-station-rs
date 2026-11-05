//! Product traits (tg `/datum/plant_gene/trait/*/on_new_plant` and the signals they register,
//! `plant_genes.dm`): what a grown item does because of its seed's genes.
//!
//! - [`on_new_plant`] (on every seed put on a grown item): slippery skin makes it a
//!   [`ProduceSlip`] (a banana keeps it for its peel), bioluminescence a [`ProduceGlow`] that
//!   lights the item or whoever carries it, prickly adhesion sets `throwforce`, oculary mimicry
//!   adds the googly eyes.
//! - Liquid contents squash it ([`squash`]) when it lands or hits something uncaught, when
//!   slipped on, or squeezed in hand for 1.5 s (BandaStation): a smudge, its reagents exposed
//!   to the tile, then the `COMSIG_PLANT_ON_SQUASH` effects (bluespace teleport, electric zap).
//! - A slip ([`on_slip`]): hypodermic prickles inject, hallucinatory feedback laughs, liquid
//!   contents squash, else bluespace activity teleports and electrical activity zaps.
//! - At harvest ([`on_harvest`], `COMSIG_PLANT_ON_HARVEST`): auto-distilling ferments the
//!   nutriments, exo/endothermic activity heats or cools the reagents with them.
//! - Cable on a plant with capacitive cell production makes a potato battery.
// shortcut: gaseous decomposition makes no reagent smoke (no chem smoke here): the reagents
// are exposed like any squash; electrical activity recharges no cells when eaten; prickly
// adhesion does not embed (no embedding); auto-juicing does nothing (`juice_typepath()` is a
// proc); fire resistance changes nothing (no item burning); teleports have no sparks or
// teleport channels.

use bevy::prelude::*;
use ss13_mapload::plants::GROWN_INEDIBLE_ROOT;
use ss13_mapload::{DmClothing, DmFood, DmItem, IconIndex};
use ss13_reagents::{Holder, is_under};
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, Dir, LightSource, Optics, Overlay, TilePos};
use ss13_world::{Moved, SpatialIndex};

use super::genes::{Gene, PLANT_GENE_REMOVABLE};
use super::seeds::{Dice, Seed, dm_round};
use super::{Botany, BotanyJob, Plants};
use crate::chem::{Chem, MobReagents, Reagents, map_pos, name_of};
use crate::combat::CombatRng;
use crate::construction::Job;
use crate::health::Health;
use crate::inventory::{CarriedBy, Inventory};
use crate::storage::StoredIn;
use crate::tools::{Actor, DoAfter, do_after, tell};

pub const SQUASH: &str = "/datum/plant_gene/trait/squash";
pub const SLIP: &str = "/datum/plant_gene/trait/slip";
pub const CELL_CHARGE: &str = "/datum/plant_gene/trait/cell_charge";
pub const GLOW: &str = "/datum/plant_gene/trait/glow";
pub const GLOW_SHADOW: &str = "/datum/plant_gene/trait/glow/shadow";
pub const TELEPORT: &str = "/datum/plant_gene/trait/teleport";
pub const BATTERY: &str = "/datum/plant_gene/trait/battery";
pub const STINGING: &str = "/datum/plant_gene/trait/stinging";
pub const SMOKE: &str = "/datum/plant_gene/trait/smoke";
pub const INVASIVE: &str = "/datum/plant_gene/trait/invasive";
pub const BREWING: &str = "/datum/plant_gene/trait/brewing";
pub const PLANT_LAUGHTER: &str = "/datum/plant_gene/trait/plant_laughter";
pub const EYES: &str = "/datum/plant_gene/trait/eyes";
pub const STICKY: &str = "/datum/plant_gene/trait/sticky";
pub const CHEM_HEATING: &str = "/datum/plant_gene/trait/chem_heating";
pub const CHEM_COOLING: &str = "/datum/plant_gene/trait/chem_cooling";
/// `TRAIT_SHOW_EXAMINE`.
pub const TRAIT_SHOW_EXAMINE: u32 = 1 << 2;
/// `PLANT_GENE_GRAFTABLE`.
pub const PLANT_GENE_GRAFTABLE: u32 = 1 << 2;

const BANANA_PEEL: &str = "/obj/item/grown/bananapeel";
const PLANT_SMUDGE: &str = "/obj/effect/decal/cleanable/food/plant_smudge";
const MOLTEN_OBJECT: &str = "/obj/effect/decal/cleanable/molten_object";
const POTATO_CELL: &str = "/obj/item/stock_parts/power_store/cell/potato";
const CABLE_COIL: &str = "/obj/item/stack/cable_coil";
const LUBE: &str = "/datum/reagent/lube";
const NUTRIMENT: &str = "/datum/reagent/consumable/nutriment";
const VITAMIN: &str = "/datum/reagent/consumable/nutriment/vitamin";
const FRUIT_WINE: &str = "/datum/reagent/consumable/ethanol/fruit_wine";
const HARVEST_DMI: &str = "icons/obj/service/hydroponics/harvest.dmi";
/// The misc smudge colour of an inedible plant.
const MISC_SMUDGE_COLOR: [u8; 4] = [0x82, 0xb9, 0x00, 0xff];
/// BandaStation `SQUASH_WITH_HANDS_DELAY`, deciseconds.
const SQUASH_WITH_HANDS_DELAY: f32 = 15.0;
/// `/datum/plant_gene/trait/battery` `cables_needed_per_battery`.
const CABLES_PER_BATTERY: u32 = 5;
/// `STANDARD_CELL_CHARGE` (J), `STANDARD_CELL_RATE` (W).
const STANDARD_CELL_CHARGE: f64 = 10_000.0;
const STANDARD_CELL_RATE: f64 = 10_000.0;
const SITCOM_LAUGHS: [&str; 3] = [
    "sound/items/sitcom_laugh/sitcomLaugh1.ogg",
    "sound/items/sitcom_laugh/sitcomLaugh2.ogg",
    "sound/items/sitcom_laugh/sitcomLaugh3.ogg",
];
const SPRITZ_SOUND: &str = "sound/effects/wounds/sizzle2.ogg";
const SPACE_WIND_SOUND: &str = "sound/effects/space_wind.ogg";

/// The slippery component of slippery skin: the knockdown, seconds (`lube_flags = NONE`:
/// walking does not help).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ProduceSlip {
    pub knockdown: f32,
}

/// The overlay light of bioluminescence (negative power: shadow emission).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ProduceGlow(pub LightSource);

/// On a glowing plant whose light [`sync_glow`] put on `.0` (its holder or itself).
#[derive(Component, Clone, Copy, Debug)]
struct GlowLitOn(Entity, LightSource);

pub(super) fn build(app: &mut App) {
    app.add_observer(on_new_plant)
        .add_observer(unlight)
        .add_systems(
            Update,
            (
                sync_glow,
                slip_on_produce
                    .after(ss13_world::WorldSystems::Movement)
                    .before(crate::status_effects::StatusSystems),
            ),
        );
}

pub fn is_grown(path: &str) -> bool {
    is_under(path, ss13_mapload::plants::GROWN_ROOT) || is_under(path, GROWN_INEDIBLE_ROOT)
}

/// The `rate` of the seed's first trait gene of `root` (or a subtype) and its type.
fn gene_rate<'a>(seed: &'a Seed, root: &str) -> Option<(&'a str, f32)> {
    seed.genes.iter().find_map(|g| match g {
        Gene::Trait { path, rate, .. } if is_under(path, root) => Some((path.as_str(), *rate)),
        _ => None,
    })
}

/// `/datum/plant_gene/trait/slip/on_new_plant`: `potency * rate` deciseconds, a third of it
/// unless it is a banana peel or holds lube, at most 14 s. Seconds.
pub fn slip_knockdown(potency: f32, rate: f32, full: bool) -> f32 {
    let mut stun = potency * rate;
    if !full {
        stun /= 3.0;
    }
    stun.min(140.0) / 10.0
}

/// `"#RRGGBB"` as a colour; `None` when it is not one.
pub fn parse_color(text: &str) -> Option<[u8; 3]> {
    let hex = text.strip_prefix('#')?;
    if hex.len() < 6 {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// `glow_range`/`glow_power` of a glow gene: `1.4 + potency * rate` tiles and
/// `max(potency * (rate + 0.01), 0.1)`; shadow emission `-max(potency * rate * 0.2, 0.2)`.
pub fn glow_light(potency: f32, rate: f32, shadow: bool, color: [u8; 3]) -> LightSource {
    let power = if shadow {
        -(potency * rate * 0.2).max(0.2)
    } else {
        (potency * (rate + 0.01)).max(0.1)
    };
    LightSource {
        range: 1.4 + potency * rate,
        power,
        color,
        height: 1.0,
        angle: 360,
        dir: Dir::South,
        offset: [0, 0],
    }
}

/// `max(round(potency / 10), 1)`: how far bluespace activity throws.
pub fn teleport_radius(potency: f32) -> i32 {
    (dm_round(potency / 10.0) as i32).max(1)
}

/// `/datum/plant_gene/get_name`: removable genes by name, the others "Essential" (graftable)
/// or "Immutable".
pub fn gene_name(plants: &Plants, gene: &Gene) -> String {
    let name = match gene {
        Gene::Trait { path, .. } => plants
            .trait_def(path)
            .map_or_else(|| path.clone(), |t| t.name.clone()),
        Gene::Reagent { path, .. } => path.rsplit('/').next().unwrap_or(path).to_owned(),
    };
    let mutability = gene.mutability();
    if mutability & PLANT_GENE_REMOVABLE != 0 {
        name
    } else if mutability & PLANT_GENE_GRAFTABLE != 0 {
        format!("Essential {name}")
    } else {
        format!("Immutable {name}")
    }
}

/// Every trait's `on_new_plant` that leaves a lasting effect, run again whenever a seed is
/// put on a grown item (spawned with its type's, then the harvested copy).
#[allow(clippy::type_complexity)]
fn on_new_plant(
    insert: On<Insert<Seed>>,
    mut items: Query<(
        &mut DmItem,
        &Seed,
        Option<&DmFood>,
        Option<&Reagents>,
        Option<&mut Appearance>,
    )>,
    botany: Option<Res<Botany>>,
    chem: Option<Res<Chem>>,
    icons: Option<Res<IconIndex>>,
    mut commands: Commands,
) {
    let e = insert.entity;
    let Ok((mut item, seed, food, reagents, appearance)) = items.get_mut(e) else {
        return;
    };
    if !is_grown(&item.path) {
        return;
    }
    // Slippery skin: on the peel when the trash is a grown item.
    let grown_trash = food.is_some_and(|f| is_under(&f.trash_type, GROWN_INEDIBLE_ROOT));
    match gene_rate(seed, SLIP).filter(|_| !grown_trash) {
        Some((_, rate)) => {
            let lube = chem
                .as_ref()
                .and_then(|c| c.0.id(LUBE))
                .is_some_and(|id| reagents.is_some_and(|r| r.0.amount(id) > 0.0));
            let full = is_under(&item.path, BANANA_PEEL) || lube;
            commands.entity(e).insert(ProduceSlip {
                knockdown: slip_knockdown(seed.potency, rate, full),
            });
        }
        None => {
            commands.entity(e).try_remove::<ProduceSlip>();
        }
    }
    match gene_rate(seed, GLOW) {
        Some((path, rate)) => {
            let color = botany
                .as_ref()
                .and_then(|b| b.0.trait_def(path).and_then(|t| parse_color(&t.glow_color)))
                .unwrap_or([0xc3, 0xe3, 0x81]);
            let light = glow_light(seed.potency, rate, is_under(path, GLOW_SHADOW), color);
            commands.entity(e).insert(ProduceGlow(light));
        }
        None => {
            commands.entity(e).try_remove::<(ProduceGlow, GlowLitOn)>();
        }
    }
    if seed.has_gene(STICKY) {
        let throwforce = seed.potency / 20.0;
        if item.throwforce != throwforce {
            item.throwforce = throwforce;
        }
    }
    if seed.has_gene(EYES)
        && let (Some(mut appearance), Some(icons)) = (appearance, icons)
        && let Some(id) = icons.get(HARVEST_DMI, "eyes")
        && !appearance.overlays.iter().any(|o| o.icon_state == id)
    {
        appearance.overlays.push(Overlay::new(id));
    }
}

/// The light follows the plant: on its holder while carried, on itself on the floor, nowhere
/// in a container (as `crate::modpc::pda`'s light).
#[allow(clippy::type_complexity)]
fn sync_glow(
    changed: Query<
        Entity,
        (
            With<ProduceGlow>,
            Or<(
                Changed<ProduceGlow>,
                Changed<CarriedBy>,
                Added<TilePos>,
                Added<StoredIn>,
            )>,
        ),
    >,
    mut dropped: RemovedComponents<CarriedBy>,
    all: Query<(
        Entity,
        &ProduceGlow,
        Option<&CarriedBy>,
        Has<TilePos>,
        Option<&GlowLitOn>,
    )>,
    mut optics: Query<&mut Optics>,
    mut commands: Commands,
) {
    let mut todo: Vec<Entity> = changed.iter().chain(dropped.read()).collect();
    todo.sort_unstable();
    todo.dedup();
    for (entity, glow, carried, on_map, lit) in todo.into_iter().filter_map(|e| all.get(e).ok()) {
        let target = match carried {
            Some(c) => Some(c.0),
            None => on_map.then_some(entity),
        };
        if lit.map(|l| (l.0, l.1)) == target.map(|t| (t, glow.0)) {
            continue;
        }
        if lit.is_some() {
            commands.entity(entity).remove::<GlowLitOn>();
        }
        let Some(target) = target else {
            continue;
        };
        match optics.get_mut(target) {
            Ok(mut o) if o.light.is_none() => o.light = Some(glow.0),
            Ok(_) => continue,
            Err(_) => {
                commands.entity(target).insert(Optics {
                    light: Some(glow.0),
                    ..default()
                });
            }
        }
        commands.entity(entity).insert(GlowLitOn(target, glow.0));
    }
}

/// The light goes out where [`sync_glow`] put it (moved, plant gone).
fn unlight(
    remove: On<Remove<GlowLitOn>>,
    lit: Query<&GlowLitOn>,
    mut optics: Query<&mut Optics>,
    mut commands: Commands,
) {
    let Ok(&GlowLitOn(target, light)) = lit.get(remove.entity) else {
        return;
    };
    if let Ok(mut o) = optics.get_mut(target)
        && o.light == Some(light)
    {
        if !o.opaque && o.tile.is_none() {
            commands.entity(target).try_remove::<Optics>();
        } else {
            o.light = None;
        }
    }
}

/// The slippery component of slippery produce (`Slip` on `COMSIG_ATOM_ENTERED` of its turf):
/// a living mob stepping onto it slips (`handle_slip`), then the plant's slip effects.
fn slip_on_produce(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<Moved>>) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<bevy::ecs::message::Messages<Moved>>())
        .copied()
        .collect();
    // Cost: one tile lookup per mob step (a few per player per second).
    for m in moved {
        if world.get::<Health>(m.entity).is_none() {
            continue;
        }
        let plant = world
            .resource::<SpatialIndex>()
            .on_tile(m.to)
            .iter()
            .copied()
            .find_map(|e| world.get::<ProduceSlip>(e).map(|s| (e, s.knockdown)));
        let Some((plant, knockdown)) = plant else {
            continue;
        };
        if crate::janitor::try_slip(world, m.entity, (knockdown, false)) {
            on_slip(world, plant, m.entity);
        }
    }
}

/// `COMSIG_PLANT_ON_SLIP` of a plant `mob` slipped on.
pub fn on_slip(world: &mut World, plant: Entity, mob: Entity) {
    let Some(seed) = world.get::<Seed>(plant).cloned() else {
        return;
    };
    if seed.has_gene(STINGING) {
        prickles_inject(world, plant, mob, &seed);
    }
    if seed.has_gene(PLANT_LAUGHTER)
        && let Some(pos) = map_pos(world, plant)
    {
        let name = name_of(world, plant);
        crate::security::show(
            world,
            pos,
            crate::health::DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (Span::Notice, format!("{name} lets out burst of laughter.")),
            &[],
        );
        let i = world.resource_mut::<CombatRng>().pick(SITCOM_LAUGHS.len());
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, SITCOM_LAUGHS[i], 100.0, false)
                .extrarange(crate::sound::SHORT_RANGE_SOUND_EXTRARANGE),
        );
    }
    if seed.has_gene(SQUASH) {
        squash(world, plant, Some(mob));
        return;
    }
    if seed.has_gene(CELL_CHARGE) {
        zap_target(world, plant, mob, &seed);
    }
    if seed.has_gene(TELEPORT) {
        slip_teleport(world, plant, mob, &seed);
    }
}

/// `COMSIG_MOVABLE_IMPACT` of a thrown plant (`hit`: what it hit, `None`: it landed): prickles
/// inject a living target, liquid contents squash it unless caught. True when it is gone.
pub fn thrown(world: &mut World, item: Entity, hit: Option<Entity>) -> bool {
    let is_plant = world.get::<DmItem>(item).is_some_and(|i| is_grown(&i.path));
    let Some(seed) = world.get::<Seed>(item).cloned().filter(|_| is_plant) else {
        return false;
    };
    if let Some(hit) = hit
        && seed.has_gene(STINGING)
        && world.get::<Health>(hit).is_some()
    {
        prickles_inject(world, item, hit, &seed);
    }
    let caught = world.get::<CarriedBy>(item).is_some();
    if seed.has_gene(SQUASH) && !caught {
        squash(world, item, hit);
        return true;
    }
    false
}

/// `/datum/plant_gene/trait/squash/squash_plant(our_plant, target)`: a smudge on the target's
/// tile, a smack, `COMSIG_PLANT_ON_SQUASH`, the reagents exposed to the tile and all on it,
/// the plant gone.
pub fn squash(world: &mut World, plant: Entity, target: Option<Entity>) {
    let Some(pos) = target
        .and_then(|t| map_pos(world, t))
        .or_else(|| map_pos(world, plant))
    else {
        return;
    };
    let name = name_of(world, plant);
    let path = world
        .get::<DmItem>(plant)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    let def = world
        .get_resource::<Botany>()
        .and_then(|b| b.0.grown_def(&path).cloned());
    let edible = world.get::<DmFood>(plant).is_some();
    let (splat, color) = match &def {
        Some(d) if edible && !d.splat_type.is_empty() => (
            d.splat_type.clone(),
            parse_color(&d.filling_color).map(|[r, g, b]| [r, g, b, 0xff]),
        ),
        Some(_) if edible => (String::new(), None),
        _ => (PLANT_SMUDGE.to_owned(), Some(MISC_SMUDGE_COLOR)),
    };
    if !splat.is_empty()
        && let Some(smudge) = crate::construction::spawn_obj(world, &splat, pos)
        && is_under(&splat, PLANT_SMUDGE)
    {
        world
            .entity_mut(smudge)
            .insert(Name::new(format!("{name} smudge")));
        if let Some(color) = color
            && let Some(mut appearance) = world.get_mut::<Appearance>(smudge)
        {
            appearance.color = Some(color);
        }
    }
    crate::security::show(
        world,
        pos,
        crate::health::DEFAULT_MESSAGE_RANGE,
        ChatChannel::Warning,
        (Span::Warning, format!("{name} is squashed.")),
        &[],
    );
    // `COMSIG_PLANT_ON_SQUASH`.
    if let Some(seed) = world.get::<Seed>(plant).cloned()
        && let Some(target) = target
    {
        if seed.has_gene(TELEPORT) && world.get::<Health>(target).is_some() {
            squash_teleport(world, target, &seed);
        }
        if seed.has_gene(CELL_CHARGE) {
            zap_target(world, plant, target, &seed);
        }
    }
    // `reagents.expose(turf)` and every thing on it.
    if let Some(holder) = world.get::<Reagents>(plant).map(|r| r.0.clone())
        && holder.total_volume > 0.0
    {
        let on_tile: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .filter(|&e| e != plant)
            .collect();
        for e in on_tile {
            if world.get_entity(e).is_ok() {
                crate::janitor::expose(world, e, &holder, 1.0);
            }
        }
    }
    if world.get_entity(plant).is_ok() {
        world.despawn(plant);
    }
}

/// BandaStation `squash_plant_in_hands`: squeezing a plant with liquid contents in hand
/// squashes it after 1.5 s. False for other items.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let is_plant = world.get::<DmItem>(item).is_some_and(|i| is_grown(&i.path));
    if !is_plant || !world.get::<Seed>(item).is_some_and(|s| s.has_gene(SQUASH)) {
        return false;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(item),
    };
    do_after(
        world,
        actor,
        mob,
        SQUASH_WITH_HANDS_DELAY,
        0,
        Job::Botany(BotanyJob::Squash),
    );
    true
}

/// The end of squeezing: the plant is still in the squeezer's hands.
pub fn complete_squash(world: &mut World, mob: Entity, action: &DoAfter) {
    let Some(plant) = action.item else {
        return;
    };
    if world.get_entity(plant).is_err() || world.get::<CarriedBy>(plant).map(|c| c.0) != Some(mob) {
        return;
    }
    squash(world, plant, Some(mob));
}

/// `cell_charge/zap_target`: a carbon is shocked for `round(potency * rate)` with a chance of
/// as many percent, gloves do not help.
fn zap_target(world: &mut World, plant: Entity, target: Entity, seed: &Seed) {
    if world.get::<Inventory>(target).is_none() || world.get::<Health>(target).is_none() {
        return;
    }
    let Some((_, rate)) = gene_rate(seed, CELL_CHARGE) else {
        return;
    };
    let power = seed.potency * rate;
    if world.resource_mut::<CombatRng>().prob(power) {
        crate::wires::electrocute_act(world, target, dm_round(power), plant, 1.0);
    }
}

/// `living.can_inject()` without a user: the chest is not under thick clothing.
fn can_inject(world: &World, mob: Entity) -> bool {
    let bit = crate::chem::body_part_bit(crate::zone::BodyZone::Chest);
    !world.get::<Inventory>(mob).is_some_and(|inv| {
        inv.equipped().any(|item| {
            world.get::<DmClothing>(item).is_some_and(|c| {
                c.body_parts_covered & bit != 0
                    && c.clothing_flags & crate::chem::THICKMATERIAL != 0
            })
        })
    })
}

/// `stinging/prickles_inject`: `max(1, potency * 0.2)` units of the plant's reagents into the
/// target's blood.
fn prickles_inject(world: &mut World, plant: Entity, target: Entity, seed: &Seed) {
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let Some(mut source) = world.get::<Reagents>(plant).map(|r| r.0.clone()) else {
        return;
    };
    if source.total_volume <= 0.0
        || world.get::<MobReagents>(target).is_none()
        || !can_inject(world, target)
    {
        return;
    }
    let amount = (seed.potency * 0.2).max(1.0);
    if let Some(mut m) = world.get_mut::<MobReagents>(target) {
        source.trans_to(&chem, &mut m.blood, amount, 1.0, None, false, false);
    }
    if let Some(mut r) = world.get_mut::<Reagents>(plant) {
        r.0 = source;
    }
    let name = name_of(world, plant);
    if let Some(client) = world
        .get::<crate::server::PlayerMob>(target)
        .map(|p| p.client)
    {
        tell(
            world,
            client,
            Span::Danger,
            format!("You are pricked by {name}!"),
        );
    }
}

/// `do_teleport(atom, center, radius)`: a random free tile within `radius` (`None`: none).
fn teleport_dest(world: &mut World, center: TilePos, radius: i32) -> Option<TilePos> {
    let index = world.resource::<SpatialIndex>();
    let mut tiles = Vec::new();
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let pos = TilePos::new(center.x + dx, center.y + dy, center.z);
            if index.in_bounds(pos) && index.dense_on_tile(pos).is_empty() {
                tiles.push(pos);
            }
        }
    }
    if tiles.is_empty() {
        return None;
    }
    let i = world.resource_mut::<CombatRng>().pick(tiles.len());
    Some(tiles[i])
}

fn teleport(world: &mut World, e: Entity, center: TilePos, radius: i32) {
    if world.get::<TilePos>(e).is_none() {
        return; // carried things go with their holder
    }
    if let Some(dest) = teleport_dest(world, center, radius) {
        world.entity_mut(e).insert(dest);
    }
}

/// `teleport/squash_teleport`: goo on the target's tile, the target thrown about.
fn squash_teleport(world: &mut World, target: Entity, seed: &Seed) {
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    crate::construction::spawn_obj(world, MOLTEN_OBJECT, pos);
    teleport(world, target, pos, teleport_radius(seed.potency));
}

/// `teleport/slip_teleport`: the slipper is thrown about; the plant follows half the time,
/// else it melts into goo.
fn slip_teleport(world: &mut World, plant: Entity, target: Entity, seed: &Seed) {
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    let radius = teleport_radius(seed.potency);
    if let Some(client) = world
        .get::<crate::server::PlayerMob>(target)
        .map(|p| p.client)
    {
        tell(world, client, Span::Warning, "You slip through spacetime!");
    }
    teleport(world, target, pos, radius);
    if world.resource_mut::<CombatRng>().prob(50.0) {
        teleport(world, plant, pos, radius);
    } else {
        crate::construction::spawn_obj(world, MOLTEN_OBJECT, pos);
        world.despawn(plant);
    }
}

/// `COMSIG_PLANT_ON_HARVEST` at the end of `prepare_result`, on a fresh product.
pub fn on_harvest(world: &mut World, product: Entity, seed: &Seed) {
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let Some(mut holder) = world.get::<Reagents>(product).map(|r| r.0.clone()) else {
        return;
    };
    let before = holder.clone();
    let path = world
        .get::<DmItem>(product)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    let distill = world
        .get_resource::<Botany>()
        .and_then(|b| b.0.grown_def(&path).map(|d| d.distill_reagent.clone()))
        .unwrap_or_default();
    if seed.has_gene(BREWING) && world.get::<DmFood>(product).is_some() {
        let purity =
            world.resource_scope(|_, mut rng: Mut<CombatRng>| seed.reagent_purity(&mut *rng));
        ferment(&chem, &mut holder, &distill, purity);
    }
    let heating = seed.has_gene(CHEM_HEATING);
    if heating || seed.has_gene(CHEM_COOLING) {
        let name = name_of(world, product);
        let text = if heating {
            format!("{name} releases freezing air, consuming its nutriments to heat its contents.")
        } else {
            format!(
                "{name} releases a blast of hot air, consuming its nutriments to cool its contents."
            )
        };
        let nutriment = chem.id(NUTRIMENT).map_or(0.0, |id| {
            let amount = holder.amount(id);
            holder.remove_reagent(id, amount);
            amount
        });
        holder.chem_temp = if heating {
            (holder.chem_temp + nutriment * 25.0).min(1000.0)
        } else {
            (holder.chem_temp - nutriment * 5.0).max(3.0)
        };
        holder.handle_reactions(&chem);
        if let Some(pos) = map_pos(world, product) {
            crate::security::show(
                world,
                pos,
                crate::health::DEFAULT_MESSAGE_RANGE,
                ChatChannel::Info,
                (Span::Notice, text),
                &[],
            );
            let (sound, vol) = if heating {
                (SPRITZ_SOUND, 5.0)
            } else {
                (SPACE_WIND_SOUND, 50.0)
            };
            crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, vol, true));
        }
    }
    if holder != before
        && let Some(mut r) = world.get_mut::<Reagents>(product)
    {
        r.0 = holder;
    }
}

/// `/obj/item/food/grown/ferment()`: nutriment and vitamins become as much of the plant's
/// `distill_reagent`, else fruit wine, at the seed's purity.
// shortcut: no wine data (names, colour, tastes, boozepwr, quality).
pub fn ferment(chem: &ss13_reagents::Chemistry, holder: &mut Holder, distill: &str, purity: f32) {
    let into = if distill.is_empty() {
        chem.id(FRUIT_WINE)
    } else {
        chem.id(distill)
    };
    let sources: Vec<(u16, f32)> = holder
        .reagents
        .iter()
        .filter(|r| chem.def(r.id).path == NUTRIMENT || chem.def(r.id).path == VITAMIN)
        .map(|r| (r.id, r.volume))
        .collect();
    for (id, volume) in sources {
        if let Some(into) = into {
            holder.add_reagent_list(chem, &[(into, volume)], Some(purity), false);
        }
        holder.remove_reagent(id, volume);
    }
}

/// `battery/make_battery`: five lengths of cable turn a plant with capacitive cell production
/// into a potato battery of `potency * 0.02` standard cells (times `rate * 100` with
/// electrical activity), full. True when the click was this.
pub fn battery_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    plant: Entity,
) -> bool {
    let cable = world
        .get::<DmItem>(held)
        .is_some_and(|i| is_under(&i.path, CABLE_COIL));
    let is_plant = world
        .get::<DmItem>(plant)
        .is_some_and(|i| is_grown(&i.path));
    let Some(seed) = world
        .get::<Seed>(plant)
        .cloned()
        .filter(|s| cable && is_plant && s.has_gene(BATTERY))
    else {
        return false;
    };
    let name = name_of(world, plant);
    if !crate::tools::use_stack(world, held, CABLES_PER_BATTERY) {
        tell(
            world,
            client,
            Span::Warning,
            format!("You need five lengths of cable to make a {name} battery!"),
        );
        return true;
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("You add some cable to {name} and slide it inside the battery encasing."),
    );
    let Some(pos) = map_pos(world, mob) else {
        return true;
    };
    let icon = world.get::<Appearance>(plant).map(|a| a.icon_state);
    world.despawn(plant);
    let Some(cell) = crate::construction::spawn_obj(world, POTATO_CELL, pos) else {
        return true;
    };
    world.flush();
    let mut maxcharge = f64::from(seed.potency) * 0.02 * STANDARD_CELL_CHARGE;
    if let Some((_, rate)) = gene_rate(&seed, CELL_CHARGE) {
        maxcharge *= f64::from(rate) * 100.0;
    }
    let chargerate = world
        .get::<crate::mecha::core::ItemCell>(cell)
        .map_or(STANDARD_CELL_RATE * 0.05, |c| c.0.chargerate);
    world
        .entity_mut(cell)
        .insert(crate::mecha::core::ItemCell(crate::power::Cell {
            charge: maxcharge,
            maxcharge,
            chargerate,
        }));
    world
        .entity_mut(cell)
        .insert(Name::new(format!("{name} battery")));
    if let (Some(icon), Some(mut appearance)) = (icon, world.get_mut::<Appearance>(cell)) {
        appearance.icon_state = icon;
    }
    true
}

/// Examine lines of a grown item's traits with `TRAIT_SHOW_EXAMINE`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    let is_plant = world
        .get::<DmItem>(target)
        .is_some_and(|i| is_grown(&i.path));
    let (Some(seed), Some(botany)) = (
        world.get::<Seed>(target).filter(|_| is_plant),
        world.get_resource::<Botany>(),
    ) else {
        return;
    };
    for gene in &seed.genes {
        if let Gene::Trait { path, flags, .. } = gene
            && flags & TRAIT_SHOW_EXAMINE != 0
            && let Some(def) = botany.0.trait_def(path)
            && !def.description.is_empty()
        {
            lines.push((Span::Info, def.description.clone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slip_lasts_potency_times_rate_a_third_off_peels() {
        // A banana at potency 10, rate 1.6: 16 ds on its peel.
        assert_eq!(slip_knockdown(10.0, 1.6, true), 1.6);
        assert!((slip_knockdown(30.0, 1.6, false) - 1.6).abs() < 1e-5);
        // Capped at 140 ds.
        assert_eq!(slip_knockdown(100.0, 1.6, true), 14.0);
    }

    #[test]
    fn glow_and_shadow_follow_potency() {
        let light = glow_light(50.0, 0.03, false, [1, 2, 3]);
        assert!((light.range - 2.9).abs() < 1e-5);
        assert!((light.power - 2.0).abs() < 1e-5);
        assert_eq!(light.color, [1, 2, 3]);
        assert_eq!(glow_light(0.0, 0.03, false, [0; 3]).power, 0.1);
        let shadow = glow_light(50.0, 0.04, true, [0; 3]);
        assert!((shadow.power + 0.4).abs() < 1e-5);
        assert_eq!(glow_light(10.0, 0.04, true, [0; 3]).power, -0.2);
        assert_eq!(parse_color("#C3E381"), Some([0xc3, 0xe3, 0x81]));
        assert_eq!(parse_color("red"), None);
    }

    #[test]
    fn teleport_reaches_a_tenth_of_potency() {
        assert_eq!(teleport_radius(5.0), 1);
        assert_eq!(teleport_radius(39.0), 3);
        assert_eq!(teleport_radius(100.0), 10);
    }

    #[test]
    fn gene_names_mark_unremovable_genes() {
        let plants = super::super::test_plants();
        let gene = |mutability| Gene::Trait {
            path: "/datum/plant_gene/trait/carnivory".into(),
            rate: 0.05,
            mutability,
            ids: 0,
            flags: 0,
        };
        assert_eq!(gene_name(&plants, &gene(7)), "carnivory");
        assert_eq!(gene_name(&plants, &gene(4)), "Essential carnivory");
        assert_eq!(gene_name(&plants, &gene(0)), "Immutable carnivory");
    }
}

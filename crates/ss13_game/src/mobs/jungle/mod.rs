//! Jungle fauna (server), after tg `code/modules/mob/living/basic/jungle/`: the leaper, the mega
//! arachnid, seedlings and the venus human trap with the kudzu flower bud that grows it. Their
//! trees are `crate::mobs::ai::trees::jungle`, the leaves [`ai`].
//!
//! - Leaper: a poison bubble (7 s, `projectile/leaper`: paralyzes 5 s, 5 u of leaper venom into
//!   a carbon or 30 brute to anything else; at the end of its 7 tiles it floats on as a bubble),
//!   the belly flop (14 s: a flight of up to 7 tiles onto the target's tile, landing for 35 brute
//!   on everyone next to it, thrown 3 tiles away), the blood rain (10 s: after 1.5 s up to 8
//!   bubbles drop 0.1 s apart on open tiles it sees within 5) and two suicide frogs (30 s, at
//!   most two alive, of its faction: they die after 15 s and blow up on whatever living they
//!   bite, `explode_on_attack`). A floating bubble (10 s) pops on any living non-leaper that walks
//!   into it the same way, leaving leaper sludge.
//! - Mega arachnid: nearly invisible (alpha 40) until it has a target (`appearance_on_aggro`);
//!   flees anything within 5 tiles leaving slippery acid behind for 3 s (`secrete_acid`, 15 s:
//!   a 6 s slip, the acid lasts 5 s); farther away it snares humans (`flesh_restraints`, 6 s,
//!   8 tiles: a fleshy bear trap springs on them) and bites only what is legcuffed, not human or
//!   under half health. It smashes the lights it sees (`surveillance_equipment`).
//! - Seedlings: the solar volley (10 s: 3 s charging, then 10 bolts 0.2 s apart, 10 burn each,
//!   10° spread, harmless to the jungle faction) and the solar beam (30 s: 3 s charging, then 30
//!   burn and set alight on everything living on the target tile; a tray there gets +10 plant
//!   health). Charging and firing show on the sprite and block both abilities. The meanie fights
//!   with them; the plain seedling beams the hurt plants it sees.
//! - Venus human trap: heals 5 brute every 2 s on a kudzu vine and takes 12.5 off it ("не
//!   покидайте лозы!"), heals 5 per bite (`lifesteal`), tangles (8 s, 4 tiles, a clear line): a
//!   leash of 4 tiles for 2 s (4 s on objects) and a 2 s knockdown. A player's ranged click
//!   tangles.
//! - Flower bud (kudzu's Flowering mutation): grows a stage every 30 s, bears fruit after 120 s
//!   and is then a ghost role spawner of a venus human trap; its four thorny vines to the tiles
//!   two away diagonally cut whoever walks on them (5 brute) unless vine-immune.
// shortcut: no pet commands (leaper, meanie), riding the leaper, wall smashing, tree climbing,
// colour overlays, cell line swabs, camera smashing; no seedling watering can errands; no vine
// beams drawn (leash, bud vines), no bud countdown, mutations or venus antag datum; a bubble
// destroyed by damage does not pop; the flop flight is a straight walk of one tile per 0.05 s.

pub mod ai;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::ProjectileData;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mobs::MobDef;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Dir, TilePos};
use ss13_world::{Density, Moved, SpatialIndex};

use crate::combat::CombatRng;
use crate::effects::temp_visual::{BELOW_MOB_LAYER, Visual};
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType, Health, Stat};
use crate::mobs::ai::{AiController, Key};
use crate::sound::{PlaySound, playsound};
use crate::status_effects::Incap;

pub const LEAPER: &str = "/mob/living/basic/leaper";
pub const MEGA_ARACHNID: &str = "/mob/living/basic/mega_arachnid";
pub const SEEDLING: &str = "/mob/living/basic/seedling";
pub const VENUS_TRAP: &str = "/mob/living/basic/venus_human_trap";
pub const SUICIDE_FROG: &str = "/mob/living/basic/frog/suicide";
const FROG: &str = "/mob/living/basic/frog";
const LEAPER_BUBBLE: &str = "/obj/structure/leaper_bubble";
const LEAPER_SLUDGE: &str = "/obj/effect/decal/cleanable/leaper_sludge";
const ARACHNID_TRAP: &str = "/obj/item/restraints/legcuffs/beartrap/mega_arachnid";
const SLIPPERY_ACID: &str = "/obj/effect/slippery_acid";
const LEAPER_VENOM: &str = "/datum/reagent/toxin/leaper_venom";
const BUBBLE_PROJECTILE: &str = "/obj/projectile/leaper";
const SNARE_PROJECTILE: &str = "/obj/projectile/mega_arachnid";
const SEED_PROJECTILE: &str = "/obj/projectile/seedling";
const SNAP: &str = "sound/effects/snap.ogg";
const PROJECTILES_DMI: &str = "icons/obj/weapons/guns/projectiles.dmi";
const EFFECTS_DMI: &str = "icons/effects/effects.dmi";
const SEEDLING_DMI: &str = "icons/mob/simple/jungle/seedling.dmi";
const SPACEVINES_DMI: &str = "icons/mob/spacevines.dmi";
/// `FACTION_JUNGLE`, `FACTION_PLANTS`, `FACTION_VINES`.
pub const FACTION_JUNGLE: &str = "jungle";
const FACTION_PLANTS: &str = "plants";
const FACTION_VINES: &str = "vines";

/// The mega arachnid controller's `BB_BASIC_MOB_FLEE_DISTANCE`.
pub const ARACHNID_FLEE_DISTANCE: u32 = 5;

/// `leaper_bubble` `cooldown_time`; the projectile's `range`, `paralyze`; what it does to a
/// non-carbon; the venus dose.
const BUBBLE_COOLDOWN: f64 = 7.0;
const BUBBLE_RANGE: f32 = 7.0;
const BUBBLE_PARALYZE: f32 = 5.0;
const BUBBLE_DAMAGE: f32 = 30.0;
const VENOM_DOSE: f32 = 5.0;
/// `/obj/structure/leaper_bubble` `QDEL_IN`.
const FLOATING_BUBBLE_LIFE: f64 = 10.0;
/// `belly_flop`: `cooldown_time`, the throw's range, a tile of it (`speed` 1), the landing.
const FLOP_COOLDOWN: f64 = 14.0;
const FLOP_RANGE: u32 = 7;
const FLOP_STEP: f64 = 0.05;
const FLOP_DAMAGE: f32 = 35.0;
const FLOP_THROW: u32 = 3;
/// `blood_rain`: `cooldown_time`, `volley_count`, `fire_interval`, the timer before the
/// droplets, the falling droplet's `duration`; `oview(5)`.
const VOLLEY_COOLDOWN: f64 = 10.0;
const VOLLEY_COUNT: usize = 8;
const VOLLEY_INTERVAL: f64 = 0.1;
const VOLLEY_DELAY: f64 = 1.5;
const DROP_FALL: f64 = 0.7;
const VOLLEY_RANGE: u32 = 5;
/// `create_suicide_toads`: `cooldown_time`, `summon_radius`, `summon_amount`, `max_summons`;
/// the frog's `existence_period`.
const SUMMON_COOLDOWN: f64 = 30.0;
const SUMMON_RADIUS: i32 = 2;
const SUMMON_AMOUNT: usize = 2;
const MAX_SUMMONS: usize = 2;
const FROG_LIFE: f64 = 15.0;
/// `flesh_restraints` `cooldown_time`, `cast_range`; `secrete_acid` `cooldown_time`, the
/// trail's span; `slippery_acid` `duration_time` and its slip.
const RESTRAIN_COOLDOWN: f64 = 6.0;
pub const RESTRAIN_RANGE: u32 = 8;
const SLIP_COOLDOWN: f64 = 15.0;
const ACID_TRAIL: f64 = 3.0;
const ACID_LIFE: f64 = 5.0;
const ACID_SLIP: f32 = 6.0;
/// `appearance_on_aggro` of the arachnid: `alpha` and `alpha_on_aggro`.
const ARACHNID_ALPHA: u8 = 40;
/// `rapid_fire/seedling`: `cooldown_time`, `charge_up_timer`, `shot_count`, `shot_delay`,
/// `default_projectile_spread`; the 4 s it shows firing.
const SEEDS_COOLDOWN: f64 = 10.0;
const CHARGE_UP: f64 = 3.0;
const SHOT_COUNT: u32 = 10;
const SHOT_DELAY: f64 = 0.2;
const SEED_SPREAD: u32 = 10;
const FIRING_SHOWN: f64 = 4.0;
/// `solarbeam`: `cooldown_time`, `beam_charge_up`, the burn, fire stacks and the tray heal.
const BEAM_COOLDOWN: f64 = 30.0;
const BEAM_DAMAGE: f32 = 30.0;
const BEAM_STACKS: f32 = 0.2;
const BEAM_TRAY_HEAL: f32 = 10.0;
/// `venus_human_trap`: `weed_heal`, `no_weed_damage` per `Life` (`SSmobs`, 2 s); the lifesteal.
const LIFE_PERIOD: f64 = 2.0;
const WEED_HEAL: f32 = 5.0;
const NO_WEED_DAMAGE: f32 = 12.5;
const LIFESTEAL: f32 = 5.0;
/// `vine_tangle`: `cooldown_time`, `vine_grab_distance`, `vine_duration`; the knockdown.
const TANGLE_COOLDOWN: f64 = 8.0;
pub const TANGLE_RANGE: u32 = 4;
const VINE_DURATION: f64 = 2.0;
const TANGLE_KNOCKDOWN: f32 = 2.0;
/// `flower_bud` `growth_time` (a stage every quarter); the thorny vines' cut.
const GROWTH_TIME: f64 = 120.0;
const FINAL_BUD_STAGE: u8 = 3;
const VINE_CUT: f32 = 5.0;

/// A leaper's four abilities and the flop or rain under way.
#[derive(Component, Clone, Debug, Default)]
pub struct LeaperPowers {
    pub bubble_ready: f64,
    pub flop_ready: f64,
    pub volley_ready: f64,
    pub summon_ready: f64,
    /// The suicide frogs it summoned (`limit_summons`).
    pub toads: Vec<Entity>,
    flop: Option<Flop>,
    /// The blood rain: tiles to drop on and when (`fire_droplets`, `fall_effect`), then the
    /// drops landing (`create_blood_structure`).
    drops: Vec<(f64, TilePos)>,
    landings: Vec<(f64, TilePos)>,
}

#[derive(Clone, Copy, Debug)]
struct Flop {
    next: f64,
    to: TilePos,
    left: u32,
}

/// A floating leaper bubble (`/obj/structure/leaper_bubble`).
#[derive(Component, Clone, Copy, Debug)]
pub struct FloatingBubble {
    pub pops_at: f64,
}

/// A suicide frog's `existence_period`.
#[derive(Component, Clone, Copy, Debug)]
pub struct SuicideFrog {
    pub dies_at: f64,
}

/// A mega arachnid's snare and acid.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ArachnidPowers {
    pub restrain_ready: f64,
    pub slip_ready: f64,
    /// `secrete_acid` is on until then (`COMSIG_MOVABLE_MOVED` registered).
    pub acid_until: f64,
}

/// `/obj/effect/slippery_acid`.
#[derive(Component, Clone, Copy, Debug)]
pub struct SlipperyAcid {
    pub until: f64,
}

/// A seedling's `combatant_state`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SeedState {
    #[default]
    Neutral,
    Warmup,
    Active,
}

/// A seedling's volley and beam.
#[derive(Component, Clone, Debug, Default)]
pub struct SeedlingPowers {
    pub seeds_ready: f64,
    pub beam_ready: f64,
    pub state: SeedState,
    /// The volley: shots left at the target, the next one's time.
    volley: Option<(Entity, u32, f64)>,
    /// The volley's charge ends at.
    volley_charge: Option<(Entity, f64)>,
    /// The beam lands on the tile at.
    beam: Option<(TilePos, f64)>,
    /// Back to `Neutral` at (after firing).
    calm_at: Option<f64>,
}

/// A venus human trap's tangle, its vines and `Life`.
#[derive(Component, Clone, Debug, Default)]
pub struct VenusTrap {
    pub tangle_ready: f64,
    /// Leashed atoms and when their vine goes.
    pub vines: Vec<(Entity, f64)>,
    next_life: f64,
    alert_shown: bool,
}

/// A kudzu flower bud growing a venus human trap.
#[derive(Component, Clone, Copy, Debug)]
pub struct FlowerBud {
    pub planted: f64,
    pub stage: u8,
    pub ready: bool,
}

pub(super) fn build(app: &mut App) {
    app.add_systems(
        Update,
        (
            leapers,
            floating_bubbles,
            suicide_frogs,
            arachnids,
            seedlings,
            venus_traps,
            buds,
            on_moved,
            player_tangles.after(crate::click::ResolveClicks),
        )
            .before(crate::health::HealthSystems),
    );
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn alive(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) == Some(&Stat::Conscious)
}

fn living(world: &World, mob: Entity) -> bool {
    world.get_entity(mob).is_ok() && world.get::<Stat>(mob).is_some_and(|s| *s != Stat::Dead)
}

fn roll<T>(world: &mut World, f: impl FnOnce(&mut CombatRng) -> T) -> T {
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let out = f(&mut rng);
    world.resource_mut::<CombatRng>().0 = rng.0;
    out
}

fn name(world: &World, e: Entity) -> String {
    crate::security::name_of(world, e)
}

fn show(world: &mut World, at: TilePos, span: Span, text: String) {
    crate::security::show(
        world,
        at,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (span, text),
        &[],
    );
}

fn has_faction(world: &World, mob: Entity, factions: &[&str]) -> bool {
    world
        .get::<crate::mobs::Faction>(mob)
        .is_some_and(|f| f.0.iter().any(|x| factions.contains(&x.as_str())))
}

/// tg `isvineimmune`: of the vines or plants faction.
pub fn is_vine_immune(world: &World, mob: Entity) -> bool {
    world.get::<Health>(mob).is_some() && has_faction(world, mob, &[FACTION_VINES, FACTION_PLANTS])
}

/// tg `/mob/living/basic/*/Initialize` of the jungle mobs: their abilities.
pub(super) fn attach(world: &mut World, mob: Entity, def: &MobDef) {
    let path = def.path.as_str();
    let now = now(world);
    let mut e = world.entity_mut(mob);
    if is_under(path, LEAPER) {
        e.insert(LeaperPowers::default());
    } else if is_under(path, MEGA_ARACHNID) {
        e.insert(ArachnidPowers::default());
    } else if is_under(path, SEEDLING) {
        e.insert(SeedlingPowers::default());
    } else if is_under(path, VENUS_TRAP) {
        e.insert(VenusTrap {
            next_life: now + LIFE_PERIOD,
            ..VenusTrap::default()
        });
    } else if is_under(path, SUICIDE_FROG) {
        e.insert(SuicideFrog {
            dies_at: now + FROG_LIFE,
        });
    }
}

/// An ability holds the mob still (the leaper's flight, `TRAIT_IMMOBILIZED`): its AI waits.
pub fn busy(world: &World, mob: Entity) -> bool {
    world
        .get::<LeaperPowers>(mob)
        .is_some_and(|l| l.flop.is_some())
}

fn spawn_visual(world: &mut World, pos: TilePos, v: Visual) {
    crate::effects::temp_visual::spawn(world, pos, v);
}

fn set_state(world: &mut World, mob: Entity, icon: &str, state: &str) {
    let Some(id) = world
        .get_resource::<ss13_mapload::IconIndex>()
        .and_then(|i| i.get(icon, state))
    else {
        return;
    };
    if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(mob)
        && look.icon_state != id
    {
        look.icon_state = id;
    }
}

/// Adds `amount` of `path` to a mob's blood reagents (`reagents.add_reagent`).
fn add_reagent(world: &mut World, mob: Entity, path: &str, amount: f32) {
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return;
    };
    let (Some(id), Some(mut reagents)) = (
        chem.id(path),
        world.get::<crate::chem::MobReagents>(mob).cloned(),
    ) else {
        return;
    };
    reagents
        .blood
        .add_reagent(&chem, id, amount, 310.15, None, None, false, false);
    world.entity_mut(mob).insert(reagents);
}

/// What a leaper bubble does to a living mob (`projectile/leaper/on_hit`, the floating bubble's
/// `on_entered`): venom into a carbon, 30 brute to anything else.
fn bubble_mob(world: &mut World, mob: Entity) {
    if crate::throw::is_carbon(world, mob) {
        add_reagent(world, mob, LEAPER_VENOM, VENOM_DOSE);
    } else {
        super::defense::hurt(world, mob, DamageType::Brute, BUBBLE_DAMAGE, None);
    }
}

/// `/obj/effect/temp_visual/leaper_projectile_impact`: the pop (3 s) and the sludge it leaves.
fn bubble_pop(world: &mut World, pos: TilePos) {
    spawn_visual(
        world,
        pos,
        Visual {
            dir: Some(Dir::South),
            ..Visual::new(PROJECTILES_DMI, "leaper_bubble_pop", 3.0)
        },
    );
    crate::construction::spawn_obj(world, LEAPER_SLUDGE, pos);
}

// ---------------------------------------------------------------------------------------------
// Projectiles

/// `/obj/projectile/leaper`.
pub fn bubble_projectile() -> ProjectileData {
    ProjectileData {
        path: BUBBLE_PROJECTILE.to_owned(),
        damage: 0.0,
        damage_type: "brute".to_owned(),
        armor_flag: "bullet".to_owned(),
        paralyze: BUBBLE_PARALYZE * 10.0,
        speed: 1.25,
        range: BUBBLE_RANGE,
        pass_flags: crate::projectile::PASSTABLE,
        accurate_range: 100.0,
        accuracy_falloff: 7.0,
        ..default()
    }
}

/// `/obj/projectile/mega_arachnid`.
pub fn snare_projectile() -> ProjectileData {
    ProjectileData {
        path: SNARE_PROJECTILE.to_owned(),
        damage: 0.0,
        damage_type: "brute".to_owned(),
        armor_flag: "bullet".to_owned(),
        speed: 1.25,
        range: 50.0,
        pass_flags: crate::projectile::PASSTABLE,
        accurate_range: 100.0,
        accuracy_falloff: 7.0,
        ..default()
    }
}

/// `/obj/projectile/seedling`.
pub fn seed_projectile() -> ProjectileData {
    ProjectileData {
        path: SEED_PROJECTILE.to_owned(),
        damage: 10.0,
        damage_type: "burn".to_owned(),
        armor_flag: "energy".to_owned(),
        speed: 0.66,
        range: 50.0,
        pass_flags: crate::projectile::PASSTABLE,
        accurate_range: 100.0,
        accuracy_falloff: 7.0,
        ..default()
    }
}

fn shoot(world: &mut World, firer: Entity, data: &ProjectileData, target: Entity, deviation: f32) {
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(firer), world.get::<TilePos>(target))
    else {
        return;
    };
    if let Some(dir) = crate::pull::dir_between(from, to)
        && let Some(mut facing) = world.get_mut::<ss13_shared::Facing>(firer)
    {
        facing.set_if_neq(ss13_shared::Facing(dir));
    }
    crate::projectile::fire(
        world,
        crate::projectile::Shot {
            data,
            firer,
            from,
            target: Some(target),
            target_pos: to,
            zone: crate::zone::BodyZone::Chest,
            hit_prone: true,
            pixel: [0, 0],
            deviation,
            fired_from: None,
        },
    );
}

/// `seedling/on_hit`: a mob of the jungle faction blocks the bolt (`BULLET_ACT_BLOCK`).
pub fn blocks_projectile(world: &World, p: &crate::projectile::Projectile, target: Entity) -> bool {
    p.data.path == SEED_PROJECTILE && has_faction(world, target, &[FACTION_JUNGLE])
}

/// The `on_hit` of the leaper bubble and the flesh snare.
pub fn on_projectile_hit(world: &mut World, p: &crate::projectile::Projectile, target: Entity) {
    let path = p.data.path.as_str();
    if path == BUBBLE_PROJECTILE {
        if let Some(&at) = world.get::<TilePos>(target) {
            bubble_pop(world, at);
        }
        if world.get::<Health>(target).is_some() {
            bubble_mob(world, target);
        }
    } else if path == SNARE_PROJECTILE && crate::throw::is_carbon(world, target) {
        let Some(&at) = world.get::<TilePos>(target) else {
            return;
        };
        if let Some(trap) = crate::construction::spawn_obj(world, ARACHNID_TRAP, at) {
            if let Some(mut cuffs) = world.get_mut::<ss13_mapload::DmLegcuffs>(trap) {
                cuffs.armed = true;
            }
            crate::legcuffs::spring_trap(world, trap, target, false, false);
        }
    }
}

/// `projectile/leaper/on_range`: the bubble floats on where it stopped.
pub fn on_projectile_range(world: &mut World, path: &str, at: TilePos) {
    if path == BUBBLE_PROJECTILE {
        float_bubble(world, at);
    }
}

fn float_bubble(world: &mut World, at: TilePos) {
    let pops_at = now(world) + FLOATING_BUBBLE_LIFE;
    if let Some(bubble) = crate::construction::spawn_obj(world, LEAPER_BUBBLE, at) {
        world.entity_mut(bubble).insert(FloatingBubble { pops_at });
    }
}

// ---------------------------------------------------------------------------------------------
// Leaper

pub fn bubble_ready(world: &World, mob: Entity, now: f64) -> bool {
    world
        .get::<LeaperPowers>(mob)
        .is_some_and(|l| now >= l.bubble_ready)
}

/// `projectile_attack/leaper_bubble/Activate`: one bubble at the target.
pub fn fire_bubble(world: &mut World, mob: Entity, target: Entity) -> bool {
    let now = now(world);
    if !bubble_ready(world, mob, now) || !alive(world, mob) {
        return false;
    }
    if let Some(mut l) = world.get_mut::<LeaperPowers>(mob) {
        l.bubble_ready = now + BUBBLE_COOLDOWN;
    }
    if let Some(&at) = world.get::<TilePos>(mob) {
        playsound(world, PlaySound::new(at, SNAP, 100.0, true));
    }
    shoot(world, mob, &bubble_projectile(), target, 0.0);
    true
}

/// `belly_flop/Activate`: not onto a wall or space; the crush mark and the flight.
pub fn start_flop(world: &mut World, mob: Entity, target: Entity) -> bool {
    let now = now(world);
    let ready = world
        .get::<LeaperPowers>(mob)
        .is_some_and(|l| now >= l.flop_ready && l.flop.is_none());
    let (Some(&to), true) = (world.get::<TilePos>(target), ready && alive(world, mob)) else {
        return false;
    };
    let closed = crate::construction::turf_at(world, to).is_none_or(|t| {
        world.get::<Density>(t).is_some()
            || world
                .get::<ss13_mapload::DmTurf>(t)
                .is_some_and(|d| d.closed)
    });
    if closed || crate::construction::is_space(world, to) {
        crate::balloon::balloon_alert(world, mob, mob, "неподходящее место!");
        return false;
    }
    spawn_visual(
        world,
        to,
        Visual {
            layer: BELOW_MOB_LAYER,
            offset: [-32, -32],
            dir: Some(Dir::South),
            ..Visual::new("icons/effects/96x96.dmi", "lily_pad", 3.0)
        },
    );
    if let Some(mut l) = world.get_mut::<LeaperPowers>(mob) {
        l.flop_ready = now + FLOP_COOLDOWN;
        l.flop = Some(Flop {
            next: now,
            to,
            left: FLOP_RANGE,
        });
    }
    true
}

/// `blood_rain/Activate`: open tiles it sees within 5 without a bubble; the droplet rises.
pub fn start_volley(world: &mut World, mob: Entity) -> bool {
    let now = now(world);
    let ready = world
        .get::<LeaperPowers>(mob)
        .is_some_and(|l| now >= l.volley_ready);
    let Some(&from) = world
        .get::<TilePos>(mob)
        .filter(|_| ready && alive(world, mob))
    else {
        return false;
    };
    let r = VOLLEY_RANGE as i32;
    let mut tiles: Vec<TilePos> = (-r..=r)
        .flat_map(|dx| (-r..=r).map(move |dy| TilePos::new(from.x + dx, from.y + dy, from.z)))
        .filter(|&p| p != from)
        .filter(|&p| crate::mobs::ai::behaviors::can_see(world, from, p, VOLLEY_RANGE))
        .filter(|&p| !blocked(world, p) && !crate::construction::is_space(world, p))
        .filter(|&p| !bubble_on(world, p))
        .collect();
    if tiles.is_empty() {
        return false;
    }
    playsound(
        world,
        PlaySound::new(from, "sound/effects/magic/fireball.ogg", 70.0, true),
    );
    spawn_visual(
        world,
        from,
        Visual {
            dir: Some(Dir::South),
            ..Visual::new(PROJECTILES_DMI, "leaper", 1.0)
        },
    );
    // `fire_droplets` after 1.5 s: `pick_n_take` of up to 8, 0.1 s apart.
    let count = VOLLEY_COUNT.min(tiles.len());
    let mut drops = Vec::with_capacity(count);
    for i in 1..=count {
        let at = roll(world, |r| r.range(0, tiles.len() as u32 - 1)) as usize;
        let tile = tiles.swap_remove(at);
        drops.push((now + VOLLEY_DELAY + i as f64 * VOLLEY_INTERVAL, tile));
    }
    if let Some(mut l) = world.get_mut::<LeaperPowers>(mob) {
        l.volley_ready = now + VOLLEY_COOLDOWN;
        l.drops.extend(drops);
    }
    true
}

/// `is_blocked_turf()`: a dense turf or a dense thing on it.
fn blocked(world: &World, pos: TilePos) -> bool {
    !world
        .resource::<SpatialIndex>()
        .dense_on_tile(pos)
        .is_empty()
}

fn bubble_on(world: &World, pos: TilePos) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| world.get::<FloatingBubble>(e).is_some())
}

/// `create_suicide_toads`: up to two frogs alive, on open tiles within 2, of its faction.
pub fn summon_toads(world: &mut World, mob: Entity) -> bool {
    let now = now(world);
    let Some(powers) = world.get::<LeaperPowers>(mob).cloned() else {
        return false;
    };
    let toads: Vec<Entity> = powers
        .toads
        .iter()
        .copied()
        .filter(|&t| living(world, t))
        .collect();
    let room = MAX_SUMMONS.saturating_sub(toads.len()).min(SUMMON_AMOUNT);
    let Some(&from) = world.get::<TilePos>(mob) else {
        return false;
    };
    if now < powers.summon_ready || room == 0 || !alive(world, mob) {
        return false;
    }
    let spots: Vec<TilePos> = (-SUMMON_RADIUS..=SUMMON_RADIUS)
        .flat_map(|dx| {
            (-SUMMON_RADIUS..=SUMMON_RADIUS)
                .map(move |dy| TilePos::new(from.x + dx, from.y + dy, from.z))
        })
        .filter(|&p| !blocked(world, p) && crate::construction::turf_at(world, p).is_some())
        .collect();
    if spots.is_empty() {
        return false;
    }
    let faction = world.get::<crate::mobs::Faction>(mob).cloned();
    let mut made = toads;
    for _ in 0..room {
        let at = spots[roll(world, |r| r.range(0, spots.len() as u32 - 1)) as usize];
        let Some(frog) = crate::mobs::spawn::spawn_mob(world, SUICIDE_FROG, at) else {
            continue;
        };
        // `SET_FACTION_AND_ALLIES_FROM`.
        if let Some(f) = faction.clone() {
            world.entity_mut(frog).insert(f);
        }
        made.push(frog);
    }
    if let Some(mut l) = world.get_mut::<LeaperPowers>(mob) {
        l.summon_ready = now + SUMMON_COOLDOWN;
        l.toads = made;
    }
    true
}

fn leapers(world: &mut World) {
    let now = now(world);
    let busy: Vec<(Entity, LeaperPowers)> = world
        .query::<(Entity, &LeaperPowers)>()
        .iter(world)
        .filter(|(_, l)| l.flop.is_some() || !l.drops.is_empty() || !l.landings.is_empty())
        .map(|(e, l)| (e, l.clone()))
        .collect();
    for (mob, powers) in busy {
        // The droplets fall even if the leaper died meanwhile (tg timers on the ability).
        let (due, later): (Vec<(f64, TilePos)>, Vec<(f64, TilePos)>) =
            powers.drops.into_iter().partition(|(t, _)| now >= *t);
        let (landed, falling): (Vec<(f64, TilePos)>, Vec<(f64, TilePos)>) =
            powers.landings.into_iter().partition(|(t, _)| now >= *t);
        let mut falling = falling;
        for (_, tile) in due {
            drop_falls(world, tile);
            falling.push((now + DROP_FALL, tile));
        }
        for (_, tile) in landed {
            playsound(world, PlaySound::new(tile, SNAP, 50.0, true));
            float_bubble(world, tile);
        }
        if let Some(mut l) = world.get_mut::<LeaperPowers>(mob) {
            l.drops = later;
            l.landings = falling;
        }
        let Some(flop) = powers.flop.filter(|f| now >= f.next) else {
            continue;
        };
        if flop.left > 0 && alive(world, mob) && fly_step(world, mob, flop.to) {
            if let Some(mut l) = world.get_mut::<LeaperPowers>(mob) {
                l.flop = Some(Flop {
                    next: now + FLOP_STEP,
                    left: flop.left - 1,
                    ..flop
                });
            }
            continue;
        }
        if let Some(mut l) = world.get_mut::<LeaperPowers>(mob) {
            l.flop = None;
        }
        if alive(world, mob) {
            land(world, mob);
        }
    }
}

/// `fall_effect`: the falling drop and its shadow (the bubble lands 0.7 s later).
fn drop_falls(world: &mut World, tile: TilePos) {
    for state in ["blood_effect_falling", "shadow_telegraph"] {
        spawn_visual(
            world,
            tile,
            Visual {
                dir: Some(Dir::South),
                ..Visual::new(EFFECTS_DMI, state, DROP_FALL)
            },
        );
    }
}

/// One tile of a flight; false when it cannot go on.
fn fly_step(world: &mut World, mob: Entity, to: TilePos) -> bool {
    let Some(&from) = world.get::<TilePos>(mob) else {
        return false;
    };
    let Some(dir) = crate::pull::dir_between(from, to) else {
        return false;
    };
    let index = world.resource::<SpatialIndex>();
    if !crate::mobs::ai::movement::can_step(index, mob, from, dir) {
        return false;
    }
    world.entity_mut(mob).insert(from.step(dir));
    true
}

/// `belly_flop/flop_on_turf`: 35 brute to everyone living within a tile, thrown clear.
fn land(world: &mut World, mob: Entity) {
    let Some(&at) = world.get::<TilePos>(mob) else {
        return;
    };
    playsound(
        world,
        PlaySound::new(at, "sound/effects/meteorimpact.ogg", 200.0, true),
    );
    let victims: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(at, 1)
        .filter(|&e| e != mob && world.get::<Health>(e).is_some())
        .filter(|&e| world.get::<TilePos>(e).is_some_and(|p| p.z == at.z))
        .collect();
    let leaper = name(world, mob);
    for victim in victims {
        super::defense::hurt(world, victim, DamageType::Brute, FLOP_DAMAGE, None);
        if world.get_entity(victim).is_err() || world.get::<Stat>(victim) == Some(&Stat::Dead) {
            continue;
        }
        let Some(&pos) = world.get::<TilePos>(victim) else {
            continue;
        };
        // Away from the leaper; a mob under it goes anywhere.
        let dir = match crate::pull::dir_between(at, pos) {
            Some(d) => d,
            None => Dir::DMI_ORDER[roll(world, |r| r.range(0, 7)) as usize],
        };
        crate::martial::fling(world, victim, dir, FLOP_THROW, 1.0);
        let who = ss13_chat::capitalize(&name(world, victim));
        if let Some(&now_at) = world.get::<TilePos>(victim) {
            show(
                world,
                now_at,
                Span::Warning,
                format!("{who} отбрасывает от {leaper}!"),
            );
        }
    }
}

fn floating_bubbles(world: &mut World) {
    let now = now(world);
    let gone: Vec<(Entity, Option<TilePos>)> = world
        .query::<(Entity, &FloatingBubble, Option<&TilePos>)>()
        .iter(world)
        .filter(|(_, b, _)| now >= b.pops_at)
        .map(|(e, _, p)| (e, p.copied()))
        .collect();
    for (bubble, at) in gone {
        pop_floating(world, bubble, at);
    }
}

/// `leaper_bubble/Destroy`: the pop, the snap.
fn pop_floating(world: &mut World, bubble: Entity, at: Option<TilePos>) {
    if let Some(at) = at {
        bubble_pop(world, at);
        playsound(world, PlaySound::new(at, SNAP, 50.0, true));
    }
    if let Ok(e) = world.get_entity_mut(bubble) {
        e.despawn();
    }
}

fn suicide_frogs(world: &mut World) {
    let now = now(world);
    let due: Vec<Entity> = world
        .query::<(Entity, &SuicideFrog, &Stat)>()
        .iter(world)
        .filter(|(_, f, s)| now >= f.dies_at && **s != Stat::Dead)
        .map(|(e, _, _)| e)
        .collect();
    for frog in due {
        if let Some(mut stat) = world.get_mut::<Stat>(frog) {
            stat.set_if_neq(Stat::Dead);
        }
    }
}

/// `explode_on_attack` of the suicide frog (`COMSIG_HOSTILE_PRE_ATTACKINGTARGET`): a living
/// target that is not a frog or a leaper blows it up (light 1) instead of the bite.
pub(super) fn pre_melee(world: &mut World, attacker: Entity, target: Entity) -> bool {
    if world.get::<SuicideFrog>(attacker).is_none() || world.get::<Health>(target).is_none() {
        return false;
    }
    let spared = world
        .get::<super::BasicMob>(target)
        .is_some_and(|b| is_under(&b.def.path, FROG) || is_under(&b.def.path, LEAPER));
    if spared {
        return false;
    }
    let Some(&at) = world.get::<TilePos>(attacker) else {
        return false;
    };
    let mut ex = crate::explosions::Explosion::new(0, 0, 1);
    ex.cause = "suicide frog".to_owned();
    crate::explosions::explosion(world, at, ex);
    if world.get_entity(attacker).is_ok() {
        crate::health::ghostize_gone(world, attacker);
        world.despawn(attacker);
    }
    true
}

// ---------------------------------------------------------------------------------------------
// Mega arachnid

pub fn restrain_ready(world: &World, mob: Entity, now: f64) -> bool {
    world
        .get::<ArachnidPowers>(mob)
        .is_some_and(|a| now >= a.restrain_ready)
}

pub fn slip_ready(world: &World, mob: Entity, now: f64) -> bool {
    world
        .get::<ArachnidPowers>(mob)
        .is_some_and(|a| now >= a.slip_ready)
}

/// `flesh_restraints`: a snare at the target within 8 tiles.
pub fn throw_snare(world: &mut World, mob: Entity, target: Entity) -> bool {
    let now = now(world);
    if !restrain_ready(world, mob, now) || !alive(world, mob) {
        return false;
    }
    if let Some(mut a) = world.get_mut::<ArachnidPowers>(mob) {
        a.restrain_ready = now + RESTRAIN_COOLDOWN;
    }
    shoot(world, mob, &snare_projectile(), target, 0.0);
    true
}

/// `secrete_acid/Activate`: acid wherever it steps for 3 s.
pub fn secrete_acid(world: &mut World, mob: Entity) -> bool {
    let now = now(world);
    if !slip_ready(world, mob, now) || !alive(world, mob) {
        return false;
    }
    if let Some(mut a) = world.get_mut::<ArachnidPowers>(mob) {
        a.slip_ready = now + SLIP_COOLDOWN;
        a.acid_until = now + ACID_TRAIL;
    }
    true
}

/// `release_acid` on a step: acid on the new tile unless there is some.
fn release_acid(world: &mut World, at: TilePos) {
    let present = world
        .resource::<SpatialIndex>()
        .on_tile(at)
        .iter()
        .any(|&e| world.get::<SlipperyAcid>(e).is_some());
    if present {
        return;
    }
    let until = now(world) + ACID_LIFE;
    if let Some(acid) = crate::construction::spawn_obj(world, SLIPPERY_ACID, at) {
        world.entity_mut(acid).insert(SlipperyAcid { until });
    }
}

/// The acid drying up; `appearance_on_aggro` (alpha 255 with a target or a player).
fn arachnids(world: &mut World) {
    let now = now(world);
    let dried: Vec<Entity> = world
        .query::<(Entity, &SlipperyAcid)>()
        .iter(world)
        .filter(|(_, a)| now >= a.until)
        .map(|(e, _)| e)
        .collect();
    for acid in dried {
        world.despawn(acid);
    }
    let spiders: Vec<(Entity, bool)> = world
        .query_filtered::<(Entity, Option<&AiController>), With<ArachnidPowers>>()
        .iter(world)
        .map(|(e, c)| (e, c.and_then(|c| c.bb.entity(Key::CurrentTarget)).is_some()))
        .collect();
    for (spider, aggro) in spiders {
        let shown = aggro || world.get::<crate::server::PlayerMob>(spider).is_some();
        let alpha = if shown { 255 } else { ARACHNID_ALPHA };
        if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(spider) {
            let color = Some([255, 255, 255, alpha]);
            if look.color != color {
                look.color = color;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Seedlings

fn seed_state(world: &mut World, mob: Entity, state: SeedState) {
    if let Some(mut s) = world.get_mut::<SeedlingPowers>(mob) {
        s.state = state;
    }
    let icon = match state {
        SeedState::Neutral => "seedling",
        SeedState::Warmup => "seedling_charging",
        SeedState::Active => "seedling_fire",
    };
    if alive(world, mob) {
        set_state(world, mob, SEEDLING_DMI, icon);
    }
}

/// `IsAvailable` of either ability: off cooldown and `SEEDLING_STATE_NEUTRAL`.
pub fn seeds_ready(world: &World, mob: Entity, now: f64) -> bool {
    world
        .get::<SeedlingPowers>(mob)
        .is_some_and(|s| s.state == SeedState::Neutral && now >= s.seeds_ready)
}

pub fn beam_ready(world: &World, mob: Entity, now: f64) -> bool {
    world
        .get::<SeedlingPowers>(mob)
        .is_some_and(|s| s.state == SeedState::Neutral && now >= s.beam_ready)
}

/// `rapid_fire/seedling/Activate`: charging, the volley in 3 s.
pub fn start_seeds(world: &mut World, mob: Entity, target: Entity) -> bool {
    let now = now(world);
    if !seeds_ready(world, mob, now) || !alive(world, mob) {
        return false;
    }
    if let Some(mut s) = world.get_mut::<SeedlingPowers>(mob) {
        s.seeds_ready = now + SEEDS_COOLDOWN;
        s.volley_charge = Some((target, now + CHARGE_UP));
    }
    seed_state(world, mob, SeedState::Warmup);
    true
}

/// `solarbeam/Activate`: charging, the beam on the target's tile in 3 s.
pub fn start_beam(world: &mut World, mob: Entity, target: Entity) -> bool {
    let now = now(world);
    let Some(&at) = world.get::<TilePos>(target) else {
        return false;
    };
    if !beam_ready(world, mob, now) || !alive(world, mob) {
        return false;
    }
    if let Some(mut s) = world.get_mut::<SeedlingPowers>(mob) {
        s.beam_ready = now + BEAM_COOLDOWN;
        s.beam = Some((at, now + CHARGE_UP));
    }
    seed_state(world, mob, SeedState::Warmup);
    if let Some(&from) = world.get::<TilePos>(mob) {
        playsound(
            world,
            PlaySound::new(from, "sound/effects/seedling_chargeup.ogg", 100.0, false),
        );
        let beam = Visual {
            dir: Some(Dir::South),
            ..Visual::new("icons/effects/beam.dmi", "solar_beam", CHARGE_UP)
        };
        spawn_visual(world, from, beam);
        spawn_visual(world, at, beam);
    }
    true
}

fn seedlings(world: &mut World) {
    let now = now(world);
    let busy: Vec<(Entity, SeedlingPowers)> = world
        .query::<(Entity, &SeedlingPowers)>()
        .iter(world)
        .filter(|(_, s)| {
            s.volley.is_some()
                || s.volley_charge.is_some()
                || s.beam.is_some()
                || s.calm_at.is_some()
        })
        .map(|(e, s)| (e, s.clone()))
        .collect();
    for (mob, s) in busy {
        if let Some((target, at)) = s.volley_charge.filter(|(_, at)| now >= *at) {
            // `attack_sequence`: firing for 4 s, the bolts.
            if let Some(mut p) = world.get_mut::<SeedlingPowers>(mob) {
                p.volley_charge = None;
                p.volley = Some((target, SHOT_COUNT, at));
                p.calm_at = Some(now + FIRING_SHOWN);
            }
            seed_state(world, mob, SeedState::Active);
        }
        let volley = world.get::<SeedlingPowers>(mob).and_then(|p| p.volley);
        if let Some((target, left, next)) = volley.filter(|v| now >= v.2) {
            // The ability's timers fire on even if the seedling died; at a gone target the
            // rest are lost.
            if world.get::<TilePos>(target).is_some() {
                let spread =
                    roll(world, |r| r.range(0, SEED_SPREAD * 2)) as f32 - SEED_SPREAD as f32;
                shoot(world, mob, &seed_projectile(), target, spread);
            }
            let rest = (left > 1 && world.get::<TilePos>(target).is_some()).then_some((
                target,
                left - 1,
                next + SHOT_DELAY,
            ));
            if let Some(mut p) = world.get_mut::<SeedlingPowers>(mob) {
                p.volley = rest;
            }
        }
        if let Some((tile, _)) = s.beam.filter(|(_, at)| now >= *at) {
            if let Some(mut p) = world.get_mut::<SeedlingPowers>(mob) {
                p.beam = None;
            }
            launch_beam(world, tile);
            seed_state(world, mob, SeedState::Neutral);
        }
        if s.calm_at.is_some_and(|t| now >= t) {
            if let Some(mut p) = world.get_mut::<SeedlingPowers>(mob) {
                p.calm_at = None;
            }
            seed_state(world, mob, SeedState::Neutral);
        }
    }
}

/// `solarbeam/launch_beam`: trays heal, the living burn.
fn launch_beam(world: &mut World, tile: TilePos) {
    let here: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(tile).to_vec();
    for e in here {
        let healed = world
            .get_mut::<crate::botany::tray::Tray>(e)
            .map(|mut tray| tray.adjust_plant_health(BEAM_TRAY_HEAL))
            .is_some();
        if healed {
            spawn_visual(
                world,
                tile,
                Visual {
                    color: Some([0x5B, 0xF5, 0xEB, 0xFF]),
                    dir: Some(Dir::South),
                    ..Visual::new("icons/effects/anomalies.dmi", "shield2", 0.5)
                },
            );
        }
        if world.get::<Health>(e).is_none() {
            continue;
        }
        crate::fire::adjust_fire_stacks(world, e, BEAM_STACKS);
        crate::fire::ignite_mob(world, e);
        super::defense::hurt(world, e, DamageType::Burn, BEAM_DAMAGE, None);
    }
    playsound(
        world,
        PlaySound::new(tile, "sound/effects/magic/lightningbolt.ogg", 50.0, true),
    );
}

/// `targeting_strategy/beamable_hydro` over the trays the seedling sees: a living plant under
/// its seed's endurance.
pub fn beamable_tray(world: &World, mob: Entity, range: u32) -> Option<Entity> {
    let from = *world.get::<TilePos>(mob)?;
    let mut trays: Vec<(u32, Entity)> = world
        .resource::<SpatialIndex>()
        .in_radius(from, range)
        .filter(|&e| {
            world.get::<crate::botany::tray::Tray>(e).is_some_and(|t| {
                t.myseed
                    .as_ref()
                    .is_some_and(|seed| t.plant_health < seed.endurance)
            })
        })
        .filter_map(|e| {
            let to = *world.get::<TilePos>(e)?;
            crate::mobs::ai::behaviors::can_see(world, from, to, range)
                .then(|| (from.chebyshev(to).unwrap_or(0), e))
        })
        .collect();
    trays.sort_by_key(|&(d, e)| (d, e.to_bits()));
    trays.first().map(|&(_, e)| e)
}

// ---------------------------------------------------------------------------------------------
// Venus human trap

pub fn tangle_ready(world: &World, mob: Entity, now: f64) -> bool {
    world
        .get::<VenusTrap>(mob)
        .is_some_and(|v| now >= v.tangle_ready)
}

/// `vine_tangle/Activate`: a movable, unanchored target within 4 tiles over a clear line.
pub fn tangle(world: &mut World, mob: Entity, target: Entity) -> bool {
    let now = now(world);
    if !tangle_ready(world, mob, now) || !alive(world, mob) || target == mob {
        return false;
    }
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(mob), world.get::<TilePos>(target)) else {
        return false;
    };
    let is_vine = world
        .get::<crate::events::space_vines::Vine>(target)
        .is_some();
    let anchored = world.get::<Health>(target).is_none()
        && world
            .get::<crate::obj_damage::Anchored>(target)
            .map(|a| a.0)
            .or_else(|| world.get::<ss13_mapload::DmObj>(target).map(|o| o.anchored))
            .unwrap_or(false);
    if is_vine || world.get::<ss13_mapload::Turf>(target).is_some() {
        return false;
    }
    if anchored {
        crate::balloon::balloon_alert(world, mob, mob, "не подтянуть!");
        return false;
    }
    if from.chebyshev(to).is_none_or(|d| d > TANGLE_RANGE) {
        crate::balloon::balloon_alert(world, mob, mob, "слишком далеко!");
        return false;
    }
    // `get_line` without the ends: no dense turf or object (mobs excluded) in the way.
    let index = world.resource::<SpatialIndex>();
    let line = crate::radiation::get_line(from, to);
    let jammed = line.iter().filter(|&&p| p != from && p != to).any(|&p| {
        index
            .dense_on_tile(p)
            .iter()
            .any(|&e| world.get::<Health>(e).is_none())
    });
    if jammed {
        crate::balloon::balloon_alert(world, mob, mob, "путь прегражден!");
        return false;
    }
    let mob_target = world.get::<Health>(target).is_some();
    let lasts = if mob_target {
        VINE_DURATION
    } else {
        VINE_DURATION * 2.0
    };
    if let Some(mut v) = world.get_mut::<VenusTrap>(mob) {
        v.tangle_ready = now + TANGLE_COOLDOWN;
        v.vines.retain(|(e, _)| *e != target);
        v.vines.push((target, now + lasts));
    }
    if mob_target {
        crate::status_effects::knockdown(world, target, TANGLE_KNOCKDOWN, 0.0);
    }
    true
}

/// `venus_human_trap/Life` every 2 s; the leashes (`/datum/component/leash`: what drifts past 4
/// tiles is pulled a step back toward the trap, a leash that cannot pull breaks).
fn venus_traps(world: &mut World) {
    let now = now(world);
    let traps: Vec<(Entity, VenusTrap)> = world
        .query::<(Entity, &VenusTrap)>()
        .iter(world)
        .map(|(e, v)| (e, v.clone()))
        .collect();
    for (trap, v) in traps {
        let alive_trap = alive(world, trap);
        let mut vines = v.vines.clone();
        vines.retain(|&(e, until)| alive_trap && now < until && world.get_entity(e).is_ok());
        for &(leashed, _) in &vines.clone() {
            let (Some(&owner_at), Some(&at)) =
                (world.get::<TilePos>(trap), world.get::<TilePos>(leashed))
            else {
                continue;
            };
            if at.chebyshev(owner_at).is_none_or(|d| d <= TANGLE_RANGE) {
                continue;
            }
            let pulled = crate::pull::dir_between(at, owner_at)
                .is_some_and(|dir| crate::pull::step_once(world, leashed, dir, None).is_ok());
            if !pulled {
                vines.retain(|(e, _)| *e != leashed);
            }
        }
        let mut alert = v.alert_shown;
        let mut next_life = v.next_life;
        if alive_trap && now >= v.next_life {
            next_life = now + LIFE_PERIOD;
            let on_vines = world.get::<TilePos>(trap).is_some_and(|&p| {
                world
                    .resource::<SpatialIndex>()
                    .on_tile(p)
                    .iter()
                    .any(|&e| world.get::<crate::events::space_vines::Vine>(e).is_some())
            });
            if !on_vines && !alert {
                alert = true;
                crate::balloon::balloon_alert(world, trap, trap, "не покидайте лозы!");
            } else if on_vines {
                alert = false;
            }
            if on_vines {
                if let Some(mut h) = world.get_mut::<Health>(trap) {
                    h.adjust(DamageType::Brute, -WEED_HEAL);
                }
            } else {
                super::defense::hurt(world, trap, DamageType::Brute, NO_WEED_DAMAGE, None);
            }
        }
        if let Some(mut t) = world.get_mut::<VenusTrap>(trap) {
            t.vines = vines;
            t.alert_shown = alert;
            t.next_life = next_life;
        }
    }
}

/// `/datum/element/lifesteal` of the venus trap: a bite on a living mob heals it.
pub(super) fn after_melee(world: &mut World, attacker: Entity, target: Entity) {
    if world.get::<VenusTrap>(attacker).is_none() || !living(world, target) {
        return;
    }
    if let Some(mut h) = world.get_mut::<Health>(attacker) {
        let brute = h.brute.min(LIFESTEAL);
        let burn = h.burn.min(LIFESTEAL - brute);
        h.adjust(DamageType::Brute, -brute);
        h.adjust(DamageType::Burn, -burn);
    }
}

/// `venus_human_trap/RangedAttack` of a player trap: the tangle.
fn player_tangles(world: &mut World, mut cursor: Local<MessageCursor<crate::click::RangedClick>>) {
    let clicks: Vec<crate::click::RangedClick> = cursor
        .read(world.resource::<Messages<crate::click::RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        if world.get::<VenusTrap>(click.mob).is_some() {
            tangle(world, click.mob, click.target);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Flower bud

/// `flower_bud/Initialize`: a new bud starts growing (called where kudzu flowers).
pub fn bud_planted(world: &mut World, bud: Entity) {
    let now = now(world);
    world.entity_mut(bud).insert((
        FlowerBud {
            planted: now,
            stage: 0,
            ready: false,
        },
        crate::ghost_verbs::GhostRoleSpawner {
            name: "venus human trap".to_owned(),
            desc: "Вы — венерина мухоловка! Защищайте кудзу любой ценой и пожирайте тех, кто \
                   вам противостоит!"
                .to_owned(),
            uses: 0,
            def: None,
        },
    ));
}

pub fn is_bud(world: &World, spawner: Entity) -> bool {
    world.get::<FlowerBud>(spawner).is_some()
}

/// `progress_growth` every 30 s, `bear_fruit` at 120 s: the spawner opens, ghosts are told.
fn buds(world: &mut World) {
    let now = now(world);
    let buds: Vec<(Entity, FlowerBud)> = world
        .query::<(Entity, &FlowerBud)>()
        .iter(world)
        .filter(|(_, b)| !b.ready)
        .map(|(e, b)| (e, *b))
        .collect();
    for (bud, b) in buds {
        let elapsed = now - b.planted;
        let stage = ((elapsed / (GROWTH_TIME / 4.0)) as u8).min(FINAL_BUD_STAGE);
        if stage != b.stage {
            set_state(world, bud, SPACEVINES_DMI, &format!("bud{stage}"));
        }
        let ready = elapsed >= GROWTH_TIME;
        if let Some(mut fb) = world.get_mut::<FlowerBud>(bud) {
            fb.stage = stage;
            fb.ready = ready;
        }
        if !ready {
            continue;
        }
        if let Some(&at) = world.get::<TilePos>(bud) {
            show(
                world,
                at,
                Span::Danger,
                "Растение принесло плод!".to_owned(),
            );
        }
        if let Some(mut s) = world.get_mut::<crate::ghost_verbs::GhostRoleSpawner>(bud) {
            s.uses = 1;
        }
        for ghost in crate::ghost_poll::ghosts(world) {
            crate::tools::tell_mob(
                world,
                ghost,
                Span::Notice,
                "Venus Human Trap: цветочный бутон принёс плод!",
            );
        }
    }
}

/// `mob_spawn/ghost_role/venus_human_trap` `allow_spawn` + `create` + `special`: the trap for
/// the ghost's player, the bud gone. Rechecked: ready, the ghost still the player's.
pub fn take_bud(world: &mut World, client: Entity, ghost: Entity, bud: Entity) {
    let Some(b) = world.get::<FlowerBud>(bud).copied() else {
        return;
    };
    if !b.ready {
        crate::tools::tell(world, client, Span::Warning, "Бутон ещё не принёс плод!");
        return;
    }
    if world.get::<crate::ghost::Ghost>(ghost).is_none()
        || world
            .get::<crate::server::PlayerMob>(ghost)
            .map(|p| p.client)
            != Some(client)
    {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(bud) else {
        return;
    };
    let Some(mob) = crate::mobs::spawn::spawn_mob(world, VENUS_TRAP, pos) else {
        warn!("no prototype for {VENUS_TRAP}");
        return;
    };
    if !crate::medical::brainmob::possess(world, client, ghost, mob) {
        world.despawn(mob);
        return;
    }
    world.despawn(ghost);
    world.despawn(bud);
    crate::noncrew_jobs::set_assigned_role(world, mob, "/datum/job/venus_human_trap");
    for (span, text) in [
        (Span::BoldNotice, "Вы — венерина мухоловка."),
        (
            Span::Notice,
            "Защищайте кудзу любой ценой и пожирайте тех, кто вам противостоит!",
        ),
    ] {
        crate::tools::tell(world, client, span, text);
    }
}

/// The bud's four thorny vines: the tiles one and two away diagonally.
fn on_bud_vine(world: &World, at: TilePos) -> bool {
    let index = world.resource::<SpatialIndex>();
    [
        (1, 1),
        (2, 2),
        (-1, 1),
        (-2, 2),
        (1, -1),
        (2, -2),
        (-1, -1),
        (-2, -2),
    ]
    .iter()
    .any(|&(dx, dy)| {
        index
            .on_tile(TilePos::new(at.x - dx, at.y - dy, at.z))
            .iter()
            .any(|&e| world.get::<FlowerBud>(e).is_some())
    })
}

// ---------------------------------------------------------------------------------------------
// Stepping onto things

/// `COMSIG_ATOM_ENTERED` of a floating bubble, the acid, the bud's vines; the arachnid's acid
/// trail (`COMSIG_MOVABLE_MOVED`).
fn on_moved(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    let now = now(world);
    for m in moved {
        if world.get_entity(m.entity).is_err() {
            continue;
        }
        if world
            .get::<ArachnidPowers>(m.entity)
            .is_some_and(|a| now < a.acid_until)
        {
            release_acid(world, m.to);
        }
        if world.get::<Health>(m.entity).is_none() {
            continue;
        }
        let here: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(m.to).to_vec();
        let leaper = world.get::<LeaperPowers>(m.entity).is_some();
        if !leaper
            && let Some(&bubble) = here
                .iter()
                .find(|&&e| world.get::<FloatingBubble>(e).is_some())
        {
            // `leaper_bubble/on_entered`.
            playsound(world, PlaySound::new(m.to, SNAP, 50.0, true));
            crate::status_effects::apply(world, m.entity, Incap::Paralyze, BUBBLE_PARALYZE);
            bubble_mob(world, m.entity);
            pop_floating(world, bubble, Some(m.to));
        }
        let acid = here.iter().any(|&e| world.get::<SlipperyAcid>(e).is_some());
        if acid && crate::throw::is_carbon(world, m.entity) {
            crate::janitor::try_slip(world, m.entity, (ACID_SLIP, false));
        }
        if !is_vine_immune(world, m.entity) && on_bud_vine(world, m.to) {
            super::defense::hurt(world, m.entity, DamageType::Brute, VINE_CUT, None);
            crate::tools::tell_mob(
                world,
                m.entity,
                Span::Warning,
                "Вы порезались о колючие лозы.",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jungle_projectiles_match_their_types() {
        let bubble = bubble_projectile();
        assert_eq!(bubble.damage, 0.0);
        // 5 s paralysis, in deciseconds like the converted vars.
        assert_eq!(bubble.paralyze, 50.0);
        assert_eq!(bubble.range, 7.0);
        let seed = seed_projectile();
        assert_eq!((seed.damage, seed.damage_type.as_str()), (10.0, "burn"));
        assert_eq!(snare_projectile().damage, 0.0);
    }
}

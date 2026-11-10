//! Smoking (server), after tg `code/game/objects/items/lighter.dm` and `cigarettes.dm`.
//!
//! - Lighters ([`Lighter`]) are tg `TOOL_WELDER`s: their fuel is the [`Welder`]'s (`maximum_fuel`
//!   6, cheap ones 3; 0.25 burnt per 5 s while lit), so they refill at fuel tanks, weld what
//!   needs no more than their heat (1400 K, the slime zippo 2400 K) and are open flames. Used in
//!   hand they light (fuel left) or go out; a cheap one burns a bare finger one time in four
//!   (gloves rated over 360 K protect). Lit, they burn for 5 instead of hitting for nothing, set
//!   a mob with fire stacks alight on a hit and light the cigarette in someone's mouth.
//! - Matches ([`Match`]) strike on a matchbox (10% nothing, 2% snapped), burn 10 s (a firebrand
//!   40 s, lit from the start) as open flames and go out when they leave the hand; the battery
//!   lighter strikes after 4 s for 5 burn on the hand.
//! - Cigarettes, cigars, rollies and pipes ([`Smokable`], `DmSmokable` vars) light from a hot
//!   item or a fire (`attempt_light`: oxygen in it or around, something to smoke), from a
//!   burning body, or with a hot item on the mob wearing them. Lit, every second: the holder
//!   with fire stacks catches fire, no oxygen puts it out, the time burns down to the butt
//!   (`type_butt`), it is an open flame; every `dragtime` a drag (`handle_reagents`): heated to
//!   its `heat`, `REAGENTS_METABOLISM` (all of it spread over the time left for `smoke_all`) is
//!   inhaled by the mob wearing it on the mask (exposed `INHALE`, into the blood, `lung_harm`
//!   to the lungs, half for smokers) or lost. Used in hand a lit one is put out (a butt; on a
//!   floor trodden into ash). Flash powder burns it up. An unlit one dips into a cup to soak
//!   its reagents.
//! - A pipe is packed with a dried plant or a drug (13 minutes of its reagents), emptied into
//!   ash by hand, and only goes out; rolling paper and a dryable plant make a rollie.
//! - Vapes ([`Vape`]): 100 u holding 50 u nicotine, no reactions until worn. A screwdriver
//!   opens the cap (refillable, emptied by hand), a multitool on an open cap switches the high
//!   voltage, an emag on it maximizes it. Worn on the mask with the cap closed, every 8 s a puff
//!   of 0.2 u into the blood (welding fuel sets the vaper on fire); high voltage also fills
//!   `DIAMOND_AREA(1)` tiles with its chemical smoke, emagged `DIAMOND_AREA(4)`, and has a 5%
//!   chance to blow up in the mouth (20 burn to the head, 30 s paralysis).
// shortcut: no light from lighters, matches, cigarettes or vapes; no smoke particles, long
// exhale or smoke in the face; no choking on a cigarette when forced to say something; no
// knock-off component, no eating cigarettes, no gas mask holding a cigarette, no helmet air for
// `check_oxygen`; flash powder burns a cigarette up without the flash and death hallucination;
// no random greytide or carp additions; lit and burnt names, pipe names and greyscale lighter
// colours stay the type's; no vape greyscale states; the bright and mime lighters' flashes and
// texts are the plain ones; a rollie takes one plant.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmClothing, DmFood, DmItem, DmSmokable, DmTool};
use ss13_reagents::{Holder, Owner, REAGENTS_METABOLISM, flags};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::chem::{Chem, MobReagents, Reagents, map_pos, name_of, tell_mob, visible};
use crate::combat::{CombatMode, CombatRng};
use crate::health::DamageType;
use crate::inventory::{CarriedBy, Inventory};
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, Welder, do_after};
use crate::zone::BodyZone;

pub const LIGHTER_ROOT: &str = "/obj/item/lighter";
pub const MATCH_ROOT: &str = "/obj/item/match";
const FIREBRAND: &str = "/obj/item/match/firebrand";
const BATTERY_LIGHTER: &str = "/obj/item/match/battery";
pub const MATCHBOX: &str = "/obj/item/storage/box/matches";
const PIPE_ROOT: &str = "/obj/item/cigarette/pipe";
pub const ROLLIE: &str = "/obj/item/cigarette/rollie";
const ROLLING_PAPER: &str = "/obj/item/rollingpaper";
pub const VAPE_ROOT: &str = "/obj/item/vape";
const CUP_ROOT: &str = "/obj/item/reagent_containers/cup";
const MORTAR: &str = "/obj/item/reagent_containers/cup/mortar";
const GROWN_ROOT: &str = "/obj/item/food/grown";
const DRUG_FOOD_ROOT: &str = "/obj/item/food/drug";
pub const ASH: &str = "/obj/effect/decal/cleanable/ash";
const OXYGEN: &str = "/datum/reagent/oxygen";
const FLASH_POWDER: &str = "/datum/reagent/flash_powder";
const METH: &str = "/datum/reagent/drug/methamphetamine";
const FUEL: &str = "/datum/reagent/fuel";
const NICOTINE: &str = "/datum/reagent/drug/nicotine";
/// `HIGH_TEMPERATURE_REQUIRED - 100`, the slime zippo's `+ 1000`, `TCMB`.
const LIGHTER_HEAT: f32 = 1400.0;
const SLIME_HEAT: f32 = 2400.0;
const TCMB: f32 = 2.7;
/// `/obj/item/match::heat`.
const MATCH_HEAT: f32 = 1000.0;
/// Lit lighter, match and cigarette `force`.
const LIGHTER_FORCE: f32 = 5.0;
const MATCH_FORCE: f32 = 3.0;
const CIGARETTE_FORCE: f32 = 4.0;
/// `use(0.25)` per `TOOL_FUEL_BURN_INTERVAL`, `use(0.5)` per hit.
const LIGHTER_BURN: f32 = 0.25;
const LIGHTER_HIT_FUEL: f32 = 0.5;
/// `open_flame()` volume.
const FLAME_VOLUME: f64 = 5.0;
/// A pipe's `smoketime` once packed: 13 minutes.
const PIPE_SMOKETIME: f32 = 780.0;
/// `/obj/item/vape`: `chem_volume`, the nicotine it starts with, `dragtime` (s).
const VAPE_VOLUME: f32 = 100.0;
const VAPE_NICOTINE: f32 = 50.0;
const VAPE_DRAGTIME: f64 = 8.0;
/// `carry_limit` of the vape's smoke.
const VAPE_SMOKE_CARRY: f32 = 20.0;
/// The battery lighter's `do_after`, deciseconds.
const BATTERY_STRIKE_DS: f32 = 40.0;
/// `SSobj` wait.
const TICK_SECS: f64 = 1.0;

/// A lighter type's numbers (`maximum_fuel`, `heat_while_on`, `fancy`, `spawns_with_reagent`,
/// `overlay_list`). Most specific type first.
struct LighterSpec {
    root: &'static str,
    max_fuel: f32,
    heat: f32,
    fancy: bool,
    full: bool,
    overlays: &'static [&'static str],
}

const ZIPPO_OVERLAYS: &[&str] = &["plain", "dame", "thirteen", "snake"];
const CHEAP_OVERLAYS: &[&str] = &["transp", "tall", "matte", "zoppo"];

const LIGHTERS: [LighterSpec; 8] = [
    LighterSpec {
        root: "/obj/item/lighter/greyscale/empty",
        max_fuel: 3.0,
        heat: LIGHTER_HEAT,
        fancy: false,
        full: false,
        overlays: CHEAP_OVERLAYS,
    },
    LighterSpec {
        root: "/obj/item/lighter/greyscale",
        max_fuel: 3.0,
        heat: LIGHTER_HEAT,
        fancy: false,
        full: true,
        overlays: CHEAP_OVERLAYS,
    },
    LighterSpec {
        root: "/obj/item/lighter/slime",
        max_fuel: 6.0,
        heat: SLIME_HEAT,
        fancy: true,
        full: true,
        overlays: &["slime"],
    },
    LighterSpec {
        root: "/obj/item/lighter/skull",
        max_fuel: 6.0,
        heat: LIGHTER_HEAT,
        fancy: true,
        full: true,
        overlays: &["skull"],
    },
    LighterSpec {
        root: "/obj/item/lighter/mime",
        max_fuel: 6.0,
        heat: TCMB,
        fancy: false,
        full: true,
        overlays: &["mime"],
    },
    LighterSpec {
        root: "/obj/item/lighter/bright",
        max_fuel: 6.0,
        heat: LIGHTER_HEAT,
        fancy: false,
        full: true,
        overlays: &["bright"],
    },
    LighterSpec {
        root: "/obj/item/lighter/empty",
        max_fuel: 6.0,
        heat: LIGHTER_HEAT,
        fancy: true,
        full: false,
        overlays: ZIPPO_OVERLAYS,
    },
    LighterSpec {
        root: LIGHTER_ROOT,
        max_fuel: 6.0,
        heat: LIGHTER_HEAT,
        fancy: true,
        full: true,
        overlays: ZIPPO_OVERLAYS,
    },
];

fn lighter_spec(path: &str) -> Option<&'static LighterSpec> {
    LIGHTERS.iter().find(|s| is_under(path, s.root))
}

/// A lighter's own state; lit and fuel are its [`Welder`]'s.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Lighter {
    pub fancy: bool,
    pub max_fuel: f32,
    /// `heat_while_on`.
    pub heat: f32,
    /// `overlay_state`.
    overlay: String,
}

/// `/obj/item/match`: `lit`, `burnt`, `broken`, `smoketime` (s).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Match {
    pub state: MatchState,
    pub smoketime: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchState {
    Unlit,
    Lit,
    Burnt,
    Broken,
}

/// A smokable's live state: `lit`, `smoketime` left (s), the drag cooldown, a pipe's
/// `packeditem`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Smokable {
    pub lit: bool,
    pub smoketime: f32,
    next_drag: f64,
    pub packed: Option<String>,
}

/// `/obj/item/vape`: `screw`, `super`, `EMAGGED`, puffing (processing while worn).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Vape {
    pub screw: bool,
    pub super_mode: bool,
    pub emagged: bool,
    pub puffing: bool,
    next_drag: f64,
}

/// A timed smoking action ([`crate::construction::Job::Smoking`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmokingJob {
    /// The battery lighter held tight.
    BatteryStrike,
}

#[derive(Resource, Default)]
struct SmokingClock(f64);

pub struct SmokingPlugin;

impl Plugin for SmokingPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // Before `tools::attack_self`: lighters are welders.
        app.add_click_handler(ClickPhase::AttackSelf, 199, |w, c| {
            attack_self(w, c.client, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::OtherHand, 489, |w, c| {
            c.held
                .is_some_and(|h| item_on_item(w, c.client, c.mob, h, c.target))
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 229, |w, c| {
            c.held
                .is_some_and(|h| held_on_target(w, c.client, c.mob, h, c.target))
        });
        app.init_resource::<SmokingClock>()
            .add_observer(add_item_state)
            .add_observer(add_smokable)
            .add_systems(Update, process.after(ss13_world::WorldSystems::Movement));
    }
}

fn path_of(world: &World, e: Entity) -> String {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

/// `get_temperature()` of a lit match or smokable (`lit * heat`); `None` for anything else.
pub fn lit_heat(world: &World, item: Entity) -> Option<f32> {
    if let Some(m) = world.get::<Match>(item) {
        return Some(if m.state == MatchState::Lit {
            MATCH_HEAT
        } else {
            0.0
        });
    }
    let smokable = world.get::<Smokable>(item)?;
    let heat = world.get::<DmSmokable>(item).map_or(0.0, |d| d.heat);
    Some(if smokable.lit { heat } else { 0.0 })
}

fn heat_of(world: &World, item: Entity) -> f32 {
    crate::medical::surgery::ToolInfo::read(world, Some(item)).temperature
}

fn holder_of(world: &World, item: Entity) -> Option<Entity> {
    world.get::<CarriedBy>(item).map(|c| c.0)
}

fn in_mask(world: &World, mob: Entity, item: Entity) -> bool {
    world.get::<Inventory>(mob).and_then(|i| i.mask) == Some(item)
}

fn in_hands(world: &World, mob: Entity, item: Entity) -> bool {
    world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.hands.contains(&Some(item)))
}

fn sound_at(world: &mut World, e: Entity, file: &str, vol: f32, vary: bool) {
    if let Some(pos) = map_pos(world, e) {
        playsound(world, PlaySound::new(pos, file, vol, vary));
    }
}

fn say_near(world: &mut World, pos: TilePos, span: Span, text: &str) {
    let clients = crate::chem::clients_near(world, pos, None);
    if !clients.is_empty() {
        world.write_message(ss13_net::chat::server::SendChat {
            clients,
            message: crate::chem::line(span, text),
        });
    }
}

/// The item's live `force` and `damtype`, and the holder's in-hand sprite.
fn set_force(world: &mut World, item: Entity, force: f32, burn: bool, inhand: Option<&str>) {
    let damtype = if burn { "burn" } else { "brute" };
    if let Some(mut dm) = world.get_mut::<DmItem>(item) {
        dm.force = force;
        dm.damtype = damtype.to_owned();
        if let Some(state) = inhand {
            dm.inhand_icon_state = state.to_owned();
        }
    }
    if let Some(holder) = holder_of(world, item)
        && let Some(mut inventory) = world.get_mut::<Inventory>(holder)
    {
        inventory.set_changed();
    }
}

/// `if(male_model.fire_stacks && !male_model.on_fire) ignite_mob()` of a lighter or match
/// lit in a living holder's hands.
fn ignite_holder(world: &mut World, item: Entity) {
    if let Some(holder) = holder_of(world, item) {
        crate::fire::ignite_mob(world, holder);
    }
}

fn add_item_state(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    let e = add.entity;
    if lighter_spec(&item.path).is_some() {
        commands.queue(move |world: &mut World| init_lighter(world, e));
    } else if is_under(&item.path, MATCH_ROOT) {
        commands.queue(move |world: &mut World| init_match(world, e));
    } else if is_under(&item.path, VAPE_ROOT) {
        commands.queue(move |world: &mut World| init_vape(world, e));
    }
}

// --- Lighters ---------------------------------------------------------------------------------

fn init_lighter(world: &mut World, e: Entity) {
    if world.get_entity(e).is_err() || world.get::<Lighter>(e).is_some() {
        return;
    }
    let Some(spec) = lighter_spec(&path_of(world, e)) else {
        return;
    };
    let pick = world
        .resource_mut::<CombatRng>()
        .range(0, spec.overlays.len() as u32 - 1) as usize;
    let mut welder = Welder::new(spec.max_fuel);
    welder.burn_use = LIGHTER_BURN;
    if !spec.full {
        welder.fuel = 0.0;
    }
    world.entity_mut(e).insert((
        welder,
        Lighter {
            fancy: spec.fancy,
            max_fuel: spec.max_fuel,
            heat: spec.heat,
            overlay: spec.overlays[pick].to_owned(),
        },
    ));
    refresh_lighter(world, e);
}

/// `update_icon_state`, `update_overlays` (`lighter_overlay_[overlay_state][-on]`) and the lit
/// `force`/`damtype` of `set_lit`.
pub fn refresh_lighter(world: &mut World, item: Entity) {
    let (Some(lighter), Some(welder)) = (
        world.get::<Lighter>(item).cloned(),
        world.get::<Welder>(item).cloned(),
    ) else {
        return;
    };
    // `heat = heat_while_on` while lit; `get_temperature` reads it only then. Set here too:
    // the tool vars may arrive after the item's.
    if let Some(mut tool) = world.get_mut::<DmTool>(item)
        && tool.heat != lighter.heat
    {
        tool.heat = lighter.heat;
    }
    let on = if welder.on { "-on" } else { "" };
    let base = world
        .get::<DmAtom>(item)
        .map(|a| a.icon_state.clone())
        .unwrap_or_default();
    crate::obj_damage::set_icon_state(world, item, &format!("{base}{on}"));
    let overlay = world.get::<DmAtom>(item).and_then(|a| {
        world
            .get_resource::<ss13_mapload::IconIndex>()
            .and_then(|i| i.get(&a.icon, &format!("lighter_overlay_{}{on}", lighter.overlay)))
    });
    if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(item) {
        let overlays: Vec<ss13_shared::Overlay> =
            overlay.into_iter().map(ss13_shared::Overlay::new).collect();
        if look.overlays != overlays {
            look.overlays = overlays;
        }
    }
    if welder.on {
        set_force(world, item, LIGHTER_FORCE, true, None);
    } else {
        set_force(world, item, 0.0, false, None);
    }
}

/// `set_lit`: the state, the sound, a holder with fire stacks catching fire.
fn set_lighter_lit(world: &mut World, item: Entity, lit: bool) {
    let fancy = world.get::<Lighter>(item).is_some_and(|l| l.fancy);
    match world.get_mut::<Welder>(item) {
        Some(mut w) if w.on != lit => w.on = lit,
        _ => return,
    }
    let sound = match (fancy, lit) {
        (true, true) => "sound/items/lighter/zippo_on.ogg",
        (true, false) => "sound/items/lighter/zippo_off.ogg",
        (false, true) => "sound/items/lighter/lighter_on.ogg",
        (false, false) => "sound/items/lighter/lighter_off.ogg",
    };
    sound_at(world, item, sound, 100.0, true);
    if lit {
        ignite_holder(world, item);
    }
    refresh_lighter(world, item);
}

/// `/obj/item/lighter/attack_self`.
fn lighter_attack_self(world: &mut World, mob: Entity, item: Entity) -> bool {
    let Some(lighter) = world.get::<Lighter>(item).cloned() else {
        return false;
    };
    let Some(welder) = world.get::<Welder>(item).cloned() else {
        return false;
    };
    let (user, name) = (name_of(world, mob), name_of(world, item));
    if welder.on {
        set_lighter_lit(world, item, false);
        if lighter.fancy {
            visible(
                world,
                mob,
                &format!("Вы тихо закрываете {name}, даже не глядя. Вау."),
                &format!("Вы слышите тихий щелчок: {user}, даже не глядя, закрывает {name}. Вау."),
                Span::Notice,
            );
        } else {
            visible(
                world,
                mob,
                &format!("Вы тихо гасите {name}."),
                &format!("{user} тихо гасит {name}."),
                Span::Notice,
            );
        }
        return true;
    }
    if welder.fuel <= 0.0 {
        return true;
    }
    set_lighter_lit(world, item, true);
    if lighter.fancy {
        visible(
            world,
            mob,
            &format!("Не сбавляя шага, вы одним плавным движением открываете и зажигаете {name}."),
            &format!(
                "Не сбавляя шага, {user} одним плавным движением открывает и зажигает {name}."
            ),
            Span::Notice,
        );
        return true;
    }
    // Gloves rated over 360 K keep the thumb whole; a mob without gloves slots is not human.
    let protected = world.get::<Inventory>(mob).is_none_or(|i| {
        i.gloves
            .and_then(|g| world.get::<DmClothing>(g))
            .is_some_and(|c| c.max_heat_protection_temperature > 360.0)
    });
    if protected || world.resource_mut::<CombatRng>().chance(75.0) {
        visible(
            world,
            mob,
            &format!("После нескольких попыток вам удаётся зажечь {name}."),
            &format!("После нескольких попыток {user} зажигает {name}."),
            Span::Notice,
        );
        return true;
    }
    let hand = world.get::<Inventory>(mob).map_or(0, |i| i.active);
    let zone = if hand == 0 {
        BodyZone::LArm
    } else {
        BodyZone::RArm
    };
    crate::mobs::defense::hurt(world, mob, DamageType::Burn, 5.0, Some(zone));
    visible(
        world,
        mob,
        "Вы обжигаетесь, зажигая зажигалку!",
        &format!("После нескольких попыток {user} зажигает {name}, но обжигает палец."),
        Span::Warning,
    );
    true
}

/// `get_fuel()` for examine.
pub fn examine_lines(world: &World, item: Entity, lines: &mut Vec<(Span, String)>) {
    if let (Some(lighter), Some(welder)) = (world.get::<Lighter>(item), world.get::<Welder>(item)) {
        if welder.fuel <= 0.0 {
            lines.push((
                Span::Warning,
                "Закончилась жидкость для зажигалки! Заправьте её сварочным топливом.".to_owned(),
            ));
        } else {
            lines.push((
                Span::Notice,
                format!(
                    "Содержит {} ед. топлива из {}.",
                    crate::chem::dm_num(welder.fuel),
                    crate::chem::dm_num(lighter.max_fuel)
                ),
            ));
        }
    }
    if let Some(s) = world.get::<Smokable>(item)
        && let Some(packed) = &s.packed
    {
        lines.push((Span::Notice, format!("Набита: {packed}.")));
    }
}

// --- Matches ----------------------------------------------------------------------------------

fn init_match(world: &mut World, e: Entity) {
    if world.get_entity(e).is_err() || world.get::<Match>(e).is_some() {
        return;
    }
    let path = path_of(world, e);
    // `smoketime`: 10 s, the firebrand's 40 s.
    let smoketime = if is_under(&path, FIREBRAND) {
        40.0
    } else {
        10.0
    };
    world.entity_mut(e).insert(Match {
        state: MatchState::Unlit,
        smoketime,
    });
    if is_under(&path, FIREBRAND) {
        match_ignite(world, e);
    }
}

fn refresh_match(world: &mut World, item: Entity) {
    let Some(m) = world.get::<Match>(item).cloned() else {
        return;
    };
    let suffix = match m.state {
        MatchState::Unlit => "unlit",
        MatchState::Lit => "lit",
        MatchState::Burnt => "burnt",
        MatchState::Broken => "broken",
    };
    // `base_icon_state`: the unlit state without its suffix.
    let base = world
        .get::<DmAtom>(item)
        .map(|a| a.icon_state.trim_end_matches("_unlit").to_owned())
        .unwrap_or_default();
    crate::obj_damage::set_icon_state(world, item, &format!("{base}_{suffix}"));
    let lit = m.state == MatchState::Lit;
    let inhand = if lit { "cigon" } else { "cigoff" };
    let force = if lit { MATCH_FORCE } else { 0.0 };
    set_force(world, item, force, lit, Some(inhand));
}

/// `matchignite`.
pub fn match_ignite(world: &mut World, item: Entity) {
    match world.get_mut::<Match>(item) {
        Some(mut m) if m.state == MatchState::Unlit => m.state = MatchState::Lit,
        _ => return,
    }
    sound_at(world, item, "sound/items/match_strike.ogg", 15.0, true);
    ignite_holder(world, item);
    refresh_match(world, item);
}

/// `matchburnout`.
fn match_burnout(world: &mut World, item: Entity) {
    match world.get_mut::<Match>(item) {
        Some(mut m) if m.state == MatchState::Lit => m.state = MatchState::Burnt,
        _ => return,
    }
    refresh_match(world, item);
}

/// `snap`.
fn match_snap(world: &mut World, item: Entity) {
    if world
        .get::<Match>(item)
        .is_none_or(|m| m.state == MatchState::Broken)
    {
        return;
    }
    match_burnout(world, item);
    sound_at(world, item, "sound/effects/snap.ogg", 15.0, true);
    if let Some(mut m) = world.get_mut::<Match>(item) {
        m.state = MatchState::Broken;
    }
    refresh_match(world, item);
}

/// `/datum/element/ignites_matches/on_interact`: a match swiped over a matchbox.
fn strike_match(world: &mut World, mob: Entity, held: Entity, target: Entity) -> bool {
    if !is_under(&path_of(world, target), MATCHBOX)
        || world
            .get::<Match>(held)
            .is_none_or(|m| m.state != MatchState::Unlit)
    {
        return false;
    }
    let (user, what, box_name) = (
        name_of(world, mob),
        name_of(world, held),
        name_of(world, target),
    );
    let (fails, snaps) = {
        let mut rng = world.resource_mut::<CombatRng>();
        (rng.chance(10.0), rng.chance(2.0))
    };
    if fails {
        visible(
            world,
            mob,
            &format!("Вы чиркаете {what} о {box_name}, но она не загорается."),
            &format!("{user} чиркает {what} о {box_name}, но ничего не происходит."),
            Span::Warning,
        );
    } else if snaps {
        visible(
            world,
            mob,
            &format!("Вы чиркаете {what} о {box_name} слишком быстро и ломаете её пополам."),
            &format!("{user} чиркает {what} о {box_name} и случайно ломает её."),
            Span::Warning,
        );
        match_snap(world, held);
    } else {
        visible(
            world,
            mob,
            &format!("Вы чиркаете {what} о {box_name} и зажигаете её."),
            &format!("{user} чиркает {what} о {box_name} и зажигает её."),
            Span::Notice,
        );
        match_ignite(world, held);
    }
    true
}

/// The rest of a [`SmokingJob`] after its `do_after`.
pub fn complete(world: &mut World, mob: Entity, action: &crate::tools::DoAfter, job: SmokingJob) {
    match job {
        SmokingJob::BatteryStrike => {
            let Some(item) = action.item.filter(|&i| in_hands(world, mob, i)) else {
                return;
            };
            let hand = world.get::<Inventory>(mob).map_or(0, |i| i.active);
            let zone = if hand == 0 {
                BodyZone::LArm
            } else {
                BodyZone::RArm
            };
            crate::mobs::defense::hurt(world, mob, DamageType::Burn, 5.0, Some(zone));
            match_ignite(world, item);
        }
    }
}

// --- Cigarettes, cigars, pipes ----------------------------------------------------------------

/// `Initialize`: the holder (`chem_volume`, `INJECTABLE`) with `list_reagents`, `starts_lit`.
fn add_smokable(
    add: On<Add<DmSmokable>>,
    smokables: Query<(&DmSmokable, Option<&DmItem>)>,
    chem: Option<Res<Chem>>,
    mut commands: Commands,
) {
    let Ok((dm, item)) = smokables.get(add.entity) else {
        return;
    };
    let owner = Owner {
        path: item.map(|i| i.path.clone()).unwrap_or_default(),
        living: false,
    };
    let mut holder = Holder::new(dm.chem_volume, flags::INJECTABLE, owner);
    if let Some(chem) = &chem {
        let list: Vec<(u16, f32)> = dm
            .list_reagents
            .iter()
            .filter_map(|r| chem.0.id(&r.path).map(|id| (id, r.amount)))
            .collect();
        holder.add_reagent_list(&chem.0, &list, None, true);
    }
    let e = add.entity;
    commands.entity(e).insert((
        Reagents(holder),
        Smokable {
            lit: false,
            smoketime: dm.smoketime,
            next_drag: 0.0,
            packed: None,
        },
    ));
    if dm.starts_lit {
        commands.queue(move |world: &mut World| light(world, e, None));
    }
}

fn smokable_dm(world: &World, item: Entity) -> DmSmokable {
    world.get::<DmSmokable>(item).cloned().unwrap_or_default()
}

fn is_pipe(world: &World, item: Entity) -> bool {
    is_under(&path_of(world, item), PIPE_ROOT)
}

fn reagent_amount(world: &World, item: Entity, path: &str) -> f32 {
    let Some(chem) = world.get_resource::<Chem>() else {
        return 0.0;
    };
    match (chem.0.id(path), world.get::<Reagents>(item)) {
        (Some(id), Some(r)) => r.0.amount(id),
        _ => 0.0,
    }
}

/// `check_oxygen`: oxygen in it, or a mole of it in the air around.
fn check_oxygen(world: &World, item: Entity) -> bool {
    if reagent_amount(world, item, OXYGEN) > 0.0 {
        return true;
    }
    let Some(pos) = map_pos(world, item) else {
        return false;
    };
    let Some(atmos) = world.get_resource::<ss13_atmos::Atmos>() else {
        return true;
    };
    // Levels without simulated air breathe station air (`crate::breath`).
    if atmos.level(pos.z).is_none() {
        return true;
    }
    atmos
        .moles(pos, ss13_atmos::GasId::Oxygen)
        .is_some_and(|m| m >= 1.0)
}

/// `ignition_effect(A, user)` of `tool`: its line, or `None` when it is not hot.
fn ignition_text(world: &World, tool: Entity, user: &str, target: &str) -> Option<String> {
    if heat_of(world, tool) <= 0.0 {
        return None;
    }
    let tool_name = name_of(world, tool);
    let path = path_of(world, tool);
    Some(if is_under(&path, "/obj/item/lighter/mime") {
        format!("{user} подносит {tool_name} к {target}, и та чудесным образом загорается!")
    } else if world.get::<Lighter>(tool).is_some_and(|l| l.fancy) {
        format!(
            "{user} одним движением кисти изящно поджигает {target} с помощью {tool_name}. \
             Чертовски круто."
        )
    } else if world.get::<Lighter>(tool).is_some() {
        format!("Немного повозившись, {user} поджигает {target} с помощью {tool_name}.")
    } else if world.get::<Welder>(tool).is_some() {
        format!("{user} небрежно поджигает {target} с помощью {tool_name}. Какой крутой.")
    } else {
        format!("{user} поджигает {target} с помощью {tool_name}.")
    })
}

/// `attempt_light(user, tool, text_override)`. True when the click was taken.
fn attempt_light(
    world: &mut World,
    mob: Entity,
    cig: Entity,
    tool: Entity,
    text: Option<String>,
) -> bool {
    if world.get::<Smokable>(cig).is_none_or(|s| s.lit) {
        return false;
    }
    let (user, what) = (name_of(world, mob), name_of(world, cig));
    let text = match text {
        Some(text) => Some(text),
        None => ignition_text(world, tool, &user, &what),
    };
    let Some(text) = text else {
        return false;
    };
    if !check_oxygen(world, cig) {
        crate::balloon::balloon_alert(world, mob, cig, "нет воздуха!");
        return true;
    }
    if world
        .get::<Smokable>(cig)
        .is_some_and(|s| s.smoketime <= 0.0)
    {
        tell_mob(world, mob, Span::Warning, "Здесь нечего курить!");
        return true;
    }
    light(world, cig, Some(text));
    true
}

/// `light(flavor_text)`.
pub fn light(world: &mut World, cig: Entity, flavor: Option<String>) {
    match world.get_mut::<Smokable>(cig) {
        Some(mut s) if !s.lit => s.lit = true,
        _ => return,
    }
    sound_at(world, cig, "sound/items/lighter/cig_light.ogg", 100.0, true);
    let dm = smokable_dm(world, cig);
    // Flash powder burns it up (`trick trick`).
    if reagent_amount(world, cig, FLASH_POWDER) > 0.0 {
        let name = name_of(world, cig);
        match holder_of(world, cig) {
            Some(mob) => visible(
                world,
                mob,
                "Раствор бурно взрывается!",
                &format!("{name} у {} сгорает!", name_of(world, mob)),
                Span::Danger,
            ),
            None => {
                if let Some(pos) = map_pos(world, cig) {
                    say_near(world, pos, Span::Warning, &format!("{name} сгорает!"));
                }
            }
        }
        world.despawn(cig);
        return;
    }
    let chem = world.get_resource::<Chem>().map(|c| c.0.clone());
    let meth = reagent_amount(world, cig, METH) > 0.0;
    if let (Some(chem), Some(mut r)) = (chem, world.get_mut::<Reagents>(cig)) {
        if meth {
            r.0.flags |= flags::NO_REACT;
        }
        // Reagents may react once it is lit.
        r.0.handle_reactions(&chem);
    }
    if world.get_entity(cig).is_err() {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut s) = world.get_mut::<Smokable>(cig) {
        s.next_drag = now;
    }
    crate::obj_damage::set_icon_state(world, cig, &dm.icon_on);
    let inhand = (!dm.inhand_icon_on.is_empty()).then_some(dm.inhand_icon_on.as_str());
    set_force(world, cig, CIGARETTE_FORCE, true, inhand);
    if let (Some(text), Some(pos)) = (flavor, map_pos(world, cig)) {
        say_near(world, pos, Span::Notice, &text);
    }
}

/// `extinguish()` of a lit smokable: it goes out, no reactions any more.
fn extinguish(world: &mut World, cig: Entity) {
    match world.get_mut::<Smokable>(cig) {
        Some(mut s) if s.lit => s.lit = false,
        _ => return,
    }
    if let Some(mut r) = world.get_mut::<Reagents>(cig) {
        r.0.flags |= flags::NO_REACT;
    }
    sound_at(world, cig, "sound/items/lighter/cig_snuff.ogg", 100.0, true);
    unlit_look(world, cig);
    if let Some(mob) = holder_of(world, cig) {
        let name = name_of(world, cig);
        tell_mob(world, mob, Span::Notice, &format!("{name} гаснет."));
    }
}

fn unlit_look(world: &mut World, cig: Entity) {
    let dm = smokable_dm(world, cig);
    crate::obj_damage::set_icon_state(world, cig, &dm.icon_off);
    let inhand = (!dm.inhand_icon_off.is_empty()).then_some(dm.inhand_icon_off.as_str());
    set_force(world, cig, 0.0, false, inhand);
}

/// `put_out(user, done_early)`: a cigarette leaves its butt (on a floor, trodden: ash); a pipe
/// only goes out.
fn put_out(world: &mut World, cig: Entity, user: Option<Entity>, early: bool) {
    let name = name_of(world, cig);
    if is_pipe(world, cig) {
        if let Some(mut s) = world.get_mut::<Smokable>(cig) {
            s.lit = false;
            if !early {
                s.packed = None;
            }
        }
        unlit_look(world, cig);
        if let Some(user) = user {
            if early {
                let who = name_of(world, user);
                visible(
                    world,
                    user,
                    &format!("Вы тушите {name}."),
                    &format!("{who} тушит {name}."),
                    Span::Notice,
                );
            } else {
                tell_mob(world, user, Span::Notice, &format!("{name} гаснет."));
            }
        }
        return;
    }
    let Some(pos) = map_pos(world, cig) else {
        world.despawn(cig);
        return;
    };
    if let Some(user) = user {
        if early {
            let who = name_of(world, user);
            if crate::multiz::has_gravity(world, pos) {
                visible(
                    world,
                    user,
                    &format!("Вы спокойно бросаете {name} и затаптываете её."),
                    &format!("{who} спокойно бросает {name} и затаптывает её."),
                    Span::Notice,
                );
                crate::construction::spawn_obj(world, ASH, pos);
            } else {
                visible(
                    world,
                    user,
                    &format!("Вы тушите {name} пальцами."),
                    &format!("{who} тушит {name} пальцами."),
                    Span::Notice,
                );
            }
        } else {
            tell_mob(world, user, Span::Notice, &format!("{name} гаснет."));
        }
    }
    let butt = smokable_dm(world, cig).type_butt;
    if !butt.is_empty() {
        crate::construction::spawn_obj(world, &butt, pos);
    }
    world.despawn(cig);
}

/// `to_smoke` of one drag: the metabolism rate, or with `smoke_all` the share of what is left
/// that keeps the drags even until the end.
pub fn drag_amount(smoke_all: bool, total: f32, dragtime: f32, smoketime: f32) -> f32 {
    if smoke_all && smoketime > 0.0 {
        total * (dragtime / smoketime)
    } else {
        REAGENTS_METABOLISM
    }
}

/// `trans_to(smoker, amount, methods = INHALE, ignore_stomach = TRUE)` from `item`: exposed by
/// inhaling, into the blood; what does not go in is lost.
fn inhale(world: &mut World, item: Entity, smoker: Entity, amount: f32) {
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let Some(snapshot) = world.get::<Reagents>(item).map(|r| r.0.clone()) else {
        return;
    };
    if snapshot.total_volume <= 0.0 {
        return;
    }
    let fraction = (amount / snapshot.total_volume).min(1.0);
    crate::janitor::expose_by(
        world,
        smoker,
        &snapshot,
        fraction,
        crate::janitor::INHALE,
        crate::janitor::WATER_COOLING_TEMPERATURE,
    );
    let Some(mut held) = world
        .get_mut::<Reagents>(item)
        .map(|mut r| std::mem::replace(&mut r.0, Holder::new(0.0, 0, Owner::default())))
    else {
        return;
    };
    let moved = match world.get_mut::<MobReagents>(smoker) {
        Some(mut m) => held.trans_to(&chem, &mut m.blood, amount, 1.0, None, false, false),
        None => 0.0,
    };
    if moved <= 0.0 {
        held.remove_all(amount);
    }
    if let Some(mut r) = world.get_mut::<Reagents>(item) {
        r.0 = held;
    }
}

/// `handle_reagents`: one drag.
fn drag(world: &mut World, cig: Entity) {
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let dm = smokable_dm(world, cig);
    if let Some(mut r) = world.get_mut::<Reagents>(cig) {
        if r.0.total_volume <= 0.0 {
            return;
        }
        r.0.expose_temperature(&chem, dm.heat, 0.05);
    }
    let total = world.get::<Reagents>(cig).map_or(0.0, |r| r.0.total_volume);
    if total <= 0.0 {
        return;
    }
    let left = world.get::<Smokable>(cig).map_or(0.0, |s| s.smoketime);
    let to_smoke = drag_amount(dm.smoke_all, total, dm.dragtime, left);
    let smoker = holder_of(world, cig).filter(|&m| in_mask(world, m, cig));
    let Some(smoker) = smoker else {
        if let Some(mut r) = world.get_mut::<Reagents>(cig) {
            r.0.remove_all(to_smoke);
        }
        return;
    };
    inhale(world, cig, smoker, to_smoke);
    let resist = if world
        .get::<crate::quirks::Quirks>(smoker)
        .is_some_and(|q| q.has(crate::quirks::Quirk::Smoker))
    {
        0.5
    } else {
        1.0
    };
    crate::medical::organs::adjust_organ_loss(
        world,
        smoker,
        crate::medical::organs::Slot::Lungs,
        dm.lung_harm * resist,
    );
}

/// `fire_act` of a cigarette (it lights) or a match (it strikes). True when `thing` is one.
pub fn fire_act(world: &mut World, thing: Entity) -> bool {
    if world.get::<Match>(thing).is_some() {
        match_ignite(world, thing);
        return true;
    }
    if world.get::<Smokable>(thing).is_some() {
        light(world, thing, None);
        return true;
    }
    false
}

/// A held smokable dipped into a cup (`interact_with_atom`).
fn dip(world: &mut World, mob: Entity, cig: Entity, cup: Entity) -> bool {
    let path = path_of(world, cup);
    if world.get::<Smokable>(cig).is_none_or(|s| s.lit)
        || !is_under(&path, CUP_ROOT)
        || is_under(&path, MORTAR)
    {
        return false;
    }
    let volume = smokable_dm(world, cig).chem_volume;
    let (what, glass) = (name_of(world, cig), name_of(world, cup));
    let empty = world
        .get::<Reagents>(cup)
        .is_none_or(|r| r.0.total_volume <= 0.0);
    if crate::chem::transfer(world, cup, cig, volume) > 0.0 {
        tell_mob(
            world,
            mob,
            Span::Notice,
            &format!("Вы окунаете {what} в {glass}."),
        );
    } else if empty {
        tell_mob(world, mob, Span::Warning, &format!("{glass} пуст!"));
    } else {
        tell_mob(world, mob, Span::Warning, &format!("{what} полна!"));
    }
    true
}

/// `/obj/item/cigarette/pipe/item_interaction`: a dried plant or a drug packed in.
fn pack_pipe(world: &mut World, mob: Entity, pipe: Entity, item: Entity) -> bool {
    let path = path_of(world, item);
    let grown = is_under(&path, GROWN_ROOT);
    if !is_pipe(world, pipe) || !(grown || is_under(&path, DRUG_FOOD_ROOT)) {
        return false;
    }
    if world
        .get::<Smokable>(pipe)
        .is_some_and(|s| s.packed.is_some())
    {
        tell_mob(world, mob, Span::Warning, "Уже набита!");
        return true;
    }
    if grown && world.get::<crate::food::Dried>(item).is_none() {
        tell_mob(world, mob, Span::Warning, "Сначала это нужно высушить!");
        return true;
    }
    let (what, pipe_name) = (name_of(world, item), name_of(world, pipe));
    tell_mob(
        world,
        mob,
        Span::Notice,
        &format!("Вы набиваете {what} в {pipe_name}."),
    );
    if let Some(mut s) = world.get_mut::<Smokable>(pipe) {
        s.smoketime = PIPE_SMOKETIME;
        s.packed = Some(what);
    }
    let volume = world
        .get::<Reagents>(item)
        .map_or(0.0, |r| r.0.total_volume);
    crate::chem::transfer(world, item, pipe, volume);
    world.despawn(item);
    true
}

/// `/obj/item/cigarette/pipe/attack_self` of a packed, unlit pipe: emptied into ash.
fn empty_pipe(world: &mut World, mob: Entity, pipe: Entity) -> bool {
    if !is_pipe(world, pipe)
        || world
            .get::<Smokable>(pipe)
            .is_none_or(|s| s.lit || s.packed.is_none())
    {
        return false;
    }
    let name = name_of(world, pipe);
    tell_mob(world, mob, Span::Notice, &format!("Вы высыпаете {name}."));
    if let Some(pos) = map_pos(world, pipe) {
        crate::construction::spawn_obj(world, ASH, pos);
    }
    if let Some(mut s) = world.get_mut::<Smokable>(pipe) {
        s.packed = None;
        s.smoketime = 0.0;
    }
    if let Some(mut r) = world.get_mut::<Reagents>(pipe) {
        r.0.clear();
    }
    true
}

/// `/datum/component/ingredients_holder` of rolling paper: a dryable plant makes a rollie with
/// its reagents.
fn roll(world: &mut World, mob: Entity, paper: Entity, plant: Entity) -> bool {
    if !is_under(&path_of(world, paper), ROLLING_PAPER) {
        return false;
    }
    if world
        .get::<DmFood>(plant)
        .is_none_or(|f| f.dry_result.is_empty())
    {
        crate::balloon::balloon_alert(world, mob, paper, "не подходит!");
        return true;
    }
    let Some(pos) = map_pos(world, paper) else {
        return false;
    };
    let Some(rollie) = crate::construction::spawn_obj(world, ROLLIE, pos) else {
        return false;
    };
    let volume = world
        .get::<Reagents>(plant)
        .map_or(0.0, |r| r.0.total_volume);
    // `maximum_volume += ingredient.reagents.maximum_volume`.
    let extra = world
        .get::<Reagents>(plant)
        .map_or(0.0, |r| r.0.maximum_volume);
    if let Some(mut r) = world.get_mut::<Reagents>(rollie) {
        r.0.maximum_volume += extra;
    }
    crate::chem::transfer(world, plant, rollie, volume);
    let (what, paper_name) = (name_of(world, plant), name_of(world, paper));
    tell_mob(
        world,
        mob,
        Span::Notice,
        &format!("Вы заворачиваете {what} в {paper_name}."),
    );
    let paper_in_hand = in_hands(world, mob, paper);
    world.despawn(plant);
    world.despawn(paper);
    if paper_in_hand && let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, rollie);
    }
    true
}

// --- Vapes ------------------------------------------------------------------------------------

fn init_vape(world: &mut World, e: Entity) {
    if world.get_entity(e).is_err() || world.get::<Vape>(e).is_some() {
        return;
    }
    let mut holder = Holder::new(
        VAPE_VOLUME,
        flags::NO_REACT,
        Owner {
            path: path_of(world, e),
            living: false,
        },
    );
    if let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone())
        && let Some(id) = chem.id(NICOTINE)
    {
        holder.add_reagent(
            &chem,
            id,
            VAPE_NICOTINE,
            ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
            None,
            None,
            true,
            false,
        );
    }
    world
        .entity_mut(e)
        .insert((Vape::default(), Reagents(holder)));
}

fn set_container_flags(world: &mut World, item: Entity, set: u32, on: bool) {
    if let Some(mut r) = world.get_mut::<Reagents>(item) {
        if on {
            r.0.flags |= set;
        } else {
            r.0.flags &= !set;
        }
    }
}

/// `screwdriver_act`, `multitool_act`, `emag_act` of a vape. True when taken.
fn vape_tool(world: &mut World, mob: Entity, held: Entity, vape: Entity) -> bool {
    let Some(state) = world.get::<Vape>(vape).cloned() else {
        return false;
    };
    let name = name_of(world, vape);
    if crate::emag::is_emag(world, held) {
        if !state.screw {
            crate::balloon::balloon_alert(world, mob, vape, "сначала откройте крышку!");
        } else if state.emagged {
            crate::balloon::balloon_alert(world, mob, vape, "уже взломан!");
        } else {
            if let Some(mut v) = world.get_mut::<Vape>(vape) {
                v.emagged = true;
                v.super_mode = false;
            }
            crate::balloon::balloon_alert(world, mob, vape, "напряжение максимальное");
            if let Some(pos) = map_pos(world, vape) {
                crate::effects::particles::do_sparks(world, 5, true, pos);
            }
        }
        return true;
    }
    let behaviour = world
        .get::<DmTool>(held)
        .map(|t| t.behaviour.clone())
        .unwrap_or_default();
    match behaviour.as_str() {
        "screwdriver" => {
            let open = !state.screw;
            if let Some(mut v) = world.get_mut::<Vape>(vape) {
                v.screw = open;
            }
            set_container_flags(world, vape, flags::OPENCONTAINER, open);
            let text = if open {
                format!("Вы открываете крышку {name}.")
            } else {
                format!("Вы закрываете крышку {name}.")
            };
            tell_mob(world, mob, Span::Notice, &text);
            true
        }
        "multitool" => {
            if state.screw && !state.emagged {
                if let Some(mut v) = world.get_mut::<Vape>(vape) {
                    v.super_mode = !state.super_mode;
                }
                let text = if state.super_mode {
                    format!("Вы понижаете напряжение {name}.")
                } else {
                    format!("Вы повышаете напряжение {name}.")
                };
                tell_mob(world, mob, Span::Notice, &text);
            } else if state.screw {
                tell_mob(
                    world,
                    mob,
                    Span::Warning,
                    &format!("{name} нельзя изменить!"),
                );
            }
            true
        }
        _ => false,
    }
}

/// `/obj/item/vape/attack_self`: an open one is emptied.
fn vape_attack_self(world: &mut World, mob: Entity, vape: Entity) -> bool {
    let Some(state) = world.get::<Vape>(vape).cloned() else {
        return false;
    };
    if !state.screw {
        crate::balloon::balloon_alert(world, mob, vape, "сначала откройте крышку!");
        return true;
    }
    if world
        .get::<Reagents>(vape)
        .is_some_and(|r| r.0.total_volume > 0.0)
    {
        let name = name_of(world, vape);
        tell_mob(world, mob, Span::Notice, &format!("Вы опустошаете {name}."));
        if let Some(mut r) = world.get_mut::<Reagents>(vape) {
            r.0.clear();
        }
    }
    true
}

/// `equipped`/`dropped` on the mask slot, `process`.
fn vape_tick(world: &mut World, vape: Entity, now: f64) {
    let Some(state) = world.get::<Vape>(vape).cloned() else {
        return;
    };
    let wearer = holder_of(world, vape).filter(|&m| in_mask(world, m, vape));
    let Some(mob) = wearer else {
        // `dropped` from the mask: no reactions, no puffing.
        if world.get::<VapeWorn>(vape).is_some() {
            world.entity_mut(vape).remove::<VapeWorn>();
        }
        if state.puffing {
            set_container_flags(world, vape, flags::NO_REACT, true);
            if let Some(mut v) = world.get_mut::<Vape>(vape) {
                v.puffing = false;
            }
        }
        return;
    };
    // `equipped(ITEM_SLOT_MASK)`: once per putting it on.
    if world.get::<VapeWorn>(vape).is_none() {
        world.entity_mut(vape).insert(VapeWorn);
        if state.screw {
            tell_mob(world, mob, Span::Warning, "Сначала закройте крышку!");
            return;
        }
        tell_mob(world, mob, Span::Notice, "Вы начинаете парить.");
        set_container_flags(world, vape, flags::NO_REACT, false);
        if let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone())
            && let Some(mut r) = world.get_mut::<Reagents>(vape)
        {
            r.0.handle_reactions(&chem);
        }
        if let Some(mut v) = world.get_mut::<Vape>(vape) {
            v.puffing = true;
        }
        return;
    }
    if !state.puffing {
        return;
    }
    crate::fire::ignite_mob(world, mob);
    if world
        .get::<Reagents>(vape)
        .is_none_or(|r| r.0.total_volume <= 0.0)
    {
        let name = name_of(world, vape);
        tell_mob(world, mob, Span::Warning, &format!("{name} пуст!"));
        if let Some(mut v) = world.get_mut::<Vape>(vape) {
            v.puffing = false;
        }
        return;
    }
    if now < state.next_drag {
        return;
    }
    if let Some(mut v) = world.get_mut::<Vape>(vape) {
        v.next_drag = now + VAPE_DRAGTIME;
    }
    let range = if state.emagged {
        Some(4)
    } else if state.super_mode {
        Some(1)
    } else {
        None
    };
    if let Some(range) = range {
        let amount = crate::effects::smoke::diamond_area(range);
        let carry = world.get::<Reagents>(vape).map(|r| {
            let mut copy = r.0.clone();
            let total = copy.total_volume;
            if total > VAPE_SMOKE_CARRY {
                copy.remove_all(total - VAPE_SMOKE_CARRY);
            }
            copy
        });
        if let (Some(carry), Some(pos)) = (carry, map_pos(world, vape)) {
            // shortcut: tg's `smoke_machine` smoke is see-through; this is the plain chemical one.
            crate::effects::smoke::do_chem_smoke(
                world,
                range,
                pos,
                Some(amount),
                &carry,
                crate::effects::smoke::SmokeKind::Chem,
            );
        }
        if let Some(mut r) = world.get_mut::<Reagents>(vape) {
            r.0.remove_all(amount as f32 / 24.0);
        }
        if state.emagged && world.resource_mut::<CombatRng>().chance(5.0) {
            sound_at(world, vape, "sound/effects/pop_expl.ogg", 50.0, false);
            crate::mobs::defense::hurt(world, mob, DamageType::Burn, 20.0, Some(BodyZone::Head));
            crate::status_effects::apply(world, mob, crate::status_effects::Incap::Paralyze, 30.0);
            if let Some(pos) = map_pos(world, vape) {
                crate::effects::particles::do_sparks(world, 5, true, pos);
            }
            let name = name_of(world, vape);
            tell_mob(
                world,
                mob,
                Span::BoldWarning,
                &format!("{name} внезапно взрывается у вас во рту!"),
            );
            world.despawn(vape);
            return;
        }
    }
    // `handle_reagents`.
    if reagent_amount(world, vape, FUEL) > 0.0 {
        crate::fire::adjust_fire_stacks(world, mob, 2.0);
        crate::fire::ignite_mob(world, mob);
    }
    inhale(world, vape, mob, REAGENTS_METABOLISM);
}

/// On a vape whose `equipped(ITEM_SLOT_MASK)` ran for this wearing.
#[derive(Component, Clone, Copy, Debug)]
struct VapeWorn;

// --- Clicks -----------------------------------------------------------------------------------

/// `attack_self` of lighters, the battery lighter, smokables and vapes.
fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if lighter_attack_self(world, mob, item) {
        return true;
    }
    if world.get::<Match>(item).is_some() {
        if !is_under(&path_of(world, item), BATTERY_LIGHTER)
            || world
                .get::<Match>(item)
                .is_some_and(|m| m.state != MatchState::Unlit)
        {
            return false;
        }
        let actor = Actor {
            client,
            mob,
            item: Some(item),
        };
        // The target is the user: the held lighter is not on a tile.
        do_after(
            world,
            actor,
            mob,
            BATTERY_STRIKE_DS,
            0,
            crate::construction::Job::Smoking(SmokingJob::BatteryStrike),
        );
        return true;
    }
    if world.get::<Smokable>(item).is_some() {
        if empty_pipe(world, mob, item) {
            return true;
        }
        if world.get::<Smokable>(item).is_some_and(|s| s.lit) {
            put_out(world, item, Some(mob), true);
            return true;
        }
        return false;
    }
    vape_attack_self(world, mob, item)
}

/// `held` used on `target` that is an item (in the other hand or in the world).
fn item_on_item(
    world: &mut World,
    _client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<Vape>(target).is_some() && vape_tool(world, mob, held, target) {
        return true;
    }
    if strike_match(world, mob, held, target) {
        return true;
    }
    if world.get::<Smokable>(target).is_some() {
        if pack_pipe(world, mob, target, held) {
            return true;
        }
        if attempt_light(world, mob, target, held, None) {
            return true;
        }
    }
    if world.get::<DmFood>(held).is_some() && roll(world, mob, target, held) {
        return true;
    }
    world.get::<Smokable>(held).is_some() && dip(world, mob, held, target)
}

/// `held` on a world target: items as [`item_on_item`]; on a mob, a lit lighter or match sets
/// one with fire stacks alight, an unlit cigarette lights from a burning carbon, a lit one
/// lights the cigarette in their mouth (`help_light_cig`, out of combat mode). The hit itself
/// is the attack chain's.
fn held_on_target(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<crate::health::Health>(target).is_none() {
        return item_on_item(world, client, mob, held, target);
    }
    let lighter_lit =
        world.get::<Lighter>(held).is_some() && world.get::<Welder>(held).is_some_and(|w| w.on);
    if lighter_lit {
        if let Some(mut w) = world.get_mut::<Welder>(held) {
            w.use_fuel(LIGHTER_HIT_FUEL);
        }
        refresh_lighter(world, held);
        crate::fire::ignite_mob(world, target);
    }
    if world
        .get::<Match>(held)
        .is_some_and(|m| m.state == MatchState::Lit)
    {
        crate::fire::ignite_mob(world, target);
    }
    if world.get::<Smokable>(held).is_some_and(|s| !s.lit) && crate::fire::on_fire(world, target) {
        let (user, what, victim) = (
            name_of(world, mob),
            name_of(world, held),
            name_of(world, target),
        );
        light(
            world,
            held,
            Some(format!(
                "{user} прикуривает {what} от горящего тела {victim}. Какой хладнокровный тип."
            )),
        );
        return true;
    }
    let lit = lighter_lit
        || world
            .get::<Match>(held)
            .is_some_and(|m| m.state == MatchState::Lit)
        || world.get::<Smokable>(held).is_some_and(|s| s.lit);
    let combat = world.get::<CombatMode>(mob).is_some_and(|c| c.0);
    let Some(cig) = world
        .get::<Inventory>(target)
        .and_then(|i| i.mask)
        .filter(|&m| world.get::<Smokable>(m).is_some())
    else {
        return false;
    };
    if !lit || combat {
        return false;
    }
    if world.get::<Smokable>(cig).is_some_and(|s| s.lit) {
        let what = name_of(world, cig);
        tell_mob(world, mob, Span::Warning, &format!("{what} уже зажжена!"));
        return true;
    }
    let text = if target == mob {
        None
    } else {
        Some(format!(
            "{} подносит {} к {} и поджигает {}.",
            name_of(world, mob),
            name_of(world, held),
            name_of(world, target),
            name_of(world, cig)
        ))
    };
    attempt_light(world, mob, cig, held, text);
    true
}

// --- Processing -------------------------------------------------------------------------------

/// `SSobj` `process` of lit matches and smokables and worn vapes, once a second.
// Cost: one pass over the matches, smokables and vapes (hundreds on a map) per second.
fn process(world: &mut World) {
    let delta = world.resource::<Time>().delta_secs_f64();
    {
        let mut clock = world.resource_mut::<SmokingClock>();
        clock.0 += delta;
        if clock.0 < TICK_SECS {
            return;
        }
        clock.0 -= TICK_SECS;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let dt = TICK_SECS as f32;
    let matches: Vec<Entity> = world
        .query::<(Entity, &Match)>()
        .iter(world)
        .filter(|(_, m)| m.state == MatchState::Lit)
        .map(|(e, _)| e)
        .collect();
    for item in matches {
        // `dropped` -> `matchburnout`: out of the hand it goes out.
        let held = holder_of(world, item).is_some_and(|m| in_hands(world, m, item));
        if !held {
            match_burnout(world, item);
            continue;
        }
        let left = match world.get_mut::<Match>(item) {
            Some(mut m) => {
                m.smoketime -= dt;
                m.smoketime
            }
            None => continue,
        };
        if left <= 0.0 {
            match_burnout(world, item);
        } else {
            open_flame(world, item, MATCH_HEAT);
        }
    }
    let smokables: Vec<Entity> = world
        .query::<(Entity, &Smokable)>()
        .iter(world)
        .filter(|(_, s)| s.lit)
        .map(|(e, _)| e)
        .collect();
    for cig in smokables {
        smokable_tick(world, cig, now, dt);
    }
    let vapes: Vec<Entity> = world
        .query_filtered::<Entity, With<Vape>>()
        .iter(world)
        .collect();
    for vape in vapes {
        if world.get_entity(vape).is_ok() {
            vape_tick(world, vape, now);
        }
    }
}

/// `open_flame(heat)`: the item's turf (or its holder's) exposed to a hotspot.
fn open_flame(world: &mut World, item: Entity, heat: f32) {
    if let Some(pos) = crate::fire::flame_turf(world, item)
        && let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>()
    {
        atmos.hotspot_expose(pos, f64::from(heat), FLAME_VOLUME, false);
    }
}

/// `/obj/item/cigarette/process`.
fn smokable_tick(world: &mut World, cig: Entity, now: f64, dt: f32) {
    if world.get_entity(cig).is_err() {
        return;
    }
    let user = holder_of(world, cig);
    if let Some(user) = user {
        crate::fire::ignite_mob(world, user);
    }
    if !check_oxygen(world, cig) {
        extinguish(world, cig);
        return;
    }
    let left = match world.get_mut::<Smokable>(cig) {
        Some(mut s) => {
            s.smoketime -= dt;
            s.smoketime
        }
        None => return,
    };
    if left <= 0.0 {
        put_out(world, cig, user, false);
        return;
    }
    let dm = smokable_dm(world, cig);
    open_flame(world, cig, dm.heat);
    let due = world
        .get::<Smokable>(cig)
        .is_some_and(|s| now >= s.next_drag);
    let has = world
        .get::<Reagents>(cig)
        .is_some_and(|r| r.0.total_volume > 0.0);
    if due && has {
        if let Some(mut s) = world.get_mut::<Smokable>(cig) {
            s.next_drag = now + f64::from(dm.dragtime);
        }
        drag(world, cig);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lighters_by_type() {
        let zippo = lighter_spec("/obj/item/lighter").unwrap();
        assert_eq!(
            (zippo.max_fuel, zippo.heat, zippo.fancy),
            (6.0, 1400.0, true)
        );
        let cheap = lighter_spec("/obj/item/lighter/greyscale").unwrap();
        assert_eq!(
            (cheap.max_fuel, cheap.fancy, cheap.full),
            (3.0, false, true)
        );
        assert!(
            !lighter_spec("/obj/item/lighter/greyscale/empty")
                .unwrap()
                .full
        );
        assert_eq!(
            lighter_spec("/obj/item/lighter/slime").unwrap().heat,
            2400.0
        );
        // Under `HIGH_TEMPERATURE_REQUIRED`: a zippo cannot slice walls, the slime one can.
        assert!(zippo.heat < crate::tools::HIGH_TEMPERATURE_REQUIRED);
        assert!(SLIME_HEAT >= crate::tools::HIGH_TEMPERATURE_REQUIRED);
        assert!(lighter_spec("/obj/item/lighterfluid").is_none());
    }

    #[test]
    fn drags_smoke_everything_by_the_end_with_smoke_all() {
        assert_eq!(drag_amount(false, 30.0, 10.0, 360.0), REAGENTS_METABOLISM);
        // 18 u over 240 s in 10 s drags: 0.75 u now, all of it on the last drag.
        assert_eq!(drag_amount(true, 18.0, 10.0, 240.0), 0.75);
        assert_eq!(drag_amount(true, 5.0, 10.0, 10.0), 5.0);
    }
}

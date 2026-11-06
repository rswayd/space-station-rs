//! The nightmare after tg `/datum/antagonist/nightmare` (`nightmare.dm`), its species
//! (`/datum/species/shadow/nightmare`), organs (`nightmare_organs.dm`: the tumorous brain, the
//! heart of darkness), the light eater (`nightmare_equipment.dm`, `/datum/element/light_eater`),
//! the shadow walk (`/datum/action/cooldown/spell/jaunt/shadow_walk`) and the "Nightmare" light
//! midround (`/datum/dynamic_ruleset/midround/from_ghosts/nightmare`).
//!
//! - **Selection** ([`can_spawn`], [`spawn_nightmares`], from `super::midround`): ghosts who want
//!   [`ROLE_NIGHTMARE`]; the body appears on a dark maintenance landmark
//!   (`find_maintenance_spawn(require_darkness = TRUE)`: lumcount at most 0.2) with the
//!   `ethereal_exit` sound.
//! - **The body** ([`make_nightmare`]): the nightmare species (`crate::species`: shadow limbs,
//!   no breath, hunger, blood or dismemberment, naked: `no_equip_flags`), a name from
//!   `strings/names/nightmare.txt`, the light eater in hand (the heart's `on_mob_insert`), the
//!   shadow walk button (the brain's), no station job, the fluff objective, the greeting and
//!   tg's default `AntagInfoGeneric` panel.
//! - **Light** ([`nightmare_life`], once a second): the shadow bodyparts' `nyxosynthesis` (1
//!   brute and 1 burn per second on a lit tile, 0.5 of each healed on a dark one), the brain's
//!   "Lightless Domain" on a dark tile (projectiles miss: [`dodge_projectile`]), the heart of
//!   darkness reviving its dead owner after 80 seconds in the dark (`HEART_RESPAWN_THRESHHOLD`).
//! - **Shadow walk** ([`toggle_shadow_walk`]): only on a dark tile; the mob jaunts
//!   (`crate::spells::Jaunting`, no time limit) through walls, heals 1.5 brute and burn per
//!   second, cannot enter space, is warned once before stepping into light (a second step within
//!   0.75-1 s goes through) and is thrown out by light (`phased_mob/shadow`).
//! - **The light eater** ([`item_interaction`]): a hit puts out the lights of what it hits (a
//!   light fixture breaks its tube; anything else with its own light loses it for good,
//!   [`LightEaten`]); after 7 s of shadow walking its next hit on a mob is critical (a carbon or
//!   cyborg is paralyzed, others take the force again, a corpse is gored).
//! - [`roundend_report`]: each nightmare and its objective.
// shortcut: no ghost poll (willing ghosts are taken as the other ghost rulesets do), no
// `is_safe_turf` check; the terrorize spell and its terror buildup, the light eater's butchering,
// mining tool behaviour and crit glow, the heart's eating and transplant behaviour (a removed heart
// only takes the blade away), the mood modifier and the shadow jaunt's purple look are not
// ported; a carried light (a PDA's) goes out on the holder until it is switched again (the
// item is not eaten for good); the brain's shadow walk stays with the antagonist, not the organ.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::spawner::Rng;
use ss13_net::chat::server::SendChat;
use ss13_net::server::{ClientToken, Networked};
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::{Appearance, MouseOpacity, Optics, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::dynamic::DynamicRng;
use super::objectives::RoundEnd;
use super::{AntagHudData, AntagHudUi, BrotherInfo, ObjectiveInfo};
use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::click_handlers::ClickCtx;
use crate::health::{DamageType, Health, Stat, apply_damage};
use crate::inventory::{CarriedBy, Inventory, NoDrop};
use crate::medical::organs::{self, Slot};
use crate::server::{PlayerMob, SpawnPoints};
use crate::species::SpeciesId;
use crate::spells::Jaunting;
use crate::tools::tell;

pub use super::ROLE_NIGHTMARE;
/// `name` ("\improper Nightmare").
pub const ANTAG_NAME: &str = "Кошмар";
const ROUNDEND_CATEGORY: &str = "Кошмары";
pub const LIGHT_EATER: &str = "/obj/item/light_eater";
pub const NIGHTMARE_HEART: &str = "/obj/item/organ/heart/nightmare";
pub const NIGHTMARE_BRAIN: &str = "/obj/item/organ/brain/shadow/nightmare";
/// `SHADOW_SPECIES_LIGHT_THRESHOLD`, also `UNLIT_AREA_BRIGHTNESS` of the spawn.
pub const LIGHT_THRESHOLD: f32 = 0.2;
/// `HEART_RESPAWN_THRESHHOLD`.
pub const HEART_RESPAWN_SECS: f64 = 80.0;
/// `/datum/status_effect/shadow` `duration`: one status tick longer than a life tick.
const LIGHTLESS_SECS: f64 = 2.2;
/// `/obj/effect/dummy/phased_mob/shadow` `healing_rate`, per second.
const JAUNT_HEALING: f32 = 1.5;
/// `nyxosynthesis` per second: light damage, dark healing (a whole shadow body).
const LIGHT_DAMAGE: f32 = 1.0;
const DARK_HEALING: f32 = 0.5;
/// `light_step_cooldown`, `reactivate_light_alert` timer.
const LIGHT_STEP_COOLDOWN: f64 = 0.75;
const LIGHT_ALERT_RESET: f64 = 1.0;
/// The light eater's `prepare_crit_timer`.
const CRIT_DELAY: f64 = 7.0;
/// `CLICK_CD_RAPID`.
const CLICK_CD_RAPID: f64 = 0.2;
const SHADOW_WALK_ID: &str = "nightmare:shadow_walk";
const MINOR_ANTAG_DMI: &str = "icons/mob/actions/actions_minor_antag.dmi";
const BACKGROUNDS_DMI: &str = "icons/mob/actions/backgrounds.dmi";
const POOF: &str = "sound/effects/nightmare_poof.ogg";
const REAPPEAR: &str = "sound/effects/nightmare_reappear.ogg";
const ARRIVE: &str = "sound/effects/magic/ethereal_exit.ogg";
const HEARTBEAT: &str = "sound/effects/singlebeat.ogg";
const RISE: &str = "sound/effects/hallucinations/far_noise.ogg";
const GORE: &str = "sound/effects/wounds/crackandbleed.ogg";

/// `strings/names/nightmare.txt` (`GLOB.nightmare_names`).
const NIGHTMARE_NAMES: [&str; 10] = [
    "Амерзиокс",
    "Гург-милин",
    "Хел-уае",
    "Канет'прунанс",
    "Мии'махза",
    "Некс",
    "Ноаэй'гиф",
    "У'руан",
    "Вигистаезиан",
    "Ю'схеж",
];

/// `/datum/objective/nightmare_fluff` `explanation_texts`.
const FLUFF: [&str; 7] = [
    "Consume the last glimmer of light from the space station.",
    "Bring judgment upon the daywalkers.",
    "Extinguish the flame of this hellscape.",
    "Reveal the true nature of the shadows.",
    "From the shadows, all shall perish.",
    "Conjure nightfall by blade or by flame.",
    "Bring the darkness to the light.",
];

/// On the nightmare's body (`/datum/antagonist/nightmare`).
#[derive(Component, Clone, Debug)]
pub struct Nightmare {
    /// The player's key, for the round-end report.
    pub key: String,
    /// The fluff objective (done while the nightmare lives).
    pub objective: String,
}

impl Nightmare {
    fn info(&self) -> BrotherInfo {
        BrotherInfo {
            antag_name: ANTAG_NAME.to_owned(),
            objectives: vec![ObjectiveInfo {
                count: 1,
                explanation: self.objective.clone(),
                complete: false,
            }],
        }
    }
}

/// The heart of darkness' state on its owner: `respawn_progress` and the granted `blade`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct HeartOfDarkness {
    pub respawn_progress: f64,
    pub blade: Option<Entity>,
}

/// `/datum/status_effect/shadow/nightmare` ("Lightless Domain"): ends at this game time.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct LightlessDomain(pub f64);

/// A shadow walk in progress (with [`Jaunting`]): the light step warning's state.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct ShadowWalking {
    pub light_alert_given: bool,
    /// `light_step_cooldown`: steps into light are refused until then.
    pub step_cooldown_until: f64,
    /// `reactivate_light_alert`.
    pub alert_reset_at: f64,
}

/// The light eater of `owner`; `crit_at` is the pending crit timer.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct LightEater {
    pub owner: Entity,
    pub crit_at: Option<f64>,
    pub has_crit: bool,
}

/// `/datum/element/light_eaten`: it never lights again.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct LightEaten;

/// The nightmare's info panel (tg's default `AntagInfoGeneric`).
pub struct NightmareInfoWindow;

impl UiInterface for NightmareInfoWindow {
    const NAME: &'static str = crate::antag::cult::AntagInfoGenericUi::NAME;
    type Data = ();
    type StaticData = BrotherInfo;
}

impl ServerUi for NightmareInfoWindow {
    type Source = &'static Nightmare;
    type UserParam = ();
    type Act = ();

    fn static_data(nightmare: &Nightmare) -> BrotherInfo {
        nightmare.info()
    }

    fn data(_: &Nightmare, _: Entity, _: &()) {}

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

pub struct NightmarePlugin;

impl Plugin for NightmarePlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.register_ui::<NightmareInfoWindow>()
            .add_click_handler(ClickPhase::HeldOnTarget, 1410, item_interaction)
            .add_systems(
                Update,
                (open_info, trigger_buttons).in_set(UiSystems::Handle),
            )
            .add_systems(
                Update,
                (nightmare_life, shadow_light, tick_blades, keep_eaten)
                    .before(crate::health::HealthSystems),
            );
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn with_rng<R>(world: &mut World, f: impl FnOnce(&mut Rng) -> R) -> R {
    if !world.contains_resource::<DynamicRng>() {
        world.insert_resource(DynamicRng(Rng::new(0)));
    }
    f(&mut world.resource_mut::<DynamicRng>().0)
}

/// `check_lumcount_below(threshold)`: a tile without a lighting object is never dark.
pub fn is_dark(lumcount: Option<f32>, threshold: f32) -> bool {
    lumcount.is_some_and(|l| l < threshold)
}

/// `check_lumcount_above(threshold)`: a tile without a lighting object is always lit.
pub fn is_lit(lumcount: Option<f32>, threshold: f32) -> bool {
    lumcount.is_none_or(|l| l > threshold)
}

fn dark_at(world: &World, pos: TilePos) -> bool {
    is_dark(crate::botany::lumcount(world, pos), LIGHT_THRESHOLD)
}

fn lit_at(world: &World, pos: TilePos) -> bool {
    is_lit(crate::botany::lumcount(world, pos), LIGHT_THRESHOLD)
}

fn play(world: &mut World, pos: TilePos, sound: &str, vol: f32) {
    crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, vol, true));
}

/// tg `visible_message(others, self)` around `mob`.
fn visible(world: &mut World, mob: Entity, span: Span, others: String, own: Option<String>) {
    let Some(pos) = crate::explosions::turf_of(world, mob) else {
        return;
    };
    let personal: Vec<(Entity, Span, String)> = own.map(|t| (mob, span, t)).into_iter().collect();
    for m in
        crate::pull::visible_message(world, pos, ChatChannel::Warning, (span, others), &personal)
    {
        world.write_message(m);
    }
}

fn name_of(world: &World, e: Entity) -> String {
    crate::medical::name_of(world, e)
}

// ---------------------------------------------------------------------------------------------
// Selection and the body

/// `find_maintenance_spawn(require_darkness = TRUE)`'s candidates.
fn dark_spawns(world: &World) -> Vec<TilePos> {
    world
        .get_resource::<SpawnPoints>()
        .map(|s| s.maintenance.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|&p| !is_lit(crate::botany::lumcount(world, p), LIGHT_THRESHOLD))
        .collect()
}

/// `can_be_selected`: a dark maintenance spawn.
pub fn can_spawn(world: &mut World) -> bool {
    !dark_spawns(world).is_empty()
}

/// The ruleset's `assign_role` for each of `clients` (their ghosts' clients): the bodies.
pub fn spawn_nightmares(world: &mut World, clients: &[Entity]) -> Vec<Entity> {
    let mut out = Vec::new();
    for &client in clients {
        let spawns = dark_spawns(world);
        if spawns.is_empty() {
            break;
        }
        let i = with_rng(world, |rng| rng.range(0, spawns.len() as i64 - 1)) as usize;
        let pos = spawns[i];
        let mob = crate::server::spawn_mob(world, client, pos, String::new());
        world.entity_mut(client).remove::<crate::round::NewPlayer>();
        make_nightmare(world, mob);
        play(world, pos, ARRIVE, 50.0);
        info!("{mob} ({client}) is a nightmare at {pos:?}");
        out.push(mob);
    }
    out
}

/// `add_antag_datum(/datum/antagonist/nightmare)` + `set_species(/datum/species/shadow/nightmare)`:
/// the species and name, the light eater, the shadow walk, the antagonist. `false` if `mob`
/// already is one.
pub fn make_nightmare(world: &mut World, mob: Entity) -> bool {
    if world.get::<Nightmare>(mob).is_some() || world.get_entity(mob).is_err() {
        return false;
    }
    crate::species::set_species(world, mob, SpeciesId::Nightmare);
    // `on_species_gain`: `fully_replace_character_name(null, pick(GLOB.nightmare_names))`.
    let i = with_rng(world, |rng| rng.range(0, NIGHTMARE_NAMES.len() as i64 - 1)) as usize;
    world
        .entity_mut(mob)
        .insert(Name::new(NIGHTMARE_NAMES[i].to_owned()));
    // `no_equip_flags`: nothing worn; the body is drawn naked.
    if let Some(mut look) = world.get_mut::<crate::human::HumanLook>(mob) {
        look.uniform = None;
        look.shoes = None;
    }
    if world.get::<Inventory>(mob).is_none() {
        world.entity_mut(mob).insert(Inventory::default());
    }
    // The heart's `on_mob_insert`: `put_in_hands(new /obj/item/light_eater)`.
    let blade = grant_blade(world, mob);
    world.entity_mut(mob).insert(HeartOfDarkness {
        respawn_progress: 0.0,
        blade,
    });
    crate::inventory::refresh_look(world, mob);
    // `on_gain`: `set_assigned_role(/datum/job/nightmare)`.
    world.entity_mut(mob).remove::<crate::jobs::PlayerJob>();
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let f = with_rng(world, |rng| rng.range(0, FLUFF.len() as i64 - 1)) as usize;
    let nightmare = Nightmare {
        key,
        objective: FLUFF[f].to_owned(),
    };
    world
        .entity_mut(mob)
        .insert((nightmare.clone(), AntagHudData::default()));
    refresh_buttons(world, mob);
    if let Some(client) = client {
        // `greet` + `announce_objectives`.
        tell(world, client, Span::Bold, format!("Вы - {ANTAG_NAME}."));
        tell(world, client, Span::Notice, "Ваши текущие задачи:");
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage {
                channel: ChatChannel::System,
                segments: vec![
                    ChatSegment::new(Span::Bold, "Задача #1"),
                    ChatSegment::new(Span::Plain, format!(": {}", nightmare.objective)),
                ],
            },
        });
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
        world.write_message(OpenUi::new::<NightmareInfoWindow>(
            client, mob, mob, ANTAG_NAME,
        ));
    }
    true
}

/// A new light eater in a free hand of `mob` (on the floor under it without one), stuck there.
fn grant_blade(world: &mut World, mob: Entity) -> Option<Entity> {
    let blade = crate::construction::spawn_detached(world, LIGHT_EATER)?;
    world.entity_mut(blade).insert(LightEater {
        owner: mob,
        crit_at: None,
        has_crit: false,
    });
    let hand = world.get::<Inventory>(mob).and_then(|i| {
        [i.active, 1 - i.active]
            .into_iter()
            .find(|&h| i.hands[h].is_none())
    });
    match hand {
        Some(h) => {
            world
                .entity_mut(blade)
                .insert((Networked, CarriedBy(mob), NoDrop));
            if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
                inventory.hands[h] = Some(blade);
            }
        }
        None => {
            let pos = world.get::<TilePos>(mob).copied().unwrap_or_default();
            world.entity_mut(blade).insert((Networked, pos));
            crate::construction::tile_changed(world, pos);
        }
    }
    Some(blade)
}

// ---------------------------------------------------------------------------------------------
// Buttons and the shadow walk

/// The shadow walk button, kept beside the other buttons of the mob.
fn refresh_buttons(world: &mut World, mob: Entity) {
    let icon = |world: &World, dmi: &str, state: &str| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(dmi, state))
    };
    let mut buttons: Vec<(ActionButton, f64)> = world
        .get::<ActionButtons>(mob)
        .map(|b| {
            b.0.iter()
                .filter(|(button, _)| button.id != SHADOW_WALK_ID)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    buttons.push((
        ActionButton {
            id: SHADOW_WALK_ID.to_owned(),
            name: "Shadow Walk".to_owned(),
            icon: icon(world, MINOR_ANTAG_DMI, "ninja_cloak"),
            background: icon(world, BACKGROUNDS_DMI, "bg_alien"),
            border: icon(world, BACKGROUNDS_DMI, "bg_alien_border"),
            available: true,
            cooldown: 0.0,
            active: world.get::<ShadowWalking>(mob).is_some(),
            maptext: None,
        },
        0.0,
    ));
    set_buttons(world, mob, buttons);
}

fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if id != SHADOW_WALK_ID
            || actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world.get::<Nightmare>(actor).is_none()
        {
            continue;
        }
        toggle_shadow_walk(world, client, actor);
    }
}

/// `shadow_walk/can_cast_spell` + `cast`: out of a shadow walk, or into one on a dark tile.
pub fn toggle_shadow_walk(world: &mut World, client: Entity, mob: Entity) {
    // `AB_CHECK_CONSCIOUS`.
    if world.get::<Stat>(mob) != Some(&Stat::Conscious) {
        return;
    }
    if world.get::<ShadowWalking>(mob).is_some() {
        exit_shadow_walk(world, mob, false);
        return;
    }
    // Another jaunt (an ethereal one) is not ours to end.
    if world.get::<Jaunting>(mob).is_some() {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    if !dark_at(world, pos) {
        tell(world, client, Span::Warning, "It isn't dark enough here!");
        return;
    }
    play(world, pos, POOF, 50.0);
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        Span::BoldWarning,
        format!("{user} melts into the shadows!"),
        None,
    );
    // `SetAllImmobility(0)`, `set_stamina_loss(0)`.
    if let Some(mut effects) = crate::status_effects::effects_mut(world, mob) {
        effects.set_all_immobility(0.0);
        effects.stamina = 0.0;
        effects.stamcrit = false;
    }
    let (color, mouse_opacity) = world
        .get::<Appearance>(mob)
        .map_or((None, MouseOpacity::Icon), |a| (a.color, a.mouse_opacity));
    let t = now(world);
    world.entity_mut(mob).insert((
        Jaunting {
            start: pos,
            until: f64::INFINITY,
            exit_points: Vec::new(),
            reappear_at: None,
            next_step: t,
            color,
            mouse_opacity,
        },
        ShadowWalking::default(),
    ));
    // The light eater's `COMSIG_MOB_ENTER_JAUNT`: the crit comes in 7 s.
    set_crit_timer(world, mob, Some(t + CRIT_DELAY));
    refresh_buttons(world, mob);
}

/// `phased_mob/shadow/eject_jaunter(forced_out)`: out where it is.
fn exit_shadow_walk(world: &mut World, mob: Entity, forced: bool) {
    let user = name_of(world, mob);
    let text = if forced {
        format!("{user} is revealed by the light!")
    } else {
        format!("{user} emerges from the darkness!")
    };
    visible(world, mob, Span::BoldWarning, text, None);
    if let Some(pos) = crate::explosions::turf_of(world, mob) {
        play(world, pos, REAPPEAR, 50.0);
    }
    world.entity_mut(mob).remove::<ShadowWalking>();
    crate::spells::effects::exit_jaunt(world, mob);
    // `COMSIG_MOB_AFTER_EXIT_JAUNT`: `stop_crit_timer`.
    set_crit_timer(world, mob, None);
    refresh_buttons(world, mob);
}

/// `phased_mob/shadow/phased_check` of a step into `to` (any jaunter may call it; only a shadow
/// walk is checked): no space, and a warning before the first step into light.
pub fn shadow_step_allowed(world: &mut World, mob: Entity, to: TilePos) -> bool {
    let Some(walk) = world.get::<ShadowWalking>(mob).copied() else {
        return true;
    };
    let space = crate::construction::turf_at(world, to)
        .and_then(|t| crate::construction::path_of(world, t))
        .is_some_and(|p| is_under(&p, "/turf/open/space"));
    if space {
        if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
            tell(
                world,
                client,
                Span::Warning,
                "It really would not be wise to go into space.",
            );
        }
        return false;
    }
    if !lit_at(world, to) {
        return true;
    }
    let t = now(world);
    let (allowed, next) = light_step_warning(walk, t);
    if !walk.light_alert_given
        && let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client)
    {
        tell(world, client, Span::Warning, "leaving the shadows...");
    }
    if let Some(mut w) = world.get_mut::<ShadowWalking>(mob) {
        *w = next;
    }
    allowed
}

/// `light_step_warning` at `t`: whether the step goes through, and the new state.
pub fn light_step_warning(walk: ShadowWalking, t: f64) -> (bool, ShadowWalking) {
    let walk = if walk.light_alert_given && t >= walk.alert_reset_at {
        ShadowWalking {
            light_alert_given: false,
            ..walk
        }
    } else {
        walk
    };
    if !walk.light_alert_given {
        return (
            false,
            ShadowWalking {
                light_alert_given: true,
                step_cooldown_until: t + LIGHT_STEP_COOLDOWN,
                alert_reset_at: t + LIGHT_ALERT_RESET,
            },
        );
    }
    if t < walk.step_cooldown_until {
        return (false, walk);
    }
    (
        true,
        ShadowWalking {
            light_alert_given: false,
            ..walk
        },
    )
}

/// `phased_mob/shadow/relaymove`: a shadow walker that moved into light is thrown out; one whose
/// jaunt ended otherwise (unconscious) loses the marker.
fn shadow_light(world: &mut World) {
    let walkers: Vec<(Entity, Option<TilePos>, bool)> = world
        .query_filtered::<(Entity, Option<&TilePos>, Has<Jaunting>), With<ShadowWalking>>()
        .iter(world)
        .map(|(e, p, j)| (e, p.copied(), j))
        .collect();
    for (mob, pos, jaunting) in walkers {
        if !jaunting {
            world.entity_mut(mob).remove::<ShadowWalking>();
            set_crit_timer(world, mob, None);
            refresh_buttons(world, mob);
            continue;
        }
        if pos.is_some_and(|p| lit_at(world, p)) {
            exit_shadow_walk(world, mob, true);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Light and the heart

/// Once a second: shadow walk healing, `nyxosynthesis`, the Lightless Domain and the heart of
/// darkness of every shadow body.
// Cost: once a second, a pass over the mobs with a species and one lumcount (radius 8) per
// shadow or nightmare body (a handful per round).
fn nightmare_life(world: &mut World, mut last: Local<Option<f64>>) {
    let t = now(world);
    let since = t - last.unwrap_or(t);
    if last.is_some() && since < 1.0 {
        return;
    }
    *last = Some(t);
    if since <= 0.0 {
        return;
    }
    let secs = since as f32;
    let bodies: Vec<Entity> = world
        .query::<(Entity, &crate::species::Species)>()
        .iter(world)
        .filter(|(_, s)| matches!(s.0, SpeciesId::Shadow | SpeciesId::Nightmare))
        .map(|(e, _)| e)
        .collect();
    for mob in bodies {
        shadow_tick(world, mob, secs, t);
    }
}

fn shadow_tick(world: &mut World, mob: Entity, secs: f32, t: f64) {
    if world.get::<Health>(mob).is_none() {
        return;
    }
    // A closet is the mob's `loc`: not a turf, no light check.
    if world.get::<crate::closets::InCloset>(mob).is_some() {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let dark = dark_at(world, pos);
    if world.get::<ShadowWalking>(mob).is_some() {
        // `phased_mob/shadow/process`: the dummy heals; the body's own effects do not run.
        heal(world, mob, JAUNT_HEALING * secs);
    } else if dark {
        heal(world, mob, DARK_HEALING * secs);
        let has_brain =
            organs::state(world, mob, Slot::Brain).is_some_and(|b| b.path == NIGHTMARE_BRAIN);
        if has_brain {
            world
                .entity_mut(mob)
                .insert(LightlessDomain(t + LIGHTLESS_SECS));
        }
    } else {
        hurt(world, mob, LIGHT_DAMAGE * secs);
    }
    heart_tick(world, mob, pos, dark, f64::from(secs));
}

/// `heal_overall_damage(brute = amount, burn = amount)`.
fn heal(world: &mut World, mob: Entity, amount: f32) {
    let Some(mut health) = world.get::<Health>(mob).cloned() else {
        return;
    };
    if health.brute <= 0.0 && health.burn <= 0.0 {
        return;
    }
    health.adjust(DamageType::Brute, -amount);
    health.adjust(DamageType::Burn, -amount);
    world.entity_mut(mob).insert(health);
}

/// `take_overall_damage(brute = amount, burn = amount)`.
fn hurt(world: &mut World, mob: Entity, amount: f32) {
    let (Some(mut health), Some(mut stat)) = (
        world.get::<Health>(mob).cloned(),
        world.get::<Stat>(mob).copied(),
    ) else {
        return;
    };
    let a = apply_damage(&mut health, &mut stat, DamageType::Brute, amount);
    let b = apply_damage(&mut health, &mut stat, DamageType::Burn, amount);
    world.entity_mut(mob).insert(health);
    if a.is_some() || b.is_some() {
        world.entity_mut(mob).insert(stat);
    }
}

/// The heart of darkness: the blade goes with a removed heart (`on_mob_remove`); a dead owner in
/// the dark comes back after [`HEART_RESPAWN_SECS`] (`on_death`).
fn heart_tick(world: &mut World, mob: Entity, pos: TilePos, dark: bool, secs: f64) {
    let Some(heart) = world.get::<HeartOfDarkness>(mob).copied() else {
        return;
    };
    let has_heart =
        organs::state(world, mob, Slot::Heart).is_some_and(|h| h.path == NIGHTMARE_HEART);
    if !has_heart {
        if let Some(blade) = heart.blade.filter(|&b| world.get_entity(b).is_ok()) {
            let name = name_of(world, blade);
            visible(
                world,
                mob,
                Span::Warning,
                format!("{name} disintegrates!"),
                None,
            );
            drop_from_hands(world, mob, blade);
            world.despawn(blade);
        }
        world.entity_mut(mob).remove::<HeartOfDarkness>();
        return;
    }
    if world.get::<Stat>(mob) != Some(&Stat::Dead) {
        if heart.respawn_progress != 0.0
            && let Some(mut h) = world.get_mut::<HeartOfDarkness>(mob)
        {
            h.respawn_progress = 0.0;
        }
        return;
    }
    let mut progress = heart.respawn_progress;
    if dark {
        progress += secs;
        play(world, pos, HEARTBEAT, 40.0);
    }
    if progress < HEART_RESPAWN_SECS {
        if let Some(mut h) = world.get_mut::<HeartOfDarkness>(mob) {
            h.respawn_progress = progress;
        }
        return;
    }
    // `owner.revive(HEAL_ALL & ~HEAL_REFRESH_ORGANS)`.
    crate::spells::effects::full_heal(world, mob);
    crate::ghost::return_to_body(world, mob);
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        Span::Warning,
        format!("{user} staggers to their feet!"),
        None,
    );
    play(world, pos, RISE, 50.0);
    if let Some(mut h) = world.get_mut::<HeartOfDarkness>(mob) {
        h.respawn_progress = 0.0;
    }
    info!("{mob} revived by the heart of darkness");
}

fn drop_from_hands(world: &mut World, mob: Entity, item: Entity) {
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        for hand in inventory.hands.iter_mut() {
            if *hand == Some(item) {
                *hand = None;
            }
        }
    }
}

/// `/datum/status_effect/shadow/nightmare/dodge_bullets` (`COMSIG_ATOM_PRE_BULLET_ACT`): true
/// when `mob` evades projectile `projectile` (it flies on, `COMPONENT_BULLET_PIERCED`).
pub fn dodge_projectile(world: &mut World, mob: Entity, projectile: Entity) -> bool {
    let Some(&LightlessDomain(until)) = world.get::<LightlessDomain>(mob) else {
        return false;
    };
    if now(world) >= until {
        world.entity_mut(mob).remove::<LightlessDomain>();
        return false;
    }
    let (user, bullet) = (name_of(world, mob), name_of(world, projectile));
    visible(
        world,
        mob,
        Span::Danger,
        format!("{user} dances in the shadows, evading {bullet}!"),
        Some(format!("You evade {bullet} with the cover of darkness!")),
    );
    if let Some(pos) = crate::explosions::turf_of(world, mob) {
        play(world, pos, "bullet_miss", 75.0);
    }
    true
}

// ---------------------------------------------------------------------------------------------
// The light eater

/// The light eater of `mob`'s heart, if any.
fn blade_of(world: &World, mob: Entity) -> Option<Entity> {
    world
        .get::<HeartOfDarkness>(mob)
        .and_then(|h| h.blade)
        .filter(|&b| world.get::<LightEater>(b).is_some())
}

fn set_crit_timer(world: &mut World, mob: Entity, at: Option<f64>) {
    // `equipped`: only a true nightmare's blade crits.
    if world.get::<Nightmare>(mob).is_none() {
        return;
    }
    let Some(blade) = blade_of(world, mob) else {
        return;
    };
    if let Some(mut eater) = world.get_mut::<LightEater>(blade)
        && !eater.has_crit
        && eater.crit_at != at
    {
        eater.crit_at = at;
    }
}

/// `add_crit` when the timer runs out: "critical strike ready".
fn tick_blades(world: &mut World) {
    let t = now(world);
    let ready: Vec<(Entity, Entity)> = world
        .query::<(Entity, &LightEater)>()
        .iter(world)
        .filter(|(_, e)| e.crit_at.is_some_and(|at| t >= at))
        .map(|(b, e)| (b, e.owner))
        .collect();
    for (blade, owner) in ready {
        if let Some(mut eater) = world.get_mut::<LightEater>(blade) {
            eater.crit_at = None;
            eater.has_crit = true;
        }
        if let Some(client) = world.get::<PlayerMob>(owner).map(|p| p.client) {
            tell(world, client, Span::Notice, "critical strike ready");
        }
    }
}

/// `light_eaten`'s `block_light_*`: an eaten light stays out whatever relights it.
fn keep_eaten(mut eaten: Query<&mut Optics, (With<LightEaten>, Changed<Optics>)>) {
    for mut optics in &mut eaten {
        if optics.light.is_some() {
            optics.light = None;
        }
    }
}

/// `/datum/element/light_eater/on_interacting_with` and the light eater's `attack`: the
/// target's lights go out; a crit on a mob. A mob still takes the hit from the click chain.
fn item_interaction(world: &mut World, ctx: &ClickCtx) -> bool {
    let Some(held) = ctx.held else {
        return false;
    };
    if world.get::<LightEater>(held).is_none() {
        return false;
    }
    let target = ctx.target;
    let ate = eat_lights(world, held, target);
    if world.get::<Health>(target).is_some() {
        critical_hit(world, ctx.mob, held, target);
        return false;
    }
    if ate {
        // The "pretend" attack on what cannot normally be hit.
        let t = now(world);
        world
            .entity_mut(ctx.mob)
            .insert(crate::combat::NextMove(t + CLICK_CD_RAPID));
    }
    ate
}

/// `eat_lights(food, eater)`: every light of `food` goes out; true when one did.
pub fn eat_lights(world: &mut World, eater: Entity, food: Entity) -> bool {
    let mut ate = false;
    // `/obj/machinery/light/on_light_eater`: the tube breaks.
    if world.get::<crate::power::LightFixture>(food).is_some() {
        ate = crate::power::break_light_tube(world, food);
    } else if world.get::<ss13_mapload::Turf>(food).is_none()
        && world
            .get::<Optics>(food)
            .is_some_and(|o| o.light.is_some_and(|l| l.power > 0.0 && l.range > 0.0))
    {
        if let Some(mut optics) = world.get_mut::<Optics>(food) {
            optics.light = None;
        }
        // A mob's light is what it carries: out, but not eaten for good.
        if world.get::<Health>(food).is_none() {
            world.entity_mut(food).insert(LightEaten);
        }
        ate = true;
    }
    if ate {
        let (food_name, eater_name) = (name_of(world, food), name_of(world, eater));
        visible(
            world,
            food,
            Span::Danger,
            format!(
                "Something dark in {eater_name} lashes out at {food_name} and its light goes out \
                 in an instant!"
            ),
            Some(format!(
                "You feel something dark in {eater_name} lash out and gnaw through your light in \
                 an instant! It recedes just as fast, but you can feel that it left something \
                 hungry behind."
            )),
        );
    }
    ate
}

/// The light eater's `attack` with `has_crit`.
fn critical_hit(world: &mut World, user: Entity, blade: Entity, target: Entity) {
    if !world.get::<LightEater>(blade).is_some_and(|e| e.has_crit) {
        return;
    }
    if let Some(pos) = crate::explosions::turf_of(world, target) {
        play(world, pos, GORE, 100.0);
    }
    let (u, v, b) = (
        name_of(world, user),
        name_of(world, target),
        name_of(world, blade),
    );
    let silicon = world
        .get::<crate::silicon::cyborg::Cyborg>(target)
        .is_some();
    let carbon = world.get::<crate::human::HumanLook>(target).is_some();
    if world.get::<Stat>(target) == Some(&Stat::Dead) {
        visible(
            world,
            user,
            Span::Warning,
            format!("{u} gores {v} with {b}!"),
            Some(format!(
                "You gore {v} with {b}, which doesn't accomplish much, but it does make you feel \
                 a little better."
            )),
        );
    } else if carbon || silicon {
        visible(
            world,
            user,
            Span::BoldWarning,
            format!("{u} gores {v} with {b}, bringing them to a halt!"),
            Some(format!("You gore {v} with {b}, bringing them to a halt!")),
        );
        let secs = if silicon { 2.0 } else { 1.0 };
        crate::status_effects::apply(world, target, crate::status_effects::Incap::Paralyze, secs);
    } else {
        visible(
            world,
            user,
            Span::BoldWarning,
            format!("{u} gores {v} with {b}, ripping into them!"),
            Some(format!("You gore {v} with {b}, ripping into them!")),
        );
        let force = world
            .get::<ss13_mapload::DmItem>(blade)
            .map_or(25.0, |i| i.force);
        let (Some(mut health), Some(mut stat)) = (
            world.get::<Health>(target).cloned(),
            world.get::<Stat>(target).copied(),
        ) else {
            return;
        };
        let changed = apply_damage(&mut health, &mut stat, DamageType::Brute, force);
        world.entity_mut(target).insert(health);
        if changed.is_some() {
            world.entity_mut(target).insert(stat);
        }
    }
    if let Some(mut eater) = world.get_mut::<LightEater>(blade) {
        eater.has_crit = false;
        eater.crit_at = None;
    }
}

// ---------------------------------------------------------------------------------------------
// Info and the round end

/// `/datum/action/antag_info/Trigger` of a nightmare.
fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if world.get::<Nightmare>(source).is_some() {
            world.write_message(OpenUi::new::<NightmareInfoWindow>(
                client, actor, source, ANTAG_NAME,
            ));
        }
    }
}

/// Each nightmare and its objective (`/datum/objective/nightmare_fluff/check_completion`: alive).
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let mut query = world.query::<(Entity, &Nightmare)>();
    let mut all: Vec<(Entity, Nightmare)> =
        query.iter(world).map(|(e, n)| (e, n.clone())).collect();
    if all.is_empty() {
        return Vec::new();
    }
    all.sort_by_key(|(e, _)| *e);
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = vec![line(vec![ChatSegment::new(
        Span::Bold,
        format!("{ROUNDEND_CATEGORY} были:"),
    )])];
    for (mob, nightmare) in all {
        out.push(super::traitor::print_player(world, mob, &nightmare.key));
        let done = world.get::<Stat>(mob).is_some_and(|s| *s != Stat::Dead);
        out.push(line(vec![
            ChatSegment::new(Span::Bold, "Задача #1"),
            ChatSegment::new(Span::Plain, format!(": {} ", nightmare.objective)),
            if done {
                ChatSegment::new(Span::GreenText, "Успех!")
            } else {
                ChatSegment::new(Span::RedText, "Неудача.")
            },
        ]));
        out.push(line(vec![if done {
            ChatSegment::new(Span::GreenText, format!("{ANTAG_NAME} был успешен!"))
        } else {
            ChatSegment::new(Span::RedText, format!("{ANTAG_NAME} провалился!"))
        }]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn darkness_follows_the_lumcount_checks() {
        // No lighting object: never dark, always lit.
        assert!(!is_dark(None, LIGHT_THRESHOLD));
        assert!(is_lit(None, LIGHT_THRESHOLD));
        assert!(is_dark(Some(0.1), LIGHT_THRESHOLD));
        // `lums >= value` is not below; `lums > value` is above.
        assert!(!is_dark(Some(0.2), LIGHT_THRESHOLD));
        assert!(!is_lit(Some(0.2), LIGHT_THRESHOLD));
        assert!(is_lit(Some(0.3), LIGHT_THRESHOLD));
    }

    #[test]
    fn a_step_into_light_needs_a_second_try_after_the_warning() {
        let walk = ShadowWalking::default();
        // The first try warns and is refused.
        let (ok, walk) = light_step_warning(walk, 10.0);
        assert!(!ok && walk.light_alert_given);
        // Within the cooldown: refused.
        let (ok, walk) = light_step_warning(walk, 10.5);
        assert!(!ok);
        // After 0.75 s and before the alert resets at 1 s: through.
        let (ok, after) = light_step_warning(walk, 10.8);
        assert!(ok && !after.light_alert_given);
        // Past the reset the warning comes again.
        let (ok, again) = light_step_warning(walk, 11.5);
        assert!(!ok && again.light_alert_given);
    }

    #[test]
    fn names_and_objectives_are_tgs() {
        assert_eq!(NIGHTMARE_NAMES.len(), 10);
        assert!(FLUFF.iter().all(|f| f.ends_with('.')));
        let n = Nightmare {
            key: String::new(),
            objective: FLUFF[0].to_owned(),
        };
        assert_eq!(n.info().objectives[0].explanation, FLUFF[0]);
    }
}

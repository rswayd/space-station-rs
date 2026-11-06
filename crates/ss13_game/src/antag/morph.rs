//! The morph after tg `/mob/living/basic/morph` (`morph.dm`), its antagonist
//! (`/datum/antagonist/morph`), the `assume_form` ability
//! (`/datum/action/cooldown/mob_cooldown/assume_form`) and the "Morph" light midround
//! (`/datum/dynamic_ruleset/midround/from_ghosts/morph`, weight 0 in the config: admins force it).
//!
//! - **Selection** ([`can_spawn`], [`spawn_morphs`], from `super::midround`): ghosts who want
//!   [`ROLE_MORPH`]; a morph (`crate::mobs`: 150 health, 20 brute "glomps") appears on a
//!   maintenance landmark and the ghost's client takes it over; no station job, the greeting,
//!   tg's default `AntagInfoGeneric` panel.
//! - **Assume Form** (the button, then a click on what to copy, [`assume_click`]): the morph
//!   takes the clicked object's or mob's look and name (`owner.appearance = target.appearance`,
//!   at least 150 alpha), moves faster (`morph_disguised`: -1 decisecond) and cannot attack or
//!   eat (`melee_damage_disguised = 0`); clicking itself drops the disguise. 1.5 s cooldown.
//! - **Clicks** ([`morph_clicks`], `early_melee_attack`): a living mob is glomped
//!   (`crate::mobs::defense::melee_attack`); a corpse is eaten in 3 s (heals 50 brute), an
//!   unanchored item in 2 s; what it ate is inside it.
//! - **Death**: a disguised morph "dissolves into a pile of green flesh", the disguise drops
//!   and everything it ate comes out (`content_barfer`).
//! - [`roundend_report`]: each morph.
// shortcut: no ghost poll (willing ghosts are taken), no shift-click alias of the ability, no
// vent crawling, no "doesn't look quite right" examine line within 3 tiles (the examine is the
// copied name only), the medical HUD is not hidden, a morph despawned without dying loses what
// it ate, and an eaten corpse stays hidden where it was eaten (it comes out under the morph).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{DmItem, IconIndex};
use ss13_net::server::ClientToken;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::Mover;

use super::dynamic::DynamicRng;
use super::objectives::RoundEnd;
use super::{AntagHudData, AntagHudUi, BrotherInfo};
use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::click::{ClickIntercept, ClickResolved, InterceptedClick};
use crate::combat::{CLICK_CD_MELEE, NextMove};
use crate::construction::Job;
use crate::health::{DamageType, Health, Stat};
use crate::inventory::CarriedBy;
use crate::server::{PlayerMob, SpawnPoints};
use crate::tools::{Actor, DoAfter, do_after, tell};

pub use super::ROLE_MORPH;
/// `name` ("\improper Morph").
pub const ANTAG_NAME: &str = "Морф";
const ROUNDEND_CATEGORY: &str = "Морфы";
pub const MORPH_MOB: &str = "/mob/living/basic/morph";
/// `assume_form` `cooldown_time`.
pub const ASSUME_FORM_COOLDOWN: f64 = 1.5;
/// `/datum/movespeed_modifier/morph_disguised` `multiplicative_slowdown`, seconds.
const DISGUISED_SPEEDUP: f32 = 0.1;
/// `assume_form`: "fucking chameleons".
const MIN_DISGUISE_ALPHA: u8 = 150;
/// `eat(delay = 3 SECONDS, update_health = -50)` of a corpse, `eat(delay = 2 SECONDS)` of an
/// item; deciseconds.
const EAT_CORPSE_DS: f32 = 30.0;
const EAT_ITEM_DS: f32 = 20.0;
const CORPSE_HEALING: f32 = 50.0;
const ASSUME_FORM_ID: &str = "morph:assume_form";
const ITEMS_DMI: &str = "icons/mob/actions/actions_items.dmi";
const BACKGROUNDS_DMI: &str = "icons/mob/actions/backgrounds.dmi";
/// `assume_form` `blacklist_typecache` (screens, energy balls and singularities are not here).
const BLACKLIST: [&str; 2] = ["/obj/effect", "/obj/narsie"];

/// What the morph was before it took a form (`initial()` of its vars).
#[derive(Clone, Debug, PartialEq)]
pub struct Disguise {
    pub look: Appearance,
    pub name: String,
    pub delay: f32,
}

/// On the morph (`/datum/antagonist/morph` and the mob's own state).
#[derive(Component, Clone, Debug)]
pub struct Morph {
    /// The player's key, for the round-end report.
    pub key: String,
    /// `TRAIT_DISGUISED`: the morph's own look while it wears another.
    pub disguise: Option<Disguise>,
    /// The ability waits for its target click.
    pub aiming: bool,
    /// `next_use_time` of the ability.
    pub ready_at: f64,
}

/// Something a morph ate (`forceMove(src)`): it comes out when the morph dies.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Swallowed(pub Entity);

/// The morph's info panel (tg's `AntagInfoMorph`, drawn as the generic one).
pub struct MorphInfoWindow;

impl UiInterface for MorphInfoWindow {
    const NAME: &'static str = crate::antag::cult::AntagInfoGenericUi::NAME;
    type Data = ();
    type StaticData = BrotherInfo;
}

impl ServerUi for MorphInfoWindow {
    type Source = &'static Morph;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &Morph) -> BrotherInfo {
        BrotherInfo {
            antag_name: ANTAG_NAME.to_owned(),
            objectives: Vec::new(),
        }
    }

    fn data(_: &Morph, _: Entity, _: &()) {}

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

pub struct MorphPlugin;

impl Plugin for MorphPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<MorphInfoWindow>()
            .add_systems(
                Update,
                (open_info, trigger_buttons, assume_click).in_set(UiSystems::Handle),
            )
            .add_systems(
                Update,
                morph_clicks
                    .after(crate::click::ResolveClicks)
                    .before(crate::health::HealthSystems),
            )
            .add_systems(Update, on_death.after(crate::health::HealthSystems));
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

fn name_of(world: &World, e: Entity) -> String {
    crate::medical::name_of(world, e)
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

fn tell_mob(world: &mut World, mob: Entity, span: Span, text: &str) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

// ---------------------------------------------------------------------------------------------
// Selection

/// `can_be_selected`: a maintenance spawn (`find_maintenance_spawn(require_darkness = FALSE)`).
pub fn can_spawn(world: &mut World) -> bool {
    world
        .get_resource::<SpawnPoints>()
        .is_some_and(|s| !s.maintenance.is_empty())
}

/// `create_ruleset_body` + `assign_role` for each `(client, ghost)`: the morphs.
pub fn spawn_morphs(world: &mut World, ghosts: &[(Entity, Entity)]) -> Vec<Entity> {
    let mut out = Vec::new();
    for &(client, ghost) in ghosts {
        let tiles = world
            .get_resource::<SpawnPoints>()
            .map(|s| s.maintenance.clone())
            .unwrap_or_default();
        if tiles.is_empty() {
            break;
        }
        let i = with_rng(world, |rng| rng.range(0, tiles.len() as i64 - 1)) as usize;
        let Some(morph) = crate::mobs::spawn::spawn_mob(world, MORPH_MOB, tiles[i]) else {
            break;
        };
        if !crate::medical::brainmob::possess(world, client, ghost, morph) {
            world.despawn(morph);
            continue;
        }
        world.try_despawn(ghost).ok();
        make_morph(world, morph);
        info!("{morph} ({client}) is a morph at {:?}", tiles[i]);
        out.push(morph);
    }
    out
}

/// `add_antag_datum(/datum/antagonist/morph)` with the mob's ability: no station job, the
/// greeting, the panel, the button. `false` if `mob` already is one.
pub fn make_morph(world: &mut World, mob: Entity) -> bool {
    if world.get::<Morph>(mob).is_some() || world.get_entity(mob).is_err() {
        return false;
    }
    world.entity_mut(mob).remove::<crate::jobs::PlayerJob>();
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    world.entity_mut(mob).insert((
        Morph {
            key,
            disguise: None,
            aiming: false,
            ready_at: 0.0,
        },
        AntagHudData::default(),
    ));
    refresh_buttons(world, mob);
    if let Some(client) = client {
        tell(world, client, Span::Bold, format!("Вы - {ANTAG_NAME}."));
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
        world.write_message(OpenUi::new::<MorphInfoWindow>(client, mob, mob, ANTAG_NAME));
    }
    true
}

// ---------------------------------------------------------------------------------------------
// Assume form

fn refresh_buttons(world: &mut World, mob: Entity) {
    let Some(morph) = world.get::<Morph>(mob).cloned() else {
        return;
    };
    let icon = |world: &World, dmi: &str, state: &str| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(dmi, state))
    };
    let mut buttons: Vec<(ActionButton, f64)> = world
        .get::<ActionButtons>(mob)
        .map(|b| {
            b.0.iter()
                .filter(|(button, _)| button.id != ASSUME_FORM_ID)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    buttons.push((
        ActionButton {
            id: ASSUME_FORM_ID.to_owned(),
            name: "Assume Form".to_owned(),
            icon: icon(world, ITEMS_DMI, "sniper_zoom"),
            background: icon(world, BACKGROUNDS_DMI, "bg_alien"),
            border: icon(world, BACKGROUNDS_DMI, "bg_alien_border"),
            available: world.get::<Stat>(mob) == Some(&Stat::Conscious),
            cooldown: 0.0,
            active: morph.aiming,
            maptext: None,
        },
        morph.ready_at,
    ));
    set_buttons(world, mob, buttons);
}

/// The button arms the ability (its next click picks the form) or disarms it.
fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if id != ASSUME_FORM_ID
            || actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
        {
            continue;
        }
        let Some(aiming) = world.get::<Morph>(actor).map(|m| m.aiming) else {
            continue;
        };
        set_aim(world, actor, !aiming);
    }
}

fn set_aim(world: &mut World, mob: Entity, on: bool) {
    if let Some(mut morph) = world.get_mut::<Morph>(mob) {
        morph.aiming = on;
    }
    if on {
        world.entity_mut(mob).insert(ClickIntercept);
    } else {
        world.entity_mut(mob).remove::<ClickIntercept>();
    }
    refresh_buttons(world, mob);
}

/// `InterceptClickOn` -> `Activate` -> `determine_intent`: the clicked thing's form, or the own
/// one back on a click on itself.
fn assume_click(world: &mut World, mut clicks: Local<MessageCursor<InterceptedClick>>) {
    let clicks: Vec<InterceptedClick> = clicks
        .read(world.resource::<Messages<InterceptedClick>>())
        .copied()
        .collect();
    let t = now(world);
    for click in clicks {
        let Some(morph) = world.get::<Morph>(click.mob).cloned() else {
            continue;
        };
        if !morph.aiming
            || world.get::<PlayerMob>(click.mob).map(|p| p.client) != Some(click.client)
        {
            continue;
        }
        // `AB_CHECK_CONSCIOUS`, the cooldown.
        if world.get::<Stat>(click.mob) != Some(&Stat::Conscious) || t < morph.ready_at {
            continue;
        }
        if click.target == click.mob {
            reset_form(world, click.mob);
        } else if can_assume_form(world, click.target) {
            assume_form(world, click.mob, click.target);
        } else {
            continue;
        }
        if let Some(mut m) = world.get_mut::<Morph>(click.mob) {
            m.ready_at = t + ASSUME_FORM_COOLDOWN;
        }
        set_aim(world, click.mob, false);
    }
}

/// `can_assume_form`: an object (not an effect) or a mob with a look.
pub fn can_assume_form(world: &World, target: Entity) -> bool {
    if world.get::<Appearance>(target).is_none() {
        return false;
    }
    if world.get::<Health>(target).is_some() {
        return true;
    }
    crate::construction::path_of(world, target)
        .is_some_and(|p| is_under(&p, "/obj") && !BLACKLIST.iter().any(|b| is_under(&p, b)))
}

/// `assume_appearances` + `on_disguise`.
fn assume_form(world: &mut World, mob: Entity, target: Entity) {
    let (Some(mut look), Some(own_look)) = (
        world.get::<Appearance>(target).cloned(),
        world.get::<Appearance>(mob).cloned(),
    ) else {
        return;
    };
    let target_name = name_of(world, target);
    // Already disguised: the own look stays the one kept.
    let kept = world.get::<Morph>(mob).and_then(|m| m.disguise.clone());
    let disguise = match kept {
        Some(d) => d,
        None => Disguise {
            look: own_look,
            name: name_of(world, mob),
            delay: world.get::<Mover>(mob).map_or(0.0, |m| m.delay),
        },
    };
    look.color = Some(disguised_color(look.color));
    let own_name = disguise.name.clone();
    let delay = disguise.delay;
    if let Some(mut m) = world.get_mut::<Morph>(mob) {
        m.disguise = Some(disguise);
    }
    world
        .entity_mut(mob)
        .insert((look, Name::new(target_name.clone())));
    if let Some(mut mover) = world.get_mut::<Mover>(mob) {
        mover.delay = disguised_delay(delay);
    }
    visible(
        world,
        mob,
        Span::Warning,
        format!("{own_name} suddenly twists and changes shape, becoming a copy of {target_name}!"),
        Some(format!(
            "You twist your body and assume the form of {target_name}."
        )),
    );
}

/// `owner.alpha = max(target_atom.alpha, 150)` on the copied colour.
pub fn disguised_color(color: Option<[u8; 4]>) -> [u8; 4] {
    let [r, g, b, a] = color.unwrap_or([255, 255, 255, 255]);
    [r, g, b, a.max(MIN_DISGUISE_ALPHA)]
}

/// The move delay with `morph_disguised` (-1 decisecond, not below 0).
pub fn disguised_delay(delay: f32) -> f32 {
    (delay - DISGUISED_SPEEDUP).max(0.0)
}

/// `reset_appearances` + `on_undisguise`: the own look, name and speed. False when it was not
/// disguised.
fn reset_form(world: &mut World, mob: Entity) -> bool {
    let Some(disguise) = world
        .get_mut::<Morph>(mob)
        .and_then(|mut m| m.disguise.take())
    else {
        return false;
    };
    world
        .entity_mut(mob)
        .insert((disguise.look, Name::new(disguise.name.clone())));
    if let Some(mut mover) = world.get_mut::<Mover>(mob) {
        mover.delay = disguise.delay;
    }
    let dead = world.get::<Stat>(mob) == Some(&Stat::Dead);
    if !dead {
        visible(
            world,
            mob,
            Span::Warning,
            format!(
                "{} suddenly collapses in on itself, dissolving into a pile of green flesh!",
                disguise.name
            ),
            Some("You reform to your normal body.".to_owned()),
        );
    }
    crate::mobs::set_look(world, mob, dead);
    true
}

// ---------------------------------------------------------------------------------------------
// Attacks and eating

/// `early_melee_attack` of a player's morph on what it clicked next to it.
fn morph_clicks(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    let t = now(world);
    for click in clicks {
        let Some(morph) = world.get::<Morph>(click.mob).cloned() else {
            continue;
        };
        let (mob, target) = (click.mob, click.target);
        if target == mob
            || world.get::<Stat>(mob) != Some(&Stat::Conscious)
            || world.get::<NextMove>(mob).is_some_and(|n| t < n.0)
            || world.get::<DoAfter>(mob).is_some()
        {
            continue;
        }
        if morph.disguise.is_some() {
            tell_mob(world, mob, Span::Warning, "can't attack while disguised!");
            world.entity_mut(mob).insert(NextMove(t + CLICK_CD_MELEE));
            continue;
        }
        let actor = Actor {
            client: click.client,
            mob,
            item: None,
        };
        if world.get::<Health>(target).is_some() {
            if world.get::<Stat>(target) != Some(&Stat::Dead) {
                crate::mobs::defense::melee_attack(world, mob, target);
                continue;
            }
            start_eating(world, actor, target, EAT_CORPSE_DS, true);
            continue;
        }
        if edible_item(world, target) {
            start_eating(world, actor, target, EAT_ITEM_DS, false);
        }
    }
}

/// An unanchored item on the floor.
fn edible_item(world: &World, item: Entity) -> bool {
    world.get::<DmItem>(item).is_some_and(DmItem::pickable)
        && world.get::<TilePos>(item).is_some()
        && world.get::<CarriedBy>(item).is_none()
}

/// `eat(eatable, delay)`: "eating..." and the `do_after`.
fn start_eating(world: &mut World, actor: Actor, target: Entity, delay_ds: f32, corpse: bool) {
    tell(world, actor.client, Span::Notice, "eating...");
    do_after(world, actor, target, delay_ds, 0, Job::Morph { corpse });
    world
        .entity_mut(actor.mob)
        .insert(NextMove(now(world) + CLICK_CD_MELEE));
}

/// The end of `eat`'s `do_after`: still undisguised, the corpse still dead or the item still
/// on the floor; it goes inside the morph.
pub fn complete_eat(world: &mut World, mob: Entity, action: &DoAfter, corpse: bool) {
    let target = action.target;
    if world.get::<Morph>(mob).is_none_or(|m| m.disguise.is_some()) {
        return;
    }
    let valid = if corpse {
        world.get::<Stat>(target) == Some(&Stat::Dead)
            && world.get::<crate::closets::InCloset>(target).is_none()
    } else {
        edible_item(world, target)
    };
    if !valid {
        return;
    }
    let (user, food) = (name_of(world, mob), name_of(world, target));
    visible(
        world,
        mob,
        Span::Warning,
        format!("{user} swallows {food} whole!"),
        None,
    );
    info!("{mob} (morph) ate {target}");
    if corpse {
        crate::closets::put_mob_inside(world, target, mob);
        let healed = world.get::<Health>(mob).cloned().map(|mut h| {
            h.adjust(DamageType::Brute, -CORPSE_HEALING);
            h
        });
        if let Some(health) = healed {
            world.entity_mut(mob).insert(health);
        }
    } else {
        let pos = action.target_pos;
        world
            .entity_mut(target)
            .remove::<TilePos>()
            .insert(Swallowed(mob));
        crate::construction::tile_changed(world, pos);
    }
}

/// A morph's death: the disguise drops (`death`, `reset_appearances` on `COMSIG_LIVING_DEATH`)
/// and what it ate comes out (`content_barfer`).
fn on_death(world: &mut World) {
    let died: Vec<Entity> = world
        .query_filtered::<(Entity, Ref<Stat>), With<Morph>>()
        .iter(world)
        .filter(|(_, s)| s.is_changed() && **s == Stat::Dead)
        .map(|(e, _)| e)
        .collect();
    for mob in died {
        let disguised = world
            .get::<Morph>(mob)
            .is_some_and(|m| m.disguise.is_some());
        if disguised {
            let user = name_of(world, mob);
            visible(
                world,
                mob,
                Span::Warning,
                format!("{user} twists and dissolves into a pile of green flesh!"),
                Some(
                    "Your skin ruptures! Your flesh breaks apart! No disguise can ward off de--"
                        .to_owned(),
                ),
            );
            reset_form(world, mob);
        }
        if world.get::<Morph>(mob).is_some_and(|m| m.aiming) {
            set_aim(world, mob, false);
        }
        barf(world, mob);
    }
}

/// `content_barfer`: everything the morph ate lands under it.
pub fn barf(world: &mut World, mob: Entity) {
    let Some(pos) = crate::explosions::turf_of(world, mob) else {
        return;
    };
    let items: Vec<Entity> = world
        .query::<(Entity, &Swallowed)>()
        .iter(world)
        .filter(|(_, s)| s.0 == mob)
        .map(|(e, _)| e)
        .collect();
    for item in items {
        world.entity_mut(item).remove::<Swallowed>().insert(pos);
    }
    let corpses: Vec<Entity> = world
        .query::<(Entity, &crate::closets::InCloset)>()
        .iter(world)
        .filter(|(_, c)| c.closet == mob)
        .map(|(e, _)| e)
        .collect();
    for corpse in corpses {
        world.entity_mut(corpse).insert(pos);
        crate::closets::let_out(world, corpse);
    }
    crate::construction::tile_changed(world, pos);
}

// ---------------------------------------------------------------------------------------------
// Info and the round end

fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if world.get::<Morph>(source).is_some() {
            world.write_message(OpenUi::new::<MorphInfoWindow>(
                client, actor, source, ANTAG_NAME,
            ));
        }
    }
}

/// Each morph and whether it lived.
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let mut query = world.query::<(Entity, &Morph)>();
    let mut all: Vec<(Entity, String)> =
        query.iter(world).map(|(e, m)| (e, m.key.clone())).collect();
    if all.is_empty() {
        return Vec::new();
    }
    all.sort_by_key(|(e, _)| *e);
    let mut out = vec![ChatMessage {
        channel: ChatChannel::System,
        segments: vec![ChatSegment::new(
            Span::Bold,
            format!("{ROUNDEND_CATEGORY} были:"),
        )],
    }];
    for (mob, key) in all {
        out.push(super::traitor::print_player(world, mob, &key));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_disguise_is_never_too_faint_and_is_faster() {
        assert_eq!(disguised_color(None), [255, 255, 255, 255]);
        assert_eq!(disguised_color(Some([1, 2, 3, 40])), [1, 2, 3, 150]);
        assert_eq!(disguised_color(Some([1, 2, 3, 200])), [1, 2, 3, 200]);
        assert!((disguised_delay(0.15) - 0.05).abs() < 1e-6);
        assert_eq!(disguised_delay(0.05), 0.0);
    }

    #[test]
    fn the_ruleset_takes_the_morph_role() {
        let r = super::super::midround::ruleset(super::super::midround::MORPH).unwrap();
        assert_eq!(r.role, Some(ROLE_MORPH));
        let n = super::super::midround::ruleset(super::super::midround::NIGHTMARE).unwrap();
        assert_eq!(n.role, Some(super::super::ROLE_NIGHTMARE));
        assert_eq!(n.min_pop, 15);
    }
}

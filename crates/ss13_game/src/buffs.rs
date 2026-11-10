//! Buffs and neutral status effects (server), after tg `code/datums/status_effects/neutral.dm`
//! (`offering`, `grouped/surrender`, `surrender_timed`), `living_item_handling.dm` (`give`,
//! `take`), `screen_objects/alert.dm` (`alert/give`), `living/emote.dm` (`surrender`),
//! `buffs/food/_food_effect.dm` and `haste.dm`, `debuffs/drugginess.dm` and the reagents that
//! drive them.
//!
//! - Offering: ctrl+shift on a living mob next to the giver or the V key to everyone next to it
//!   ([`crate::click::GiveClick`], `ClientIntent::GiveItem`, tg
//!   `click_ctrl_shift` -> `give(src)`) offers the active hand's item. The taker gets an alert
//!   ([`AlertCategory::Offer`]); its click ([`crate::hud::ALERT_CLICK`]) takes the item into an
//!   empty hand (`take`). The offer ends when the item leaves the giver's hands or the taker is
//!   out of reach or incapacitated. Ctrl+shift on oneself moves the item to the other hand.
//! - Surrender: the `*surrender` emote ([`ss13_chat::server::Emoted`]) or a click on the
//!   surrender alert paralyzes for 20 s and clears the alert. The security pen
//!   (`/obj/item/pen/red/security`, a click past reach) gives the mob on the clicked tile the
//!   alert for 30 s (`surrender_timed`), every 30 s.
//! - Food buffs: the first bite of a handmade food of `crafting_complexity` 1-5 gives tg's only
//!   random buff, haste (`GLOB.food_buffs`): -0.04 per strength off the move delay and -0.06 per
//!   strength off `do_after` (`crate::tools`), for 5 minutes times twice the purity, with its
//!   alert ([`FoodBuff`]). A newer one replaces it.
//! - Drugginess ([`Timed::Drugginess`]): the `high` alert and moodlet while it lasts; space
//!   drugs set it. Bath salts in the blood make the mob stun immune
//!   ([`crate::status_effects::StunImmunity`]).
// shortcut: no give key (G, a new client intent) and no offer to everyone around; no high
// fives, helping up or borg offers (no hand items); no shift-click examine of the offered item,
// no item overlay on the alert, no `on_offered`/`on_offer_taken` hooks, no pulling exemption
// from the reach checks. No gunpoint (`holdup`/`heldup`) and no security holosign effect. The
// per-food `crafted_food_buff` overrides (chilling ice cream, trait and accent foods) are not
// imported: every handmade food gives haste. Drugginess has no screen effect (the `druggy`
// overlay multiplies the view, which the client cannot draw), no drugged sound environment and
// no beach bum language; space drugs do not step or emote, their overdose is not here.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_chat::server::Emoted;
use ss13_net::server::IntentReceived;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::protocol::ClientIntent;
use ss13_tgui::server::{UiActed, UiSystems};
use ss13_world::SpatialIndex;

use crate::breath::{AlertCategory, Alerts, clicked_alert};
use crate::chem::{name_of, tell_mob, visible};
use crate::click::{GiveClick, RangedClick, ResolveClicks};
use crate::health::{HealthSystems, Stat};
use crate::hud::AlertHudUi;
use crate::inventory::Inventory;
use crate::mood::MoodEvent;
use crate::status_effects::{self, Incap, StatusEffects, Timed};

/// `/datum/emote/living/surrender/run_emote`: `Paralyze(20 SECONDS)`.
pub const SURRENDER_PARALYZE_SECS: f32 = 20.0;
/// `/datum/status_effect/surrender_timed::duration`.
pub const SURRENDER_TIMED_SECS: f64 = 30.0;
/// The security pen's `holosign_cooldown`.
pub const PEN_COOLDOWN_SECS: f64 = 30.0;
/// `/datum/status_effect/food::duration`, seconds.
pub const FOOD_BUFF_SECS: f64 = 300.0;
const SECURITY_PEN: &str = "/obj/item/pen/red/security";
const SPACE_DRUGS: &str = "/datum/reagent/drug/space_drugs";
const BATH_SALTS: &str = "/datum/reagent/drug/bath_salts";
/// The `TRAIT_STUNIMMUNE` source of bath salts (`metabolized_traits`).
const BATH_SALTS_SOURCE: &str = "bath_salts";

/// `/datum/mood_event/high` (BandaStation's text).
const HIGH: MoodEvent = MoodEvent {
    id: "high",
    description: "Вооооу, чувааааак... Меня таааак качааааает...",
    mood_change: 6.0,
    timeout: 0.0,
    hidden: false,
};

/// `/datum/status_effect/offering` on the giver: the item and who may take it, with the text
/// of their alert (`alert/give/setup`).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Offering {
    pub item: Entity,
    pub takers: Vec<Entity>,
    pub alert_name: String,
    pub alert_desc: String,
}

/// `/datum/status_effect/surrender_timed`: the surrender alert until this game time.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct SurrenderPrompt {
    pub until: f64,
}

/// The security pen's `holosign_cooldown`: ready again at this game time.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct PenCooldown(pub f64);

/// `/datum/status_effect/drugginess` is on: its alert and moodlet are given.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Drugged;

/// `/datum/status_effect/food/haste` of `strength` (1-5; 0: none) until `until`. Kept at 0
/// instead of removed, so the move speed follows its end.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct FoodBuff {
    pub strength: u8,
    pub until: f64,
}

impl FoodBuff {
    /// `/datum/movespeed_modifier/food_haste`, deciseconds per tile.
    pub fn move_slowdown(&self) -> f32 {
        -0.04 * f32::from(self.strength)
    }

    /// `/datum/actionspeed_modifier/status_effect/food_haste`: added to the `do_after` factor.
    pub fn action_slowdown(&self) -> f32 {
        -0.06 * f32::from(self.strength)
    }
}

/// The food haste's share of `cached_multiplicative_actions_slowdown` (0 without it).
pub fn action_slowdown(world: &World, mob: Entity) -> f32 {
    world
        .get::<FoodBuff>(mob)
        .map_or(0.0, FoodBuff::action_slowdown)
}

pub struct BuffsPlugin;

impl Plugin for BuffsPlugin {
    fn build(&self, app: &mut App) {
        // Also without the chat server (tests): its emotes are what the surrender reads.
        app.add_message::<Emoted>()
            .add_systems(
                Update,
                (give_clicks, give_key, security_pen_clicks)
                    .chain()
                    .after(ResolveClicks),
            )
            .add_systems(Update, alert_clicks.in_set(UiSystems::Handle))
            .add_systems(
                Update,
                (
                    emote_effects,
                    offer_upkeep,
                    surrender_upkeep,
                    food_buff_upkeep,
                    exercise_upkeep,
                    drug_upkeep,
                    bath_salts_upkeep,
                )
                    .chain()
                    .after(status_effects::StatusSystems)
                    .before(HealthSystems),
            );
    }
}

fn throw_alert(
    world: &mut World,
    mob: Entity,
    category: AlertCategory,
    state: &'static str,
    severity: u8,
) -> bool {
    match world.get_mut::<Alerts>(mob) {
        Some(mut alerts) => {
            alerts.throw(category, state, severity);
            true
        }
        None => false,
    }
}

fn clear_alert(world: &mut World, mob: Entity, category: AlertCategory) {
    if let Some(mut alerts) = world.get_mut::<Alerts>(mob)
        && alerts.get(category).is_some()
    {
        alerts.clear(category);
    }
}

use crate::tools::now;

/// Living, conscious and not incapacitated (tg `!incapacitated` of a living mob).
fn able(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && !status_effects::incapacitation(world, mob).incapacitated
}

fn adjacent(world: &World, a: Entity, b: Entity) -> bool {
    match (world.get::<TilePos>(a), world.get::<TilePos>(b)) {
        (Some(&a), Some(&b)) => a.chebyshev(b).is_some_and(|d| d <= 1),
        _ => false,
    }
}

/// `TRAIT_CAN_HOLD_ITEMS`: an inventory and a hand.
fn can_hold_items(world: &World, mob: Entity) -> bool {
    world.get::<Inventory>(mob).is_some()
        && (0..2).any(|hand| crate::medical::bodyparts::has_hand(world, mob, hand))
}

// --- Offering --------------------------------------------------------------------------------

fn give_clicks(world: &mut World, mut clicks: Local<MessageCursor<GiveClick>>) {
    let clicks: Vec<GiveClick> = clicks
        .read(world.resource::<Messages<GiveClick>>())
        .copied()
        .collect();
    for click in clicks {
        // `/mob/living/click_ctrl_shift`: only on a living mob that can hold items.
        if world.get::<Stat>(click.target).is_some() && can_hold_items(world, click.target) {
            give(world, click.mob, Some(click.target));
        }
    }
}

/// `/datum/keybinding/living/give`: an offer to everyone next to the mob.
fn give_key(world: &mut World, mut intents: Local<MessageCursor<IntentReceived>>) {
    // Also without the network (tests of the other offer routes).
    let Some(messages) = world.get_resource::<Messages<IntentReceived>>() else {
        return;
    };
    let clients: Vec<Entity> = intents
        .read(messages)
        .filter(|i| i.intent == ClientIntent::GiveItem)
        .map(|i| i.client)
        .collect();
    for client in clients {
        let Some(mob) = world.resource::<crate::server::Players>().mob(client) else {
            continue;
        };
        // `can_use`: an active-hand item and `TRAIT_CAN_HOLD_ITEMS`.
        if world
            .get::<Inventory>(mob)
            .and_then(Inventory::active_item)
            .is_some()
            && can_hold_items(world, mob)
        {
            give(world, mob, None);
        }
    }
}

/// tg `/mob/living/proc/give(offered)`: offers the active hand's item to `taker` (`None`:
/// everyone next to the giver who can take it).
pub fn give(world: &mut World, giver: Entity, taker: Option<Entity>) {
    let tell = |world: &mut World, text: &str| tell_mob(world, giver, Span::Warning, text);
    if world.get::<Offering>(giver).is_some() {
        return tell(world, "Вы уже передаёте что-то!");
    }
    if !able(world, giver) {
        return tell(world, "Вы не можете передать что-то в вашем положении!");
    }
    let Some(inventory) = world.get::<Inventory>(giver).cloned() else {
        return;
    };
    let Some(item) = inventory.active_item() else {
        return tell(world, "Вы не держите ничего, чтобы передать!");
    };
    let item_name = name_of(world, item);
    if let Some(taker) = taker {
        if taker == giver {
            // `swap_hand(get_inactive_hand_index())`, then `put_in_active_hand`.
            let other = 1 - inventory.active;
            if !crate::medical::bodyparts::has_hand(world, giver, other) {
                return tell(
                    world,
                    &format!("Вы пытаетесь взять {item_name} у себя, но не смогли."),
                );
            }
            let mut inventory = world.get_mut::<Inventory>(giver).expect("cloned above");
            inventory.active = other;
            if inventory.hands[other].is_some() {
                return tell(
                    world,
                    &format!("Вы пытаетесь взять {item_name} у себя, но не смогли."),
                );
            }
            inventory.hands[1 - other] = None;
            inventory.hands[other] = Some(item);
            return tell_mob(
                world,
                giver,
                Span::Notice,
                &format!("Вы берёте {item_name} у себя"),
            );
        }
        if !able(world, taker) {
            return tell(world, "Он не может ничего взять в текущем состоянии!");
        }
        if !adjacent(world, giver, taker) {
            return tell(world, "Вы должны быть рядом для передачи!");
        }
        if !can_hold_items(world, taker) {
            return tell(world, "Он не может удержать ничего, что вы предлагаете!");
        }
    }
    // `orange(1, src)` of living mobs, `is_taker_elligible`.
    let around: Vec<Entity> = world
        .get::<TilePos>(giver)
        .copied()
        .map(|pos| {
            world
                .resource::<SpatialIndex>()
                .in_radius(pos, 1)
                .filter(|&m| {
                    m != giver
                        && world.get::<Stat>(m).is_some()
                        && world.get::<TilePos>(m) != Some(&pos)
                })
                .collect()
        })
        .unwrap_or_default();
    if taker.is_none() && around.is_empty() {
        return tell(world, "Рядом с вами нет никого, кто мог бы взять это!");
    }
    let giver_name = name_of(world, giver);
    let target_text = taker.map_or(String::new(), |t| format!("{} ", name_of(world, t)));
    crate::balloon::balloon_alert_to_viewers(world, giver, "предлагает что-то", None, &[]);
    visible(
        world,
        giver,
        &format!("Вы предлагаете {target_text}{item_name}."),
        &format!("{giver_name} предлагает {target_text}{item_name}."),
        Span::Notice,
    );
    let candidates: Vec<Entity> = match taker {
        Some(t) => vec![t],
        None => around
            .into_iter()
            .filter(|&m| adjacent(world, giver, m) && able(world, m) && can_hold_items(world, m))
            .collect(),
    };
    // `offering/on_creation` -> `register_candidate`: no alert, no taker; no takers, no offer.
    let mut takers = Vec::new();
    for candidate in candidates {
        if throw_alert(world, candidate, AlertCategory::Offer(giver), "template", 0) {
            takers.push(candidate);
        }
    }
    if takers.is_empty() {
        return;
    }
    let alert_name = format!("{giver_name} предлагает {item_name}");
    world.entity_mut(giver).insert(Offering {
        item,
        takers,
        alert_desc: format!("{alert_name}. Нажмите на уведомление, чтобы взять."),
        alert_name,
    });
}

/// Ends `giver`'s offer: every taker's alert goes (`offering/Destroy`).
fn end_offer(world: &mut World, giver: Entity) {
    let Some(offer) = world.get::<Offering>(giver).cloned() else {
        return;
    };
    for taker in offer.takers {
        clear_alert(world, taker, AlertCategory::Offer(giver));
    }
    if let Ok(mut entity) = world.get_entity_mut(giver) {
        entity.remove::<Offering>();
    }
}

/// `remove_candidate`: the last one gone ends the offer.
fn remove_taker(world: &mut World, giver: Entity, taker: Entity) {
    clear_alert(world, taker, AlertCategory::Offer(giver));
    let Some(mut offer) = world.get_mut::<Offering>(giver) else {
        return;
    };
    offer.takers.retain(|t| *t != taker);
    if offer.takers.is_empty() {
        world.entity_mut(giver).remove::<Offering>();
    }
}

/// tg `/mob/living/proc/take(offerer, offered_item)` after a click on the give alert.
pub fn take(world: &mut World, taker: Entity, giver: Entity) {
    let Some(offer) = world.get::<Offering>(giver).cloned() else {
        clear_alert(world, taker, AlertCategory::Offer(giver));
        return;
    };
    // `clear_alert("[offerer]")`.
    remove_taker(world, giver, taker);
    let tell = |world: &mut World, text: &str| tell_mob(world, taker, Span::Warning, text);
    let giver_name = name_of(world, giver);
    if !able(world, taker) {
        return tell(world, "Вы не можете ничего взять в вашем положении!");
    }
    if !adjacent(world, giver, taker) {
        return tell(world, &format!("{giver_name} вне досягаемости!"));
    }
    let held = world
        .get::<Inventory>(giver)
        .and_then(Inventory::active_item);
    if held != Some(offer.item) {
        return tell(
            world,
            &format!("У {giver_name} больше нет в руке предмета, который предлагался!"),
        );
    }
    let Some(inventory) = world.get::<Inventory>(taker).cloned() else {
        return;
    };
    let empty_hand = (0..2).any(|hand| {
        inventory.hands[hand].is_none() && crate::medical::bodyparts::has_hand(world, taker, hand)
    });
    if !empty_hand {
        return tell(world, "У вас нет пустых рук!");
    }
    let item_name = name_of(world, offer.item);
    // `temporarilyRemoveItemFromInventory` fails for `TRAIT_NODROP`.
    if crate::inventory::no_drop(world, offer.item) {
        let text = format!("{giver_name} пытается передать {item_name}, но он застрял в руке...");
        return visible(world, taker, &text, &text, Span::Notice);
    }
    let (Some(&giver_pos), Some(slot)) = (
        world.get::<TilePos>(giver),
        world
            .get::<Inventory>(giver)
            .and_then(|i| i.slot_of(offer.item)),
    ) else {
        return;
    };
    crate::inventory::unequip(world, giver, giver_pos, slot);
    crate::storage::put_in_hands(world, taker, &inventory, offer.item);
    let taker_name = name_of(world, taker);
    visible(
        world,
        taker,
        &format!("Вы берёте {item_name} от {giver_name}."),
        &format!("{taker_name} берёт {item_name} от {giver_name}."),
        Span::Notice,
    );
    // The item left the giver's hands (`COMSIG_ITEM_DROPPED` -> `dropped_item`).
    end_offer(world, giver);
}

/// `offering/dropped_item`, `check_taker_in_range`, `check_owner_in_range` each tick. Cost: per
/// tick, one pass over the few mobs offering something.
fn offer_upkeep(world: &mut World) {
    let mut query = world.query::<(Entity, &Offering)>();
    let offers: Vec<(Entity, Offering)> = query.iter(world).map(|(e, o)| (e, o.clone())).collect();
    for (giver, offer) in offers {
        let holds = world
            .get::<Inventory>(giver)
            .is_some_and(|i| i.hands.contains(&Some(offer.item)));
        if !holds || world.get::<Stat>(giver) == Some(&Stat::Dead) {
            end_offer(world, giver);
            continue;
        }
        for taker in offer.takers {
            if world.get_entity(taker).is_err() {
                remove_taker(world, giver, taker);
            } else if !adjacent(world, giver, taker) {
                let text = format!("Вы отошли слишком далеко от {}!", name_of(world, giver));
                tell_mob(world, taker, Span::Warning, &text);
                remove_taker(world, giver, taker);
            } else if !able(world, taker) {
                remove_taker(world, giver, taker);
            }
        }
    }
}

// --- Alerts ----------------------------------------------------------------------------------

/// Clicks on the alerts that do something (tg `alert/*/Click`, the owner's own).
fn alert_clicks(world: &mut World, mut acts: Local<MessageCursor<UiActed<AlertHudUi>>>) {
    let acts: Vec<(Entity, Entity, crate::breath::AlertClick)> = acts
        .read(world.resource::<Messages<UiActed<AlertHudUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, source, click) in acts {
        if actor != source {
            continue;
        }
        match clicked_alert(world, actor, &click) {
            Some(AlertCategory::Offer(giver)) => take(world, actor, giver),
            Some(AlertCategory::Surrender) => surrender_click(world, actor),
            _ => {}
        }
    }
}

// --- Surrender -------------------------------------------------------------------------------

/// `/datum/emote/living/surrender/run_emote`'s effect: 20 s of paralysis, the prompt goes.
pub fn surrendered(world: &mut World, mob: Entity) {
    status_effects::apply(world, mob, Incap::Paralyze, SURRENDER_PARALYZE_SECS);
    if let Ok(mut entity) = world.get_entity_mut(mob) {
        entity.remove::<SurrenderPrompt>();
    }
    clear_alert(world, mob, AlertCategory::Surrender);
}

/// `alert/status_effect/surrender/Click` -> `owner.emote("surrender")`.
// shortcut: the emote cooldown of a typed `*surrender` is not shared with the alert.
fn surrender_click(world: &mut World, mob: Entity) {
    if world.get::<Stat>(mob) != Some(&Stat::Conscious) {
        return;
    }
    if let Some(emote) = ss13_chat::emote::find("surrender") {
        crate::mobs::defense::emote(world, mob, emote.message);
    }
    surrendered(world, mob);
}

/// tg `run_emote` overrides of the chat's emotes.
fn emote_effects(world: &mut World, mut emotes: Local<MessageCursor<Emoted>>) {
    let surrendered_mobs: Vec<Entity> = emotes
        .read(world.resource::<Messages<Emoted>>())
        .filter(|e| e.key == "surrender")
        .map(|e| e.speaker)
        .collect();
    for mob in surrendered_mobs {
        if world.get::<Stat>(mob).is_some() {
            surrendered(world, mob);
        }
    }
}

/// `apply_status_effect(/datum/status_effect/surrender_timed)`: the surrender alert for 30 s
/// (unique: a running one keeps its time).
pub fn surrender_timed(world: &mut World, mob: Entity) {
    if world.get::<SurrenderPrompt>(mob).is_some() || world.get::<Stat>(mob).is_none() {
        return;
    }
    let until = now(world) + SURRENDER_TIMED_SECS;
    world.entity_mut(mob).insert(SurrenderPrompt { until });
    throw_alert(world, mob, AlertCategory::Surrender, "surrender", 0);
}

fn surrender_upkeep(world: &mut World) {
    let now = now(world);
    let mut query = world.query::<(Entity, &SurrenderPrompt)>();
    let ended: Vec<Entity> = query
        .iter(world)
        .filter(|(_, p)| now >= p.until)
        .map(|(e, _)| e)
        .collect();
    for mob in ended {
        world.entity_mut(mob).remove::<SurrenderPrompt>();
        clear_alert(world, mob, AlertCategory::Surrender);
    }
}

/// `/obj/item/pen/red/security/ranged_interact_with_atom`: the living mob on the clicked tile
/// (not the user) is asked to surrender.
fn security_pen_clicks(world: &mut World, mut clicks: Local<MessageCursor<RangedClick>>) {
    let clicks: Vec<RangedClick> = clicks
        .read(world.resource::<Messages<RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        if click.modifiers.right {
            continue;
        }
        let Some(pen) = world
            .get::<Inventory>(click.mob)
            .and_then(Inventory::active_item)
        else {
            continue;
        };
        let is_pen = crate::construction::path_of(world, pen)
            .is_some_and(|p| crate::obj_damage::is_under(&p, SECURITY_PEN));
        if !is_pen {
            continue;
        }
        let now = now(world);
        if world.get::<PenCooldown>(pen).is_some_and(|c| now < c.0) {
            tell_mob(world, click.mob, Span::Warning, "не готово!");
            continue;
        }
        let target = world
            .resource::<ss13_world::SpatialIndex>()
            .on_tile(click.target_pos)
            .iter()
            .copied()
            .find(|&e| e != click.mob && world.get::<Stat>(e).is_some());
        let Some(target) = target else {
            continue;
        };
        surrender_timed(world, target);
        let text = format!(
            "{} требует вашей немедленной сдачи! У вас есть 30 секунд, чтобы подчиниться!",
            name_of(world, click.mob)
        );
        tell_mob(world, target, Span::BoldWarning, &text);
        world
            .entity_mut(pen)
            .insert(PenCooldown(now + PEN_COOLDOWN_SECS));
    }
}

// --- Food buffs ------------------------------------------------------------------------------

/// `/datum/component/edible/proc/apply_buff` on the first bite: `complexity` is
/// `get_recipe_complexity` (0 for food nobody made), `purity` the food's average purity.
pub fn apply_food_buff(world: &mut World, eater: Entity, complexity: u32, purity: f32) {
    if complexity == 0 || world.get::<Stat>(eater).is_none() {
        return;
    }
    // `min(recipe_complexity, FOOD_COMPLEXITY_5)`; the buff's strength is the complexity.
    let strength = complexity.min(5) as u8;
    // `timeout_mod = purity * 2`: 100% at 50%.
    let until = now(world) + FOOD_BUFF_SECS * f64::from(purity * 2.0);
    world.entity_mut(eater).insert(FoodBuff { strength, until });
    // STATUS_EFFECT_REPLACE: the new alert replaces the old one (`food_buff_[strength]`).
    throw_alert(
        world,
        eater,
        AlertCategory::FoodBuff,
        "food_buff_",
        strength,
    );
}

fn food_buff_upkeep(world: &mut World) {
    let now = now(world);
    let mut query = world.query::<(Entity, &FoodBuff)>();
    let ended: Vec<Entity> = query
        .iter(world)
        .filter(|(_, b)| b.strength > 0 && now >= b.until)
        .map(|(e, _)| e)
        .collect();
    for mob in ended {
        world.entity_mut(mob).insert(FoodBuff::default());
        clear_alert(world, mob, AlertCategory::FoodBuff);
    }
}

// --- Exercise --------------------------------------------------------------------------------

/// `/datum/status_effect/exercised::duration`, seconds.
pub const EXERCISED_SECS: f64 = 15.0;
/// `SLEEP_QUALITY_WORKOUT_MULTIPLER`.
pub const SLEEP_QUALITY_WORKOUT_MULTIPLIER: f32 = 10.0;
const PUMPUP: &str = "/datum/reagent/drug/pumpup";
const OXANDROLONE_INVERSE: &str = "/datum/reagent/inverse/oxandrolone";
/// `supplementary_reagents_bonus`, seconds.
const WORKOUT_FOODS: [(&str, f32); 13] = [
    ("/datum/reagent/consumable/ethanol/protein_blend", 10.0),
    (OXANDROLONE_INVERSE, 8.0),
    ("/datum/reagent/consumable/nutriment/protein", 5.0),
    ("/datum/reagent/consumable/nutriment/vitamin", 4.0),
    ("/datum/reagent/consumable/milk", 4.0),
    ("/datum/reagent/consumable/rice", 3.0),
    ("/datum/reagent/consumable/eggwhite", 3.0),
    ("/datum/reagent/consumable/eggyolk", 2.0),
    ("/datum/reagent/consumable/nutraslop", 2.0),
    ("/datum/reagent/consumable/soymilk", 1.0),
    ("/datum/reagent/consumable/sugar", -1.0),
    ("/datum/reagent/consumable/monkey_energy", -1.0),
    ("/datum/reagent/consumable/nutriment/fat", -1.0),
];

/// `/datum/status_effect/exercised` (`STATUS_EFFECT_REFRESH`) until `until`: its alert and the
/// `exercise` moodlet; sleeping turns it into athletics experience
/// ([`sleep_workout`]).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Exercised {
    pub until: f64,
}

/// `workout_duration` on `current` seconds left: `(bonus + food + (athletics - 1) * 2 s) ×
/// modifier` more, up to the athletics `SKILL_VALUE_MODIFIER` (`Some(limit)`: exhausted).
pub fn workout_duration(
    current: f32,
    bonus: f32,
    food: f32,
    level: u8,
    modifier: f32,
    limit: f32,
) -> (f32, bool) {
    if bonus == 0.0 {
        return (current, false);
    }
    let bonus = (bonus + food + (f32::from(level) - 1.0) * 2.0) * modifier;
    if current + bonus >= limit {
        (limit, true)
    } else {
        (current + bonus, false)
    }
}

/// `/datum/mood_event/exercise` with `fitness_level`: the athletics level, halved for most,
/// a chore (−½) for the lazy, in full for the athletic.
pub fn exercise_mood(level: u8, lazy: bool, athletic: bool) -> MoodEvent {
    let base = f32::from(level);
    let (mood_change, description) = if lazy {
        (base * -0.5, "Тренировка - это такая рутинная работа!")
    } else if !athletic {
        (
            base * 0.5,
            "Тренировка - это немного рутинная работа, но она приносит большое удовлетворение.",
        )
    } else {
        (base, "Спорт высвобождает эндорфины!")
    };
    MoodEvent {
        id: "exercise",
        description,
        mood_change,
        timeout: 0.0,
        hidden: false,
    }
}

/// `apply_status_effect(/datum/status_effect/exercised, bonus_time)`: a new one lasts 15 s plus
/// the workout; a running one gets the workout added (`refresh`). Only mobs with a mind.
// tg's `refresh` compares the absolute end time with the limit and so always exhausts; the
// limit is meant for the time left, as on creation.
pub fn exercise(world: &mut World, mob: Entity, bonus_secs: f32) {
    use crate::personality::Personality;
    use crate::skills::{Modifier, Skill};
    if world.get::<crate::mind::MindData>(mob).is_none() {
        return;
    }
    let now = now(world);
    let current = world
        .get::<Exercised>(mob)
        .filter(|e| e.until > now)
        .map_or(EXERCISED_SECS as f32, |e| (e.until - now) as f32);
    // `workout_duration`: carbons only.
    let carbon = world.get::<Inventory>(mob).is_some();
    let mut modifier = 1.0;
    if crate::genetics::effects::is_hulk(world, mob) {
        modifier += 0.5;
    }
    if world.get::<crate::hunger::Fat>(mob).is_some_and(|f| f.0) {
        modifier -= 0.5;
    }
    let read: &World = world;
    let in_blood = |path: &str| crate::chem::blood_amount(read, mob, path) > 0.0;
    if in_blood(PUMPUP) {
        modifier += 3.0;
    }
    if in_blood(OXANDROLONE_INVERSE) {
        modifier += 2.0;
    }
    let food: f32 = WORKOUT_FOODS
        .iter()
        .filter(|&&(path, _)| in_blood(path))
        .map(|&(_, secs)| secs)
        .sum();
    let level = crate::skills::level(world, mob, Skill::Athletics);
    let limit = crate::skills::modifier(world, mob, Skill::Athletics, Modifier::Value);
    let bonus = if carbon { bonus_secs } else { 0.0 };
    let (secs, exhausted) = workout_duration(current, bonus, food, level, modifier, limit);
    if exhausted {
        tell_mob(
            world,
            mob,
            Span::BoldWarning,
            "Ваши мышцы истощены! Пожалуй, стоит поспать...",
        );
        crate::medical::diseases::emote(world, mob, "scream");
    }
    world.entity_mut(mob).insert(Exercised {
        until: now + f64::from(secs),
    });
    // `on_apply` / `refresh`: the moodlet anew, for the current fitness.
    let mood = exercise_mood(
        level,
        crate::personality::has(world, mob, Personality::Lazy),
        crate::personality::has(world, mob, Personality::Athletic),
    );
    crate::mood::clear_event(world, mob, "exercise");
    crate::mood::add_event(world, mob, "exercise", &mood);
    throw_alert(world, mob, AlertCategory::Exercised, "exercised", 0);
}

/// `sleeping/tick` of an exercised carbon with a mind and a sleep quality above 0: athletics
/// experience `secs × quality × 10`, the same off the exercise, a 2% "fitness" line.
pub fn sleep_workout(world: &mut World, mob: Entity, secs: f32, quality: f32) {
    let now = now(world);
    let Some(ex) = world
        .get::<Exercised>(mob)
        .copied()
        .filter(|e| e.until > now)
    else {
        return;
    };
    if quality <= 0.0 || world.get::<crate::mind::MindData>(mob).is_none() {
        return;
    }
    let gain = secs * quality * SLEEP_QUALITY_WORKOUT_MULTIPLIER;
    crate::skills::adjust_experience(world, mob, crate::skills::Skill::Athletics, gain, false);
    world.entity_mut(mob).insert(Exercised {
        until: ex.until - f64::from(gain),
    });
    if world
        .get_resource_or_init::<crate::combat::CombatRng>()
        .chance(2.0)
    {
        tell_mob(
            world,
            mob,
            Span::Notice,
            "Вы чувствуете, как улучшается ваша физическая форма!",
        );
    }
}

/// `exercised/on_remove`: the moodlet and the alert go with it.
fn exercise_upkeep(world: &mut World) {
    let now = now(world);
    let mut query = world.query::<(Entity, &Exercised)>();
    let ended: Vec<Entity> = query
        .iter(world)
        .filter(|(_, e)| now >= e.until)
        .map(|(e, _)| e)
        .collect();
    for mob in ended {
        world.entity_mut(mob).remove::<Exercised>();
        crate::mood::clear_event(world, mob, "exercise");
        clear_alert(world, mob, AlertCategory::Exercised);
    }
}

// --- Drugs -----------------------------------------------------------------------------------

/// The reagents of this module in `mob`'s blood, each `SSmobs` fire of `secs`
/// (`crate::chem`): space drugs set the drugginess (`30 SECONDS * metabolization_ratio *
/// seconds_per_tick`, the ratio taken as 1), bath salts make it stun immune, tinlux makes it
/// glow ([`tinlux_light`]).
pub fn reagents(world: &mut World, mob: Entity, secs: f32) {
    use crate::chem::blood_amount;
    // `REAGENT_DEAD_PROCESS`: also in a corpse.
    tinlux_light(world, mob, blood_amount(world, mob, TINLUX) > 0.0);
    if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return;
    }
    if blood_amount(world, mob, SPACE_DRUGS) > 0.0 {
        status_effects::set_timed(world, mob, Timed::Drugginess, 30.0 * secs, false);
    }
    if blood_amount(world, mob, BATH_SALTS) > 0.0 {
        status_effects::add_stun_immunity(world, mob, BATH_SALTS_SOURCE);
    }
}

const TINLUX: &str = "/datum/reagent/consumable/tinlux";

/// `/datum/status_effect/tinlux_light` is on (its light is the mob's).
#[derive(Component, Clone, Copy, Debug)]
pub struct TinluxLit;

/// `tinlux_light/on_apply`: `mob_light(2, 1.5, "#ccff33")`.
pub const TINLUX_LIGHT: ss13_shared::LightSource = ss13_shared::LightSource {
    range: 2.0,
    power: 1.5,
    color: [0xCC, 0xFF, 0x33],
    height: 1.0,
    angle: 360,
    dir: ss13_shared::Dir::South,
    offset: [0, 0],
};

/// Tinlux's `on_mob_add` / `on_mob_delete`: the glow follows the reagent.
// shortcut: the glow is the mob's own light, not a `moblight` dummy: it does not replace another
// light the mob already has (a glowy mutation, a fire) and leaves one that replaced it.
fn tinlux_light(world: &mut World, mob: Entity, in_blood: bool) {
    use ss13_shared::Optics;
    let lit = world.get::<TinluxLit>(mob).is_some();
    if in_blood && !lit {
        world.entity_mut(mob).insert(TinluxLit);
        match world.get_mut::<Optics>(mob) {
            Some(mut optics) if optics.light.is_none() => optics.light = Some(TINLUX_LIGHT),
            Some(_) => {}
            None => {
                world.entity_mut(mob).insert(Optics {
                    light: Some(TINLUX_LIGHT),
                    ..default()
                });
            }
        }
    } else if !in_blood && lit {
        world.entity_mut(mob).remove::<TinluxLit>();
        if let Some(mut optics) = world.get_mut::<Optics>(mob)
            && optics.light == Some(TINLUX_LIGHT)
        {
            optics.light = None;
        }
    }
}

/// `drugginess/on_apply` and `on_remove`: the alert and the moodlet follow the timer.
fn drug_upkeep(world: &mut World) {
    let mut started = world.query_filtered::<(Entity, &StatusEffects), Without<Drugged>>();
    let started: Vec<Entity> = started
        .iter(world)
        .filter(|(_, e)| e.drugginess > 0.0)
        .map(|(e, _)| e)
        .collect();
    let mut ended = world.query_filtered::<(Entity, Option<&StatusEffects>), With<Drugged>>();
    let ended: Vec<Entity> = ended
        .iter(world)
        .filter(|(_, e)| e.is_none_or(|e| e.drugginess <= 0.0))
        .map(|(e, _)| e)
        .collect();
    for mob in started {
        world.entity_mut(mob).insert(Drugged);
        crate::mood::add_event(world, mob, "drugged", &HIGH);
        throw_alert(world, mob, AlertCategory::Drugged, "high", 0);
    }
    for mob in ended {
        world.entity_mut(mob).remove::<Drugged>();
        crate::mood::clear_event(world, mob, "drugged");
        clear_alert(world, mob, AlertCategory::Drugged);
    }
}

/// `on_mob_end_metabolize`: bath salts gone from the blood, their stun immunity goes.
fn bath_salts_upkeep(world: &mut World) {
    let mut query = world.query::<(Entity, &status_effects::StunImmunity)>();
    let immune: Vec<Entity> = query
        .iter(world)
        .filter(|(_, i)| i.0.contains(&BATH_SALTS_SOURCE))
        .map(|(e, _)| e)
        .collect();
    for mob in immune {
        let dead = world.get::<Stat>(mob) == Some(&Stat::Dead);
        if dead || crate::chem::blood_amount(world, mob, BATH_SALTS) <= 0.0 {
            status_effects::remove_stun_immunity(world, mob, BATH_SALTS_SOURCE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn food_haste_scales_with_strength() {
        let buff = FoodBuff {
            strength: 5,
            until: 0.0,
        };
        assert!((buff.move_slowdown() + 0.2).abs() < 1e-6);
        assert!((buff.action_slowdown() + 0.3).abs() < 1e-6);
        assert_eq!(FoodBuff::default().move_slowdown(), 0.0);
    }

    #[test]
    fn food_buff_lasts_by_purity_and_ends_with_its_alert() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let mob = world.spawn((Stat::Conscious, Alerts::default())).id();
        apply_food_buff(&mut world, mob, 0, 0.5);
        assert!(world.get::<FoodBuff>(mob).is_none(), "nobody made it");
        apply_food_buff(&mut world, mob, 7, 0.25);
        let buff = *world.get::<FoodBuff>(mob).unwrap();
        assert_eq!(buff.strength, 5);
        assert_eq!(buff.until, FOOD_BUFF_SECS * 0.5);
        let alert = *world
            .get::<Alerts>(mob)
            .unwrap()
            .get(AlertCategory::FoodBuff)
            .unwrap();
        assert_eq!(alert.icon_state(), "food_buff_5");
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(151));
        food_buff_upkeep(&mut world);
        assert_eq!(world.get::<FoodBuff>(mob).unwrap().strength, 0);
        assert!(
            world
                .get::<Alerts>(mob)
                .unwrap()
                .get(AlertCategory::FoodBuff)
                .is_none()
        );
    }

    #[test]
    fn exercise_lasts_and_ends_with_its_alert() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let mob = world
            .spawn((
                Stat::Conscious,
                Alerts::default(),
                crate::mind::MindData::new("key", "name"),
            ))
            .id();
        exercise(&mut world, mob, 0.0);
        assert_eq!(world.get::<Exercised>(mob).unwrap().until, EXERCISED_SECS);
        let alerts = world.get::<Alerts>(mob).unwrap();
        assert_eq!(
            alerts.get(AlertCategory::Exercised).unwrap().state,
            "exercised"
        );
        // Without a mind nothing happens (`on_apply`).
        let mindless = world.spawn(Alerts::default()).id();
        exercise(&mut world, mindless, 0.0);
        assert!(world.get::<Exercised>(mindless).is_none());
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(16));
        exercise_upkeep(&mut world);
        assert!(world.get::<Exercised>(mob).is_none());
        let alerts = world.get::<Alerts>(mob).unwrap();
        assert!(alerts.get(AlertCategory::Exercised).is_none());
    }

    #[test]
    fn workouts_add_time_up_to_the_athletics_limit() {
        // No bonus time: nothing added (the punching bag).
        assert_eq!(
            workout_duration(15.0, 0.0, 5.0, 3, 2.0, 60.0),
            (15.0, false)
        );
        // (10 + 4 food + (3 - 1) × 2) × 1.5 = 27.
        assert_eq!(
            workout_duration(15.0, 10.0, 4.0, 3, 1.5, 60.0),
            (42.0, false)
        );
        assert_eq!(
            workout_duration(50.0, 10.0, 4.0, 3, 1.5, 60.0),
            (60.0, true)
        );
        assert_eq!(exercise_mood(4, false, true).mood_change, 4.0);
        assert_eq!(exercise_mood(4, false, false).mood_change, 2.0);
        assert_eq!(exercise_mood(4, true, false).mood_change, -2.0);
    }

    #[test]
    fn tinlux_glows_while_in_the_blood() {
        let mut world = World::new();
        let mob = world.spawn_empty().id();
        tinlux_light(&mut world, mob, true);
        let optics = world.get::<ss13_shared::Optics>(mob).unwrap();
        assert_eq!(optics.light, Some(TINLUX_LIGHT));
        tinlux_light(&mut world, mob, false);
        assert_eq!(world.get::<ss13_shared::Optics>(mob).unwrap().light, None);
        // Another light stays the mob's.
        let glowing = world
            .spawn(ss13_shared::Optics {
                light: Some(ss13_shared::LightSource {
                    power: 9.0,
                    ..TINLUX_LIGHT
                }),
                ..default()
            })
            .id();
        tinlux_light(&mut world, glowing, true);
        tinlux_light(&mut world, glowing, false);
        let light = world.get::<ss13_shared::Optics>(glowing).unwrap().light;
        assert_eq!(light.map(|l| l.power), Some(9.0));
    }

    #[test]
    fn drugginess_gives_the_high_alert_while_it_lasts() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let mob = world.spawn((Stat::Conscious, Alerts::default())).id();
        status_effects::set_timed(&mut world, mob, Timed::Drugginess, 10.0, false);
        drug_upkeep(&mut world);
        let alerts = world.get::<Alerts>(mob).unwrap();
        assert_eq!(alerts.get(AlertCategory::Drugged).unwrap().state, "high");
        world.entity_mut(mob).remove::<StatusEffects>();
        drug_upkeep(&mut world);
        assert!(world.get::<Drugged>(mob).is_none());
        assert!(
            world
                .get::<Alerts>(mob)
                .unwrap()
                .get(AlertCategory::Drugged)
                .is_none()
        );
    }

    #[test]
    fn surrender_prompt_runs_out_and_the_surrender_paralyzes() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let mob = world.spawn((Stat::Conscious, Alerts::default())).id();
        surrender_timed(&mut world, mob);
        assert!(
            world
                .get::<Alerts>(mob)
                .unwrap()
                .get(AlertCategory::Surrender)
                .is_some()
        );
        surrendered(&mut world, mob);
        assert_eq!(
            world.get::<StatusEffects>(mob).unwrap().paralyze,
            SURRENDER_PARALYZE_SECS
        );
        assert!(world.get::<SurrenderPrompt>(mob).is_none());
        assert!(
            world
                .get::<Alerts>(mob)
                .unwrap()
                .get(AlertCategory::Surrender)
                .is_none()
        );
        let other = world.spawn((Stat::Conscious, Alerts::default())).id();
        surrender_timed(&mut world, other);
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(31));
        surrender_upkeep(&mut world);
        assert!(world.get::<SurrenderPrompt>(other).is_none());
        assert!(
            world
                .get::<Alerts>(other)
                .unwrap()
                .get(AlertCategory::Surrender)
                .is_none()
        );
    }
}

//! Hunger (server): tg `/mob/living/carbon/human` `nutrition`, `satiety` and `overeatduration`,
//! the stomach's `handle_hunger` (`stomach/_stomach.dm`) every `SSmobs` fire, obesity
//! (`TRAIT_FAT`, `/datum/movespeed_modifier/obesity`) and the hunger bar
//! (`/atom/movable/screen/hunger`).
//!
//! - A mob starts with `rand(NUTRITION_LEVEL_START_MIN, NUTRITION_LEVEL_START_MAX)`
//!   (`/mob/Initialize`).
//! - Every fire a living mob with nutrition loses `HUNGER_FACTOR` per second (scaled by
//!   `crate::mood` sanity, tripled while `satiety` is negative); consumable reagents in the
//!   blood give it back (`ss13_reagents::metabolism`, `/datum/reagent/consumable/on_mob_life`).
//! - Over `NUTRITION_LEVEL_FULL` the mob overeats; after `OVEREAT_TIME_LIMIT` it gets fat (move
//!   slowdown 1.5), and loses it once the overeating time drains below the limit again.
//! - The metabolism efficiency follows nutrition: 1.25 when well fed with high satiety, 0.8
//!   below 200 ("sluggish"); it scales how fast non-food reagents metabolize.
//! - [`HungerHudData`] reaches the owner as the headless tgui interface [`HungerHudUi`]: the
//!   hunger state and the bar's fullness (`get_fullness(only_consumable = TRUE)`).
// shortcut: no glutton species, no stomach lining reagent.
// The nutrition moodlets are `crate::mood`'s.

use bevy::prelude::*;
use ss13_net::chat::server::SendChat;
use ss13_reagents::metabolism;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiSystems};

use crate::breath::MobsClock;
use crate::chem::{Chem, MobReagents};
use crate::combat::CombatRng;
use crate::health::{Health, HealthSystems, Stat};
use crate::hud::{HungerHudData, HungerHudUi};
use crate::medical::organs::{Organ, OrganHost, Organs, Slot};
use crate::server::PlayerMob;

/// `HUNGER_FACTOR`: nutrition lost per second.
pub const HUNGER_FACTOR: f32 = 0.05;
pub const NUTRITION_LEVEL_FAT: f32 = 600.0;
pub const NUTRITION_LEVEL_FULL: f32 = 550.0;
pub const NUTRITION_LEVEL_WELL_FED: f32 = 450.0;
pub const NUTRITION_LEVEL_FED: f32 = 350.0;
pub const NUTRITION_LEVEL_HUNGRY: f32 = 250.0;
pub const NUTRITION_LEVEL_VERY_HUNGRY: f32 = 200.0;
pub const NUTRITION_LEVEL_STARVING: f32 = 150.0;
pub const NUTRITION_LEVEL_START_MIN: u32 = 250;
pub const NUTRITION_LEVEL_START_MAX: u32 = 400;
/// `OVEREAT_TIME_LIMIT`, seconds.
pub const OVEREAT_TIME_LIMIT: f32 = 200.0;
/// `handle_hunger` caps `overeatduration` at 20 minutes.
const OVEREAT_CAP: f32 = 1200.0;
/// `MAX_SATIETY`.
pub const MAX_SATIETY: f32 = 600.0;
/// `/datum/movespeed_modifier/obesity` `multiplicative_slowdown`, deciseconds.
pub const OBESITY_SLOWDOWN: f32 = 1.5;

/// A human's nutrition state (tg vars on the mob).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Hunger {
    pub nutrition: f32,
    pub satiety: f32,
    /// `overeatduration`, seconds.
    pub overeat_secs: f32,
    pub metabolism_efficiency: f32,
}

impl Hunger {
    pub fn new(nutrition: f32) -> Self {
        Self {
            nutrition,
            satiety: 0.0,
            overeat_secs: 0.0,
            metabolism_efficiency: 1.0,
        }
    }

    /// tg `adjust_nutrition`: never below 0.
    pub fn adjust(&mut self, change: f32) {
        self.nutrition = (self.nutrition + change).max(0.0);
    }
}

/// tg `TRAIT_FAT` from obesity (`OBESITY` source).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fat(pub bool);

/// `HUNGER_STATE_*` of the hunger bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum HungerState {
    Starving = 0,
    Hungry = 2,
    Fine = 3,
    Full = 4,
    Fat = 5,
}

/// tg `/atom/movable/screen/hunger/update_hunger_state` for a fullness (already rounded).
pub fn hunger_state(fullness: f32, fat: bool) -> HungerState {
    if fat {
        HungerState::Fat
    } else if fullness >= 1.0 + NUTRITION_LEVEL_FULL {
        HungerState::Full
    } else if fullness >= 1.0 + NUTRITION_LEVEL_VERY_HUNGRY {
        HungerState::Fine
    } else if fullness >= 1.0 + NUTRITION_LEVEL_STARVING {
        HungerState::Hungry
    } else {
        HungerState::Starving
    }
}

/// [`handle_hunger_at`] at the neutral sanity of a new mood.
pub fn handle_hunger(
    hunger: &mut Hunger,
    fat: &mut bool,
    dead: bool,
    seconds: f32,
) -> Vec<(Span, &'static str)> {
    handle_hunger_at(hunger, fat, dead, seconds, crate::mood::SANITY_NEUTRAL)
}

/// tg `/obj/item/organ/stomach/handle_hunger` for `seconds` at `sanity`. Returns the lines for
/// the mob (`to_chat`), in order.
pub fn handle_hunger_at(
    hunger: &mut Hunger,
    fat: &mut bool,
    dead: bool,
    seconds: f32,
    sanity: f32,
) -> Vec<(Span, &'static str)> {
    let mut told = Vec::new();
    if *fat {
        if hunger.overeat_secs < OVEREAT_TIME_LIMIT {
            told.push((Span::Notice, "You feel fit again!"));
            *fat = false;
        }
    } else if hunger.overeat_secs >= OVEREAT_TIME_LIMIT {
        told.push((Span::Danger, "You suddenly feel blubbery!"));
        *fat = true;
    }
    if hunger.nutrition > 0.0 && !dead {
        let mut rate = HUNGER_FACTOR;
        if sanity > crate::mood::SANITY_DISTURBED {
            rate *= (1.0 - 0.002 * sanity).max(0.5);
        }
        if hunger.satiety > MAX_SATIETY {
            hunger.satiety = MAX_SATIETY;
        } else if hunger.satiety > 0.0 {
            hunger.satiety -= 1.0;
        } else if hunger.satiety < -MAX_SATIETY {
            hunger.satiety = -MAX_SATIETY;
        } else if hunger.satiety < 0.0 {
            hunger.satiety += 1.0;
            rate = 3.0 * HUNGER_FACTOR;
        }
        hunger.adjust(-rate * seconds);
    }
    let nutrition = hunger.nutrition;
    if nutrition > NUTRITION_LEVEL_FULL {
        if hunger.overeat_secs < OVEREAT_CAP {
            hunger.overeat_secs = (hunger.overeat_secs + seconds).min(OVEREAT_CAP);
        }
    } else if hunger.overeat_secs > 0.0 {
        hunger.overeat_secs = (hunger.overeat_secs - 2.0 * seconds).max(0.0);
    }
    if nutrition > NUTRITION_LEVEL_FAT {
        hunger.metabolism_efficiency = 1.0;
    } else if nutrition > NUTRITION_LEVEL_FED && hunger.satiety > 80.0 {
        if hunger.metabolism_efficiency != 1.25 {
            told.push((Span::Notice, "You feel vigorous."));
            hunger.metabolism_efficiency = 1.25;
        }
    } else if nutrition < NUTRITION_LEVEL_STARVING + 50.0 {
        if hunger.metabolism_efficiency != 0.8 {
            told.push((Span::Notice, "You feel sluggish."));
        }
        hunger.metabolism_efficiency = 0.8;
    } else {
        if hunger.metabolism_efficiency == 1.25 {
            told.push((Span::Notice, "You no longer feel vigorous."));
        }
        hunger.metabolism_efficiency = 1.0;
    }
    told
}

/// The hunger bar's data: tg rounds the fullness to 0.05 for the state, and the bar to 0.05 of
/// `NUTRITION_LEVEL_FULL`; it is hidden while "fine" with nothing left to digest.
pub fn hud_data(fullness: f32, nutrition: f32, fat: bool) -> HungerHudData {
    let fullness = if fat {
        NUTRITION_LEVEL_FAT
    } else {
        ss13_reagents::round_to(fullness, 0.05)
    };
    let state = hunger_state(fullness, fat);
    HungerHudData {
        state: state as u8,
        bar: ss13_reagents::round_to(fullness / NUTRITION_LEVEL_FULL, 0.05),
        shown: state != HungerState::Fine || (fullness - nutrition).abs() >= 1.0,
    }
}

impl ServerUi for HungerHudUi {
    type Source = &'static HungerHudData;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &HungerHudData) {}

    fn data(source: &HungerHudData, _: Entity, _: &()) -> HungerHudData {
        *source
    }
}

pub struct HungerServerPlugin;

impl Plugin for HungerServerPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<HungerHudUi>()
            .add_observer(give_stomach)
            .add_systems(
                Update,
                (hunger_life, sync_hunger_hud)
                    .chain()
                    .before(HealthSystems)
                    .before(UiSystems::Open),
            );
    }
}

/// `/mob/Initialize`: `set_nutrition(rand(NUTRITION_LEVEL_START_MIN, NUTRITION_LEVEL_START_MAX))`.
fn give_stomach(
    add: On<Add<Health>>,
    silicons: Query<(), With<crate::silicon::Silicon>>,
    basics: Query<(), With<crate::mobs::BasicMob>>,
    mut rng: ResMut<CombatRng>,
    mut commands: Commands,
) {
    // Silicons do not eat (tg `/mob/living/silicon` has no stomach); nor do basic mobs here.
    if silicons.contains(add.entity) || basics.contains(add.entity) {
        return;
    }
    let nutrition = rng.range(NUTRITION_LEVEL_START_MIN, NUTRITION_LEVEL_START_MAX) as f32;
    commands.entity(add.entity).insert((
        Hunger::new(nutrition),
        Fat(false),
        HungerHudData::default(),
    ));
}

/// [`handle_hunger`] of every mob each `SSmobs` fire.
// Cost: every 2 s one pass over mobs with a stomach (one per player), a few comparisons each.
fn hunger_life(
    clock: Res<MobsClock>,
    mut last_fired: Local<u64>,
    mut mobs: Query<
        (
            Entity,
            &mut Hunger,
            &mut Fat,
            &Stat,
            Option<&PlayerMob>,
            Option<&Organs>,
            Has<OrganHost>,
            Option<&crate::mood::Mood>,
            Option<&crate::species::Species>,
        ),
        Without<crate::medical::stasis::InStasis>,
    >,
    organs: Query<&Organ>,
    mut chat: MessageWriter<SendChat>,
    mut commands: Commands,
) {
    let fires = clock.times_fired - *last_fired;
    *last_fired = clock.times_fired;
    for _ in 0..fires {
        for (mob, mut hunger, mut fat, stat, player, owned, host, mood, species) in &mut mobs {
            // `TRAIT_NOHUNGER` from the species or an organ (plasmaman lungs).
            use crate::species::{Trait, organ_traits};
            let no_hunger = species.is_some_and(|s| s.0.def().has(Trait::NoHunger))
                || owned.is_some_and(|o| {
                    o.iter()
                        .filter_map(|e| organs.get(e).ok())
                        .any(|organ| organ_traits(&organ.path).contains(&Trait::NoHunger))
                });
            if no_hunger {
                continue;
            }
            // The stomach's `on_life` runs `handle_hunger` unless it is missing or failing.
            let stomach = owned.and_then(|o| {
                o.iter()
                    .filter_map(|e| organs.get(e).ok())
                    .find(|s| s.slot == Slot::Stomach)
            });
            if host && stomach.is_none_or(|s| s.failing) {
                continue;
            }
            let (mut h, mut f) = (hunger.clone(), fat.0);
            let sanity = mood.map_or(crate::mood::SANITY_NEUTRAL, |m| m.sanity);
            let told = handle_hunger_at(
                &mut h,
                &mut f,
                *stat == Stat::Dead,
                metabolism::SSMOBS_DT,
                sanity,
            );
            // Negative satiety: `SPT_PROB(round(-satiety / 77))` of `set_jitter_if_lower(10 SECONDS)`.
            let junk = hunger.nutrition > 0.0
                && *stat != Stat::Dead
                && hunger.satiety < 0.0
                && hunger.satiety >= -MAX_SATIETY;
            if junk {
                let chance = (-h.satiety / 77.0).round();
                commands.queue(move |world: &mut World| {
                    let jitter = {
                        let mut rng = world.get_resource_or_init::<crate::combat::CombatRng>();
                        crate::medical::wounds::spt_prob(&mut rng, chance, metabolism::SSMOBS_DT)
                    };
                    if jitter {
                        crate::status_effects::set_timed(
                            world,
                            mob,
                            crate::status_effects::Timed::Jitter,
                            10.0,
                            true,
                        );
                    }
                });
            }
            hunger.set_if_neq(h);
            fat.set_if_neq(Fat(f));
            let Some(player) = player else {
                continue;
            };
            for (span, text) in told {
                chat.write(SendChat {
                    clients: vec![player.client],
                    message: ChatMessage {
                        channel: if span == Span::Danger {
                            ChatChannel::Warning
                        } else {
                            ChatChannel::Info
                        },
                        segments: vec![ChatSegment::new(span, text)],
                    },
                });
            }
        }
    }
}

/// The owner's hunger bar follows nutrition, digestion and obesity.
fn sync_hunger_hud(
    mut mobs: Query<
        (
            Entity,
            &Hunger,
            &Fat,
            Option<&MobReagents>,
            &mut HungerHudData,
            Option<&PlayerMob>,
        ),
        Or<(Changed<Hunger>, Changed<Fat>, Changed<MobReagents>)>,
    >,
    chem: Option<Res<Chem>>,
    mut open: MessageWriter<OpenUi>,
) {
    for (mob, hunger, fat, reagents, mut hud, player) in &mut mobs {
        let fullness = match (chem.as_deref(), reagents) {
            (Some(chem), Some(r)) => {
                metabolism::fullness(&chem.0, hunger.nutrition, &r.blood, &r.stomach, true)
            }
            _ => hunger.nutrition,
        };
        let data = hud_data(fullness, hunger.nutrition, fat.0);
        if hud.set_if_neq(data)
            && let Some(player) = player
        {
            open.write(OpenUi::new::<HungerHudUi>(player.client, mob, mob, ""));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nutrition_decays_at_the_neutral_mood_rate() {
        let mut h = Hunger::new(300.0);
        let mut fat = false;
        let told = handle_hunger(&mut h, &mut fat, false, 2.0);
        // HUNGER_FACTOR * (1 - 0.002 * 100) * 2 s.
        assert!(
            (h.nutrition - (300.0 - 0.08)).abs() < 1e-4,
            "{}",
            h.nutrition
        );
        assert!(told.is_empty());
        // The dead do not hunger, and nutrition never goes below 0.
        handle_hunger(&mut h, &mut fat, true, 2.0);
        assert!((h.nutrition - 299.92).abs() < 1e-4);
        let mut empty = Hunger::new(0.01);
        handle_hunger(&mut empty, &mut fat, false, 2.0);
        assert_eq!(empty.nutrition, 0.0);
        // Negative satiety (junk food) triples the rate and recovers by one per fire.
        let mut junk = Hunger::new(300.0);
        junk.satiety = -10.0;
        handle_hunger(&mut junk, &mut fat, false, 2.0);
        assert!((junk.nutrition - (300.0 - 0.3)).abs() < 1e-4);
        assert_eq!(junk.satiety, -9.0);
    }

    #[test]
    fn hunger_states_follow_the_bar_thresholds() {
        assert_eq!(hunger_state(600.0, false), HungerState::Full);
        assert_eq!(hunger_state(551.0, false), HungerState::Full);
        assert_eq!(hunger_state(550.0, false), HungerState::Fine);
        assert_eq!(hunger_state(201.0, false), HungerState::Fine);
        assert_eq!(hunger_state(200.0, false), HungerState::Hungry);
        assert_eq!(hunger_state(151.0, false), HungerState::Hungry);
        assert_eq!(hunger_state(150.0, false), HungerState::Starving);
        assert_eq!(hunger_state(0.0, false), HungerState::Starving);
        assert_eq!(hunger_state(100.0, true), HungerState::Fat);
        // Hidden while fine with nothing digesting, shown while digesting or hungry.
        assert!(!hud_data(300.0, 300.0, false).shown);
        assert!(hud_data(320.0, 300.0, false).shown);
        let starving = hud_data(100.0, 100.0, false);
        assert!(starving.shown && starving.state == HungerState::Starving as u8);
        assert_eq!(hud_data(275.0, 275.0, false).bar, 0.5);
    }

    #[test]
    fn overeating_makes_fat_and_metabolism_follows_nutrition() {
        let mut h = Hunger::new(560.0);
        let mut fat = false;
        for _ in 0..100 {
            h.nutrition = 560.0;
            handle_hunger(&mut h, &mut fat, false, 2.0);
        }
        assert_eq!(h.overeat_secs, 200.0);
        assert!(!fat, "fat only on the next fire");
        let told = handle_hunger(&mut h, &mut fat, false, 2.0);
        assert!(fat && told.contains(&(Span::Danger, "You suddenly feel blubbery!")));
        // Below FULL the overeating time drains twice as fast.
        h.nutrition = 400.0;
        handle_hunger(&mut h, &mut fat, false, 2.0);
        let told = handle_hunger(&mut h, &mut fat, false, 2.0);
        assert!(!fat && told.contains(&(Span::Notice, "You feel fit again!")));
        // Starving: sluggish metabolism.
        let mut thin = Hunger::new(190.0);
        let told = handle_hunger(&mut thin, &mut fat, false, 2.0);
        assert_eq!(thin.metabolism_efficiency, 0.8);
        assert_eq!(told, [(Span::Notice, "You feel sluggish.")]);
        // Well fed with high satiety: vigorous.
        let mut fed = Hunger::new(400.0);
        fed.satiety = 100.0;
        handle_hunger(&mut fed, &mut fat, false, 2.0);
        assert_eq!(fed.metabolism_efficiency, 1.25);
    }
}

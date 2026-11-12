//! Talking toys (server), after tg `code/game/objects/items/toys.dm`: action figures
//! (`/obj/item/toy/figure`), pull-string toys (`/obj/item/toy/talking`), the big red button and
//! the toy nuke.
//!
//! - An action figure used in hand tells its holder its line (`toysay`) with its sound, every
//!   5 s.
//! - A talking toy (owl, griffin, Codex Gigas, toy AI) used in hand: the string-pull (or button)
//!   message, a click, and the toy says one of its lines aloud (the toy AI an ion law); 3 s
//!   apart (the Codex 6 s).
//! - The big red button: a far explosion and a camera shake within 10 tiles, every 30 s.
//! - The toy nuke: the alarm and its lit sprites for 14 s, then cool until the minute is up
//!   (the display shows the seconds left meanwhile). Emagged ("explosive simulation enabled"),
//!   it explodes (light 1) 14.5 s after the button.
//! - The Captain's Aid cycles its display: port, starboard, fore, aft, off.
//!
//! Cost: one pass over running toy nukes per tick, only while any run.
// shortcut: no talking through a figure or the dummy (`/datum/element/toy_talk`, `.l`/`.r`),
// no owl and griffin phonemes (`chatter`), no ventriloquist dummy; the red button shakes the
// screens at once (tg 0.8 s later) and, like the Mini-Meteor, also those of the unconscious.

use bevy::ecs::message::Messages;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmItem};
use ss13_net::camera::server::SendCameraEffect;
use ss13_shared::chat::Span;
use ss13_shared::protocol::CameraEffect;

use crate::chem::{map_pos, name_of, tell_mob, visible};
use crate::combat::CombatRng;
use crate::sound::{PlaySound, playsound};
use crate::tools::now;

const FIGURE: &str = "/obj/item/toy/figure";
const TALKING: &str = "/obj/item/toy/talking";
const TALKING_AI: &str = "/obj/item/toy/talking/ai";
const CODEX: &str = "/obj/item/toy/talking/codex_gigas";
const RED_BUTTON: &str = "/obj/item/toy/redbutton";
const TOY_NUKE: &str = "/obj/item/toy/nuke";
const CAPTAINSAID: &str = "/obj/item/toy/captainsaid";
/// The Captain's Aid's `modes`.
const AID_MODES: [&str; 5] = ["off", "port", "starboard", "fore", "aft"];
const CLICK: &str = "sound/machines/click.ogg";
const EXPLOSION_FAR: &str = "sound/effects/explosion/explosionfar.ogg";
const NUKE_ALARM: &str = "sound/announcer/alarm/nuke_alarm.ogg";
/// `/obj/item/toy/figure` `cooldown = world.time + 50`.
const FIGURE_COOLDOWN: f64 = 5.0;
/// The red button's and the toy nuke's cooldowns.
const RED_BUTTON_COOLDOWN: f64 = 30.0;
const NUKE_COOLDOWN: f64 = 60.0;
const SHAKE_RANGE: u32 = 10;

/// `/obj/item/toy/figure` subtypes: `toysay`, `toysound` (`None`: silent, the mime).
const FIGURES: &[(&str, &str, Option<&str>)] = &[
    ("/cmo", "Suit sensors!", Some(CLICK)),
    ("/assistant", "Greytide world wide!", Some(CLICK)),
    ("/atmos", "Glory to Atmosia!", Some(CLICK)),
    ("/bartender", "Where is Pun Pun?", Some(CLICK)),
    ("/bitrunner", "I'm in...", Some(CLICK)),
    (
        "/borg",
        "I. LIVE. AGAIN.",
        Some("sound/mobs/non-humanoids/cyborg/liveagain.ogg"),
    ),
    ("/botanist", "Blaze it!", Some(CLICK)),
    ("/captain", "Any heads of staff?", Some(CLICK)),
    ("/cargotech", "For Cargonia!", Some(CLICK)),
    ("/ce", "Wire the solars!", Some(CLICK)),
    ("/chaplain", "Praise Space Jesus!", Some(CLICK)),
    ("/chef", "I'll make you into a burger!", Some(CLICK)),
    ("/chemist", "Get your pills!", Some(CLICK)),
    ("/clown", "Honk!", Some("sound/items/bikehorn.ogg")),
    ("/ian", "Arf!", Some(CLICK)),
    (
        "/detective",
        "This airlock has grey jumpsuit and insulated glove fibers on it.",
        Some(CLICK),
    ),
    ("/dsquad", "Kill 'em all!", Some(CLICK)),
    (
        "/engineer",
        "Oh god, the singularity is loose!",
        Some(CLICK),
    ),
    ("/geneticist", "Smash!", Some(CLICK)),
    ("/hop", "Giving out all access!", Some(CLICK)),
    ("/hos", "Go ahead, make my day.", Some(CLICK)),
    (
        "/qm",
        "Please sign this form in triplicate and we will see about geting you a welding mask \
         within 3 business days.",
        Some(CLICK),
    ),
    ("/janitor", "Look at the signs, you idiot.", Some(CLICK)),
    ("/lawyer", "My client is a dirty traitor!", Some(CLICK)),
    ("/curator", "One day while...", Some(CLICK)),
    ("/md", "The patient is already dead!", Some(CLICK)),
    ("/coroner", "Get the damn Revenant outta here!", Some(CLICK)),
    (
        "/paramedic",
        "And the best part? I'm not even a real doctor!",
        Some(CLICK),
    ),
    (
        "/psychologist",
        "Alright, just take these happy pills!",
        Some(CLICK),
    ),
    ("/prisoner", "I did not hit her! I did not!", Some(CLICK)),
    ("/mime", "...", None),
    ("/miner", "COLOSSUS RIGHT OUTSIDE THE BASE!", Some(CLICK)),
    ("/ninja", "I am the shadow warrior!", Some(CLICK)),
    ("/wizard/special", "CLANG!", Some("sound/effects/clang.ogg")),
    (
        "/wizard",
        "EI NATH!",
        Some("sound/effects/magic/disintegrate.ogg"),
    ),
    ("/rd", "Blowing all of the borgs!", Some(CLICK)),
    (
        "/roboticist",
        "Big stompy mechs!",
        Some("sound/vehicles/mecha/mechstep.ogg"),
    ),
    ("/scientist", "I call ordnance.", Some(EXPLOSION_FAR)),
    ("/syndie", "Get that fucking disk!", Some(CLICK)),
    (
        "/secofficer",
        "I am the law!",
        Some("sound/runtime/complionator/dredd.ogg"),
    ),
    (
        "/warden",
        "Seventeen minutes for coughing at an officer!",
        Some(CLICK),
    ),
];

/// `/obj/item/toy/talking` subtypes: `messages`, `recharge_time` (seconds).
const TALKERS: &[(&str, &[&str], f64)] = &[
    (
        "/owl",
        &[
            "You won't get away this time, Griffin!",
            "Stop right there, criminal!",
            "Hoot! Hoot!",
            "I am the night!",
        ],
        3.0,
    ),
    (
        "/griffin",
        &[
            "You can't stop me, Owl!",
            "My plan is flawless! The vault is mine!",
            "Caaaawwww!",
            "You will never catch me!",
        ],
        3.0,
    ),
    (
        "/codex_gigas",
        &[
            "You must challenge the devil to a dance-off!",
            "The devils true name is Ian",
            "The devil hates salt!",
            "Would you like infinite power?",
            "Would you like infinite wisdom?",
            "Would you like infinite healing?",
        ],
        6.0,
    ),
    (
        "",
        &[
            "I'm super generic!",
            "Mathematics class is of variable difficulty!",
        ],
        3.0,
    ),
];

/// The game time a toy is ready again (tg `cooldown`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct ToyCooldown(pub f64);

/// `obj_flags & EMAGGED` of a toy nuke.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExplosiveNuke;

/// A toy nuke's button sequence (tg's sleeps in `attack_self`): `step` happens at `at`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct NukeSequence {
    pub step: NukeStep,
    pub at: f64,
    pub user: Entity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NukeStep {
    /// 0.5 s: the alarm (and the lit sprite).
    Alarm,
    /// 13.5 s later: cooling down.
    Cool,
    /// The cooldown is over: idle again.
    Idle,
    /// Emagged, 14 s after the alarm: it explodes.
    Explode,
}

pub struct TalkingToysPlugin;

impl Plugin for TalkingToysPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // Figures, pull-string toys, the red button and the toy nuke used in hand.
        app.add_click_handler(ClickPhase::AttackSelf, 3327, |w, c| {
            attack_self(w, c.mob, c.target)
        });
        // An emag on a toy nuke in the other hand or on the floor.
        app.add_click_handler(ClickPhase::OtherHand, 497, |w, c| {
            c.held.is_some_and(|h| emag_nuke(w, c.mob, h, c.target))
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 3647, |w, c| {
            c.held.is_some_and(|h| emag_nuke(w, c.mob, h, c.target))
        });
        app.add_systems(
            Update,
            nuke_steps.run_if(any_with_component::<NukeSequence>),
        );
    }
}

fn path_of(world: &World, e: Entity) -> String {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

/// A figure's `toysay` and `toysound` by its type (the base figure's when not listed).
pub fn figure_line(path: &str) -> (&'static str, Option<&'static str>) {
    path.strip_prefix(FIGURE)
        .and_then(|rest| {
            FIGURES
                .iter()
                .find(|(sub, ..)| is_under(rest, sub))
                .map(|&(_, say, sound)| (say, sound))
        })
        .unwrap_or(("What the fuck did you do?", Some(CLICK)))
}

/// A talking toy's lines and recharge by its type.
pub fn talker(path: &str) -> (&'static [&'static str], f64) {
    let rest = path.strip_prefix(TALKING).unwrap_or("");
    TALKERS
        .iter()
        .find(|(sub, ..)| sub.is_empty() || is_under(rest, sub))
        .map(|&(_, lines, recharge)| (lines, recharge))
        .unwrap_or((&[], 3.0))
}

/// Ready now; starts the next `cooldown` when it is.
fn take_cooldown(world: &mut World, toy: Entity, cooldown: f64) -> Result<(), f64> {
    let t = now(world);
    let ready = world.get::<ToyCooldown>(toy).map_or(0.0, |c| c.0);
    if t < ready {
        return Err(ready - t);
    }
    world.entity_mut(toy).insert(ToyCooldown(t + cooldown));
    Ok(())
}

/// tg `attack_self` of the talking toys. True when `item` is one.
pub fn attack_self(world: &mut World, mob: Entity, item: Entity) -> bool {
    let path = path_of(world, item);
    if is_under(&path, FIGURE) {
        figure(world, mob, item, &path);
    } else if is_under(&path, TALKING) {
        talk(world, mob, item, &path);
    } else if is_under(&path, RED_BUTTON) {
        red_button(world, mob, item);
    } else if is_under(&path, TOY_NUKE) {
        nuke(world, mob, item);
    } else if is_under(&path, CAPTAINSAID) {
        captains_aid(world, mob, item);
    } else {
        return false;
    }
    true
}

/// `/obj/item/toy/figure/attack_self`.
fn figure(world: &mut World, mob: Entity, item: Entity, path: &str) {
    if take_cooldown(world, item, FIGURE_COOLDOWN).is_err() {
        return;
    }
    let (say, sound) = figure_line(path);
    let name = name_of(world, item);
    tell_mob(world, mob, Span::Notice, &format!("{name} says \"{say}\""));
    if let Some(sound) = sound
        && let Some(at) = map_pos(world, mob)
    {
        playsound(world, PlaySound::new(at, sound, 20.0, true));
    }
}

/// `/obj/item/toy/talking/attack_self`: the activation message, a click, one line said aloud.
fn talk(world: &mut World, mob: Entity, item: Entity, path: &str) {
    let (lines, recharge) = talker(path);
    if take_cooldown(world, item, recharge).is_err() {
        return;
    }
    let (user, toy) = (name_of(world, mob), name_of(world, item));
    if is_under(path, CODEX) {
        visible(
            world,
            mob,
            &format!("You press the button on {toy}."),
            &format!("{user} presses the button on {toy}."),
            Span::Notice,
        );
    } else {
        visible(
            world,
            mob,
            &format!("You pull the string on {toy}."),
            &format!("{user} pulls the string on {toy}."),
            Span::Notice,
        );
    }
    if let Some(at) = map_pos(world, item) {
        playsound(world, PlaySound::new(at, CLICK, 20.0, true));
    }
    let line = if is_under(path, TALKING_AI) {
        // `generate_messages()`: `list(generate_ion_law())`.
        let seed = u64::from(world.resource_mut::<CombatRng>().range(1, u32::MAX));
        let words = crate::events::ion_storm::IonWords::load(world);
        let mut rng = ss13_mapload::spawner::Rng::new(seed);
        crate::events::ion_storm::generate_ion_law(&words, &mut rng)
    } else if lines.is_empty() {
        return;
    } else {
        let i = world
            .resource_mut::<CombatRng>()
            .range(0, lines.len() as u32 - 1) as usize;
        lines[i].to_owned()
    };
    crate::atmos_machinery::machine_say(world, item, &line);
}

/// `/obj/item/toy/redbutton/attack_self`.
fn red_button(world: &mut World, mob: Entity, item: Entity) {
    if take_cooldown(world, item, RED_BUTTON_COOLDOWN).is_err() {
        tell_mob(world, mob, Span::Warning, "Nothing happens.");
        return;
    }
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        "You press the button, it plays a loud noise!",
        &format!("{user} presses the big red button."),
        Span::Warning,
    );
    let Some(at) = map_pos(world, item) else {
        return;
    };
    playsound(world, PlaySound::new(at, EXPLOSION_FAR, 50.0, false));
    if world.contains_resource::<Messages<SendCameraEffect>>() {
        for client in crate::obj_damage::clients_in_range(world, at, SHAKE_RANGE, &[]) {
            world.write_message(SendCameraEffect {
                client,
                effect: CameraEffect::Shake {
                    duration_ds: 2.0,
                    strength: 1.0,
                },
            });
        }
    }
}

/// The Captain's Aid's `current_mode` (1-based; 1 is off).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AidMode(pub usize);

/// The next `current_mode` after `mode`: past the last it turns off.
pub fn next_aid_mode(mode: usize) -> usize {
    if mode < AID_MODES.len() { mode + 1 } else { 1 }
}

/// `/obj/item/toy/captainsaid/attack_self`: the next heading on its display.
fn captains_aid(world: &mut World, mob: Entity, item: Entity) {
    let mode = next_aid_mode(world.get::<AidMode>(item).map_or(1, |m| m.0));
    world.entity_mut(item).insert(AidMode(mode));
    if let Some(at) = map_pos(world, item) {
        playsound(
            world,
            PlaySound::new(at, "sound/items/tools/screwdriver2.ogg", 50.0, true),
        );
    }
    let text = if mode == 1 {
        "turned off".to_owned()
    } else {
        format!("set to {mode}")
    };
    crate::balloon::balloon_alert(world, mob, item, &text);
    set_state(world, item, &format!("captainsaid_{}", AID_MODES[mode - 1]));
}

/// `/obj/item/toy/nuke/attack_self`.
fn nuke(world: &mut World, mob: Entity, item: Entity) {
    if world
        .get::<NukeSequence>(item)
        .is_some_and(|s| s.step == NukeStep::Explode)
    {
        return;
    }
    if let Err(left) = take_cooldown(world, item, NUKE_COOLDOWN) {
        tell_mob(
            world,
            mob,
            Span::Warning,
            &format!(
                "Nothing happens, and '{}' appears on the small display.",
                left.round()
            ),
        );
        return;
    }
    let toy = name_of(world, item);
    if world.get::<ExplosiveNuke>(item).is_some() {
        tell_mob(
            world,
            mob,
            Span::Notice,
            &format!("You activate {toy}, it plays a loud noise!"),
        );
    } else {
        let user = name_of(world, mob);
        visible(
            world,
            mob,
            &format!("You activate {toy}, it plays a loud noise!"),
            &format!("{user} presses a button on {toy}."),
            Span::Warning,
        );
    }
    let at = now(world) + 0.5;
    world.entity_mut(item).insert(NukeSequence {
        step: NukeStep::Alarm,
        at,
        user: mob,
    });
}

/// The toy nuke's next step, when its time came.
fn nuke_steps(world: &mut World) {
    let t = now(world);
    let due: Vec<(Entity, NukeSequence)> = world
        .query::<(Entity, &NukeSequence)>()
        .iter(world)
        .filter(|(_, s)| s.at <= t)
        .map(|(e, s)| (e, *s))
        .collect();
    for (nuke, seq) in due {
        let explosive = world.get::<ExplosiveNuke>(nuke).is_some();
        let next = match seq.step {
            NukeStep::Alarm => {
                if !explosive {
                    set_state(world, nuke, "nuketoy");
                }
                if let Some(at) = map_pos(world, nuke) {
                    playsound(world, PlaySound::new(at, NUKE_ALARM, 20.0, false));
                }
                if explosive {
                    Some((NukeStep::Explode, t + 14.0))
                } else {
                    Some((NukeStep::Cool, t + 13.5))
                }
            }
            NukeStep::Cool => {
                set_state(world, nuke, "nuketoycool");
                let ready = world.get::<ToyCooldown>(nuke).map_or(t, |c| c.0);
                Some((NukeStep::Idle, ready.max(t)))
            }
            NukeStep::Idle => {
                set_state(world, nuke, "nuketoyidle");
                None
            }
            NukeStep::Explode => {
                if let Some(at) = map_pos(world, nuke) {
                    let toy = name_of(world, nuke);
                    // `user.visible_message`: from the user, wherever it went.
                    if world.get_entity(seq.user).is_ok() {
                        let text = format!("{toy} violently explodes!");
                        visible(world, seq.user, &text, &text, Span::Warning);
                    }
                    crate::explosions::explosion(
                        world,
                        at,
                        crate::explosions::Explosion::new(-1, -1, 1).cause(&toy),
                    );
                }
                if world.get_entity(nuke).is_ok() {
                    world.despawn(nuke);
                }
                None
            }
        };
        let Ok(mut e) = world.get_entity_mut(nuke) else {
            continue;
        };
        match next {
            Some((step, at)) => {
                e.insert(NukeSequence { step, at, ..seq });
            }
            None => {
                e.remove::<NukeSequence>();
            }
        }
    }
}

/// The toy's sprite shows `state` of its own icon.
fn set_state(world: &mut World, item: Entity, state: &str) {
    let Some(icon) = world.get::<DmAtom>(item).map(|a| a.icon.clone()) else {
        return;
    };
    let id = world
        .get_resource::<ss13_mapload::IconIndex>()
        .and_then(|i| i.get(&icon, state));
    if let Some(id) = id
        && let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(item)
    {
        look.icon_state = id;
    }
    if let Some(mob) = world.get::<crate::inventory::CarriedBy>(item).map(|c| c.0) {
        crate::inventory::refresh_look(world, mob);
    }
}

/// `/obj/item/toy/nuke/emag_act`. True when it was an emag on a toy nuke.
fn emag_nuke(world: &mut World, mob: Entity, held: Entity, target: Entity) -> bool {
    if !is_under(&path_of(world, target), TOY_NUKE) || !crate::emag::is_emag(world, held) {
        return false;
    }
    if world.get::<ExplosiveNuke>(target).is_none() {
        crate::balloon::balloon_alert(world, mob, target, "explosive simulation enabled");
        world.entity_mut(target).insert(ExplosiveNuke);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn figures_and_talkers_follow_the_type_tree() {
        assert_eq!(
            figure_line("/obj/item/toy/figure/wizard/special").0,
            "CLANG!"
        );
        assert_eq!(figure_line("/obj/item/toy/figure/wizard").0, "EI NATH!");
        assert_eq!(figure_line("/obj/item/toy/figure/mime"), ("...", None));
        assert_eq!(
            figure_line("/obj/item/toy/figure").0,
            "What the fuck did you do?"
        );
        assert_eq!(talker("/obj/item/toy/talking/codex_gigas").1, 6.0);
        assert_eq!(talker("/obj/item/toy/talking/owl").0.len(), 4);
        assert_eq!(talker("/obj/item/toy/talking").0.len(), 2);
        assert_eq!(next_aid_mode(1), 2);
        assert_eq!(next_aid_mode(5), 1);
    }
}

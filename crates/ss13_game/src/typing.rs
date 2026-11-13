//! tg typing indicators and forced speech of tgui-say (`code/modules/client/verbs/typing.dm`,
//! `code/modules/tgui_input/say_modal/typing.dm`, `speech.dm`).
//!
//! - While the player has the say line open in an in-character channel, the mob shows
//!   `talk.dmi`'s `[bubble_icon]3` ("thinking"); while keys are pressed, `[bubble_icon]0`
//!   ("typing"), at `TYPING_LAYER` over everything else on the mob. Everyone who sees the mob
//!   sees it.
//! - The client decides the state ([`ClientIntent::Typing`], sent on a change only, see
//!   `ss13_chat::client::panel::TypingTracker`), including tg's 5 s fallback from typing to
//!   thinking (`stop_typing` timer).
//! - The server shows it only on the client's own living mob (an AI's on its core, tg
//!   `/mob/living/silicon/ai`), not a ghost (tg `/mob/dead` has no indicator), not while
//!   unconscious or in crit (`IS_UNCONSCIOUS_OR_CRIT`), not with
//!   `TRAIT_HIDE_THINKING_INDICATOR` (the abductor tongue). Speech (`ClientIntent::Say`) ends it
//!   (tgui-say closes on Enter), as does the client leaving the mob.
//! - The overlay changes at most once per [`MIN_CHANGE_SECS`] (tgui-say debounces `thinking`
//!   by 0.4 s), so a client flipping states cannot flood replication; turning it off is never
//!   delayed.
//! - `force_say`: a human hurt by a held item (`attacked_by`) or newly stunned, knocked down or
//!   paralyzed (`/datum/status_effect/incapacitating/try_force_say`) while typing blurts out the
//!   line ([`ClientIntent::SayDraft`], tg `saved_text`) cut short with a hurt phrase
//!   (`alter_entry`), and its say line closes.
// shortcut: no `typingIndicator` preference (tg `/datum/preference/toggle/typing_indicator`):
// always on, until the game preferences have toggles.
// shortcut: the BandaStation sign-language bubble (`signlang`) and the chameleon borg's
// `robot` bubble are not switched, until those systems exist.
// shortcut: the line counts as open only in an in-character channel (the client sends nothing
// for OOC/LOOC/pray), so a force_say there (tg: a hurt phrase in say) does not happen.
// shortcut: knocked unconscious while typing says nothing (tg says the line first, in
// `on_apply`; here the forced line reaches speech a tick later, when the mob cannot talk).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_net::chat::server::SendCloseSay;
use ss13_net::server::{IntentReceived, NetServerSystems};
use ss13_shared::chat::MAX_MESSAGE_LEN;
use ss13_shared::protocol::{ClientIntent, TypingState};
use ss13_shared::{Appearance, IconStateId, Overlay};

use crate::combat::CombatRng;
use crate::ghost::Ghost;
use crate::health::{Health, Stat};
use crate::server::{PlayerMob, Players};
use crate::status_effects::{Incap, StatusEffects};

pub const TALK_DMI: &str = "icons/mob/effects/talk.dmi";
/// `/mob/living` `bubble_icon`.
pub const DEFAULT_BUBBLE: &str = "default";
/// The shortest time between two shown changes, seconds.
pub const MIN_CHANGE_SECS: f64 = 0.4;
/// `/datum/tgui_say` `hurt_phrases`.
pub const HURT_PHRASES: [&str; 7] = ["ГХА!", "ГРХ!", "УГХ!", "АРГХ!", "АУ!", "МГХ!", "АХХ!"];
/// `/datum/status_effect/incapacitating/paralyzed` `alter_phrases`: "Why am I about to be froz-".
pub const PARALYZED_PHRASES: [&str; 1] = [""];
/// The incapacitating effects that force a say, with `force_say_chance` (immobilized and
/// incapacitated have 0; unconscious and sleeping are knocked out by then, see above).
const FORCING: [(Incap, f32); 3] = [
    (Incap::Stun, 66.0),
    (Incap::Knockdown, 66.0),
    (Incap::Paralyze, 66.0),
];

/// On a mob whose player used the say line: what the player asked for and what is drawn.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct TypingIndicator {
    /// The client's state (tgui-say `window_open` in an in-character channel).
    pub asked: TypingState,
    /// `asked`, unless hidden (`TRAIT_HIDE_THINKING_INDICATOR`).
    pub wanted: TypingState,
    pub shown: TypingState,
    changed_at: f64,
    /// `[bubble_icon]0` and `[bubble_icon]3`.
    states: [IconStateId; 2],
    /// [`FORCING`] effects active last tick, by index bit: a new one forces a say.
    incaps: u8,
}

impl TypingIndicator {
    fn icon(&self, state: TypingState) -> Option<IconStateId> {
        match state {
            TypingState::Off => None,
            TypingState::Typing => Some(self.states[0]),
            TypingState::Thinking => Some(self.states[1]),
        }
    }
}

/// tg `/datum/tgui_say` `saved_text`: the line's in-character text, said by [`force_say`].
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct SayDraft(pub String);

pub struct TypingPlugin;

impl Plugin for TypingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, typing_intents).add_systems(
            PostUpdate,
            (incap_force_say, show_indicators)
                .chain()
                .before(NetServerSystems::Send),
        );
    }
}

/// The mob the client's indicator is drawn on: its own living mob, or the core of the AI whose
/// eye it controls.
fn indicator_mob(world: &World, mob: Entity) -> Option<Entity> {
    if let Some(eye) = world.get::<crate::silicon::ai::AiEye>(mob) {
        return Some(eye.ai);
    }
    (world.get::<Health>(mob).is_some() && world.get::<Ghost>(mob).is_none()).then_some(mob)
}

/// tg `bubble_icon` of `mob`.
pub fn bubble_icon(world: &World, mob: Entity) -> &str {
    if let Some(basic) = world.get::<crate::mobs::BasicMob>(mob)
        && !basic.def.bubble_icon.is_empty()
    {
        return &basic.def.bubble_icon;
    }
    if let Some(borg) = world.get::<crate::silicon::cyborg::Cyborg>(mob) {
        // `/mob/living/silicon/robot/model/syndicate`.
        let syndicate = borg.model.is_some_and(|i| {
            world
                .get_resource::<crate::silicon::Silicons>()
                .and_then(|s| s.0.models.get(i))
                .is_some_and(|m| {
                    crate::silicon::cyborg::SYNDICATE_MODELS
                        .iter()
                        .any(|(path, _)| *path == m.path)
                })
        });
        return if syndicate { "syndibot" } else { "robot" };
    }
    // `/mob/living/silicon`: the AI and the pAI.
    if world.get::<crate::silicon::Silicon>(mob).is_some()
        || world.get::<crate::silicon::pai::Pai>(mob).is_some()
    {
        return "machine";
    }
    DEFAULT_BUBBLE
}

/// `talk.dmi`'s typing and thinking states for `mob`, `default` ones for a bubble the file lacks.
fn bubble_states(world: &World, mob: Entity) -> Option<[IconStateId; 2]> {
    let icons = world.get_resource::<IconIndex>()?;
    let pair = |bubble: &str| {
        Some([
            icons.get(TALK_DMI, &format!("{bubble}0"))?,
            icons.get(TALK_DMI, &format!("{bubble}3"))?,
        ])
    };
    pair(bubble_icon(world, mob)).or_else(|| pair(DEFAULT_BUBBLE))
}

/// `/datum/tgui_say/proc/on_message` `thinking`/`typing`/`close`/`save` and
/// `delegate_speech`: the latest state and draft of each client this tick go to its mob.
// Cost: per tick, one entry per client that changed its say line or spoke.
fn typing_intents(world: &mut World, mut cursor: Local<MessageCursor<IntentReceived>>) {
    let mut latest: Vec<(Entity, Option<TypingState>, Option<String>)> = Vec::new();
    for i in cursor.read(world.resource::<Messages<IntentReceived>>()) {
        let (state, draft) = match &i.intent {
            ClientIntent::Typing(state) => (Some(*state), None),
            ClientIntent::Say(_) => (Some(TypingState::Off), None),
            ClientIntent::SayDraft(text) => (None, Some(text)),
            _ => continue,
        };
        let entry = match latest.iter_mut().position(|(c, ..)| *c == i.client) {
            Some(at) => &mut latest[at],
            None => {
                latest.push((i.client, None, None));
                latest.last_mut().expect("pushed")
            }
        };
        if let Some(state) = state {
            entry.1 = Some(state);
        }
        if let Some(draft) = draft {
            // Untrusted: kept no longer than a say line.
            entry.2 = Some(draft.chars().take(MAX_MESSAGE_LEN).collect());
        }
    }
    for (client, state, draft) in latest {
        let Some(mob) = world.resource::<Players>().mob(client) else {
            continue;
        };
        if world
            .get::<PlayerMob>(mob)
            .is_none_or(|p| p.client != client)
        {
            continue;
        }
        let Some(target) = indicator_mob(world, mob).filter(|t| world.get_entity(*t).is_ok())
        else {
            continue;
        };
        if let Some(draft) = draft {
            world.entity_mut(target).insert(SayDraft(draft));
        }
        if let Some(state) = state {
            set_wanted(world, target, state);
        }
    }
}

/// The player of `mob` asks for `state` (`start_thinking`, `start_typing`, `stop_thinking`).
pub fn set_wanted(world: &mut World, mob: Entity, state: TypingState) {
    if state == TypingState::Off {
        if let Some(mut indicator) = world.get_mut::<TypingIndicator>(mob) {
            indicator.asked = TypingState::Off;
            indicator.wanted = TypingState::Off;
        }
        world.entity_mut(mob).remove::<SayDraft>();
        return;
    }
    // `HAS_MIND_TRAIT(mob, TRAIT_HIDE_THINKING_INDICATOR)`: no thinking, hence no typing.
    let wanted =
        if crate::species::has_trait(world, mob, crate::species::Trait::HideThinkingIndicator) {
            TypingState::Off
        } else {
            state
        };
    let Some(states) = bubble_states(world, mob) else {
        return;
    };
    match world.get::<TypingIndicator>(mob).copied() {
        Some(old) => {
            // The bubble changed (a borg turned syndicate): the old one goes at once.
            if old.states != states
                && let Some(mut look) = world.get_mut::<Appearance>(mob)
            {
                look.overlays
                    .retain(|o| !old.states.contains(&o.icon_state));
            }
            if let Some(mut indicator) = world.get_mut::<TypingIndicator>(mob) {
                indicator.asked = state;
                indicator.wanted = wanted;
                if old.states != states {
                    indicator.states = states;
                    indicator.shown = TypingState::Off;
                }
            }
        }
        None => {
            let incaps = forcing_incaps(world.get::<StatusEffects>(mob));
            world.entity_mut(mob).insert(TypingIndicator {
                asked: state,
                wanted,
                shown: TypingState::Off,
                changed_at: f64::NEG_INFINITY,
                states,
                incaps,
            });
        }
    }
}

/// The [`FORCING`] effects `effects` has, by index bit.
fn forcing_incaps(effects: Option<&StatusEffects>) -> u8 {
    let Some(effects) = effects else {
        return 0;
    };
    FORCING
        .iter()
        .enumerate()
        .filter(|(_, (kind, _))| effects.remaining(*kind) > 0.0)
        .fold(0, |bits, (i, _)| bits | 1 << i)
}

/// `/mob/living/carbon/human/proc/force_say`: a human typing in character says its line cut
/// short (`alter_entry` with `phrases`, the hurt phrases by default) and its say line closes.
/// `immediate` (a stun): the saved line, a bare phrase without one; otherwise (a hit) only a
/// line with text is said (tgui-say `handleForceSay`).
pub fn force_say(
    world: &mut World,
    mob: Entity,
    phrases: Option<&[&str]>,
    immediate: bool,
) -> bool {
    if world.get::<crate::human::HumanLook>(mob).is_none()
        || world
            .get::<TypingIndicator>(mob)
            .is_none_or(|t| t.asked == TypingState::Off)
        || world.get::<Stat>(mob).is_some_and(|s| *s >= Stat::SoftCrit)
        || crate::status_effects::knocked_out(world, mob)
    {
        return false;
    }
    let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) else {
        return false;
    };
    let draft = world
        .get::<SayDraft>(mob)
        .map(|d| d.0.clone())
        .unwrap_or_default();
    if !immediate && draft.trim().is_empty() {
        return false;
    }
    let phrases = phrases.unwrap_or(&HURT_PHRASES);
    let text = {
        let mut rng = world.resource_mut::<CombatRng>();
        alter_entry(&draft, phrases, &mut rng)
    };
    set_wanted(world, mob, TypingState::Off);
    // shortcut: no `COMSIG_HUMAN_FORCESAY` listeners (a cigarette falling out of the mouth),
    // until cigarettes react to speech.
    world.write_message(SendCloseSay { client });
    if !text.is_empty() {
        world.write_message(IntentReceived {
            client,
            intent: ClientIntent::Say(text),
        });
    }
    true
}

/// `/datum/tgui_say/proc/alter_entry`: a long line cut to 39..49 characters, a short one loses
/// its last, then `-` and a phrase; no line: the phrase alone.
pub fn alter_entry(entry: &str, phrases: &[&str], rng: &mut CombatRng) -> String {
    let phrase = phrases
        .get(rng.range(0, phrases.len().saturating_sub(1) as u32) as usize)
        .copied()
        .unwrap_or_default();
    let len = entry.chars().count();
    if len == 0 {
        return phrase.to_string();
    }
    // DM `trim(text, max)`: the first `max - 1` characters, whitespace trimmed.
    let keep = if len > 50 {
        rng.range(40, 50) as usize - 1
    } else {
        len - 1
    };
    let cut: String = entry.chars().take(keep).collect();
    format!("{}-{phrase}", cut.trim())
}

/// tg `attacked_by`'s `force_say()`: damage over 10, or 5 and up a third of the time.
pub fn item_hit(world: &mut World, mob: Entity, damage: f32) {
    if damage > 10.0 || (damage >= 5.0 && world.resource_mut::<CombatRng>().chance(33.0)) {
        force_say(world, mob, None, false);
    }
}

/// `/datum/status_effect/incapacitating/try_force_say` of a newly started stun, knockdown or
/// paralysis: `force_say_chance`, then 6 s and longer always, 2 s and longer two times in three.
// Cost: per tick, one pass over mobs whose player ever used the say line.
fn incap_force_say(world: &mut World) {
    let mobs: Vec<(Entity, u8, u8)> = world
        .query::<(Entity, &TypingIndicator, Option<&StatusEffects>)>()
        .iter(world)
        .map(|(e, t, effects)| (e, t.incaps, forcing_incaps(effects)))
        .filter(|(_, was, now)| was != now)
        .collect();
    for (mob, was, now) in mobs {
        if let Some(mut indicator) = world.get_mut::<TypingIndicator>(mob) {
            indicator.incaps = now;
        }
        for (i, &(kind, chance)) in FORCING.iter().enumerate() {
            if now & !was & (1 << i) == 0 {
                continue;
            }
            let secs = world
                .get::<StatusEffects>(mob)
                .map_or(0.0, |e| e.remaining(kind));
            let forced = {
                let mut rng = world.resource_mut::<CombatRng>();
                rng.chance(chance) && (secs >= 6.0 || (secs >= 2.0 && rng.chance(66.0)))
            };
            if forced {
                let phrases: Option<&[&str]> =
                    (kind == Incap::Paralyze).then_some(&PARALYZED_PHRASES[..]);
                force_say(world, mob, phrases, true);
                break;
            }
        }
    }
}

/// Draws each mob's indicator: the wanted one while its player is on it and it is awake
/// (`create_*_indicator`'s `IS_UNCONSCIOUS_OR_CRIT`), kept the top overlay (`TYPING_LAYER`) when
/// other systems rebuild the appearance.
// Cost: per tick, one pass over mobs whose player ever used the say line, writing only on a
// change.
#[allow(clippy::type_complexity)]
fn show_indicators(
    time: Res<Time>,
    mut mobs: Query<(
        &mut TypingIndicator,
        &mut Appearance,
        Option<&Stat>,
        Option<&Health>,
        Option<&StatusEffects>,
        Has<PlayerMob>,
        Option<&crate::silicon::ai::AiCore>,
    )>,
    players: Query<(), With<PlayerMob>>,
) {
    let now = time.elapsed_secs_f64();
    for (mut indicator, mut look, stat, health, effects, player, core) in &mut mobs {
        let controlled = player || core.is_some_and(|c| players.contains(c.eye));
        if !controlled && indicator.asked != TypingState::Off {
            indicator.asked = TypingState::Off;
            indicator.wanted = TypingState::Off;
        }
        let stat = stat.copied().unwrap_or_default();
        let awake = stat < Stat::SoftCrit
            && !health.is_some_and(|h| h.knocked_out(stat))
            && !effects.is_some_and(StatusEffects::knocked_out);
        let desired = if awake {
            indicator.wanted
        } else {
            TypingState::Off
        };
        if desired != indicator.shown
            && (desired == TypingState::Off || now >= indicator.changed_at + MIN_CHANGE_SECS)
        {
            indicator.shown = desired;
            indicator.changed_at = now;
        }
        let icon = indicator.icon(indicator.shown);
        sync(&mut look, indicator.states, icon);
    }
}

/// `add_overlay`/`cut_overlay` of the indicator: `icon` the last overlay, no other of `states`.
fn sync(look: &mut Mut<Appearance>, states: [IconStateId; 2], icon: Option<IconStateId>) {
    let ours = |o: &Overlay| states.contains(&o.icon_state);
    let overlays = &look.overlays;
    let fine = match icon {
        Some(icon) => {
            overlays.last().is_some_and(|o| o.icon_state == icon)
                && overlays.iter().filter(|o| ours(o)).count() == 1
        }
        None => !overlays.iter().any(ours),
    };
    if fine {
        return;
    }
    look.overlays.retain(|o| !ours(o));
    if let Some(icon) = icon {
        look.overlays.push(Overlay::new(icon));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_shared::{Layer, MouseOpacity};

    const TYPING: IconStateId = IconStateId(10);
    const THINKING: IconStateId = IconStateId(13);
    const OTHER: IconStateId = IconStateId(99);

    fn look(overlays: Vec<Overlay>) -> Appearance {
        Appearance {
            icon_state: IconStateId(1),
            layer: Layer(4.0),
            plane: 0,
            color: None,
            pixel_offset: [0, 0],
            rotation: 0,
            overlays,
            mouse_opacity: MouseOpacity::Icon,
            glide: 0,
        }
    }

    fn app() -> App {
        let mut app = App::new();
        app.init_resource::<Time>()
            .add_systems(Update, show_indicators);
        app
    }

    fn mob(app: &mut App, wanted: TypingState) -> Entity {
        app.world_mut()
            .spawn((
                TypingIndicator {
                    asked: wanted,
                    wanted,
                    shown: TypingState::Off,
                    changed_at: f64::NEG_INFINITY,
                    states: [TYPING, THINKING],
                    incaps: 0,
                },
                look(vec![Overlay::new(OTHER)]),
                Stat::Conscious,
                PlayerMob {
                    client: Entity::PLACEHOLDER,
                },
            ))
            .id()
    }

    fn overlays(app: &App, mob: Entity) -> Vec<IconStateId> {
        app.world()
            .get::<Appearance>(mob)
            .unwrap()
            .overlays
            .iter()
            .map(|o| o.icon_state)
            .collect()
    }

    fn advance(app: &mut App, secs: f32) {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(secs));
        app.update();
    }

    #[test]
    fn indicator_tops_the_overlays_and_follows_the_wanted_state() {
        let mut app = app();
        let mob = mob(&mut app, TypingState::Thinking);
        app.update();
        assert_eq!(overlays(&app, mob), [OTHER, THINKING]);

        // Another system adds an overlay on top: ours goes back to the top.
        app.world_mut()
            .get_mut::<Appearance>(mob)
            .unwrap()
            .overlays
            .push(Overlay::new(IconStateId(50)));
        app.update();
        assert_eq!(overlays(&app, mob), [OTHER, IconStateId(50), THINKING]);

        // A quick flip waits out the cap; off never waits.
        app.world_mut()
            .get_mut::<TypingIndicator>(mob)
            .unwrap()
            .wanted = TypingState::Typing;
        advance(&mut app, 0.1);
        assert_eq!(overlays(&app, mob).last(), Some(&THINKING));
        advance(&mut app, 0.4);
        assert_eq!(overlays(&app, mob).last(), Some(&TYPING));
        app.world_mut()
            .get_mut::<TypingIndicator>(mob)
            .unwrap()
            .wanted = TypingState::Off;
        advance(&mut app, 0.05);
        assert_eq!(overlays(&app, mob), [OTHER, IconStateId(50)]);
    }

    #[test]
    fn alter_entry_cuts_the_line_like_tgui_say() {
        let mut rng = CombatRng(7);
        assert_eq!(alter_entry("", &["АУ!"], &mut rng), "АУ!");
        assert_eq!(alter_entry("привет", &["АУ!"], &mut rng), "приве-АУ!");
        assert_eq!(
            alter_entry("я ", &[""], &mut rng),
            "я-",
            "paralyzed: no phrase"
        );
        let long = "а".repeat(80);
        let cut = alter_entry(&long, &HURT_PHRASES, &mut rng);
        let (said, phrase) = cut.split_once('-').unwrap();
        assert!((39..=49).contains(&said.chars().count()), "{cut}");
        assert!(HURT_PHRASES.contains(&phrase));
    }

    #[test]
    fn crit_hides_it_and_losing_the_player_ends_it() {
        let mut app = app();
        let mob = mob(&mut app, TypingState::Typing);
        app.update();
        assert_eq!(overlays(&app, mob).last(), Some(&TYPING));
        app.world_mut().entity_mut(mob).insert(Stat::SoftCrit);
        advance(&mut app, 1.0);
        assert!(!overlays(&app, mob).contains(&TYPING));
        app.world_mut().entity_mut(mob).insert(Stat::Conscious);
        advance(&mut app, 1.0);
        assert_eq!(
            overlays(&app, mob).last(),
            Some(&TYPING),
            "still typing on waking"
        );

        app.world_mut().entity_mut(mob).remove::<PlayerMob>();
        advance(&mut app, 1.0);
        assert_eq!(overlays(&app, mob), [OTHER]);
        assert_eq!(
            app.world().get::<TypingIndicator>(mob).unwrap().wanted,
            TypingState::Off
        );
    }
}

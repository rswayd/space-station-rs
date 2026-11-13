//! Stating laws and automatic announcements (server), after tg `/datum/state_laws_ui`
//! (`code/datums/ai_laws/state_laws_ui.dm`), `/mob/living/silicon/proc/statelaws`,
//! `checklaws` and `set_autosay` (`silicon.dm`):
//!
//! - [`checklaws`] opens the `StateLawUi` window of a silicon's laws (the AI's "State Laws"
//!   button, a pAI's, a cyborg's RoboTact): every law with a checkbox, the inherent laws checked
//!   when the window is made and again whenever it closes (`ui_close` ->
//!   `update_inherent_stated_laws`).
//! - "State Laws" ([`statelaws`]) says "Текущие активные законы:" and then each checked law
//!   (zeroth, hacked, inherent, supplied, numbered as the list shows them), one a second, as the
//!   silicon's own speech prefixed with its `radiomod`, from a copy of the laws taken at the
//!   start; the window locks until 3 s after the last line.
//! - [`set_autosay`] (the AI's "Set Auto Announce Mode" verb, RoboTact's `lawchannel`): a
//!   channel list of the built-in radio, "Default" (`;`) and "None" (no prefix); the AI starts
//!   on "Default" (`ai_defines.dm` `radiomod = ";"`), the others on "None".
// shortcut: the lines go through the player's own say (`ClientIntent::Say`): a silicon with no
// player states nothing (tg forces the mob's `say`), and the say verb is "says", not "states".
// shortcut: tg's arrivals announcer reads `radiomod` too; the port's arrivals announcer is the
// AI-less machine, so only law stating uses it.

use std::collections::VecDeque;

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_net::server::IntentReceived;
use ss13_shared::chat::Span;
use ss13_shared::protocol::ClientIntent;
use ss13_tgui::UiInterface as _;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems};
use ss13_tgui::server::{
    AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};

use super::ai::{AiCore, AiEye};
use super::cyborg::Cyborg;
use super::laws::Laws;
use super::notice;
use super::ui::{STATE_LAWS, StateLawData, StateLawEntry, StateLawUi, TOGGLE_STATED};
use crate::health::Stat;
use crate::radio::{Radio, SiliconRadio};
use crate::tools::{client_of, now};

/// `sleep(1 SECONDS)` between the lines.
const LINE_DELAY: f64 = 1.0;
/// `addtimer(VARSET_CALLBACK(src, locked, FALSE), 3 SECONDS)` after the last line.
const UNLOCK_DELAY: f64 = 3.0;
/// The first line.
const HEADER: &str = "Текущие активные законы:";
/// tg's channel list starts with these two.
const DEFAULT: &str = "Default";
const NONE: &str = "None";

/// On a silicon holding laws (the AI core, a cyborg, a pAI): tg `radiomod`, the prefix of its
/// automatic announcements (`;`, `:x` or nothing).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Radiomod(pub String);

/// The window source of a silicon's `law_ui`, a child of the silicon.
#[derive(Component, Clone, Debug)]
pub struct LawUi {
    pub owner: Entity,
    /// tg `to_state`: law texts.
    pub to_state: Vec<String>,
    /// Lines still to say and when the next one goes; the unlock time after the last.
    queue: VecDeque<String>,
    next_at: f64,
    unlock_at: Option<f64>,
}

impl LawUi {
    pub fn locked(&self) -> bool {
        !self.queue.is_empty() || self.unlock_at.is_some()
    }
}

/// On a silicon: its [`LawUi`] entity.
#[derive(Component, Clone, Copy, Debug)]
pub struct LawUiOf(pub Entity);

/// On the prompt of [`set_autosay`]: the silicon whose `radiomod` it sets and the choices.
#[derive(Component, Clone, Debug)]
struct AutosayPrompt {
    holder: Entity,
    /// (shown name, prefix).
    choices: Vec<(String, String)>,
}

pub struct StateLawsPlugin;

impl Plugin for StateLawsPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<StateLawUi>()
            .add_systems(
                Update,
                (
                    acts.in_set(UiSystems::Handle),
                    autosay_answers
                        .in_set(UiSystems::Handle)
                        .after(PromptSystems::Answer),
                    speak.run_if(any_with_component::<LawUi>),
                    autosay_verb.in_set(UiSystems::Handle),
                ),
            )
            .add_observer(window_closed);
    }
}

/// The entity holding the laws of `mob`: the AI core for its eye, else `mob` itself.
pub fn laws_holder(world: &World, mob: Entity) -> Option<Entity> {
    if let Some(eye) = world.get::<AiEye>(mob) {
        return Some(eye.ai);
    }
    world.get::<Laws>(mob).is_some().then_some(mob)
}

/// The player speaking for `holder`: the AI's is on its eye.
fn speaker_client(world: &World, holder: Entity) -> Option<Entity> {
    match world.get::<AiCore>(holder) {
        Some(core) => client_of(world, core.eye),
        None => client_of(world, holder),
    }
}

/// tg `radiomod` of `holder`.
pub fn radiomod(world: &World, holder: Entity) -> String {
    match world.get::<Radiomod>(holder) {
        Some(r) => r.0.clone(),
        None if world.get::<AiCore>(holder).is_some() => ";".to_owned(),
        None => String::new(),
    }
}

/// tg `get_laws_ui_data`: the zeroth law (a cyborg's `zeroth_borg` first), hacked, inherent and
/// supplied laws; ioned and hacked laws get an ion number, the rest count from 1.
pub fn law_entries(laws: &Laws, cyborg: bool) -> Vec<StateLawEntry> {
    let entry = |text: &str, number: String, kind: &str| StateLawEntry {
        text: text.to_owned(),
        number,
        kind: kind.to_owned(),
    };
    let mut out = Vec::new();
    let zeroth = if cyborg {
        laws.zeroth_borg.as_ref().or(laws.zeroth.as_ref())
    } else {
        laws.zeroth.as_ref()
    };
    if let Some(z) = zeroth.filter(|z| !z.is_empty()) {
        out.push(entry(z, "0".to_owned(), "hacked"));
    }
    for law in &laws.hacked {
        out.push(entry(law, super::laws::ion_num().to_owned(), "hacked"));
    }
    let mut number = 1;
    for (kind, list) in [("core", &laws.inherent), ("supplied", &laws.supplied)] {
        for (law, ioned) in list {
            let n = if *ioned {
                super::laws::ion_num().to_owned()
            } else {
                number += 1;
                (number - 1).to_string()
            };
            out.push(entry(law, n, kind));
        }
    }
    out
}

/// tg `update_inherent_stated_laws`.
fn inherent_texts(world: &World, holder: Entity) -> Vec<String> {
    world
        .get::<Laws>(holder)
        .map(|l| l.inherent.iter().map(|(t, _)| t.clone()).collect())
        .unwrap_or_default()
}

/// `holder`'s [`LawUi`], made the first time (tg makes it in `Initialize`).
pub fn law_ui(world: &mut World, holder: Entity) -> Option<Entity> {
    if let Some(&LawUiOf(ui)) = world.get::<LawUiOf>(holder)
        && world.get::<LawUi>(ui).is_some()
    {
        return Some(ui);
    }
    world.get_entity(holder).ok()?;
    let to_state = inherent_texts(world, holder);
    let ui = world
        .spawn((
            LawUi {
                owner: holder,
                to_state,
                queue: VecDeque::new(),
                next_at: 0.0,
                unlock_at: None,
            },
            AlwaysState,
            ChildOf(holder),
        ))
        .id();
    world.entity_mut(holder).insert(LawUiOf(ui));
    Some(ui)
}

/// tg `checklaws`: the `StateLawUi` window of `mob`'s laws.
pub fn checklaws(world: &mut World, client: Entity, mob: Entity) {
    let Some(holder) = laws_holder(world, mob) else {
        return;
    };
    let Some(ui) = law_ui(world, holder) else {
        return;
    };
    world.write_message(OpenUi::new::<StateLawUi>(client, mob, ui, "State Laws"));
}

/// tg `statelaws(force)`: queues the lines; nothing while locked.
pub fn statelaws(world: &mut World, holder: Entity, force: bool) {
    let Some(ui) = law_ui(world, holder) else {
        return;
    };
    if world.get::<LawUi>(ui).is_none_or(LawUi::locked) {
        return;
    }
    let laws = world.get::<Laws>(holder).cloned().unwrap_or_default();
    let cyborg = world.get::<Cyborg>(holder).is_some();
    let prefix = radiomod(world, holder);
    let time = now(world);
    let Some(mut state) = world.get_mut::<LawUi>(ui) else {
        return;
    };
    let lines = stated_lines(&laws, cyborg, &state.to_state, force, &prefix);
    state.queue = lines.into();
    state.next_at = time;
}

/// The lines [`statelaws`] says: the header, then each law to state, with the prefix.
pub fn stated_lines(
    laws: &Laws,
    cyborg: bool,
    to_state: &[String],
    force: bool,
    prefix: &str,
) -> Vec<String> {
    std::iter::once(format!("{prefix} {HEADER}"))
        .chain(
            law_entries(laws, cyborg)
                .into_iter()
                .filter(|e| force || to_state.contains(&e.text))
                .map(|e| format!("{prefix} {}. {}", e.number, e.text)),
        )
        .collect()
}

/// One line a second; the lock lifts 3 s after the last.
fn speak(world: &mut World, mut uis: Local<Vec<Entity>>) {
    let time = now(world);
    uis.clear();
    uis.extend(
        world
            .query::<(Entity, &LawUi)>()
            .iter(world)
            .filter(|(_, u)| {
                (!u.queue.is_empty() && u.next_at <= time) || u.unlock_at.is_some_and(|t| t <= time)
            })
            .map(|(e, _)| e),
    );
    for ui in uis.drain(..) {
        let Some(mut state) = world.get_mut::<LawUi>(ui) else {
            continue;
        };
        if state.unlock_at.is_some_and(|t| t <= time) && state.queue.is_empty() {
            state.unlock_at = None;
            continue;
        }
        let Some(line) = state.queue.pop_front() else {
            continue;
        };
        state.next_at = time + LINE_DELAY;
        if state.queue.is_empty() {
            state.unlock_at = Some(time + UNLOCK_DELAY);
        }
        let owner = state.owner;
        // A dead silicon's say goes to deadchat, as tg's forced say would.
        if let Some(client) = speaker_client(world, owner) {
            world.write_message(IntentReceived {
                client,
                intent: ClientIntent::Say(line),
            });
        }
    }
}

/// tg `GLOB.not_incapacitated_state`: the window's acts need a conscious silicon.
fn incapacitated(world: &World, holder: Entity) -> bool {
    world.get::<Stat>(holder) != Some(&Stat::Conscious)
        || crate::status_effects::incapacitation(world, holder).incapacitated
        || (world.get::<AiCore>(holder).is_some() && super::ai_power::depowered(world, holder))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StateLawAct {
    Toggle(String),
    State,
}

impl ServerUi for StateLawUi {
    type Source = &'static LawUi;
    type UserParam = (
        Query<'static, 'static, &'static Laws>,
        Query<'static, 'static, (), With<Cyborg>>,
    );
    type Act = StateLawAct;

    fn static_data(_: &LawUi) {}

    fn data(
        ui: &LawUi,
        _: Entity,
        (laws, borgs): &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> StateLawData {
        StateLawData {
            locked: ui.locked(),
            stated_laws: ui.to_state.clone(),
            all_laws: laws
                .get(ui.owner)
                .map(|l| law_entries(l, borgs.contains(ui.owner)))
                .unwrap_or_default(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<StateLawAct, UiReject> {
        match action {
            TOGGLE_STATED => params.text("law").map(StateLawAct::Toggle),
            STATE_LAWS => Ok(StateLawAct::State),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<StateLawUi>>>) {
    let pending: Vec<(Entity, Entity, StateLawAct)> = acts
        .read(world.resource::<Messages<UiActed<StateLawUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, ui, act) in pending {
        let Some(owner) = world.get::<LawUi>(ui).map(|u| u.owner) else {
            continue;
        };
        // `ASSERT(user == owner)`, and `not_incapacitated_state`.
        if laws_holder(world, actor) != Some(owner) || incapacitated(world, owner) {
            continue;
        }
        match act {
            StateLawAct::Toggle(law) => {
                if let Some(mut state) = world.get_mut::<LawUi>(ui) {
                    if let Some(i) = state.to_state.iter().position(|l| *l == law) {
                        state.to_state.remove(i);
                    } else if !law.is_empty() {
                        state.to_state.push(law);
                    }
                }
            }
            StateLawAct::State => {
                statelaws(world, owner, false);
                close_windows(world, ui);
            }
        }
    }
}

/// `SStgui.close_uis(src)`.
fn close_windows(world: &mut World, ui: Entity) {
    let windows: Vec<Entity> = world
        .query::<(Entity, &UiWindow)>()
        .iter(world)
        .filter(|(_, w)| w.source == ui && w.interface == StateLawUi::NAME)
        .map(|(e, _)| e)
        .collect();
    for window in windows {
        world.try_despawn(window).ok();
    }
}

/// `ui_close`: the inherent laws are checked again.
fn window_closed(
    remove: On<Remove<UiWindow>>,
    windows: Query<&UiWindow>,
    mut uis: Query<&mut LawUi>,
    laws: Query<&Laws>,
) {
    let Ok(window) = windows.get(remove.entity) else {
        return;
    };
    if window.interface != StateLawUi::NAME {
        return;
    }
    if let Ok(mut ui) = uis.get_mut(window.source) {
        ui.to_state = laws
            .get(ui.owner)
            .map(|l| l.inherent.iter().map(|(t, _)| t.clone()).collect())
            .unwrap_or_default();
    }
}

/// tg `set_autosay`: asks which channel `mob`'s automatic announcements use.
pub fn set_autosay(world: &mut World, client: Entity, mob: Entity) {
    let Some(holder) = laws_holder(world, mob) else {
        return;
    };
    let radio = world
        .get::<SiliconRadio>(mob)
        .or_else(|| world.get::<SiliconRadio>(holder))
        .and_then(|r| world.get::<Radio>(r.0));
    let Some(radio) = radio else {
        notice(world, client, Span::Warning, "Radio not detected.");
        return;
    };
    let choices = autosay_choices(radio);
    let items = choices.iter().map(|(name, _)| name.clone()).collect();
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Channel Selection",
            "Select a channel",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(AutosayPrompt { holder, choices });
}

/// tg's list (`"Default"`, `"None"`, then `radio.channels`) with each choice's prefix and the
/// name it is reported as ("Default (1459)").
pub fn autosay_choices(radio: &Radio) -> Vec<(String, String)> {
    let mut out = vec![
        (DEFAULT.to_owned(), ";".to_owned()),
        (NONE.to_owned(), String::new()),
    ];
    for (channel, _) in &radio.channels {
        // A channel without a key keeps the old prefix in tg; it is not offered here.
        if let Some(key) = channel_key(channel) {
            out.push((channel.clone(), format!(":{key}")));
        }
    }
    out
}

/// The first key of `GLOB.department_radio_keys` naming `channel`.
fn channel_key(channel: &str) -> Option<char> {
    use ss13_chat::radio::*;
    Some(match channel {
        RADIO_CHANNEL_COMMAND => 'c',
        RADIO_CHANNEL_SCIENCE => 'n',
        RADIO_CHANNEL_MEDICAL => 'm',
        RADIO_CHANNEL_ENGINEERING => 'e',
        RADIO_CHANNEL_SECURITY => 's',
        RADIO_CHANNEL_JUSTICE => 'l',
        RADIO_CHANNEL_SUPPLY => 'u',
        RADIO_CHANNEL_SERVICE => 'v',
        RADIO_CHANNEL_SYNDICATE => 't',
        RADIO_CHANNEL_UPLINK => 'z',
        RADIO_CHANNEL_CENTCOM => 'y',
        RADIO_CHANNEL_AI_PRIVATE => 'o',
        RADIO_CHANNEL_ENTERTAINMENT => 'p',
        _ => return None,
    })
}

/// The AI's `set_automatic_say_channel` verb (Status panel): nothing while incapacitated.
fn autosay_verb(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<crate::statpanel::StatPanelUi>>>,
) {
    use crate::statpanel::{AI_TAB, AI_VERB_AUTOSAY, StatPanelHost};
    let pending: Vec<(Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<crate::statpanel::StatPanelUi>>>())
        .filter(|a| a.act.tab == AI_TAB && a.act.name == AI_VERB_AUTOSAY)
        .map(|a| (a.client, a.source))
        .collect();
    for (client, source) in pending {
        if world.get::<StatPanelHost>(source).map(|h| h.client) != Some(client) {
            continue;
        }
        let Some(mob) = world.resource::<crate::server::Players>().mob(client) else {
            continue;
        };
        let Some(core) = laws_holder(world, mob).filter(|&c| world.get::<AiCore>(c).is_some())
        else {
            continue;
        };
        if incapacitated(world, core) {
            continue;
        }
        set_autosay(world, client, mob);
    }
}

fn autosay_answers(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in pending {
        let Some(asked) = world.get::<AutosayPrompt>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let Some((name, prefix)) = asked.choices.get(i).cloned() else {
            continue;
        };
        if laws_holder(world, actor) != Some(asked.holder) {
            continue;
        }
        let shown = if name == DEFAULT {
            let freq = world
                .get::<SiliconRadio>(actor)
                .and_then(|r| world.get::<Radio>(r.0))
                .map_or(0, |r| r.frequency);
            format!("{DEFAULT} ({freq})")
        } else {
            name.clone()
        };
        world.entity_mut(asked.holder).insert(Radiomod(prefix));
        let text = if name == NONE {
            "Automatic announcements will not use the radio.".to_owned()
        } else {
            format!("Automatic announcements set to {shown}.")
        };
        notice(world, client, Span::Notice, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn laws() -> Laws {
        Laws {
            zeroth: Some("z".into()),
            zeroth_borg: Some("zb".into()),
            inherent: vec![
                ("i1".into(), false),
                ("ion".into(), true),
                ("i2".into(), false),
            ],
            supplied: vec![("s1".into(), false)],
            hacked: vec!["h".into()],
            ..default()
        }
    }

    #[test]
    fn entries_number_like_get_laws_ui_data() {
        let list: Vec<(String, String, String)> = law_entries(&laws(), false)
            .into_iter()
            .map(|e| (e.number, e.text, e.kind))
            .collect();
        let row = |n: &str, t: &str, k: &str| (n.to_owned(), t.to_owned(), k.to_owned());
        assert_eq!(
            list,
            [
                row("0", "z", "hacked"),
                row("#", "h", "hacked"),
                row("1", "i1", "core"),
                row("#", "ion", "core"),
                row("2", "i2", "core"),
                row("3", "s1", "supplied"),
            ]
        );
        assert_eq!(law_entries(&laws(), true)[0].text, "zb");
    }

    #[test]
    fn stated_lines_follow_the_checked_laws() {
        let to_state = vec!["i2".to_owned(), "s1".to_owned()];
        assert_eq!(
            stated_lines(&laws(), false, &to_state, false, ";"),
            ["; Текущие активные законы:", "; 2. i2", "; 3. s1"]
        );
        assert_eq!(stated_lines(&laws(), false, &[], true, "").len(), 7);
    }

    #[test]
    fn autosay_prefixes_follow_the_radio_keys() {
        let radio = Radio {
            kind: crate::radio::RadioKind::Headset,
            on: true,
            frequency: ss13_chat::radio::FREQ_COMMON,
            broadcasting: true,
            listening: true,
            canhear_range: 0,
            special_channels: 0,
            channels: vec![
                (ss13_chat::radio::RADIO_CHANNEL_SCIENCE.to_owned(), true),
                (ss13_chat::radio::RADIO_CHANNEL_AI_PRIVATE.to_owned(), true),
            ],
            tx_cut: false,
        };
        let prefixes: Vec<String> = autosay_choices(&radio)
            .into_iter()
            .map(|(_, p)| p)
            .collect();
        assert_eq!(prefixes, [";", "", ":n", ":o"]);
    }
}

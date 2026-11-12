//! Radios (server): headsets, intercoms and station bounced radios after tg
//! `code/game/objects/items/devices/radio/` and `code/game/machinery/telecomms/broadcasting.dm`.
//!
//! - Every entity with [`DmRadio`] (map, outfits, closet contents) gets a live [`Radio`] from its
//!   vars and its type's `Initialize()` ([`initialize`]).
//! - `ss13_chat` publishes what a mob said as [`Spoke`]. Its radio target talks into the devices
//!   (`talk_into`): `;`, `:h` and the channel keys the ears slot, `:r`/`:l` the held item, `:i`
//!   every intercom within 1 tile. Broadcasting radios within their `canhear_range` of the
//!   speaker relay the spoken words (`/obj/item/radio/Hear`).
//! - A transmission goes through the telecomms machines ([`crate::telecomms::Net::route`]): a
//!   broadcast reaches every radio that is on, listens and can receive the frequency on the
//!   broadcast's levels, its words garbled while still compressed (no working processor). A
//!   signal no broadcaster sent falls back, for radios that are not subspace-only (everything but
//!   headsets), to a mundane broadcast on the source's levels that only such radios hear
//!   (`backup_transmission`). CentCom radios on the CentCom frequency reach every CentCom radio
//!   (`TRANSMISSION_SUPERSPACE`). A map without telecomms machines keeps one ideal network per
//!   level. Hearers are the mobs within `canhear_range` of each radio (0 for a headset: its
//!   wearer), plus every ghost (`CHAT_GHOSTRADIO`), each once.
//!
//! Cost per transmission: one pass over the radios (MetaStation: ~230 mapped plus one headset
//! per player) and one spatial query of `(2r+1)^2` tiles per receiving radio; Common reaches the
//! ~200 intercoms. Speech is rate-limited by `ss13_chat` (9 messages per 5 s per client).

use bevy::prelude::*;
use ss13_chat::radio::{
    FREQ_AI_PRIVATE, FREQ_CENTCOM, FREQ_CONFESSIONAL, FREQ_ENTERTAINMENT, FREQ_INTERROGATION,
    FREQ_STATUS_DISPLAYS, FREQ_SYNDICATE, MAX_FREE_FREQ, MAX_FREQ, MIN_FREE_FREQ, MIN_FREQ,
    RADIO_SPECIAL_CENTCOM, RADIO_SPECIAL_SYNDIE, channel_freq, is_channel_freq, radio_message,
    sanitize_frequency,
};
use ss13_chat::server::{Listeners, SaySystems, Spoke};
use ss13_chat::speech::{EarsWorn, RadioTarget};
use ss13_mapload::fidelity::is_under;
use ss13_mapload::radio::{HEADSET_ROOT, INTERCOM_ROOT};
use ss13_mapload::{Areas, DmRadio};
use ss13_net::chat::server::SendChat;
use ss13_net::sound::server::SendSound;
use ss13_shared::protocol::SoundEvent;
use ss13_shared::TilePos;
use ss13_tgui::server::{
    AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_tgui::{UiInterface as _, UiStatus};
use ss13_world::SpatialIndex;

use crate::combat::CombatRng;
use crate::ghost::Ghost;
use crate::inventory::{CarriedBy, Inventory};
use crate::power::{AreaPower, Channel};
use crate::radio_ui::{self, RadioData, RadioUi};
use crate::telecomms::{
    Levels, Net, Signal, Telecomms, TelecommsLevels, TelecommsSystems, broadcast_text,
    vocal_compression,
};

/// tg `MODE_RANGE_INTERCOM`: `:i` talks into intercoms this close.
pub const INTERCOM_RANGE: u32 = 1;
/// `/obj/machinery` `verb_say` ("beeps"), BandaStation's translation.
const MACHINE_VERB: &str = "сигналит";

/// A radio built into a silicon (tg `/mob/living/silicon` `radio`): it has no tile of its own,
/// transmits and receives from `listener`'s tile, and only `listener`'s client hears it.
#[derive(Component, Clone, Copy, Debug)]
pub struct BuiltInRadio {
    pub listener: Entity,
}

/// On a silicon: its [`BuiltInRadio`], which the headset keys talk into.
#[derive(Component, Clone, Copy, Debug)]
pub struct SiliconRadio(pub Entity);

/// On a living mob with an internal radio implant (`crate::implants::parts`): its
/// [`BuiltInRadio`], which `;`, `:h` and the radio's own channel keys talk into before the
/// headset (`/mob/living/radio`).
#[derive(Component, Clone, Copy, Debug)]
pub struct ImplantRadio(pub Entity);

/// A machine talking into its internal radio (tg `radio.talk_into(src, text, channel)`), e.g.
/// the supermatter's warnings: always on, transmits from `z`.
#[derive(Message, Clone, Debug)]
pub struct MachineRadio {
    pub freq: u16,
    pub z: u16,
    pub name: String,
    pub text: String,
}

/// A living speaker talked into `radio` (tg `talk_into_impl` past the superspace broadcast).
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RadioTalked {
    pub speaker: Entity,
    pub radio: Entity,
}

/// tg `radio_talk.ogg`, sent to the speaker only.
pub const RADIO_TALK_SOUND: &str = "sound/items/radio/radio_talk.ogg";
/// `/obj/item/radio/entertainment`: `radio_noise = FALSE`.
const ENTERTAINMENT_ROOT: &str = "/obj/item/radio/entertainment";
/// `audio_cooldown`, per radio.
const NOISE_COOLDOWN: f64 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadioKind {
    /// `/obj/item/radio/headset`: listens only while a mob carries it (`possibly_deactivate_in_loc`),
    /// talks only while `broadcasting`, never relays nearby speech.
    Headset,
    /// `/obj/item/radio/intercom`: on while its area has equipment power (`AreaPowerCheck`),
    /// receives every frequency it is tuned to, ignores `:i` speech it already transmitted.
    Intercom,
    Other,
}

/// A radio's live state (tg `/obj/item/radio` vars after `Initialize`).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Radio {
    pub kind: RadioKind,
    pub on: bool,
    pub frequency: u16,
    /// Relays speech heard within `canhear_range` (and, for headsets, may talk at all).
    pub broadcasting: bool,
    pub listening: bool,
    pub canhear_range: u32,
    pub special_channels: u8,
    /// Channel name, listening (`FREQ_LISTENING`), in key order: `channels[1]` is `:h`.
    pub channels: Vec<(String, bool)>,
    /// `WIRE_TX` cut: receives, never transmits.
    pub tx_cut: bool,
}

impl Radio {
    /// tg `/obj/item/radio/Initialize` (and its subtypes') on the type's and map's vars.
    pub fn new(dm: &DmRadio) -> Self {
        let path = dm.path.as_str();
        let syndie = dm.special_channels & RADIO_SPECIAL_SYNDIE != 0;
        let kind = if is_under(path, HEADSET_ROOT) {
            RadioKind::Headset
        } else if is_under(path, INTERCOM_ROOT) {
            RadioKind::Intercom
        } else {
            RadioKind::Other
        };
        let mut radio = Self {
            kind,
            on: dm.on,
            frequency: sanitize_frequency(dm.frequency, dm.freerange, syndie),
            broadcasting: dm.broadcasting,
            listening: dm.listening,
            canhear_range: dm.canhear_range.max(0.0) as u32,
            special_channels: dm.special_channels,
            channels: dm
                .channels
                .iter()
                .map(|c| (c.name.clone(), c.listening))
                .collect(),
            tx_cut: false,
        };
        initialize(path, &mut radio);
        radio
    }

    /// `subspace_transmission`: headsets (and silicons' built-in radios) talk and listen only
    /// through the telecomms.
    pub fn subspace(&self) -> bool {
        self.kind == RadioKind::Headset
    }

    /// tg `can_receive` of a radio in `GLOB.all_radios[freq]` (on and listening), the z-level
    /// checked by the caller.
    pub fn receives(&self, freq: u16) -> bool {
        if !self.on || !self.listening {
            return false;
        }
        let syndie = self.special_channels & RADIO_SPECIAL_SYNDIE != 0;
        if freq == FREQ_SYNDICATE && !syndie {
            return false;
        }
        if freq == self.frequency {
            return true;
        }
        // Syndicate radios hear every well-known channel (`broadcast()`).
        if syndie && is_channel_freq(freq) {
            return true;
        }
        self.channels.iter().any(|(name, listening)| {
            // An intercom's `can_receive` takes every frequency it is registered on.
            (*listening || self.kind == RadioKind::Intercom) && channel_freq(name) == Some(freq)
        })
    }

    /// tg `talk_into_impl`'s choice of frequency: a named channel (or `:h`, the first one) on a
    /// radio with channels needs that channel switched on; without channels, or without a
    /// channel, the radio's own frequency.
    pub fn transmit_freq(&self, channel: Option<RadioTarget>) -> Option<u16> {
        let name = match channel {
            Some(RadioTarget::Department) => self.channels.first().map(|c| c.0.as_str()),
            Some(RadioTarget::Channel(name)) => Some(name),
            _ => None,
        };
        match name {
            Some(name) if !self.channels.is_empty() => {
                let (_, on) = self.channels.iter().find(|c| c.0 == name)?;
                if *on { channel_freq(name) } else { None }
            }
            _ => Some(self.frequency),
        }
    }
}

/// The `Initialize()` overrides of radio subtypes, which run after map var edits as in tg.
fn initialize(path: &str, radio: &mut Radio) {
    let under = |root: &str| is_under(path, root);
    if radio.kind == RadioKind::Headset {
        radio.listening = true;
        radio.broadcasting = true;
    }
    if under("/obj/item/radio/off")
        || under("/obj/item/radio/intercom/freerange/ai_core")
        || under("/obj/item/radio/intercom/ai_private/freerange")
        || under("/obj/item/radio/intercom/ai_private/quiet")
    {
        radio.listening = false;
    }
    if under("/obj/item/radio/intercom/prison") {
        radio.tx_cut = true;
    }
    if under("/obj/item/radio/intercom/chapel") {
        radio.frequency = FREQ_CONFESSIONAL;
        radio.broadcasting = true;
    }
    if under("/obj/item/radio/intercom/interrogation") {
        radio.frequency = FREQ_INTERROGATION;
    }
    if under("/obj/item/radio/intercom/interrogation/inside") {
        radio.broadcasting = true;
        radio.listening = false;
    }
    if under("/obj/item/radio/intercom/ai_private") {
        radio.frequency = FREQ_AI_PRIVATE;
    }
    if under("/obj/item/radio/intercom/ai_private/broadcasting") {
        radio.broadcasting = true;
    }
    if under("/obj/item/radio/intercom/departmental")
        && let Some(freq) = radio.channels.first().and_then(|c| channel_freq(&c.0))
    {
        radio.frequency = freq;
    }
    if under("/obj/item/radio/entertainment") {
        radio.frequency = FREQ_ENTERTAINMENT;
    }
    if under("/obj/item/radio/entertainment/speakers") {
        radio.broadcasting = false;
        radio.listening = true;
        radio.tx_cut = true;
    }
    if under("/obj/item/radio/entertainment/microphone") {
        radio.broadcasting = true;
        radio.listening = false;
    }
    if under("/obj/item/radio/borg/syndicate") {
        radio.frequency = FREQ_SYNDICATE;
    }
}

/// On a map without telecomms machines: the stand-in network is down (tg `EMPED` on every
/// `/obj/machinery/telecomms`, e.g. the Communications Blackout event) until `until` (server
/// seconds). Subspace radios (headsets) neither send nor receive; the others fall back to a
/// mundane broadcast on their level that only other non-subspace radios hear (tg
/// `backup_transmission`, `TRANSMISSION_RADIO`). Maps with machines EMP the machines instead
/// (`crate::emp::machines::telecomms_emp`).
// shortcut: the backup broadcast goes out at once instead of 2 s later.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct TelecommsOutage {
    pub until: f64,
}

pub struct RadioServerPlugin;

impl Plugin for RadioServerPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // A radio opens its window.
        app.add_click_handler(ClickPhase::AttackSelf, 2700, |w, c| {
            crate::radio::attack_self(w, c.client, c.mob, c.target)
        });
        app.add_plugins(crate::telecomms::TelecommsPlugin)
            .add_message::<Spoke>()
            .add_message::<SendChat>()
            .add_message::<MachineRadio>()
            .add_message::<RadioTalked>()
            .init_resource::<Listeners>()
            .init_resource::<TelecommsOutage>()
            .register_ui::<RadioUi>()
            .add_observer(make_radio)
            .add_systems(
                Update,
                (
                    mark_ears,
                    transmit_speech.after(SaySystems).after(TelecommsSystems),
                    radio_noise.after(transmit_speech),
                    open_intercoms
                        .after(ss13_world::WorldSystems::Movement)
                        .after(crate::click::ResolveClicks)
                        .before(UiSystems::Open),
                    radio_window_status.in_set(UiSystems::StatusHook),
                    radio_acts.in_set(UiSystems::Handle),
                ),
            );
    }
}

/// The radio noise of `talk_into_impl` (BandaStation plays it on every frequency): the speaker
/// hears `radio_talk.ogg` at a random low pitch, at most once per 0.5 s per radio, unless deaf.
/// No volume preference here, so it plays at its default 100. The receive sounds are a
/// BandaStation removal.
fn radio_noise(world: &mut World, mut ready_at: Local<std::collections::HashMap<Entity, f64>>) {
    let talked: Vec<RadioTalked> = world
        .resource_mut::<Messages<RadioTalked>>()
        .drain()
        .collect();
    if talked.is_empty() {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    ready_at.retain(|_, at| *at > now);
    let can_send = world.contains_resource::<Messages<SendSound>>();
    for RadioTalked { speaker, radio } in talked {
        if ready_at.contains_key(&radio)
            || world
                .get::<Radio>(radio)
                .is_none()
            || world
                .get::<DmRadio>(radio)
                .is_some_and(|d| is_under(&d.path, ENTERTAINMENT_ROOT))
            || !crate::sound::can_hear(world, speaker)
        {
            continue;
        }
        let Some(client) = world.get::<crate::server::PlayerMob>(speaker).map(|p| p.client) else {
            continue;
        };
        ready_at.insert(radio, now + NOISE_COOLDOWN);
        // `get_rand_frequency_low_range()`.
        let Some(hz) = world
            .get_resource_mut::<crate::sound::SoundRng>()
            .map(|mut rng| rng.0.range(38000, 45000) as f32)
        else {
            continue;
        };
        if can_send {
            world.write_message(SendSound {
                client,
                sound: SoundEvent {
                    file: RADIO_TALK_SOUND.into(),
                    volume: 100.0,
                    speed: crate::sound::speed_of(hz),
                    offset: None,
                },
            });
        }
    }
}

fn make_radio(add: On<Add<DmRadio>>, radios: Query<&DmRadio>, mut commands: Commands) {
    if let Ok(dm) = radios.get(add.entity) {
        let radio = Radio::new(dm);
        let tuning = RadioTuning::new(dm);
        let mut entity = commands.entity(add.entity);
        // A carried radio's window follows its holder (`inventory_state`), an intercom's the
        // distance (`default_state`).
        if radio.kind != RadioKind::Intercom {
            entity.try_insert(AlwaysState);
        }
        entity.try_insert((radio, tuning));
    }
}

/// The tuning limits of a radio: `freerange` (the free band) and `freqlock`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RadioTuning {
    pub freerange: bool,
    pub freqlock: bool,
}

impl RadioTuning {
    pub fn new(dm: &DmRadio) -> Self {
        let path = dm.path.as_str();
        let freqlock = [
            "/obj/item/radio/intercom/chapel",
            "/obj/item/radio/intercom/interrogation",
            "/obj/item/radio/entertainment",
        ]
        .iter()
        .any(|root| is_under(path, root));
        Self {
            freerange: dm.freerange,
            freqlock,
        }
    }

    /// `minFrequency`, `maxFrequency`.
    pub fn band(self) -> (u16, u16) {
        if self.freerange {
            (MIN_FREE_FREQ, MAX_FREE_FREQ)
        } else {
            (MIN_FREQ, MAX_FREQ)
        }
    }
}

impl ServerUi for RadioUi {
    type Source = (&'static Radio, Option<&'static RadioTuning>);
    type UserParam = ();
    type Act = RadioAct;

    fn static_data(_: (&Radio, Option<&RadioTuning>)) {}

    fn data((radio, tuning): (&Radio, Option<&RadioTuning>), _: Entity, _: &()) -> RadioData {
        let tuning = tuning.copied().unwrap_or_default();
        let (min_frequency, max_frequency) = tuning.band();
        RadioData {
            broadcasting: radio.broadcasting,
            listening: radio.listening,
            frequency: radio.frequency,
            min_frequency,
            max_frequency,
            freqlock: tuning.freqlock,
            channels: radio.channels.clone(),
            headset: radio.kind == RadioKind::Headset,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<RadioAct, UiReject> {
        let channel = || {
            params
                .get(radio_ui::CHANNEL)
                .map(str::to_owned)
                .ok_or_else(|| UiReject::BadParams("channel: missing".into()))
        };
        Ok(match action {
            radio_ui::FREQUENCY => {
                if let Ok(adjust) = params.number(radio_ui::ADJUST) {
                    RadioAct::Adjust(adjust)
                } else {
                    RadioAct::Tune(params.number(radio_ui::TUNE)?)
                }
            }
            radio_ui::TUNE_TO_CHANNEL => RadioAct::TuneToChannel(channel()?),
            radio_ui::LISTEN => RadioAct::Listen,
            radio_ui::BROADCAST => RadioAct::Broadcast,
            radio_ui::CHANNEL => RadioAct::Channel(channel()?),
            // shortcut: no loud mode (`command`) or subspace switch (no radio has them).
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RadioAct {
    /// kHz to add.
    Adjust(f64),
    /// kHz.
    Tune(f64),
    TuneToChannel(String),
    Listen,
    Broadcast,
    Channel(String),
}

/// `/obj/item/radio/attack_self` -> `interact`: the window of a radio used in hand.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(radio) = world.get::<Radio>(item) else {
        return false;
    };
    if radio.kind == RadioKind::Intercom || world.get::<BuiltInRadio>(item).is_some() {
        return false;
    }
    let title = crate::chem::name_of(world, item);
    world.write_message(OpenUi::new::<RadioUi>(client, mob, item, title));
    true
}

/// `/obj/item/radio/intercom/attack_hand`: an empty-hand click opens the intercom's window.
fn open_intercoms(
    mut clicks: MessageReader<crate::click::ClickResolved>,
    radios: Query<(&Radio, Option<&ss13_mapload::DmName>)>,
    inventories: Query<&Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok((radio, name)) = radios.get(click.target) else {
            continue;
        };
        let empty = inventories
            .get(click.mob)
            .is_ok_and(|i| i.active_item().is_none());
        if radio.kind != RadioKind::Intercom || !empty {
            continue;
        }
        let title = name
            .map(|n| crate::vending::plain_name(&n.name).to_owned())
            .unwrap_or_default();
        open.write(OpenUi::new::<RadioUi>(
            click.client,
            click.mob,
            click.target,
            title,
        ));
    }
}

/// `inventory_state` of a carried radio: only while its user carries it; an implant's radio
/// (`deep_inventory_state`) while it is in its user.
fn radio_window_status(
    mut windows: Query<&mut UiWindow>,
    radios: Query<&Radio>,
    carried: Query<&CarriedBy>,
    builtin: Query<&BuiltInRadio>,
) {
    for mut window in &mut windows {
        if window.interface != RadioUi::NAME {
            continue;
        }
        let held = radios
            .get(window.source)
            .is_ok_and(|r| r.kind == RadioKind::Intercom)
            || carried
                .get(window.source)
                .is_ok_and(|c| c.0 == window.actor)
            || builtin
                .get(window.source)
                .is_ok_and(|b| b.listener == window.actor);
        if !held {
            window.cap_status(UiStatus::Close);
        }
    }
}

/// `/obj/item/radio/ui_act`, checked against the radio's state now.
fn radio_acts(
    mut acts: MessageReader<UiActed<RadioUi>>,
    mut radios: Query<(&mut Radio, Option<&RadioTuning>)>,
) {
    for acted in acts.read() {
        if let Ok((mut radio, tuning)) = radios.get_mut(acted.source) {
            apply_act(&mut radio, tuning.copied().unwrap_or_default(), &acted.act);
        }
    }
}

/// One act on `radio`: a locked frequency stays, a channel must be one of the keys'.
pub fn apply_act(radio: &mut Radio, tuning: RadioTuning, act: &RadioAct) {
    let syndie = radio.special_channels & RADIO_SPECIAL_SYNDIE != 0;
    match act {
        RadioAct::Adjust(_) | RadioAct::Tune(_) | RadioAct::TuneToChannel(_) if tuning.freqlock => {
        }
        RadioAct::Adjust(adjust) => {
            let tune = f64::from(radio.frequency) + (adjust * 10.0).round();
            radio.frequency = sanitize_frequency(tune, tuning.freerange, syndie);
        }
        // tg reads no `tune` param (`else if(tune)` on a zero); the intended tuning is ported.
        RadioAct::Tune(khz) => {
            radio.frequency = sanitize_frequency(khz * 10.0, tuning.freerange, syndie);
        }
        RadioAct::TuneToChannel(name) => {
            // Bypasses the band: tunes to the key's encrypted channel.
            if radio.channels.iter().any(|(c, _)| c == name)
                && let Some(freq) = channel_freq(name)
            {
                radio.frequency = freq;
            }
        }
        RadioAct::Listen => radio.listening = !radio.listening,
        RadioAct::Broadcast => radio.broadcasting = !radio.broadcasting,
        RadioAct::Channel(name) => {
            if let Some(entry) = radio.channels.iter_mut().find(|(c, _)| c == name) {
                entry.1 = !entry.1;
            }
        }
    }
}

/// [`EarsWorn`] follows the ears slot (department keys need it).
fn mark_ears(
    // A silicon's built-in radio counts as worn (`crate::silicon`).
    mobs: Query<
        (Entity, &Inventory, Has<EarsWorn>, Has<ImplantRadio>),
        (Changed<Inventory>, Without<SiliconRadio>),
    >,
    mut commands: Commands,
) {
    for (mob, inventory, marked, implant) in &mobs {
        match (inventory.ears.is_some() || implant, marked) {
            (true, false) => {
                commands.entity(mob).try_insert(EarsWorn);
            }
            (false, true) => {
                commands.entity(mob).try_remove::<EarsWorn>();
            }
            _ => {}
        }
    }
}

/// One radio message: tg's signal `data` after telecomms processed it (no compression).
struct Transmission {
    freq: u16,
    z: u16,
    verb: &'static str,
    text: String,
    italics: bool,
    /// The language of the words: listeners hear them translated (`Hear`).
    language: Option<ss13_chat::language::LanguageId>,
    /// From a subspace radio (a headset: tg `subspace_transmission`).
    subspace: bool,
    /// A CentCom radio on the CentCom frequency (`TRANSMISSION_SUPERSPACE`).
    superspace: bool,
    /// What the server logs make of the speaker (`mobtype`).
    race: &'static str,
}

/// Which radios a transmission reaches, and its words.
struct Reach {
    /// The frequency it is heard on (a bus may have rewritten it).
    freq: u16,
    levels: Levels,
    /// `TRANSMISSION_RADIO`: only radios that are not subspace-only.
    mundane: bool,
    /// `TRANSMISSION_SUPERSPACE`: only CentCom radios, on every level.
    superspace: bool,
    text: String,
}

impl Reach {
    fn hears(&self, radio: &Radio) -> bool {
        if self.superspace {
            return radio.special_channels & RADIO_SPECIAL_CENTCOM != 0;
        }
        !(self.mundane && radio.subspace())
    }
}

/// `talk_into_impl` from the signal on: superspace, then the telecomms (`send_to_receivers`),
/// then `backup_transmission` for radios that are not subspace-only.
fn reach(
    tx: &Transmission,
    net: Option<&Net>,
    telecomms_down: bool,
    rng: &mut CombatRng,
    logged: &mut Vec<(Entity, u8)>,
) -> Option<Reach> {
    let reach = |levels, mundane, text| Reach {
        freq: tx.freq,
        levels,
        mundane,
        superspace: false,
        text,
    };
    if tx.superspace {
        return Some(Reach {
            freq: tx.freq,
            levels: Levels {
                all: true,
                z: Vec::new(),
            },
            mundane: false,
            superspace: true,
            text: tx.text.clone(),
        });
    }
    let Some(net) = net.filter(|n| !n.is_empty()) else {
        // No machines: the ideal network of the source's level, or the mundane fallback while
        // it is down.
        if telecomms_down && tx.subspace {
            return None;
        }
        return Some(reach(Levels::of([tx.z]), telecomms_down, tx.text.clone()));
    };
    let signal = Signal::vocal(tx.freq, net.connected(tx.z), vocal_compression(rng));
    let routed = net.route_logged(&signal);
    *logged = routed.logged;
    match routed.broadcast {
        Some(b) => {
            let text = broadcast_text(&tx.text, b.compression, rng);
            Some(Reach {
                freq: b.freq,
                ..reach(b.levels, false, text)
            })
        }
        None if !tx.subspace => Some(reach(net.connected(tx.z), true, tx.text.clone())),
        None => None,
    }
}

type RadioItem<'a> = (
    Entity,
    &'a Radio,
    Option<&'a TilePos>,
    Option<&'a CarriedBy>,
    Option<&'a BuiltInRadio>,
);

/// Where a radio is: its tile, or its carrier's tile while a mob holds or wears it directly.
fn radio_pos(
    (radio, pos, carried, builtin): (
        Entity,
        Option<&TilePos>,
        Option<&CarriedBy>,
        Option<&BuiltInRadio>,
    ),
    mobs: &Query<(&TilePos, &Inventory)>,
    positions: &Query<&TilePos>,
) -> Option<(TilePos, bool)> {
    if let Some(b) = builtin {
        return positions.get(b.listener).ok().map(|p| (*p, true));
    }
    if let Some(pos) = pos {
        return Some((*pos, false));
    }
    let (pos, inventory) = mobs.get(carried?.0).ok()?;
    inventory.holds(radio).then_some((*pos, true))
}

fn transmit_speech(
    mut spoken: MessageReader<Spoke>,
    radios: Query<RadioItem>,
    uplinks: Query<&crate::antag::uplink::Uplink>,
    mobs: Query<(&TilePos, &Inventory)>,
    positions: Query<&TilePos>,
    silicons: Query<&SiliconRadio>,
    ghosts: Query<Entity, With<Ghost>>,
    areas: Option<Res<Areas>>,
    area_power: Query<&AreaPower>,
    index: Res<SpatialIndex>,
    listeners: Res<Listeners>,
    senses: ss13_chat::server::Senses,
    mut chat: MessageWriter<SendChat>,
    mut machine: MessageReader<MachineRadio>,
    (
        outage,
        time,
        machines,
        telecomms_levels,
        mut rng,
        understanding,
        mut server_logs,
        implant_radios,
        (controls, codewords),
        mut talked,
    ): (
        Res<TelecommsOutage>,
        Res<Time>,
        Query<(Entity, &Telecomms, &TilePos)>,
        Res<TelecommsLevels>,
        ResMut<CombatRng>,
        ss13_chat::server::Understanding,
        Query<&mut crate::telecomms_config::ServerLog>,
        Query<&ImplantRadio>,
        (
            Query<&ss13_net::server::ControlledEntity>,
            Query<&ss13_chat::server::Codewords>,
        ),
        mut talked: MessageWriter<RadioTalked>,
    ),
) {
    let now = time.elapsed_secs_f64();
    let telecomms_down = outage.until > now;
    let powered = |pos: TilePos| {
        let area = areas.as_deref().and_then(|a| a.area_at(pos));
        // No power model for the area (or no area): tg `requires_power = FALSE` is powered.
        area.and_then(|a| area_power.get(a).ok())
            .is_none_or(|p| p.powered(Channel::Equip))
    };
    let live = |radio: &Radio, pos: TilePos| {
        radio.on && (radio.kind != RadioKind::Intercom || powered(pos))
    };
    let mut sent: Vec<(String, Transmission)> = Vec::new();
    // `signal.virt.source.type` as the log browser reads it.
    let race_of = |speaker: Entity| {
        if silicons.contains(speaker) {
            "Artificial Life"
        } else if mobs.contains(speaker) {
            "Humanoid"
        } else {
            "Unidentifiable"
        }
    };
    for spoke in spoken.read() {
        let race = race_of(spoke.speaker);
        let mut transmissions = Vec::new();
        // `talk_into` of the target's devices (`radio()`).
        let devices: Vec<(Entity, Option<RadioTarget>)> = match spoke.radio {
            // Saymodes (binary, the changeling hivemind) never reach a radio.
            None | Some(RadioTarget::Binary | RadioTarget::Changeling | RadioTarget::Hive) => {
                Vec::new()
            }
            Some(RadioTarget::Intercom) => index
                .in_radius(spoke.pos, INTERCOM_RANGE)
                .filter(|&e| {
                    radios
                        .get(e)
                        .is_ok_and(|(_, r, ..)| r.kind == RadioKind::Intercom)
                })
                .map(|e| (e, None))
                .collect(),
            Some(RadioTarget::Hand(hand)) => mobs
                .get(spoke.speaker)
                .ok()
                .and_then(|(_, inv)| inv.hands[hand])
                .map(|e| (e, None))
                .into_iter()
                .collect(),
            Some(channel) => {
                // `/mob/living/radio`: a switched-on implant radio takes `;`, `:h` and its own
                // channels.
                let implant = implant_radios
                    .get(spoke.speaker)
                    .ok()
                    .map(|r| r.0)
                    .filter(|&r| {
                        radios.get(r).is_ok_and(|(_, radio, ..)| {
                            radio.on
                                && match channel {
                                    RadioTarget::Headset | RadioTarget::Department => true,
                                    RadioTarget::Channel(name) => {
                                        radio.channels.iter().any(|c| c.0 == name)
                                    }
                                    _ => false,
                                }
                        })
                    });
                implant
                    .or_else(|| mobs.get(spoke.speaker).ok().and_then(|(_, inv)| inv.ears))
                    .or_else(|| silicons.get(spoke.speaker).ok().map(|r| r.0))
                    .map(|e| (e, Some(channel).filter(|c| *c != RadioTarget::Headset)))
                    .into_iter()
                    .collect()
            }
        };
        for (device, channel) in devices {
            let Ok((e, radio, pos, carried, builtin)) = radios.get(device) else {
                continue;
            };
            // The uplink took the code (tg `COMPONENT_CANNOT_USE_RADIO`): nothing goes out.
            if crate::antag::uplink::swallows(uplinks.get(device).ok(), channel, &spoke.text) {
                continue;
            }
            let Some((pos, _)) = radio_pos((e, pos, carried, builtin), &mobs, &positions) else {
                continue;
            };
            if !live(radio, pos)
                || radio.tx_cut
                || (radio.kind == RadioKind::Headset && !radio.broadcasting)
            {
                continue;
            }
            let Some(freq) = radio.transmit_freq(channel) else {
                continue;
            };
            let superspace = radio.special_channels & RADIO_SPECIAL_CENTCOM != 0
                && (freq == FREQ_CENTCOM || freq == FREQ_STATUS_DISPLAYS);
            // The superspace broadcast returns before the noise.
            if !superspace {
                talked.write(RadioTalked {
                    speaker: spoke.speaker,
                    radio: e,
                });
            }
            transmissions.push(Transmission {
                freq,
                z: pos.z,
                verb: spoke.verb,
                text: if spoke.whisper {
                    spoke.starred.clone()
                } else {
                    spoke.text.clone()
                },
                italics: spoke.whisper,
                language: spoke.language,
                subspace: radio.subspace(),
                superspace,
                race,
            });
        }
        // Broadcasting radios that heard the words relay them (`/obj/item/radio/Hear`); the
        // intercoms a `:i` message went into do not hear it again.
        // shortcut: range only, no line of sight, like local speech in `ss13_chat`.
        let relay_reach = 7;
        // The held radio a `:r`/`:l` message went into is not heard twice.
        let talked_into = match spoke.radio {
            Some(RadioTarget::Hand(hand)) => {
                mobs.get(spoke.speaker).ok().and_then(|m| m.1.hands[hand])
            }
            _ => None,
        };
        let mut nearby: Vec<Entity> = Vec::new();
        for e in index.in_radius(spoke.pos, relay_reach) {
            if radios.contains(e) {
                nearby.push(e);
            } else if let Ok((_, inventory)) = mobs.get(e) {
                nearby.extend(inventory.hands.iter().flatten());
            }
        }
        for e in nearby {
            let Ok((e, radio, pos, carried, builtin)) = radios.get(e) else {
                continue;
            };
            if radio.kind == RadioKind::Headset
                || talked_into == Some(e)
                || !radio.broadcasting
                || radio.tx_cut
                || (radio.kind == RadioKind::Intercom && spoke.radio == Some(RadioTarget::Intercom))
            {
                continue;
            }
            let Some((pos, _)) = radio_pos((e, pos, carried, builtin), &mobs, &positions) else {
                continue;
            };
            if !live(radio, pos)
                || pos
                    .chebyshev(spoke.pos)
                    .is_none_or(|d| d > radio.canhear_range)
            {
                continue;
            }
            transmissions.push(Transmission {
                freq: radio.frequency,
                z: pos.z,
                verb: spoke.heard_verb(),
                text: if spoke.heard_whisper {
                    spoke.starred.clone()
                } else {
                    spoke.text.clone()
                },
                italics: spoke.heard_whisper,
                language: spoke.language,
                subspace: false,
                superspace: radio.special_channels & RADIO_SPECIAL_CENTCOM != 0
                    && (radio.frequency == FREQ_CENTCOM || radio.frequency == FREQ_STATUS_DISPLAYS),
                race,
            });
        }
        sent.extend(transmissions.into_iter().map(|tx| (spoke.name.clone(), tx)));
    }
    // A machine's internal radio (`radio.talk_into(src, ...)`): it is always on and in range.
    sent.extend(machine.read().map(|m| {
        (
            m.name.clone(),
            Transmission {
                freq: m.freq,
                z: m.z,
                verb: MACHINE_VERB,
                text: m.text.clone(),
                italics: false,
                // Machines speak Galactic Common (`atom_basic`).
                language: Some(ss13_chat::language::LanguageId::Common),
                subspace: false,
                superspace: false,
                race: "Machinery",
            },
        )
    }));
    let net = (!sent.is_empty()).then(|| Net::new(machines.iter(), &telecomms_levels, now));
    for (name, tx) in sent {
        let mut logged = Vec::new();
        let reach = reach(&tx, net.as_ref(), telecomms_down, &mut rng, &mut logged);
        for (server, compression) in logged {
            if let Ok(mut log) = server_logs.get_mut(server) {
                let entry = crate::telecomms_config::LogEntry::new(
                    &name,
                    &tx.text,
                    tx.language,
                    tx.race,
                    compression,
                    &mut rng,
                );
                log.push(entry);
            }
        }
        let Some(reach) = reach else {
            continue;
        };
        let mut clients: Vec<Entity> = Vec::new();
        for (e, radio, pos, carried, builtin) in &radios {
            if !radio.receives(reach.freq) || !reach.hears(radio) {
                continue;
            }
            if let Some(b) = builtin {
                // Like a worn headset: switched on, on a level the signal reaches.
                let heard = radio.on
                    && positions
                        .get(b.listener)
                        .is_ok_and(|p| reach.levels.contains(p.z));
                if let Some(client) = listeners.client_of(b.listener).filter(|_| heard)
                    && !clients.contains(&client)
                {
                    clients.push(client);
                }
                continue;
            }
            let Some((pos, carried)) = radio_pos((e, pos, carried, None), &mobs, &positions) else {
                continue;
            };
            if !reach.levels.contains(pos.z)
                || !live(radio, pos)
                || (radio.kind == RadioKind::Headset && !carried)
            {
                continue;
            }
            let hearers = index
                .in_radius(pos, radio.canhear_range)
                .filter(|&body| senses.perceives(body, pos))
                .filter_map(|body| listeners.client_of(body));
            for client in hearers {
                if !clients.contains(&client) {
                    clients.push(client);
                }
            }
        }
        for ghost in &ghosts {
            if let Some(client) = listeners.client_of(ghost)
                && !clients.contains(&client)
            {
                clients.push(client);
            }
        }
        if clients.is_empty() {
            continue;
        }
        info!(target: "say", "radio {} from {}: {}", tx.freq, name, reach.text);
        for heard in understanding.split(clients, tx.language, &reach.text) {
            let mut message = radio_message(reach.freq, &name, tx.verb, &heard.text, tx.italics);
            // After the channel, before the name (tg `compose_message`'s `languageicon`).
            if let Some(tag) = heard.tag {
                message.segments.insert(
                    2,
                    ss13_shared::chat::ChatSegment::new(ss13_shared::chat::Span::Plain, tag),
                );
            }
            ss13_chat::server::write_heard(
                &mut chat,
                heard.clients,
                message,
                &heard.text,
                &reach.text,
                |client| {
                    controls
                        .get(client)
                        .ok()
                        .and_then(|body| codewords.get(body.0).ok())
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_chat::radio::*;
    use ss13_mapload::radio::RadioChannelVar;

    fn dm(path: &str, channels: &[(&str, bool)]) -> DmRadio {
        DmRadio {
            path: path.into(),
            on: true,
            frequency: f64::from(FREQ_COMMON),
            listening: true,
            canhear_range: 3.0,
            channels: channels
                .iter()
                .map(|&(name, listening)| RadioChannelVar {
                    name: name.into(),
                    listening,
                })
                .collect(),
            ..default()
        }
    }

    #[test]
    fn channel_access_follows_talk_into_and_can_receive() {
        let eng = Radio::new(&dm(
            "/obj/item/radio/headset/headset_eng",
            &[(RADIO_CHANNEL_ENGINEERING, true)],
        ));
        assert_eq!(eng.kind, RadioKind::Headset);
        assert!(eng.broadcasting && eng.listening, "headset Initialize");
        let eng_channel = Some(RadioTarget::Channel(RADIO_CHANNEL_ENGINEERING));
        assert_eq!(eng.transmit_freq(eng_channel), Some(FREQ_ENGINEERING));
        assert_eq!(
            eng.transmit_freq(Some(RadioTarget::Department)),
            Some(FREQ_ENGINEERING)
        );
        assert_eq!(eng.transmit_freq(None), Some(FREQ_COMMON));
        // A channel the key lacks: nothing is sent.
        let sec = Some(RadioTarget::Channel(RADIO_CHANNEL_SECURITY));
        assert_eq!(eng.transmit_freq(sec), None);
        assert!(eng.receives(FREQ_ENGINEERING) && eng.receives(FREQ_COMMON));
        assert!(!eng.receives(FREQ_SECURITY) && !eng.receives(FREQ_SYNDICATE));

        // A plain headset has no channels: every key goes out on its own frequency.
        let plain = Radio::new(&dm("/obj/item/radio/headset", &[]));
        assert_eq!(plain.transmit_freq(eng_channel), Some(FREQ_COMMON));
        assert_eq!(
            plain.transmit_freq(Some(RadioTarget::Department)),
            Some(FREQ_COMMON)
        );
        assert!(!plain.receives(FREQ_ENGINEERING));

        // The captain's key grants engineering switched off: neither talk nor listen.
        let captain = Radio::new(&dm(
            "/obj/item/radio/headset/heads/captain",
            &[
                (RADIO_CHANNEL_COMMAND, true),
                (RADIO_CHANNEL_ENGINEERING, false),
            ],
        ));
        assert_eq!(captain.transmit_freq(eng_channel), None);
        assert!(!captain.receives(FREQ_ENGINEERING) && captain.receives(FREQ_COMMAND));

        // Syndicate keys hear every well-known channel.
        let mut syndie = dm("/obj/item/radio/headset/syndicate", &[]);
        syndie.special_channels = RADIO_SPECIAL_SYNDIE;
        let syndie = Radio::new(&syndie);
        assert!(syndie.receives(FREQ_SECURITY) && syndie.receives(FREQ_SYNDICATE));
        assert!(!syndie.receives(1453), "not a channel frequency");

        // A switched-off radio hears nothing.
        let mut off = Radio::new(&dm("/obj/item/radio/off", &[]));
        assert!(!off.listening && !off.receives(FREQ_COMMON));
        off.listening = true;
        off.on = false;
        assert!(!off.receives(FREQ_COMMON));
    }

    #[test]
    fn the_window_tunes_and_switches() {
        let mut radio = Radio::new(&dm(
            "/obj/item/radio/headset/headset_eng",
            &[(RADIO_CHANNEL_ENGINEERING, true)],
        ));
        let open = RadioTuning::default();
        apply_act(&mut radio, open, &RadioAct::Adjust(0.2));
        assert_eq!(radio.frequency, FREQ_COMMON + 2);
        apply_act(&mut radio, open, &RadioAct::Adjust(-10.0));
        assert_eq!(radio.frequency, MIN_FREQ, "kept in the band");
        apply_act(&mut radio, open, &RadioAct::Tune(145.9));
        assert_eq!(radio.frequency, FREQ_COMMON);
        apply_act(
            &mut radio,
            open,
            &RadioAct::TuneToChannel(RADIO_CHANNEL_ENGINEERING.into()),
        );
        assert_eq!(radio.frequency, FREQ_ENGINEERING);
        apply_act(
            &mut radio,
            open,
            &RadioAct::TuneToChannel(RADIO_CHANNEL_SECURITY.into()),
        );
        assert_eq!(radio.frequency, FREQ_ENGINEERING, "not a key channel");
        apply_act(
            &mut radio,
            open,
            &RadioAct::Channel(RADIO_CHANNEL_ENGINEERING.into()),
        );
        assert!(!radio.channels[0].1, "the channel is switched off");
        apply_act(&mut radio, open, &RadioAct::Listen);
        assert!(!radio.listening);
        // A locked intercom keeps its frequency.
        let mut chapel = Radio::new(&dm("/obj/item/radio/intercom/chapel/directional/west", &[]));
        let lock = RadioTuning::new(&dm("/obj/item/radio/intercom/chapel/directional/west", &[]));
        assert!(lock.freqlock);
        apply_act(&mut chapel, lock, &RadioAct::Adjust(1.0));
        assert_eq!(chapel.frequency, FREQ_CONFESSIONAL);
    }

    #[test]
    fn initialize_overrides() {
        let intercom = Radio::new(&dm("/obj/item/radio/intercom/directional/north", &[]));
        assert_eq!(intercom.kind, RadioKind::Intercom);
        assert!(!intercom.broadcasting && intercom.listening);
        let chapel = Radio::new(&dm("/obj/item/radio/intercom/chapel/directional/west", &[]));
        assert_eq!(chapel.frequency, FREQ_CONFESSIONAL);
        assert!(chapel.broadcasting);
        let prison = Radio::new(&dm(
            "/obj/item/radio/intercom/prison/directional/north",
            &[],
        ));
        assert!(prison.tx_cut && prison.receives(FREQ_COMMON));
        let inside = Radio::new(&dm(
            "/obj/item/radio/intercom/interrogation/inside/directional/south",
            &[],
        ));
        assert_eq!(inside.frequency, FREQ_INTERROGATION);
        assert!(inside.broadcasting && !inside.listening);
        let eng = Radio::new(&dm(
            "/obj/item/radio/intercom/departmental/engineering",
            &[(RADIO_CHANNEL_ENGINEERING, true)],
        ));
        assert_eq!(eng.frequency, FREQ_ENGINEERING);
        // Map edits are sanitized: an even or out-of-band frequency is moved into the band.
        let mut edited = dm("/obj/item/radio/intercom", &[]);
        edited.frequency = 1446.0;
        assert_eq!(Radio::new(&edited).frequency, FREQ_AI_PRIVATE);
    }

    #[test]
    fn radio_noise_goes_to_the_speaker_once_per_half_second() {
        let mut world = World::new();
        world.init_resource::<Messages<RadioTalked>>();
        world.init_resource::<Messages<SendSound>>();
        world.init_resource::<Time>();
        world.init_resource::<crate::sound::SoundRng>();
        let client = world.spawn_empty().id();
        let speaker = world.spawn(crate::server::PlayerMob { client }).id();
        let headset = dm("/obj/item/radio/headset", &[]);
        let radio = world.spawn((Radio::new(&headset), headset)).id();
        let speakers = dm("/obj/item/radio/entertainment/speakers", &[]);
        let quiet = world.spawn((Radio::new(&speakers), speakers)).id();
        let mut schedule = Schedule::default();
        schedule.add_systems(radio_noise);
        let mut talk = |world: &mut World, radio: Entity| {
            world.write_message(RadioTalked { speaker, radio });
            schedule.run(world);
            world
                .resource_mut::<Messages<SendSound>>()
                .drain()
                .map(|s| (s.client, s.sound.file, s.sound.speed))
                .collect::<Vec<_>>()
        };
        let sent = talk(&mut world, radio);
        assert_eq!(sent.len(), 1);
        assert_eq!((sent[0].0, sent[0].1.as_str()), (client, RADIO_TALK_SOUND));
        assert!((38000.0 / 44100.0..=45000.0 / 44100.0).contains(&sent[0].2));
        assert!(talk(&mut world, radio).is_empty(), "audio_cooldown");
        assert!(talk(&mut world, quiet).is_empty(), "radio_noise = FALSE");
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_millis(600));
        assert_eq!(talk(&mut world, radio).len(), 1);
    }
}

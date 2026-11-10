//! Assemblies (server), after tg `code/modules/assembly/` (`assembly.dm`, `holder.dm`,
//! `signaler.dm`, `timer.dm`, `proximity.dm`, `igniter.dm`, `voice.dm`, `infrared.dm`,
//! `health.dm`) and `code/game/communications.dm` (`post_signal`).
//!
//! - Every signaler, timer, proximity sensor, igniter/condenser, voice analyzer, infrared
//!   emitter and health sensor gets an [`Assembly`] when it spawns. A screwdriver toggles
//!   `secured`; only unsecured assemblies join others. Two unsecured assemblies make an
//!   [`AssemblyHolder`] on the floor (`assemble`), more go into it (up to 12, never two
//!   igniters); a screwdriver takes the holder apart (the parts drop, secured).
//! - [`pulse`] is tg `pulse()`: through the wires the assembly is attached to
//!   (`crate::wires::pulse_assembly`), else to the other parts of its holder (`pulsed` ->
//!   `activate`, 3 s cooldown each) and the holder's master (a tank bomb), else to the valve it
//!   is attached to (`crate::assemblies::ttv`).
//! - A signaler sends its code on its frequency to every other signaler on it (`range` 0:
//!   everywhere, the low-power one: 9 tiles on the same level); one with the same code pulses
//!   and beeps. Used in hand (secured) the devices open their windows; the window closes when
//!   the assembly (or what holds it) leaves the user's hands (`hands_state`) or is unsecured.
//! - Timers count down and pulse (`loop` restarts them); proximity sensors arm after their
//!   delay and pulse when something moves within `sensitivity` tiles of their host (on a turf,
//!   or anywhere in a chemical grenade's landmine mode); health sensors pulse once when the
//!   mob carrying them drops to the target health; infrared emitters pulse when something dense
//!   steps into their beam; voice analyzers record a phrase (or a voice) and pulse 2 s after
//!   hearing it again; igniters heat their tile (`hotspot_expose(1000, 1000)`), condensers cool
//!   its air.
//!
//! Cost: per tick one pass over the assemblies that are counting down or scanning ([`Active`],
//! a handful) and, when any proximity sensor or beam is on, over the steps taken this tick; a
//! signal is one pass over the signalers (hundreds at most, 1 s cooldown each).
// shortcut: no suicides, shock kits or door controls as items here (the controls
// are `crate::wallmount::controls`); no assembly overlays or holder composite sprite (the
// holder shows its own icon), no visible infrared beam and no `simple_rotation` (an emitter
// beams the way its user faced when turning it on, else south), no igniter sparks or
// `COMSIG_IGNITER_ACTIVATE` (rigged fuel tanks, heated cups), no mapped instance vars (mapped
// signalers start on the default frequency and code), no `on_found` or `attack_hand` triggers,
// no right-click signalling, no signal logs (`investigate_log`).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_chat::radio::{MAX_FREE_FREQ, MIN_FREE_FREQ, sanitize_frequency};
use ss13_chat::server::Spoke;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::{Dir, Facing, TilePos};
use ss13_tgui::server::{
    AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_tgui::{UiInterface as _, UiStatus};
use ss13_world::{Density, Moved, SpatialIndex, WorldSystems};

use super::*;
use crate::health::{HEALTH_THRESHOLD_CRIT, HEALTH_THRESHOLD_DEAD, Health};
use crate::inventory::{CarriedBy, Inventory};
use crate::storage::{Stored, StoredIn, put_inside, take_out};
use crate::tools::{ToolKind, tool_of};

pub const ASSEMBLY_ROOT: &str = "/obj/item/assembly";
pub const HOLDER_PATH: &str = ss13_mapload::runtime::ASSEMBLY_HOLDER;
const SIGNALER: &str = "/obj/item/assembly/signaler";
const SIGNALER_LOW_RANGE: &str = "/obj/item/assembly/signaler/low_range";
const SIGNALER_INTERNAL: &str = "/obj/item/assembly/signaler/internal";
const SIGNALER_CYBORG: &str = "/obj/item/assembly/signaler/cyborg";
const TIMER: &str = "/obj/item/assembly/timer";
const PROX: &str = "/obj/item/assembly/prox_sensor";
const IGNITER: &str = "/obj/item/assembly/igniter";
const CONDENSER: &str = "/obj/item/assembly/igniter/condenser";
const VOICE: &str = "/obj/item/assembly/voice";
const INFRA: &str = "/obj/item/assembly/infra";
const HEALTH_SENSOR: &str = "/obj/item/assembly/health";
const MOUSETRAP: &str = "/obj/item/assembly/mousetrap";
const MOUSETRAP_ARMED: &str = "/obj/item/assembly/mousetrap/armed";
const TRAP_SOUND: &str = "sound/items/weapons/handcuffs.ogg";
const SNAP_SOUND: &str = "sound/effects/snap.ogg";
/// The mousetrap's `Paralyze(6 SECONDS)` on bare feet.
const TRAP_PARALYZE: f32 = 6.0;

pub const BEEP_SOUND: &str = "sound/machines/beep/triple_beep.ogg";
const VOICE_SOUND: &str = "sound/effects/whirthunk.ogg";
/// `ASSEMBLY_BEEP_VOLUME`.
pub const BEEP_VOLUME: f32 = 5.0;
/// `HOLDER_MAX_ASSEMBLIES`.
const HOLDER_MAX: usize = 12;
/// `activation_cooldown` (3 s) and the signaler's `cooldown_length` (1 s).
const ACTIVATION_COOLDOWN: f64 = 3.0;
const SIGNALER_COOLDOWN: f64 = 1.0;
/// `FREQ_SIGNALER`, `DEFAULT_SIGNALER_CODE`.
pub const FREQ_SIGNALER: u16 = 1457;
pub const DEFAULT_SIGNALER_CODE: i32 = 30;
/// `/obj/item/assembly/signaler/low_range` `range`.
const LOW_RANGE: u32 = 9;
/// Timer `time`/`saved_time` (10 s, clamped to 1-600), prox `time` (20 s, 0-600) and
/// `sensitivity` (0-5).
const TIMER_TIME: f64 = 10.0;
const PROX_TIME: f64 = 20.0;
const MAX_TIME: f64 = 600.0;
const MAX_SENSITIVITY: i64 = 5;
/// The beep's `hearing_range` of signalers (1), timers, prox sensors and emitters (3); the
/// health sensor's `audible_message` reaches the default 7.
const SIGNALER_HEARING: u32 = 1;
const DEVICE_HEARING: u32 = 3;
/// Igniter `heat`, the condenser's `MIN_FREEZE_TEMP`, `EXPOSED_VOLUME`, `ROOM_TEMP`,
/// `MAX_FREEZE_TEMP`.
const IGNITER_HEAT: f64 = 1000.0;
const CONDENSER_HEAT: f64 = 50.0;
const EXPOSED_VOLUME: f64 = 1000.0;
const ROOM_TEMP: f64 = 293.0;
const MAX_FREEZE_TEMP: f64 = 1_000_000.0;
/// Infrared `max_beam_length` and `beam_pass_flags` (`PASSTABLE|PASSGLASS|PASSGRILLE`).
const BEAM_LENGTH: i32 = 8;
const BEAM_PASS_FLAGS: u32 = 1 | 2 | 4;
/// Voice analyzer `send_pulse` delay.
const VOICE_DELAY: f64 = 2.0;
/// `ASSEMBLY_*` behaviour bits and `WIRES_INPUT`.
const ASSEMBLY_INPUT: u8 = 1;
const ASSEMBLY_TOGGLE_ARMED: u8 = 2;
const ASSEMBLY_FUNCTIONAL_OUTPUT: u8 = 4;
const WIRES_INPUT: u8 = 1;

/// Voice analyzer modes (`INCLUSIVE_MODE` .. `VOICE_SENSOR_MODE`).
pub const VOICE_MODES: [&str; 4] = ["inclusive", "exclusive", "recognizer", "voice sensor"];

/// What an assembly is, with its own vars.
#[derive(Clone, Debug, PartialEq)]
pub enum Device {
    Signaler {
        frequency: u16,
        code: i32,
        /// 0: everywhere; else tiles on the same level.
        range: u32,
        /// The internal and cyborg signalers: no tools, no syncing.
        locked: bool,
    },
    Timer {
        timing: bool,
        time: f64,
        saved_time: f64,
        looping: bool,
    },
    Prox {
        scanning: bool,
        timing: bool,
        time: f64,
        sensitivity: u8,
        /// In a chemical grenade (`set_ignore_if_not_on_turf(FALSE)`).
        landmine: bool,
    },
    Igniter {
        condenser: bool,
    },
    Voice {
        listening: bool,
        recorded: String,
        /// Index into [`VOICE_MODES`].
        mode: usize,
    },
    Infra {
        on: bool,
        visible: bool,
        /// The tiles of the beam as of the last tick (its start first).
        beam: Vec<TilePos>,
    },
    Health {
        scanning: bool,
        target: f32,
        scan: Option<f32>,
    },
    Mousetrap {
        armed: bool,
    },
}

impl Device {
    /// `assembly_behavior`.
    pub fn behavior(&self) -> u8 {
        match self {
            Device::Signaler { .. } => {
                ASSEMBLY_INPUT | ASSEMBLY_TOGGLE_ARMED | ASSEMBLY_FUNCTIONAL_OUTPUT
            }
            Device::Timer { .. }
            | Device::Prox { .. }
            | Device::Voice { .. }
            | Device::Health { .. }
            | Device::Mousetrap { .. } => ASSEMBLY_INPUT | ASSEMBLY_TOGGLE_ARMED,
            Device::Igniter { .. } | Device::Infra { .. } => ASSEMBLY_FUNCTIONAL_OUTPUT,
        }
    }

    /// Counting down, scanning or beaming: ticked every frame.
    fn active(&self) -> bool {
        match self {
            Device::Timer { timing, .. } => *timing,
            Device::Prox {
                scanning, timing, ..
            } => *scanning || *timing,
            Device::Health { scanning, .. } => *scanning,
            Device::Infra { on, .. } => *on,
            Device::Mousetrap { armed } => *armed,
            _ => false,
        }
    }

    fn same_kind(&self, other: &Device) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

/// What an assembly is attached to (tg `holder` / `connected`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Attached {
    #[default]
    None,
    /// An [`AssemblyHolder`].
    Holder(Entity),
    /// The wires of this entity (`connected`).
    Wires(Entity),
    /// A tank transfer valve.
    Valve(Entity),
}

/// tg `/obj/item/assembly` state.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Assembly {
    pub device: Device,
    pub secured: bool,
    pub attached: Attached,
    /// `next_activate`, game seconds.
    pub next_activate: f64,
    /// `COOLDOWN_SIGNALLER_SEND`, game seconds.
    pub send_ready: f64,
}

/// tg `/obj/item/assembly_holder`: its parts are its [`Stored`] assemblies.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AssemblyHolder {
    /// `master`: the tank it is rigged to.
    pub master: Option<Entity>,
}

/// On an assembly whose device runs every tick.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Active;

/// Something an assembly (or what it is attached to) does later: tg `addtimer`s.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Delayed {
    pub at: f64,
    pub what: Later,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Later {
    /// The voice analyzer's pulse.
    Pulse,
    /// A tank bomb's `ignite`.
    Ignite,
    /// A transfer valve's `toggle_off`.
    ValveReady,
}

pub struct AssembliesServerPlugin;

impl Plugin for AssembliesServerPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // Assemblies open their windows or act, holders use their parts; a chemical grenade casing
        // primes or opens its wires, a transfer valve its window.
        app.add_click_handler(ClickPhase::AttackSelf, 1900, |w, c| {
            crate::assemblies::server::attack_self(w, c.client, c.mob, c.target)
        });
        // Assemblies, holders, casings, valves and tanks in the other hand.
        app.add_click_handler(ClickPhase::OtherHand, 700, |w, c| {
            c.held.is_some_and(|h| {
                crate::assemblies::server::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        // Assemblies, holders, casings, valves and tanks.
        app.add_click_handler(ClickPhase::HeldOnTarget, 500, |w, c| {
            c.held.is_some_and(|h| {
                crate::assemblies::server::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        app.register_ui::<SignalerUi>()
            .register_ui::<TimerUi>()
            .register_ui::<ProximitySensorUi>()
            .register_ui::<HealthSensorUi>()
            .register_ui::<InfraredEmitterUi>()
            .register_ui::<TransferValveUi>()
            .add_observer(make_assembly)
            .add_systems(
                Update,
                (
                    mark_active,
                    tick_devices.run_if(any_with_component::<Active>),
                    run_delayed.run_if(any_with_component::<Delayed>),
                    sense_moves
                        .after(WorldSystems::Movement)
                        .run_if(any_with_component::<Active>),
                    hear_speech.after(ss13_chat::server::SaySystems),
                    window_status.in_set(UiSystems::StatusHook),
                    (
                        signaler_acts,
                        timer_acts,
                        prox_acts,
                        health_acts,
                        infra_acts,
                        super::ttv::valve_acts,
                    )
                        .in_set(UiSystems::Handle),
                ),
            );
    }
}

/// tg `Initialize` of the assembly types: their state from the type.
pub fn device_of(path: &str) -> Option<Device> {
    if !is_under(path, ASSEMBLY_ROOT) {
        return None;
    }
    Some(if is_under(path, SIGNALER) {
        Device::Signaler {
            frequency: FREQ_SIGNALER,
            code: DEFAULT_SIGNALER_CODE,
            range: if is_under(path, SIGNALER_LOW_RANGE) {
                LOW_RANGE
            } else {
                0
            },
            locked: is_under(path, SIGNALER_INTERNAL) || is_under(path, SIGNALER_CYBORG),
        }
    } else if is_under(path, TIMER) {
        Device::Timer {
            timing: false,
            time: TIMER_TIME,
            saved_time: TIMER_TIME,
            looping: false,
        }
    } else if is_under(path, PROX) {
        Device::Prox {
            scanning: false,
            timing: false,
            time: PROX_TIME,
            sensitivity: 0,
            landmine: false,
        }
    } else if is_under(path, IGNITER) {
        Device::Igniter {
            condenser: is_under(path, CONDENSER),
        }
    } else if is_under(path, VOICE) {
        Device::Voice {
            listening: false,
            recorded: String::new(),
            mode: 0,
        }
    } else if is_under(path, INFRA) {
        Device::Infra {
            on: false,
            visible: false,
            beam: Vec::new(),
        }
    } else if is_under(path, MOUSETRAP) {
        Device::Mousetrap {
            armed: is_under(path, MOUSETRAP_ARMED),
        }
    } else if is_under(path, HEALTH_SENSOR) {
        Device::Health {
            scanning: false,
            target: HEALTH_THRESHOLD_CRIT,
            scan: None,
        }
    } else {
        return None;
    })
}

fn make_assembly(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    if is_under(&item.path, HOLDER_PATH) {
        commands
            .entity(add.entity)
            .try_insert((AssemblyHolder::default(), AlwaysState));
        return;
    }
    if let Some(device) = device_of(&item.path) {
        // The windows follow the user's hands (`hands_state`, [`window_status`]).
        commands.entity(add.entity).try_insert((
            Assembly {
                device,
                secured: true,
                attached: Attached::None,
                next_activate: 0.0,
                send_ready: 0.0,
            },
            AlwaysState,
        ));
    }
}

/// [`Active`] follows the devices' state.
fn mark_active(
    assemblies: Query<(Entity, &Assembly, Has<Active>), Changed<Assembly>>,
    mut commands: Commands,
) {
    for (e, a, marked) in &assemblies {
        match (a.device.active() && a.secured, marked) {
            (true, false) => {
                commands.entity(e).try_insert(Active);
            }
            (false, true) => {
                commands.entity(e).try_remove::<Active>();
            }
            _ => {}
        }
    }
}

// --- Helpers ----------------------------------------------------------------------------

pub(crate) use crate::tools::now;

pub(crate) fn name_of(world: &World, e: Entity) -> String {
    crate::chem::name_of(world, e)
}

pub(crate) fn path_of(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmItem>(e).map(|d| d.path.as_str())
}

pub(crate) fn tell(world: &mut World, client: Entity, span: Span, text: impl Into<String>) {
    crate::tools::tell(world, client, span, text);
}

/// The outermost container of `e` (what is on the map or in a mob's inventory).
pub(crate) fn root_of(world: &World, mut e: Entity) -> Entity {
    for _ in 0..8 {
        match world.get::<StoredIn>(e) {
            Some(s) => e = s.0,
            None => break,
        }
    }
    e
}

/// `e` (or what holds it) is in one of `mob`'s hands (tg `hands_state`).
pub(crate) fn in_hands_of(world: &World, e: Entity, mob: Entity) -> bool {
    let root = root_of(world, e);
    world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.hands.contains(&Some(root)))
}

/// Held by `mob`, or lying next to it (tg `can_perform_action` reach for the tool acts).
pub(crate) fn within_reach(world: &World, mob: Entity, item: Entity) -> bool {
    let root = root_of(world, item);
    if world.get::<CarriedBy>(root).is_some_and(|c| c.0 == mob) {
        return true;
    }
    match (world.get::<TilePos>(mob), world.get::<TilePos>(root)) {
        (Some(&a), Some(&b)) => a.chebyshev(b).is_some_and(|d| d <= 1),
        _ => false,
    }
}

/// The item from wherever it is into `container` (tg `transferItemToLoc`).
pub(crate) fn insert(world: &mut World, item: Entity, container: Entity) {
    put_inside(world, item, container);
}

/// Out of its container to `mob`'s hands (a free one, the active first), else onto the floor
/// (tg `put_in_hands`).
pub(crate) fn to_hands(world: &mut World, mob: Entity, item: Entity) {
    let Some(pos) = world.get::<TilePos>(mob).copied() else {
        return;
    };
    take_out(world, item, pos);
    if let Some(inventory) = world.get::<Inventory>(mob).cloned()
        && let Some(hand) = [inventory.active, 1 - inventory.active]
            .into_iter()
            .find(|&h| inventory.hands[h].is_none())
    {
        crate::inventory::pick_up(world, mob, item, hand);
    }
}

/// Out of its container onto the tile of what held it (tg `forceMove(drop_location())`).
pub(crate) fn drop_out(world: &mut World, item: Entity, from: Entity) {
    if let Some(pos) = crate::explosions::turf_of(world, from) {
        take_out(world, item, pos);
    } else {
        world.entity_mut(item).remove::<StoredIn>();
    }
}

/// A chat line to every client within `range` of `e`'s turf.
pub(crate) fn around(world: &mut World, e: Entity, range: u32, span: Span, text: &str) {
    let Some(pos) = crate::explosions::turf_of(world, e) else {
        return;
    };
    let clients = crate::obj_damage::clients_in_range(world, pos, range, &[]);
    if clients.is_empty() {
        return;
    }
    world.write_message(SendChat {
        clients,
        message: ChatMessage {
            channel: ChatChannel::Local,
            segments: vec![ChatSegment::new(span, text)],
        },
    });
}

/// `audible_message("*beep* *beep* *beep*", hearing_range)` and the triple beep.
pub(crate) fn beep(world: &mut World, e: Entity, range: u32) {
    around(world, e, range, Span::Plain, "*beep* *beep* *beep*");
    if let Some(pos) = crate::explosions::turf_of(world, e) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, BEEP_SOUND, BEEP_VOLUME, true),
        );
    }
}

/// The device says something (`say`, `verb_say` "beeps").
fn say(world: &mut World, e: Entity, text: &str) {
    let name = crate::cooking::capitalize(&name_of(world, e));
    let line = format!("{name} сигналит, \"{text}\"");
    around(
        world,
        e,
        crate::health::DEFAULT_MESSAGE_RANGE,
        Span::Plain,
        &line,
    );
}

/// The assemblies of a holder, in attaching order.
pub fn parts_of(world: &World, holder: Entity) -> Vec<Entity> {
    world
        .get::<Stored>(holder)
        .map(|s| {
            s.items()
                .iter()
                .copied()
                .filter(|&e| world.get::<Assembly>(e).is_some())
                .collect()
        })
        .unwrap_or_default()
}

/// tg `connected?.holder || holder?.master || holder || src`: the atom the assembly works
/// through.
pub fn host_of(world: &World, a: Entity) -> Entity {
    match world.get::<Assembly>(a).map(|a| a.attached) {
        Some(Attached::Wires(h) | Attached::Valve(h)) => h,
        Some(Attached::Holder(h)) => world
            .get::<AssemblyHolder>(h)
            .and_then(|h| h.master)
            .unwrap_or(h),
        _ => a,
    }
}

fn set_name(world: &mut World, e: Entity, name: String) {
    world.entity_mut(e).insert(Name::new(name));
}

// --- Securing, joining, taking apart ----------------------------------------------------

/// tg `toggle_secure()` of each type: unsecuring stops timers, scans and listening.
pub fn toggle_secure(world: &mut World, a: Entity) -> bool {
    let Some(mut asm) = world.get_mut::<Assembly>(a) else {
        return false;
    };
    asm.secured = !asm.secured;
    let secured = asm.secured;
    match &mut asm.device {
        Device::Timer { timing, .. } if !secured => *timing = false,
        Device::Prox {
            scanning, timing, ..
        } if !secured => {
            *scanning = false;
            *timing = false;
        }
        Device::Health { scanning, .. } if !secured => *scanning = false,
        Device::Voice { listening, .. } => *listening = false,
        _ => {}
    }
    secured
}

/// `holder.attach(A)`: into the holder, secured (`toggle_secure`), `on_attach`.
fn attach_to_holder(world: &mut World, holder: Entity, a: Entity) {
    insert(world, a, holder);
    if let Some(mut asm) = world.get_mut::<Assembly>(a) {
        asm.attached = Attached::Holder(holder);
    }
    toggle_secure(world, a);
}

/// The holder's name: its parts' names joined (`add_assembly`).
fn rename_holder(world: &mut World, holder: Entity) {
    let names: Vec<String> = parts_of(world, holder)
        .into_iter()
        .map(|p| name_of(world, p))
        .collect();
    set_name(world, holder, format!("{} assembly", names.join("-")));
}

/// `/obj/item/assembly/item_interaction` with another assembly: a new holder of both on the
/// floor.
fn combine(world: &mut World, client: Entity, target: Entity, tool: Entity) {
    let (Some(t), Some(n)) = (
        world.get::<Assembly>(target).cloned(),
        world.get::<Assembly>(tool).cloned(),
    ) else {
        return;
    };
    let no_duplicates =
        matches!(t.device, Device::Igniter { .. }) || matches!(n.device, Device::Igniter { .. });
    if no_duplicates && t.device.same_kind(&n.device) {
        let name = name_of(world, tool);
        tell(
            world,
            client,
            Span::Warning,
            format!("can't attach another {name}!"),
        );
        return;
    }
    if n.secured {
        let name = name_of(world, tool);
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} is not attachable!"),
        );
        return;
    }
    if t.secured {
        let name = name_of(world, target);
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} is not attachable!"),
        );
        return;
    }
    let Some(pos) = crate::explosions::turf_of(world, target) else {
        return;
    };
    let Some(holder) = crate::construction::spawn_obj(world, HOLDER_PATH, pos) else {
        warn!("no {HOLDER_PATH} prototype");
        return;
    };
    world
        .entity_mut(holder)
        .insert((AssemblyHolder::default(), AlwaysState));
    attach_to_holder(world, holder, target);
    attach_to_holder(world, holder, tool);
    let (a, b) = (name_of(world, target), name_of(world, tool));
    set_name(world, holder, format!("{a}-{b} assembly"));
    tell(world, client, Span::Notice, "parts combined");
}

/// `try_add_assembly`.
fn try_add(world: &mut World, client: Entity, holder: Entity, a: Entity) -> bool {
    let Some(asm) = world.get::<Assembly>(a).cloned() else {
        return false;
    };
    if asm.secured {
        tell(world, client, Span::Warning, "not attachable!");
        return false;
    }
    let parts = parts_of(world, holder);
    if parts.len() >= HOLDER_MAX {
        tell(world, client, Span::Warning, "too many assemblies!");
        return false;
    }
    if matches!(asm.device, Device::Igniter { .. })
        && parts.iter().any(|&p| {
            world
                .get::<Assembly>(p)
                .is_some_and(|o| o.device.same_kind(&asm.device))
        })
    {
        tell(
            world,
            client,
            Span::Warning,
            "can't attach another of that!",
        );
        return false;
    }
    attach_to_holder(world, holder, a);
    rename_holder(world, holder);
    tell(world, client, Span::Notice, "part attached");
    true
}

/// `/obj/item/assembly_holder/screwdriver_act` -> `atom_deconstruct`: the parts drop (their
/// `on_detach`), the holder is gone.
pub fn disassemble(world: &mut World, holder: Entity) {
    for part in parts_of(world, holder) {
        detach(world, part, holder);
    }
    if world.get_entity(holder).is_ok() {
        world.despawn(holder);
    }
}

/// tg `on_detach`: out of what held it onto its tile, attached to nothing.
pub fn detach(world: &mut World, a: Entity, from: Entity) {
    if let Some(mut asm) = world.get_mut::<Assembly>(a) {
        asm.attached = Attached::None;
        if let Device::Prox { landmine, .. } = &mut asm.device {
            *landmine = false;
        }
    }
    drop_out(world, a, from);
}

/// `/obj/item/assembly/screwdriver_act`.
fn screwdriver(world: &mut World, client: Entity, a: Entity) {
    let name = crate::cooking::capitalize(&name_of(world, a));
    let text = if toggle_secure(world, a) {
        format!("{name} is ready!")
    } else {
        format!("{name} can now be attached!")
    };
    tell(world, client, Span::Notice, text);
}

/// Tools and assemblies used on an assembly or a holder (tg `item_interaction`,
/// `screwdriver_act`, the signaler's syncing, the voice analyzer's `multitool_act`), then on
/// the chemical grenade casing and the transfer valve. True when the click was handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if super::chem_grenade::item_interaction(world, client, mob, held, target)
        || super::ttv::item_interaction(world, client, mob, held, target)
    {
        return true;
    }
    if held == target || !within_reach(world, mob, target) {
        return false;
    }
    let tool = tool_of(world, held).map(|t| t.0);
    if world.get::<AssemblyHolder>(target).is_some() {
        if tool == Some(ToolKind::Screwdriver) {
            tell(world, client, Span::Notice, "disassembled");
            disassemble(world, target);
            return true;
        }
        if world.get::<Assembly>(held).is_some() {
            try_add(world, client, target, held);
            return true;
        }
        return false;
    }
    let Some(asm) = world.get::<Assembly>(target).cloned() else {
        return false;
    };
    // Parts of a holder or on wires are not reached directly (`attack_hand` returns).
    if asm.attached != Attached::None {
        return false;
    }
    let locked = matches!(asm.device, Device::Signaler { locked: true, .. });
    if tool == Some(ToolKind::Screwdriver) {
        if !locked {
            screwdriver(world, client, target);
        }
        return true;
    }
    if tool == Some(ToolKind::Multitool) && matches!(asm.device, Device::Voice { .. }) {
        let mut mode_name = "";
        if let Some(mut a) = world.get_mut::<Assembly>(target)
            && let Device::Voice {
                listening,
                recorded,
                mode,
            } = &mut a.device
        {
            *mode = (*mode + 1) % VOICE_MODES.len();
            *listening = false;
            recorded.clear();
            mode_name = VOICE_MODES[*mode];
        }
        let name = name_of(world, target);
        tell(
            world,
            client,
            Span::Notice,
            format!("You set {name} into {mode_name} mode."),
        );
        return true;
    }
    if world.get::<AssemblyHolder>(held).is_some() {
        try_add(world, client, held, target);
        return true;
    }
    let Some(tool_asm) = world.get::<Assembly>(held).cloned() else {
        return false;
    };
    if locked {
        return true;
    }
    // `/obj/item/assembly/signaler/item_interaction`: two secured signalers sync.
    if let (
        Device::Signaler { .. },
        Device::Signaler {
            frequency, code, ..
        },
    ) = (&asm.device, &tool_asm.device)
        && asm.secured
        && tool_asm.secured
    {
        let (frequency, code) = (*frequency, *code);
        if let Some(mut a) = world.get_mut::<Assembly>(target)
            && let Device::Signaler {
                frequency: f,
                code: c,
                ..
            } = &mut a.device
        {
            *f = frequency;
            *c = code;
        }
        let (from, to) = (name_of(world, held), name_of(world, target));
        tell(
            world,
            client,
            Span::Plain,
            format!("You transfer the frequency and code of {from} to {to}"),
        );
        return true;
    }
    combine(world, client, target, held);
    true
}

/// Use in hand: a holder uses each part (`/obj/item/assembly_holder/attack_self`), an
/// assembly opens its window (secured) or acts (igniter, voice analyzer); a chemical grenade
/// casing, a transfer valve or a rigged tank do theirs. False for anything else.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if super::chem_grenade::attack_self(world, client, mob, item)
        || super::ttv::attack_self(world, client, mob, item)
    {
        return true;
    }
    if world.get::<AssemblyHolder>(item).is_some() {
        holder_attack_self(world, client, mob, item);
        return true;
    }
    if world.get::<Assembly>(item).is_none() {
        return false;
    }
    assembly_attack_self(world, client, mob, item);
    true
}

pub(crate) fn holder_attack_self(world: &mut World, client: Entity, mob: Entity, holder: Entity) {
    let parts = parts_of(world, holder);
    if parts.len() == 1 {
        tell(world, client, Span::Warning, "part missing!");
        return;
    }
    for part in parts {
        assembly_attack_self(world, client, mob, part);
    }
}

/// One assembly's `attack_self`.
pub(crate) fn assembly_attack_self(world: &mut World, client: Entity, mob: Entity, a: Entity) {
    let Some(asm) = world.get::<Assembly>(a).cloned() else {
        return;
    };
    match asm.device {
        // `/obj/item/assembly/igniter/attack_self`: not when part of something.
        Device::Igniter { .. } => {
            if asm.attached == Attached::None {
                activate(world, a);
            }
        }
        Device::Voice { .. } => {
            activate(world, a);
        }
        Device::Mousetrap { armed } => {
            let name = name_of(world, a);
            let text = if armed {
                format!("You disarm {name}.")
            } else {
                format!("You arm {name}.")
            };
            tell(world, client, Span::Notice, text);
            set_armed(world, a, !armed);
        }
        _ => {
            // `ui_status`: an unsecured device closes its window (`is_secured`).
            if !asm.secured {
                let name = crate::cooking::capitalize(&name_of(world, a));
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!("{name} is unsecured!"),
                );
                return;
            }
            let title = name_of(world, a);
            let open = match asm.device {
                Device::Signaler { .. } => OpenUi::new::<SignalerUi>(client, mob, a, title),
                Device::Timer { .. } => OpenUi::new::<TimerUi>(client, mob, a, title),
                Device::Prox { .. } => OpenUi::new::<ProximitySensorUi>(client, mob, a, title),
                Device::Health { .. } => OpenUi::new::<HealthSensorUi>(client, mob, a, title),
                Device::Infra { .. } => OpenUi::new::<InfraredEmitterUi>(client, mob, a, title),
                Device::Igniter { .. } | Device::Voice { .. } | Device::Mousetrap { .. } => {
                    return;
                }
            };
            world.write_message(open);
        }
    }
}

// --- Pulses -------------------------------------------------------------------------------

/// tg `pulse()`: the assembly emits a pulse to what it is attached to.
pub fn pulse(world: &mut World, a: Entity) {
    let Some(attached) = world.get::<Assembly>(a).map(|a| a.attached) else {
        return;
    };
    match attached {
        Attached::Wires(holder) => crate::wires::pulse_assembly(world, holder, a),
        Attached::Holder(holder) => process_activation(world, holder, a),
        Attached::Valve(valve) => super::ttv::process_activation(world, valve),
        Attached::None => {}
    }
}

/// `/obj/item/assembly_holder/process_activation`: every other part is pulsed, then the
/// master (a tank bomb) gets the signal.
fn process_activation(world: &mut World, holder: Entity, device: Entity) {
    let parts = parts_of(world, holder);
    if parts.len() >= 2 {
        for part in parts {
            if part != device {
                pulsed(world, part);
            }
        }
    }
    if let Some(master) = world.get::<AssemblyHolder>(holder).and_then(|h| h.master) {
        super::ttv::tank_receive_signal(world, master);
    }
}

/// tg `pulsed()`: another device pulsed this one.
pub fn pulsed(world: &mut World, a: Entity) {
    activate(world, a);
}

/// tg `activate()` of each type: secured, off cooldown (3 s), then what the device does.
pub fn activate(world: &mut World, a: Entity) -> bool {
    let t = now(world);
    let Some(asm) = world.get::<Assembly>(a).cloned() else {
        return false;
    };
    // `/obj/item/assembly/voice/activate`: no cooldown, only alone.
    if let Device::Voice { listening, .. } = asm.device {
        if !asm.secured || asm.attached != Attached::None {
            return false;
        }
        if let Some(mut a) = world.get_mut::<Assembly>(a)
            && let Device::Voice { listening: l, .. } = &mut a.device
        {
            *l = !listening;
        }
        let text = if listening {
            "No longer recording input."
        } else {
            "Now recording input."
        };
        say(world, a, text);
        return true;
    }
    if !asm.secured || t < asm.next_activate {
        return false;
    }
    let mut ignite = None;
    let mut send = false;
    let mut toggle_trap = None;
    if let Some(mut asm) = world.get_mut::<Assembly>(a) {
        asm.next_activate = t + ACTIVATION_COOLDOWN;
        match &mut asm.device {
            Device::Signaler { .. } => send = true,
            Device::Timer { timing, .. } => *timing = !*timing,
            Device::Prox {
                scanning, timing, ..
            } => {
                if *scanning {
                    *scanning = false;
                } else {
                    *timing = !*timing;
                }
            }
            Device::Igniter { condenser } => ignite = Some(*condenser),
            Device::Health { scanning, scan, .. } => {
                *scanning = !*scanning;
                if !*scanning {
                    *scan = None;
                }
            }
            Device::Infra { on, .. } => *on = !*on,
            Device::Mousetrap { armed } => toggle_trap = Some(!*armed),
            Device::Voice { .. } => {}
        }
    }
    if let Some(armed) = toggle_trap {
        set_armed(world, a, armed);
    }
    if send {
        signal(world, a);
    }
    if let Some(condenser) = ignite {
        igniter_activate(world, a, condenser);
        // `SEND_SIGNAL(holder.loc, COMSIG_IGNITER_ACTIVATE)`: a rigged gibtonite ore primes.
        crate::mining::rocks::igniter_activated(world, a);
    }
    true
}

/// `/obj/item/assembly/igniter/activate` (and the condenser's): the tile is exposed to its
/// heat; a condenser also cools the air (`clamp(min(ROOM_TEMP, T * 0.85), 50, 1e6)`).
fn igniter_activate(world: &mut World, a: Entity, condenser: bool) {
    let Some(pos) = crate::explosions::turf_of(world, a) else {
        return;
    };
    let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() else {
        return;
    };
    let heat = if condenser {
        CONDENSER_HEAT
    } else {
        IGNITER_HEAT
    };
    atmos.hotspot_expose(pos, heat, EXPOSED_VOLUME, false);
    if condenser {
        atmos.modify_air(pos, |air| {
            air.temperature =
                (ROOM_TEMP.min(air.temperature * 0.85)).clamp(CONDENSER_HEAT, MAX_FREEZE_TEMP);
        });
    }
}

/// `/obj/item/assembly/signaler/signal` -> `post_signal`: every other signaler on the
/// frequency (within `range` tiles on the same level, when it has one) receives the code.
pub fn signal(world: &mut World, a: Entity) {
    let Some(Device::Signaler {
        frequency,
        code,
        range,
        ..
    }) = world.get::<Assembly>(a).map(|a| a.device.clone())
    else {
        return;
    };
    send_signal(world, a, frequency, code, range);
}

/// `radio_connection.post_signal(source, signal)` on the signaler band: every signaler on
/// `frequency` but `source` (within `range` tiles on the same level, when it is not 0) receives
/// the code. Also sent by the PDA's SignalCommander (`crate::modpc`).
pub fn send_signal(world: &mut World, a: Entity, frequency: u16, code: i32, range: u32) {
    let from = crate::explosions::turf_of(world, a);
    if range > 0 && from.is_none() {
        return;
    }
    info!(
        "signaler {a} at {from:?} sent {}/{code}",
        format_frequency(frequency)
    );
    post_signal(world, a, None, frequency, code, from, range);
}

/// `post_signal` on the signaler band from `source` (an assembly, or the circuit holding radio
/// component `node`): every other signaler and circuit radio on `frequency` receives `code`
/// (within `range` tiles of `from` on the same level, when it has one).
pub fn post_signal(
    world: &mut World,
    source: Entity,
    node: Option<u32>,
    frequency: u16,
    code: i32,
    from: Option<TilePos>,
    range: u32,
) {
    crate::wiremod::server::receive_signal(world, source, node, frequency, code, from, range);
    let receivers: Vec<Entity> = world
        .query::<(Entity, &Assembly)>()
        .iter(world)
        .filter(|&(e, r)| {
            e != source
                && matches!(r.device, Device::Signaler { frequency: f, .. } if f == frequency)
        })
        .map(|(e, _)| e)
        .collect();
    // Anomaly cores listen too (`crate::events::anomalies`).
    let area = (range > 0).then(|| from.map(|f| (f, range))).flatten();
    if range == 0 || area.is_some() {
        crate::events::anomalies::signal_received(world, frequency, code, area);
    }
    // Chem implants synced to a signaler listen too (`crate::implants`).
    crate::implants::chem_signal(world, frequency, code, from, range);
    for r in receivers {
        if range > 0 {
            let (Some(from), Some(to)) = (from, crate::explosions::turf_of(world, r)) else {
                continue;
            };
            if from.chebyshev(to).is_none_or(|d| d > range) {
                continue;
            }
        }
        receive_signal(world, r, code);
    }
}

/// `/obj/item/assembly/signaler/receive_signal`: the same code pulses it and it beeps.
fn receive_signal(world: &mut World, r: Entity, code: i32) {
    if !matches!(
        world.get::<Assembly>(r).map(|a| &a.device),
        Some(Device::Signaler { code: c, .. }) if *c == code
    ) {
        return;
    }
    pulse(world, r);
    if world.get_entity(r).is_ok() {
        beep(world, r, SIGNALER_HEARING);
    }
}

// --- Per tick -----------------------------------------------------------------------------

/// Timers, arming proximity sensors, health sensors and infrared beams, by `seconds_per_tick`.
// Cost: one pass over the active assemblies (a handful) per tick.
fn tick_devices(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs_f64();
    let t = now(world);
    let active: Vec<Entity> = world
        .query_filtered::<Entity, With<Active>>()
        .iter(world)
        .collect();
    for a in active {
        let Some(asm) = world.get::<Assembly>(a).cloned() else {
            continue;
        };
        match asm.device {
            Device::Timer { .. } => timer_tick(world, a, dt, t),
            Device::Prox { .. } => prox_tick(world, a, dt),
            Device::Health { .. } => health_tick(world, a),
            Device::Infra { .. } => {
                let beam = beam_tiles(world, a);
                if let Some(mut asm) = world.get_mut::<Assembly>(a)
                    && let Device::Infra { beam: b, .. } = &mut asm.device
                    && *b != beam
                {
                    *b = beam;
                }
            }
            _ => {}
        }
    }
}

/// `/obj/item/assembly/timer/process` and `timer_end`.
fn timer_tick(world: &mut World, a: Entity, dt: f64, t: f64) {
    let mut ended = None;
    if let Some(mut asm) = world.get_mut::<Assembly>(a) {
        let ready = asm.secured && asm.next_activate <= t;
        if let Device::Timer {
            timing,
            time,
            saved_time,
            looping,
        } = &mut asm.device
        {
            *time -= dt;
            if *time <= 0.0 {
                *timing = *looping;
                *time = *saved_time;
                ended = Some(ready);
            }
        }
    }
    if ended == Some(true) {
        pulse(world, a);
        if world.get_entity(a).is_ok() {
            beep(world, a, DEVICE_HEARING);
        }
    }
}

/// `/obj/item/assembly/prox_sensor/process`: arming counts down, then it scans.
fn prox_tick(world: &mut World, a: Entity, dt: f64) {
    if let Some(mut asm) = world.get_mut::<Assembly>(a)
        && let Device::Prox {
            scanning,
            timing,
            time,
            ..
        } = &mut asm.device
        && *timing
    {
        *time -= dt;
        if *time <= 0.0 {
            *timing = false;
            *scanning = true;
            // tg resets to `initial(time)`, not the time that was set.
            *time = PROX_TIME;
        }
    }
}

/// The mob carrying `a` (tg walks `loc` up to a mob; on wires, from the wires' holder).
fn carrier_mob(world: &World, a: Entity) -> Option<Entity> {
    let mut e = match world.get::<Assembly>(a)?.attached {
        Attached::Wires(h) => h,
        _ => a,
    };
    for _ in 0..8 {
        if world.get::<Health>(e).is_some() {
            return Some(e);
        }
        e = world
            .get::<StoredIn>(e)
            .map(|s| s.0)
            .or_else(|| world.get::<CarriedBy>(e).map(|c| c.0))?;
    }
    None
}

/// `/obj/item/assembly/health/process`: at or below the target it pulses once and stops.
fn health_tick(world: &mut World, a: Entity) {
    let Some(mob) = carrier_mob(world, a) else {
        return;
    };
    let health = world.get::<Health>(mob).map_or(0.0, |h| h.health());
    let mut fire = false;
    if let Some(mut asm) = world.get_mut::<Assembly>(a)
        && let Device::Health {
            scanning,
            target,
            scan,
        } = &mut asm.device
    {
        if *scan != Some(health) {
            *scan = Some(health);
        }
        if health <= *target {
            *scanning = false;
            fire = true;
        }
    }
    if fire {
        pulse(world, a);
        if world.get_entity(a).is_ok() {
            beep(world, a, crate::health::DEFAULT_MESSAGE_RANGE);
        }
    }
}

/// What starts the beam: the holder, the valve or the wired atom, else the emitter; it must
/// be on a turf (`start_loc.loc`).
fn beam_start(world: &World, a: Entity) -> Option<(Entity, TilePos)> {
    let start = match world.get::<Assembly>(a)?.attached {
        Attached::Holder(h) | Attached::Valve(h) | Attached::Wires(h) => h,
        Attached::None => a,
    };
    Some((start, *world.get::<TilePos>(start)?))
}

/// `turf_blocks_beam` for what is on a tile, `start` (and the emitter) excepted.
fn tile_blocks_beam(world: &World, pos: TilePos, ignore: &[Entity]) -> bool {
    let index = world.resource::<SpatialIndex>();
    if !index.in_bounds(pos) {
        return true;
    }
    index
        .dense_on_tile(pos)
        .iter()
        .any(|&e| !ignore.contains(&e) && !crate::inventory::lets_pass(world, e, BEAM_PASS_FLAGS))
}

/// `make_beam`: from the start tile along its dir until something blocks it, at most
/// `max_beam_length` tiles past the start.
fn beam_tiles(world: &World, a: Entity) -> Vec<TilePos> {
    let Some(asm) = world.get::<Assembly>(a) else {
        return Vec::new();
    };
    if !asm.secured || !matches!(asm.device, Device::Infra { on: true, .. }) {
        return Vec::new();
    }
    let Some((start, pos)) = beam_start(world, a) else {
        return Vec::new();
    };
    let dir = world.get::<Facing>(start).map_or(Dir::South, |f| f.0);
    let mut tiles = vec![pos];
    let mut at = pos;
    for _ in 0..BEAM_LENGTH {
        at = at.step(dir);
        if tile_blocks_beam(world, at, &[start, a]) {
            break;
        }
        tiles.push(at);
    }
    tiles
}

/// `HasProximity` of scanning proximity sensors and `beam_entered` of infrared beams, for
/// the steps taken this tick.
// Cost: (steps this tick) x (scanning sensors + beams), both a handful.
fn sense_moves(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moves: Vec<(Entity, TilePos)> = cursor
        .read(world.resource::<Messages<Moved>>())
        .map(|m| (m.entity, m.to))
        .collect();
    if moves.is_empty() {
        return;
    }
    let active: Vec<Entity> = world
        .query_filtered::<Entity, With<Active>>()
        .iter(world)
        .collect();
    for a in active {
        let Some(asm) = world.get::<Assembly>(a).cloned() else {
            continue;
        };
        match asm.device {
            Device::Prox {
                scanning: true,
                sensitivity,
                landmine,
                ..
            } => {
                let host = host_of(world, a);
                // The monitor ignores a host that is not on a turf, unless in a grenade.
                let at = if landmine {
                    crate::explosions::turf_of(world, host)
                } else {
                    world.get::<TilePos>(host).copied()
                };
                let Some(at) = at else {
                    continue;
                };
                let root = root_of(world, a);
                // Neither the sensor's own container nor whoever carries it.
                let carrier = world.get::<CarriedBy>(root).map(|c| c.0);
                let near = moves.iter().any(|&(mover, to)| {
                    mover != root
                        && mover != host
                        && Some(mover) != carrier
                        && to
                            .chebyshev(at)
                            .is_some_and(|d| d <= u32::from(sensitivity))
                });
                if near {
                    sense(world, a);
                }
            }
            Device::Mousetrap { armed: true } => trap_moves(world, a, &moves),
            Device::Infra { beam, .. } if beam.len() > 1 => {
                let start = beam_start(world, a).map(|s| s.0);
                let tripped = moves.iter().any(|&(mover, to)| {
                    beam[1..].contains(&to)
                        && Some(mover) != start
                        && mover != a
                        && world.get::<Density>(mover).is_some()
                        && !crate::inventory::lets_pass(world, mover, BEAM_PASS_FLAGS)
                });
                if tripped {
                    beam_trigger(world, a);
                }
            }
            _ => {}
        }
    }
}

/// `/obj/item/assembly/prox_sensor/proc/sense`.
fn sense(world: &mut World, a: Entity) {
    let t = now(world);
    let Some(mut asm) = world.get_mut::<Assembly>(a) else {
        return;
    };
    if !asm.secured
        || asm.next_activate > t
        || !matches!(asm.device, Device::Prox { scanning: true, .. })
    {
        return;
    }
    asm.next_activate = t + ACTIVATION_COOLDOWN;
    pulse(world, a);
    if world.get_entity(a).is_ok() {
        beep(world, a, DEVICE_HEARING);
    }
}

/// `/obj/item/assembly/infra/proc/beam_trigger`.
fn beam_trigger(world: &mut World, a: Entity) {
    let t = now(world);
    let Some(mut asm) = world.get_mut::<Assembly>(a) else {
        return;
    };
    if t < asm.next_activate {
        return;
    }
    asm.next_activate = t + ACTIVATION_COOLDOWN;
    pulse(world, a);
    if world.get_entity(a).is_ok() {
        beep(world, a, DEVICE_HEARING);
    }
}

/// The mousetrap's state, its icon (`mousetrap[armed]`) and the spring's sound.
fn set_armed(world: &mut World, a: Entity, armed: bool) {
    if let Some(mut asm) = world.get_mut::<Assembly>(a)
        && let Device::Mousetrap { armed: on } = &mut asm.device
    {
        *on = armed;
    }
    let state = if armed { "mousetraparmed" } else { "mousetrap" };
    set_icon_state(world, a, state);
    if let Some(pos) = crate::explosions::turf_of(world, a) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, TRAP_SOUND, 30.0, true).extrarange(-3.0),
        );
    }
}

/// The item shows `state` of its own icon (when the icon has it).
pub(crate) fn set_icon_state(world: &mut World, item: Entity, state: &str) {
    let Some(icon) = world
        .get::<ss13_mapload::DmAtom>(item)
        .map(|a| a.icon.clone())
    else {
        return;
    };
    let Some(id) = world
        .get_resource::<ss13_mapload::IconIndex>()
        .and_then(|i| i.get(&icon, state))
    else {
        return;
    };
    if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(item) {
        look.icon_state = id;
    }
    if let Some(mob) = world.get::<CarriedBy>(item).map(|c| c.0) {
        crate::inventory::refresh_look(world, mob);
    }
}

/// `/obj/item/assembly/mousetrap/on_entered` of its host's turf (only an item host: a holder,
/// a grenade, the trap): a running human steps on it, something dense sets it off.
fn trap_moves(world: &mut World, a: Entity, moves: &[(Entity, TilePos)]) {
    let host = host_of(world, a);
    if world.get::<DmItem>(host).is_none() {
        return;
    }
    let Some(&at) = world.get::<TilePos>(host) else {
        return;
    };
    let victim = moves.iter().find_map(|&(mover, to)| {
        if to != at || mover == host {
            return None;
        }
        if world.get::<Health>(mover).is_some() {
            let human = world.get::<Inventory>(mover).is_some();
            let walking = world
                .get::<crate::janitor::Walking>(mover)
                .is_some_and(|w| w.0);
            return (human && !walking).then_some(Some(mover));
        }
        world.get::<Density>(mover).is_some().then_some(None)
    });
    let Some(victim) = victim else {
        return;
    };
    if let Some(mob) = victim {
        let name = crate::cooking::capitalize(&name_of(world, mob));
        let trap = name_of(world, a);
        around(
            world,
            mob,
            crate::health::DEFAULT_MESSAGE_RANGE,
            Span::Warning,
            &format!("{name} accidentally steps on {trap}."),
        );
    }
    triggered(world, a, victim);
}

/// `/obj/item/assembly/mousetrap/proc/triggered(target, "feet")`: bare feet take 1 brute on a
/// leg and 6 s of paralysis (shoes protect), then the snap and the pulse.
pub fn triggered(world: &mut World, a: Entity, target: Option<Entity>) {
    if !matches!(
        world.get::<Assembly>(a).map(|a| &a.device),
        Some(Device::Mousetrap { armed: true })
    ) {
        return;
    }
    if let Some(mut asm) = world.get_mut::<Assembly>(a)
        && let Device::Mousetrap { armed } = &mut asm.device
    {
        *armed = false;
    }
    set_icon_state(world, a, "mousetrap");
    if let Some(mob) = target
        && let Some(inventory) = world.get::<Inventory>(mob)
    {
        if let Some(shoes) = inventory.shoes {
            let (shoes, trap) = (name_of(world, shoes), name_of(world, a));
            if let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    format!("Your {shoes} protects you from {trap}."),
                );
            }
        } else {
            let left = world
                .resource_mut::<crate::combat::CombatRng>()
                .chance(50.0);
            let zone = if left {
                crate::zone::BodyZone::LLeg
            } else {
                crate::zone::BodyZone::RLeg
            };
            let mut entity = world.entity_mut(mob);
            if let Some(mut stat) = entity.get::<crate::health::Stat>().copied() {
                if let Some(mut health) = entity.get_mut::<Health>() {
                    crate::health::apply_damage_zone(
                        &mut health,
                        &mut stat,
                        zone,
                        crate::health::DamageType::Brute,
                        1.0,
                    );
                }
                entity.insert(stat);
            }
            crate::status_effects::apply(
                world,
                mob,
                crate::status_effects::Incap::Paralyze,
                TRAP_PARALYZE,
            );
        }
    }
    if let Some(pos) = crate::explosions::turf_of(world, a) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, SNAP_SOUND, 50.0, true),
        );
    }
    pulse(world, a);
}

/// The `addtimer`s that came due.
fn run_delayed(world: &mut World) {
    let t = now(world);
    let due: Vec<(Entity, Later)> = world
        .query::<(Entity, &Delayed)>()
        .iter(world)
        .filter(|(_, d)| d.at <= t)
        .map(|(e, d)| (e, d.what))
        .collect();
    for (e, what) in due {
        if world.get_entity(e).is_err() {
            continue;
        }
        world.entity_mut(e).remove::<Delayed>();
        match what {
            Later::Pulse => pulse(world, e),
            Later::Ignite => super::ttv::ignite(world, e),
            Later::ValveReady => super::ttv::valve_ready(world, e),
        }
    }
}

pub(crate) fn later(world: &mut World, e: Entity, secs: f64, what: Later) {
    let at = now(world) + secs;
    world.entity_mut(e).insert(Delayed { at, what });
}

// --- Voice analyzers ----------------------------------------------------------------------

/// `/obj/item/assembly/voice/Hear` for what was said within hearing of a voice analyzer.
// Cost: per spoken line, one pass over the voice analyzers (a handful at most).
fn hear_speech(world: &mut World, mut cursor: Local<MessageCursor<Spoke>>) {
    if !world.contains_resource::<Messages<Spoke>>() {
        return;
    }
    let spoken: Vec<(Entity, TilePos, String, String, bool)> = cursor
        .read(world.resource::<Messages<Spoke>>())
        .map(|s| (s.speaker, s.pos, s.name.clone(), s.text.clone(), s.whisper))
        .collect();
    if spoken.is_empty() {
        return;
    }
    let analyzers: Vec<Entity> = world
        .query::<(Entity, &Assembly)>()
        .iter(world)
        .filter(|(_, a)| matches!(a.device, Device::Voice { .. }))
        .map(|(e, _)| e)
        .collect();
    for (speaker, pos, voice, text, whisper) in spoken {
        if whisper {
            continue;
        }
        for &a in &analyzers {
            let Some(at) = crate::explosions::turf_of(world, a) else {
                continue;
            };
            if at
                .chebyshev(pos)
                .is_none_or(|d| d > crate::health::DEFAULT_MESSAGE_RANGE)
            {
                continue;
            }
            hear(world, a, speaker, &voice, &text);
        }
    }
}

/// What a voice analyzer does with a line: record it while listening, else check it.
fn hear(world: &mut World, a: Entity, _speaker: Entity, voice: &str, text: &str) {
    let Some(Device::Voice {
        listening,
        recorded,
        mode,
    }) = world.get::<Assembly>(a).map(|a| a.device.clone())
    else {
        return;
    };
    if listening {
        let (new, reply) = match mode {
            0 | 1 => (
                Some(text.to_owned()),
                Some(format!("Activation message is '{text}'.")),
            ),
            2 => (
                Some(voice.to_owned()),
                Some("Your voice pattern is saved.".to_owned()),
            ),
            _ => (None, None),
        };
        if let Some(new) = new
            && let Some(mut asm) = world.get_mut::<Assembly>(a)
            && let Device::Voice {
                listening: l,
                recorded: r,
                ..
            } = &mut asm.device
        {
            *r = new;
            *l = false;
        }
        if let Some(reply) = reply {
            say(world, a, &reply);
        }
        if mode == 3 && !text.is_empty() {
            send_pulse(world, a);
        }
        return;
    }
    if voice_matches(mode, &recorded, voice, text) {
        send_pulse(world, a);
    }
}

/// `check_activation`: `findtext` (case-insensitive) in inclusive mode, `==` in exclusive,
/// the speaker's voice in recognizer mode, anything in voice sensor mode.
pub fn voice_matches(mode: usize, recorded: &str, voice: &str, text: &str) -> bool {
    if recorded.is_empty() {
        return false;
    }
    match mode {
        0 => text.to_lowercase().contains(&recorded.to_lowercase()),
        1 => text == recorded,
        2 => voice == recorded,
        _ => !text.is_empty(),
    }
}

/// `send_pulse`: "clicks.", the whir, the pulse 2 s later.
fn send_pulse(world: &mut World, a: Entity) {
    let name = crate::cooking::capitalize(&name_of(world, a));
    around(
        world,
        a,
        crate::health::DEFAULT_MESSAGE_RANGE,
        Span::Plain,
        &format!("{name} clicks."),
    );
    if let Some(pos) = crate::explosions::turf_of(world, a) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, VOICE_SOUND, 30.0, true),
        );
    }
    later(world, a, VOICE_DELAY, Later::Pulse);
}

// --- Windows ------------------------------------------------------------------------------

const DEVICE_WINDOWS: [&str; 5] = [
    SignalerUi::NAME,
    TimerUi::NAME,
    ProximitySensorUi::NAME,
    HealthSensorUi::NAME,
    InfraredEmitterUi::NAME,
];

/// `hands_state` and `ui_status` -> `is_secured`: the device (or what holds it) in the user's
/// hands, secured; the valve's window, the valve in the hands.
fn window_status(world: &mut World) {
    let mut windows = world.query::<(Entity, &UiWindow)>();
    let open: Vec<(Entity, Entity, Entity, bool)> = windows
        .iter(world)
        .filter(|(_, w)| {
            DEVICE_WINDOWS.contains(&w.interface) || w.interface == TransferValveUi::NAME
        })
        .map(|(e, w)| (e, w.source, w.actor, w.interface == TransferValveUi::NAME))
        .collect();
    for (window, source, actor, valve) in open {
        let secured = valve || world.get::<Assembly>(source).is_some_and(|a| a.secured);
        if (!secured || !in_hands_of(world, source, actor))
            && let Some(mut w) = world.get_mut::<UiWindow>(window)
        {
            w.cap_status(UiStatus::Close);
        }
    }
}

fn acts<T: ServerUi>(
    world: &World,
    cursor: &mut MessageCursor<UiActed<T>>,
) -> Vec<(Entity, Entity, Entity, T::Act)>
where
    T::Act: Clone,
{
    cursor
        .read(world.resource::<Messages<UiActed<T>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect()
}

fn device_mut(world: &mut World, a: Entity) -> Option<Mut<'_, Assembly>> {
    world.get_mut::<Assembly>(a)
}

impl ServerUi for SignalerUi {
    type Source = &'static Assembly;
    type UserParam = ();
    type Act = SignalerAct;

    fn static_data(_: &Assembly) {}

    fn data(a: &Assembly, _: Entity, _: &()) -> SignalerData {
        let (frequency, code) = match a.device {
            Device::Signaler {
                frequency, code, ..
            } => (frequency, code),
            _ => (FREQ_SIGNALER, DEFAULT_SIGNALER_CODE),
        };
        SignalerData {
            frequency,
            cooldown: (SIGNALER_COOLDOWN * 10.0) as u32,
            code,
            min_frequency: MIN_FREE_FREQ,
            max_frequency: MAX_FREE_FREQ,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<SignalerAct, UiReject> {
        Ok(match action {
            acts::SIGNAL => SignalerAct::Signal,
            acts::FREQ => SignalerAct::Freq(params.number(acts::FREQ)?),
            acts::CODE => SignalerAct::Code(params.number(acts::CODE)?),
            acts::RESET => SignalerAct::Reset(params.get(acts::RESET) == Some(acts::FREQ)),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SignalerAct {
    Signal,
    /// kHz (`unformat_frequency` makes deci-kHz).
    Freq(f64),
    Code(f64),
    /// True: the frequency, else the code.
    Reset(bool),
}

/// `/obj/item/assembly/signaler/ui_act`.
fn signaler_acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<SignalerUi>>>) {
    for (client, _, a, act) in acts::<SignalerUi>(world, &mut cursor) {
        let t = now(world);
        if act == SignalerAct::Signal {
            let Some(ready) = world
                .get::<Assembly>(a)
                .filter(|s| matches!(s.device, Device::Signaler { .. }))
                .map(|s| s.send_ready)
            else {
                continue;
            };
            if t < ready {
                tell(world, client, Span::Warning, "recharging!");
                continue;
            }
            if let Some(mut asm) = device_mut(world, a) {
                asm.send_ready = t + SIGNALER_COOLDOWN;
            }
            signal(world, a);
            tell(world, client, Span::Notice, "signaled");
            continue;
        }
        let Some(mut asm) = device_mut(world, a) else {
            continue;
        };
        let Device::Signaler {
            frequency, code, ..
        } = &mut asm.device
        else {
            continue;
        };
        match act {
            SignalerAct::Signal => {}
            SignalerAct::Freq(khz) => *frequency = sanitize_frequency(khz * 10.0, true, false),
            // tg takes any number (`round(text2num)`); the window's input is 1-100.
            SignalerAct::Code(c) => *code = (c.round() as i64).clamp(1, 100) as i32,
            SignalerAct::Reset(true) => *frequency = FREQ_SIGNALER,
            SignalerAct::Reset(false) => *code = DEFAULT_SIGNALER_CODE,
        }
    }
}

impl ServerUi for TimerUi {
    type Source = &'static Assembly;
    type UserParam = ();
    type Act = TimerAct;

    fn static_data(_: &Assembly) {}

    fn data(a: &Assembly, _: Entity, _: &()) -> TimerData {
        let Device::Timer {
            timing,
            time,
            looping,
            ..
        } = a.device
        else {
            return TimerData::default();
        };
        let (minutes, seconds) = minutes_seconds(time);
        TimerData {
            minutes,
            seconds,
            timing,
            looping,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<TimerAct, UiReject> {
        Ok(match action {
            acts::TIME => TimerAct::Time,
            acts::REPEAT => TimerAct::Repeat,
            acts::INPUT => TimerAct::Input(params.number(acts::ADJUST)?),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TimerAct {
    Time,
    Repeat,
    /// Seconds to add.
    Input(f64),
}

/// `/obj/item/assembly/timer/ui_act`.
fn timer_acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<TimerUi>>>) {
    for (_, actor, a, act) in acts::<TimerUi>(world, &mut cursor) {
        let Some(mut asm) = device_mut(world, a) else {
            continue;
        };
        let attached = asm.attached;
        let Device::Timer {
            timing,
            time,
            saved_time,
            looping,
        } = &mut asm.device
        else {
            continue;
        };
        match act {
            TimerAct::Time => {
                *timing = !*timing;
                if *timing && matches!(attached, Attached::Valve(_)) {
                    info!("{actor} activated a timer attachment on a transfer valve");
                }
            }
            TimerAct::Repeat => *looping = !*looping,
            TimerAct::Input(adjust) if adjust != 0.0 => {
                *time = (*time + adjust).round().clamp(1.0, MAX_TIME);
                *saved_time = *time;
            }
            TimerAct::Input(_) => {}
        }
    }
}

impl ServerUi for ProximitySensorUi {
    type Source = &'static Assembly;
    type UserParam = ();
    type Act = ProxAct;

    fn static_data(_: &Assembly) {}

    fn data(a: &Assembly, _: Entity, _: &()) -> ProximityData {
        let Device::Prox {
            scanning,
            timing,
            time,
            sensitivity,
            ..
        } = a.device
        else {
            return ProximityData::default();
        };
        let (minutes, seconds) = minutes_seconds(time);
        ProximityData {
            minutes,
            seconds,
            timing,
            scanning,
            sensitivity,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ProxAct, UiReject> {
        Ok(match action {
            acts::SCANNING => ProxAct::Scanning,
            acts::SENSE => ProxAct::Sense(params.number(acts::RANGE)?),
            acts::TIME => ProxAct::Time,
            acts::INPUT => ProxAct::Input(params.number(acts::ADJUST)?),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProxAct {
    Scanning,
    Sense(f64),
    Time,
    Input(f64),
}

/// `/obj/item/assembly/prox_sensor/ui_act`.
fn prox_acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<ProximitySensorUi>>>) {
    for (_, _, a, act) in acts::<ProximitySensorUi>(world, &mut cursor) {
        let Some(mut asm) = device_mut(world, a) else {
            continue;
        };
        let secured = asm.secured;
        let Device::Prox {
            scanning,
            timing,
            time,
            sensitivity,
            ..
        } = &mut asm.device
        else {
            continue;
        };
        match act {
            // `toggle_scan`: only secured.
            ProxAct::Scanning if secured => *scanning = !*scanning,
            ProxAct::Scanning => {}
            ProxAct::Sense(step) if step != 0.0 => {
                *sensitivity =
                    (i64::from(*sensitivity) + step.round() as i64).clamp(0, MAX_SENSITIVITY) as u8;
            }
            ProxAct::Sense(_) => {}
            ProxAct::Time => *timing = !*timing,
            ProxAct::Input(adjust) if adjust != 0.0 => {
                *time = (*time + adjust).round().clamp(0.0, MAX_TIME);
            }
            ProxAct::Input(_) => {}
        }
    }
}

impl ServerUi for HealthSensorUi {
    type Source = &'static Assembly;
    type UserParam = ();
    type Act = HealthAct;

    fn static_data(_: &Assembly) {}

    fn data(a: &Assembly, _: Entity, _: &()) -> HealthSensorData {
        let Device::Health {
            scanning,
            target,
            scan,
        } = a.device
        else {
            return HealthSensorData::default();
        };
        HealthSensorData {
            health: scan,
            scanning,
            target,
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<HealthAct, UiReject> {
        Ok(match action {
            acts::SCANNING => HealthAct::Scanning,
            acts::TARGET => HealthAct::Target,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum HealthAct {
    Scanning,
    Target,
}

/// `/obj/item/assembly/health/ui_act`: `toggle_scan`, `toggle_target`.
fn health_acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<HealthSensorUi>>>) {
    for (_, _, a, act) in acts::<HealthSensorUi>(world, &mut cursor) {
        let Some(mut asm) = device_mut(world, a) else {
            continue;
        };
        let secured = asm.secured;
        let Device::Health {
            scanning, target, ..
        } = &mut asm.device
        else {
            continue;
        };
        match act {
            HealthAct::Scanning if secured => *scanning = !*scanning,
            HealthAct::Scanning => {}
            HealthAct::Target => {
                *target = if *target == HEALTH_THRESHOLD_CRIT {
                    HEALTH_THRESHOLD_DEAD
                } else {
                    HEALTH_THRESHOLD_CRIT
                };
            }
        }
    }
}

impl ServerUi for InfraredEmitterUi {
    type Source = &'static Assembly;
    type UserParam = ();
    type Act = InfraAct;

    fn static_data(_: &Assembly) {}

    fn data(a: &Assembly, _: Entity, _: &()) -> InfraredData {
        let Device::Infra { on, visible, .. } = a.device else {
            return InfraredData::default();
        };
        InfraredData { on, visible }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<InfraAct, UiReject> {
        Ok(match action {
            acts::POWER => InfraAct::Power,
            acts::VISIBILITY => InfraAct::Visibility,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum InfraAct {
    Power,
    Visibility,
}

/// `/obj/item/assembly/infra/ui_act`: `toggle_on`, `toggle_visible`. Turning it on points the
/// beam's start the way the user faces (no `simple_rotation` here, see the module shortcut).
fn infra_acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<InfraredEmitterUi>>>) {
    for (_, actor, a, act) in acts::<InfraredEmitterUi>(world, &mut cursor) {
        let mut turned_on = false;
        {
            let Some(mut asm) = device_mut(world, a) else {
                continue;
            };
            let Device::Infra { on, visible, .. } = &mut asm.device else {
                continue;
            };
            match act {
                InfraAct::Power => {
                    *on = !*on;
                    turned_on = *on;
                }
                InfraAct::Visibility => *visible = !*visible,
            }
        }
        if turned_on && let Some(facing) = world.get::<Facing>(actor).copied() {
            let start = match world.get::<Assembly>(a).map(|a| a.attached) {
                Some(Attached::Holder(h) | Attached::Valve(h)) => h,
                _ => a,
            };
            world.entity_mut(start).insert(facing);
        }
    }
}

// --- Examine ------------------------------------------------------------------------------

/// `/obj/item/assembly/examine` and the types' own lines; the holder's parts.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    super::chem_grenade::examine_lines(world, target, lines);
    super::ttv::examine_lines(world, target, lines);
    crate::modsuit::anomalock::examine_lines(world, target, lines);
    let Some(asm) = world.get::<Assembly>(target) else {
        return;
    };
    let name = crate::cooking::capitalize(&name_of(world, target));
    lines.push((
        Span::Notice,
        if asm.secured {
            format!("{name} is secured and ready to be used!")
        } else {
            format!("{name} can be attached to other things.")
        },
    ));
    let line = match &asm.device {
        Device::Timer { timing, time, .. } => Some(if *timing {
            format!("The timer is counting down from {}.", time.round())
        } else {
            format!("The timer is set for {} seconds.", time.round())
        }),
        Device::Prox {
            scanning, timing, ..
        } => Some(format!(
            "The proximity sensor is {}.",
            if *timing {
                "arming"
            } else if *scanning {
                "armed"
            } else {
                "disarmed"
            }
        )),
        Device::Infra { on, .. } => Some(format!(
            "The infrared trigger is {}.",
            if *on { "on" } else { "off" }
        )),
        Device::Voice { .. } => Some(
            "Use a multitool to swap between \"inclusive\", \"exclusive\", \"recognizer\", and \
             \"voice sensor\" mode."
                .to_owned(),
        ),
        Device::Mousetrap { armed } => Some(format!(
            "The pressure plate is {}.",
            if *armed { "primed" } else { "safe" }
        )),
        Device::Health { scanning, scan, .. } => Some(if *scanning {
            format!(
                "The sensor is on and you can see {} displayed on the screen.",
                scan.map_or_else(String::new, |h| format!("{}", h.round()))
            )
        } else {
            "The sensor is off.".to_owned()
        }),
        _ => None,
    };
    if let Some(line) = line {
        lines.push((Span::Notice, line));
    }
}

/// Whether `a` can go on wires (`assembly_behavior & wire_behavior`, `WIRES_INPUT`).
pub fn attachable_to_wires(world: &World, a: Entity) -> bool {
    world
        .get::<Assembly>(a)
        .is_some_and(|a| a.device.behavior() & WIRES_INPUT != 0)
}

/// `/datum/wires/attach_assembly`: into the wires' holder, `connected`.
pub fn attach_to_wires(world: &mut World, holder: Entity, a: Entity) {
    insert(world, a, holder);
    if let Some(mut asm) = world.get_mut::<Assembly>(a) {
        asm.attached = Attached::Wires(holder);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices_follow_the_type_tree() {
        assert!(matches!(
            device_of("/obj/item/assembly/signaler/low_range"),
            Some(Device::Signaler {
                range: 9,
                locked: false,
                ..
            })
        ));
        assert!(matches!(
            device_of("/obj/item/assembly/signaler/cyborg"),
            Some(Device::Signaler {
                range: 0,
                locked: true,
                ..
            })
        ));
        assert!(matches!(
            device_of("/obj/item/assembly/igniter/condenser"),
            Some(Device::Igniter { condenser: true })
        ));
        assert_eq!(device_of("/obj/item/assembly/flash/handheld"), None);
        assert_eq!(
            device_of("/obj/item/assembly/mousetrap/armed"),
            Some(Device::Mousetrap { armed: true })
        );
        assert_eq!(device_of("/obj/item/assembly/shock_kit"), None);
        assert_eq!(device_of("/obj/item/toy"), None);
    }

    #[test]
    fn behaviours_pick_what_goes_on_wires() {
        let wired = |p: &str| device_of(p).unwrap().behavior() & WIRES_INPUT != 0;
        assert!(wired(SIGNALER));
        assert!(wired(TIMER));
        assert!(wired(PROX));
        assert!(wired(VOICE));
        assert!(wired(HEALTH_SENSOR));
        assert!(!wired(IGNITER));
        assert!(!wired(INFRA));
    }

    #[test]
    fn voice_modes_match_like_tg() {
        assert!(voice_matches(0, "open", "Bob", "Please OPEN the door"));
        assert!(!voice_matches(1, "open", "Bob", "Please open the door"));
        assert!(voice_matches(1, "open", "Bob", "open"));
        assert!(voice_matches(2, "Bob", "Bob", "anything"));
        assert!(!voice_matches(2, "Bob", "Alice", "anything"));
        assert!(voice_matches(3, "x", "Bob", "hi"));
        assert!(!voice_matches(0, "", "Bob", "hi"));
    }

    #[test]
    fn only_secured_active_devices_tick() {
        let timer = Device::Timer {
            timing: true,
            time: 5.0,
            saved_time: 5.0,
            looping: false,
        };
        assert!(timer.active());
        assert!(!device_of(TIMER).unwrap().active());
        assert!(!device_of(SIGNALER).unwrap().active());
    }
}

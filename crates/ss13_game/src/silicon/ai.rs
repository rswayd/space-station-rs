//! The AI (server), after tg `/mob/living/silicon/ai` (`code/modules/mob/living/silicon/ai/`),
//! its eye `/mob/eye/camera/ai` (`freelook/eye.dm`) and `code/_onclick/ai.dm`.
//!
//! - The AI job spawns the AI on the primary `/obj/effect/landmark/start/ai` (tg
//!   `get_latejoin_spawn_point`), else another AI start. The AI mob is its core: dense, never
//!   moves, has the laws and health.
//! - The client controls the eye: one tile per step anywhere on the z-level, through walls
//!   (tg `setLoc` / `abstract_move`); the replication viewer follows it, and the headless
//!   `CameraView` sends which tiles of the view the camera network covers (static elsewhere);
//!   the client is sent only what stands on covered tiles, plus the eye.
//!   The eye is invisible to the living (ghosts see it, as tg's `INVISIBILITY_OBSERVER`); the
//!   AI's client sees what the living see.
//! - Clicks (tg `/mob/living/silicon/ai/ClickOn`): only on what the cameras see, no reach.
//!   Airlocks: shift opens or closes (`AIShiftClick` -> `user_toggle_open`), ctrl drops or
//!   raises the bolts (`toggle_bolt`), alt electrifies or restores (`ai_click_alt`), all only
//!   while `canAIControl` (the AI wire not cut, not all power cut). APCs: ctrl toggles the
//!   breaker, shift the lighting, ctrl+shift the environment, unless `aidisabled`. Shift on
//!   anything else examines. A click on the own core shows the laws; a plain click on a
//!   machine opens its window (`attack_ai`, [`super::interact`]).
//! - It talks with an integrated radio (`:h`/department keys, `:o` its private channel) and in
//!   binary (`:b`). It hears speech and sees messages around its eye only where the cameras see
//!   (tg `/mob/eye/camera/ai/Hear`: `checkCameraVis`). Dead (its core), it speaks in deadchat.
// shortcut: no camera tracking, holopads, multicam, camera acceleration, law upload or malfunction.
// shortcut: the AI's speech is heard around its eye (the client's controlled entity), not
// around the core; the core's own surroundings (tg `viewers()` of the core) are not perceived.
// shortcut: the AI eye's marker sprite is drawn for the AI's own client too.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_chat::server::MaskedSenses;
use ss13_chat::speech::SpeakerStat;
use ss13_mapload::IconIndex;
use ss13_mapload::silicon::AI_TYPE;
use ss13_net::server::{Invisibility, Networked, SeeInvisible, ViewMask};
use ss13_shared::chat::Span;
use ss13_shared::protocol::{ClickModifiers, TICK_DURATION};
use ss13_shared::{Appearance, Dir, Facing, IconStateId, Layer, TilePos};
use ss13_tgui::server::OpenUi;
use ss13_world::{SpatialIndex, WorldSystems};

use super::cameras::{CAMERANET_MASK, CameraNet, CameraViewSource, CameraViewWindow, VIEW_RADIUS};
use super::laws::{Laws, round_default};
use super::ui::CameraViewData;
use super::{Silicon, Silicons, attach_client, give_radio, notice, show_laws};
use crate::click::{ExamineClick, ResolveClicks};
use crate::doors::{Door, DoorState, with_door};
use crate::health::{HealthSystems, Stat, living_bundle};
use crate::hud::{HealthHudData, HealthHudUi};
use crate::jobs::{JobSpawns, Jobs, PlayerJob};
use crate::power::model::{Apc, CHANNEL_OFF, CHANNEL_ON, Channel};
use crate::server::SpawnPoints;
use crate::wires::{Electrified, WIRE_BOLTS, WIRE_SHOCK, Wires};
use ss13_mapload::DoorKind;

/// What the eye's invisibility and the AI client's `see_invisible` are: above the living (0),
/// below ghosts (60). tg's eye is `INVISIBILITY_OBSERVER` and the AI's client sees through its
/// own eye; here the client must see the entity it controls.
pub const AI_EYE_INVISIBILITY: u8 = 25;
/// `/mob/eye/camera/ai` `icon`, `icon_state`.
const EYE_ICON: (&str, &str) = ("icons/mob/eyemob.dmi", "ai_camera");
/// The core's display (tg `display_icon_override` default: the `ai` state).
const CORE_ICON: (&str, &str) = ("icons/mob/silicon/ai.dmi", "ai");
/// tg AI channels (`/obj/item/encryptionkey/ai`, BandaStation adds justice).
const AI_CHANNELS: [&str; 10] = [
    "Командование",
    "Безопасность",
    "Юридический",
    "Инженерия",
    "Исследование",
    "Медицина",
    "Снабжение",
    "Обслуживание",
    "ИИ (личный)",
    "Развлечение",
];
/// tg `AI_WIRE_DISABLED`.
const AI_WIRE_DISABLED: i8 = 1;

/// On the AI mob (its core).
#[derive(Component, Clone, Debug)]
pub struct AiCore {
    pub eye: Entity,
    /// tg `control_disabled`.
    pub control_disabled: bool,
    /// tg `connected_robots`.
    pub connected_robots: Vec<Entity>,
}

/// On the AI's eye, the entity its client controls.
#[derive(Component, Clone, Copy, Debug)]
pub struct AiEye {
    pub ai: Entity,
}

/// One step of an AI eye, already taken from its client's queue (`crate::server`).
#[derive(Message, Clone, Copy, Debug)]
pub struct EyeStep {
    pub eye: Entity,
    pub dir: Dir,
}

/// A click of an AI's client, resolved to an entity it sees (`crate::click`).
#[derive(Message, Clone, Copy, Debug)]
pub struct AiClick {
    pub client: Entity,
    pub eye: Entity,
    pub target: Entity,
    pub target_pos: TilePos,
    pub modifiers: ClickModifiers,
}

pub struct AiPlugin;

impl Plugin for AiPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<EyeStep>()
            .add_message::<AiClick>()
            .add_systems(Update, move_eyes.in_set(WorldSystems::Movement))
            .add_systems(Update, eye_speaker_stat.after(HealthSystems))
            .add_systems(
                Update,
                (
                    ai_clicks.after(ResolveClicks),
                    refresh_eye_views.after(ai_clicks),
                ),
            );
    }
}

/// tg `/datum/job/ai/get_latejoin_spawn_point`: an unused primary AI start, else any AI start.
fn spawn_point(world: &mut World, client: Entity) -> Option<TilePos> {
    if let Some(pos) =
        crate::server::dev_spawn(world, client).or(world.resource::<SpawnPoints>().forced)
    {
        return Some(pos);
    }
    let taken: Vec<TilePos> = {
        let mut cores = world.query_filtered::<&TilePos, With<AiCore>>();
        cores.iter(world).copied().collect()
    };
    let primary = world
        .resource::<JobSpawns>()
        .primary_ai
        .iter()
        .copied()
        .find(|p| !taken.contains(p));
    primary.or_else(|| crate::jobs::job_spawn(world, "AI"))
}

/// The AI job's body: the core on its start, the eye on it, laws, radio. `None` without the
/// AI's prototype (the caller gives a human body).
pub(crate) fn join(
    world: &mut World,
    client: Entity,
    job: usize,
    name: &str,
    latejoin: bool,
) -> Option<Entity> {
    let pos = spawn_point(world, client).unwrap_or_else(|| crate::server::pick_latejoin(world));
    let core = crate::construction::spawn_obj(world, AI_TYPE, pos)?;
    let icon = |dmi: &str, state: &str, world: &World| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(dmi, state))
    };
    if let Some(id) = icon(CORE_ICON.0, CORE_ICON.1, world)
        && let Some(mut appearance) = world.get_mut::<Appearance>(core)
    {
        appearance.icon_state = id;
    }
    let laws = round_default(&world.resource::<Silicons>().0);
    let eye_icon = icon(EYE_ICON.0, EYE_ICON.1, world);
    let eye = world
        .spawn((
            AiEye { ai: core },
            pos,
            Facing(Dir::South),
            Appearance {
                icon_state: eye_icon.unwrap_or(IconStateId(u32::MAX)),
                layer: Layer(4.0),
                plane: crate::ghost::GHOST_PLANE,
                color: None,
                pixel_offset: [0, 0],
                rotation: 0,
                mouse_opacity: ss13_shared::MouseOpacity::Transparent,
                overlays: vec![],
            },
            Invisibility(AI_EYE_INVISIBILITY),
            Networked,
            Name::new(format!("{name} (AI Eye)")),
            HealthHudData {
                health: None,
                move_delay: TICK_DURATION.as_secs_f32(),
                phasing: true,
                ..default()
            },
            CameraViewSource(CameraViewData {
                center: pos,
                radius: VIEW_RADIUS,
                visible: world.resource::<CameraNet>().view_bits(pos, VIEW_RADIUS),
                moves_view: false,
            }),
            SpeakerStat::Conscious,
            MaskedSenses(CAMERANET_MASK),
            ss13_tgui::server::RemoteActor,
        ))
        .id();
    world.entity_mut(core).insert((
        Name::new(name.to_owned()),
        Silicon,
        AiCore {
            eye,
            control_disabled: false,
            connected_robots: Vec::new(),
        },
        laws.clone(),
        living_bundle(),
        PlayerJob(job),
    ));
    world.resource_mut::<Jobs>().add_position(job);
    attach_client(world, client, eye, pos);
    // The mask now, not when `cameras::assign_view_masks` runs: a join later in the tick would
    // replicate the full box around the core once.
    world
        .entity_mut(client)
        .insert((SeeInvisible(AI_EYE_INVISIBILITY), ViewMask(CAMERANET_MASK)));
    give_radio(world, eye, &AI_CHANNELS.map(str::to_owned));
    world.write_message(OpenUi::new::<HealthHudUi>(client, eye, eye, ""));
    world.write_message(OpenUi::new::<CameraViewWindow>(client, eye, eye, ""));
    // tg `/datum/job/ai/after_spawn`: borgs without an AI link to it.
    super::cyborg::link_orphans(world, core);
    show_laws(world, client, &laws);
    notice(
        world,
        client,
        Span::BoldNotice,
        "Для коммуникации с киборгами и другими ИИ используйте префикс :b.",
    );
    info!(
        "{name} ({client}) joined as AI at {pos:?}{}",
        if latejoin { " (latejoin)" } else { "" }
    );
    Some(core)
}

/// The client speaks through the eye with the core's stat (tg: the AI mob says and dies): a dead
/// AI's words go to deadchat, never to the radio or binary.
fn eye_speaker_stat(
    cores: Query<(&AiCore, &Stat), Changed<Stat>>,
    mut eyes: Query<&mut SpeakerStat, With<AiEye>>,
) {
    for (core, &stat) in &cores {
        if let Ok(mut speaker) = eyes.get_mut(core.eye) {
            speaker.set_if_neq(crate::speech::speaker_stat(stat));
        }
    }
}

/// tg `/mob/eye/camera/ai/setLoc` by a step: anywhere on the level, through walls; not while
/// the AI is dead.
fn move_eyes(
    mut steps: MessageReader<EyeStep>,
    index: Res<SpatialIndex>,
    mut eyes: Query<(&AiEye, &TilePos, &mut Facing)>,
    stats: Query<&Stat>,
    mut commands: Commands,
) {
    for &EyeStep { eye, dir } in steps.read() {
        let Ok((ai, &pos, mut facing)) = eyes.get_mut(eye) else {
            continue;
        };
        if stats.get(ai.ai).is_ok_and(|s| *s == Stat::Dead) {
            continue;
        }
        facing.set_if_neq(Facing(dir));
        let to = pos.step(dir);
        if index.in_bounds(to) {
            commands.entity(eye).insert(to);
        }
    }
}

/// The eye's CameraView follows the eye and the network.
fn refresh_eye_views(
    net: Res<CameraNet>,
    mut eyes: Query<(Ref<TilePos>, &mut CameraViewSource), With<AiEye>>,
    mut seen: Local<u64>,
) {
    let changed = std::mem::replace(&mut *seen, net.version) != net.version;
    for (pos, mut view) in &mut eyes {
        if !changed && !pos.is_changed() {
            continue;
        }
        let data = CameraViewData {
            center: *pos,
            radius: VIEW_RADIUS,
            visible: net.view_bits(*pos, VIEW_RADIUS),
            moves_view: false,
        };
        view.set_if_neq(CameraViewSource(data));
    }
}

/// tg `/mob/living/silicon/ai/ClickOn` and the `AI*Click` procs of airlocks and APCs.
fn ai_clicks(world: &mut World, mut clicks: Local<MessageCursor<AiClick>>) {
    let clicks: Vec<AiClick> = clicks
        .read(world.resource::<Messages<AiClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(&AiEye { ai }) = world.get::<AiEye>(click.eye) else {
            continue;
        };
        let Some(core) = world.get::<AiCore>(ai) else {
            continue;
        };
        if core.control_disabled || world.get::<Stat>(ai) != Some(&Stat::Conscious) {
            continue;
        }
        // tg `can_see(A)`: `GLOB.cameranet.checkTurfVis(get_turf_pixel(A))`.
        if !world.resource::<CameraNet>().visible(click.target_pos) {
            continue;
        }
        let m = click.modifiers;
        let door = world
            .get::<Door>(click.target)
            .is_some_and(|d| d.kind == DoorKind::Airlock);
        let apc = world.get::<Apc>(click.target).is_some();
        if m.shift && m.ctrl {
            if apc {
                apc_channel_toggle(world, click.client, click.target, Channel::Environ);
            }
            // shortcut: no airlock emergency access (`toggle_emergency`).
        } else if m.shift && m.middle {
            // shortcut: no pointing for the AI.
        } else if m.shift {
            let handled = if door {
                user_toggle_open(world, click.client, click.target);
                true
            } else if apc {
                apc_channel_toggle(world, click.client, click.target, Channel::Light)
            } else {
                false
            };
            if !handled {
                world.write_message(ExamineClick {
                    client: click.client,
                    examiner: click.eye,
                    target: click.target,
                });
            }
        } else if m.alt {
            if door {
                toggle_shock(world, click.client, click.target);
            }
        } else if m.ctrl {
            if door {
                toggle_bolt(world, click.client, click.target);
            } else if apc {
                toggle_breaker(world, click.client, click.target);
            }
        } else if !m.middle && !m.right && click.target == ai {
            // shortcut: the AI HUD's "Show Laws" button.
            if let Some(laws) = world.get::<Laws>(ai).cloned() {
                show_laws(world, click.client, &laws);
            }
        } else if !m.middle && !m.right {
            // tg `/mob/living/silicon/ai/ClickOn` -> `A.attack_ai(src)`.
            super::interact::attack_ai(world, click.client, click.eye, click.target);
        }
    }
}

pub(super) fn warn(world: &mut World, client: Entity, text: &str) {
    notice(world, client, Span::Warning, text);
}

/// tg airlock `user_allowed` for an AI: `canAIControl()`.
pub(super) fn can_ai_control(world: &World, door: Entity) -> bool {
    let Some(d) = world.get::<Door>(door) else {
        return false;
    };
    let all_cut = world.get::<Wires>(door).is_some_and(Door::all_power_cut);
    d.ai_control_disabled != AI_WIRE_DISABLED && !all_cut
}

/// tg `user_toggle_open`.
pub(super) fn user_toggle_open(world: &mut World, client: Entity, door: Entity) {
    if !can_ai_control(world, door) {
        return;
    }
    let Some(d) = world.get::<Door>(door) else {
        return;
    };
    if d.welded {
        warn(world, client, "The airlock has been welded shut!");
    } else if d.locked {
        warn(world, client, "The door bolts are down!");
    } else {
        with_door(world, door, |d, appearance, ctx| {
            let moved = match d.state {
                DoorState::Open => ctx.close(d, appearance, false),
                DoorState::Closed => ctx.open(d, appearance, false),
                _ => false,
            };
            ((), moved)
        });
    }
}

/// tg `toggle_bolt`.
pub(super) fn toggle_bolt(world: &mut World, client: Entity, door: Entity) {
    if !can_ai_control(world, door) {
        return;
    }
    if world
        .get::<Wires>(door)
        .is_some_and(|w| w.is_cut(WIRE_BOLTS))
    {
        warn(
            world,
            client,
            "The door bolt drop wire is cut - you can't toggle the door bolts.",
        );
        return;
    }
    let Some(d) = world.get::<Door>(door) else {
        return;
    };
    if d.locked {
        let powered = world.get::<crate::power::Unpowered>(door).is_none();
        if !d.has_power(powered) {
            warn(
                world,
                client,
                "The door has no power - you can't raise the door bolts.",
            );
        } else {
            crate::wires::set_bolts(world, door, false);
        }
    } else {
        crate::wires::set_bolts(world, door, true);
    }
}

/// tg `ai_click_alt`: `shock_perm` when not electrified, else `shock_restore`.
fn toggle_shock(world: &mut World, client: Entity, door: Entity) {
    if !can_ai_control(world, door) {
        return;
    }
    let cut = world
        .get::<Wires>(door)
        .is_some_and(|w| w.is_cut(WIRE_SHOCK));
    if world.get::<Electrified>(door).is_none() {
        if cut {
            warn(world, client, "The electrification wire has been cut.");
        } else {
            crate::wires::set_electrified(world, door, Some(Electrified::Permanent));
        }
    } else if cut {
        warn(
            world,
            client,
            "Can't un-electrify the airlock - The electrification wire is cut.",
        );
    } else {
        crate::wires::set_electrified(world, door, None);
    }
}

/// tg APC `can_use(user, loud = TRUE)` for an AI.
pub(crate) fn apc_usable(world: &mut World, client: Entity, apc: Entity) -> bool {
    let usable = world.get::<Apc>(apc).is_some_and(|a| !a.aidisabled);
    if !usable {
        warn(world, client, "Он отключён!");
    }
    usable
}

/// tg `toggle_breaker`.
pub(crate) fn toggle_breaker(world: &mut World, client: Entity, apc: Entity) {
    if !apc_usable(world, client, apc) {
        return;
    }
    let Some(mut a) = world.get_mut::<Apc>(apc) else {
        return;
    };
    // shortcut: `is_operational` (broken, maintenance) is not modelled for APCs.
    if a.failure_timer > 0.0 {
        return;
    }
    a.operating = !a.operating;
    let on = a.operating;
    crate::wires::apc_update(world, apc);
    notice(
        world,
        client,
        Span::Notice,
        format!(
            "Питание {}",
            if on {
                "включёно"
            } else {
                "выключено"
            }
        ),
    );
}

/// tg `AIShiftClick` (lighting) / `AICtrlShiftClick` (environment): `channel = channel ? OFF :
/// ON`.
fn apc_channel_toggle(world: &mut World, client: Entity, apc: Entity, channel: Channel) -> bool {
    if !apc_usable(world, client, apc) {
        return false;
    }
    if world.get::<Apc>(apc).is_none_or(|a| a.failure_timer > 0.0) {
        return false;
    }
    crate::wires::apc_channel(world, apc, channel, |v| {
        if v > CHANNEL_OFF {
            CHANNEL_OFF
        } else {
            CHANNEL_ON
        }
    });
    true
}

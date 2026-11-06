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
//! - Carded (`super::aicard`), the AI mob is inside the card: its eye stays on the card's
//!   turf, it sees everything around it (no camera mask) and clicks within 7 tiles while its
//!   wireless is on (tg `can_see` of a carded AI: `get_dist` within the view). Shunted into an
//!   APC (`super::malf`), it is inside the APC and its eye moves as usual.
//! - A right click on a holopad projects the AI's hologram (`super::holopad`).
// shortcut: no multicam or camera acceleration.
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

/// The AI mob off its own tile (tg `loc` a card or an APC): it has no `TilePos` meanwhile.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiInside {
    Card(Entity),
    Apc(Entity),
}

/// tg `linked_core`: the emptied core a shunted AI returns to (`resolve_core_link`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkedCore(pub Entity);

/// tg `can_see` range of a carded AI (half the 15-tile view).
pub const CARDED_VIEW: u32 = 7;

/// The AI mob leaves its tile into a card or an APC (tg `forceMove`). Not `StoredIn`: deleting
/// the container must not delete the AI with it (its client is on the eye).
pub fn house(world: &mut World, ai: Entity, inside: AiInside) {
    world.entity_mut(ai).remove::<TilePos>().insert(inside);
}

/// The AI mob back on `pos` out of its card or APC.
pub fn unhouse(world: &mut World, ai: Entity, pos: TilePos) {
    world.entity_mut(ai).remove::<AiInside>();
    crate::storage::take_out(world, ai, pos);
    crate::construction::tile_changed(world, pos);
}

/// Whether the AI mob `core` is in an intelliCard.
pub fn carded(world: &World, core: Entity) -> bool {
    matches!(world.get::<AiInside>(core), Some(AiInside::Card(_)))
}

/// On the AI's eye, the entity its client controls.
#[derive(Component, Clone, Copy, Debug)]
pub struct AiEye {
    pub ai: Entity,
    /// The AI lost power: no camera tile is seen ([`super::ai_power`]).
    pub blind: bool,
}

/// The replication mask of a depowered AI: never set, so nothing is seen.
pub const AI_BLIND_MASK: ss13_net::server::MaskId = ss13_net::server::MaskId(u64::MAX - 1);

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
            .add_systems(
                Update,
                pin_carded_eyes
                    .run_if(any_with_component::<AiInside>)
                    .after(WorldSystems::Movement),
            )
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
    let core = spawn_ai(world, client, pos, name, Some(job))?;
    info!(
        "{name} ({client}) joined as AI at {pos:?}{}",
        if latejoin { " (latejoin)" } else { "" }
    );
    Some(core)
}

/// A new AI mob on `pos` controlled by `client` (tg `new /mob/living/silicon/ai(loc)` with the
/// mind moved in): the core, the eye, laws, radio, HUD. `job`: the AI job's position it takes.
/// `None` without the AI's prototype.
pub(crate) fn spawn_ai(
    world: &mut World,
    client: Entity,
    pos: TilePos,
    name: &str,
    job: Option<usize>,
) -> Option<Entity> {
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
            AiEye {
                ai: core,
                blind: false,
            },
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
            super::alarms::AlertListener::silicon(true),
            super::alarms::AlarmQueue::default(),
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
        super::ai_power::AiPower::default(),
        super::ai_hud::CoreDisplay::default(),
    ));
    if let Some(job) = job {
        world.entity_mut(core).insert(PlayerJob(job));
        world.resource_mut::<Jobs>().add_position(job);
    }
    // tg `make_laws` + `link_to_first_rack`: the core rack's lawset. Without a core rack (test
    // maps) the round's default lawset stays, where tg would leave the AI lawless.
    super::law_rack::link_to_first_rack(world, core);
    let laws = world.get::<Laws>(core).cloned().unwrap_or(laws);
    attach_client(world, client, eye, pos);
    // The mask now, not when `cameras::assign_view_masks` runs: a join later in the tick would
    // replicate the full box around the core once.
    world
        .entity_mut(client)
        .insert((SeeInvisible(AI_EYE_INVISIBILITY), ViewMask(CAMERANET_MASK)));
    give_radio(world, eye, &AI_CHANNELS.map(str::to_owned));
    super::ai_hud::give_ai_buttons(world, eye);
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
    Some(core)
}

/// The client speaks through the eye with the core's stat (tg: the AI mob says and dies): a dead
/// AI's words go to deadchat, never to the radio or binary. Dying puts the eye back on the core
/// (tg `death`: `eyeobj.setLoc(get_turf(src))`).
fn eye_speaker_stat(
    cores: Query<(&AiCore, &Stat, &TilePos), Changed<Stat>>,
    mut eyes: Query<(&mut SpeakerStat, &TilePos), (With<AiEye>, Without<AiCore>)>,
    mut commands: Commands,
) {
    for (core, &stat, &pos) in &cores {
        if let Ok((mut speaker, &eye_pos)) = eyes.get_mut(core.eye) {
            speaker.set_if_neq(crate::speech::speaker_stat(stat));
            if stat == Stat::Dead && eye_pos != pos {
                commands.entity(core.eye).insert(pos);
            }
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
    inside: Query<&AiInside>,
    mut commands: Commands,
) {
    for &EyeStep { eye, dir } in steps.read() {
        let Ok((ai, &pos, mut facing)) = eyes.get_mut(eye) else {
            continue;
        };
        if stats.get(ai.ai).is_ok_and(|s| *s == Stat::Dead)
            || matches!(inside.get(ai.ai), Ok(AiInside::Card(_)))
        {
            continue;
        }
        facing.set_if_neq(Facing(dir));
        let to = pos.step(dir);
        if index.in_bounds(to) {
            commands.entity(eye).insert(to);
        }
    }
}

/// A carded AI's eye stays on the card's turf (tg `eyeobj.setLoc(loc)` of a carded AI).
fn pin_carded_eyes(world: &mut World) {
    let mut q = world.query::<(&AiCore, &AiInside)>();
    let cards: Vec<(Entity, Entity)> = q
        .iter(world)
        .filter_map(|(core, inside)| match *inside {
            AiInside::Card(card) => Some((core.eye, card)),
            AiInside::Apc(_) => None,
        })
        .collect();
    for (eye, card) in cards {
        let Some(at) = crate::explosions::turf_of(world, card) else {
            continue;
        };
        if world.get::<TilePos>(eye) != Some(&at) {
            world.entity_mut(eye).insert(at);
        }
    }
}

/// The eye's CameraView follows the eye and the network; a blind eye sees no tile, a carded
/// AI's every tile.
fn refresh_eye_views(
    net: Res<CameraNet>,
    mut eyes: Query<(Ref<TilePos>, Ref<AiEye>, &mut CameraViewSource)>,
    inside: Query<&AiInside>,
    mut seen: Local<u64>,
) {
    let changed = std::mem::replace(&mut *seen, net.version) != net.version;
    for (pos, eye, mut view) in &mut eyes {
        if !changed && !pos.is_changed() && !eye.is_changed() {
            continue;
        }
        let visible = if eye.blind {
            super::cameras::box_bits(*pos, VIEW_RADIUS, |_| false)
        } else if matches!(inside.get(eye.ai), Ok(AiInside::Card(_))) {
            super::cameras::box_bits(*pos, VIEW_RADIUS, |_| true)
        } else {
            net.view_bits(*pos, VIEW_RADIUS)
        };
        let data = CameraViewData {
            center: *pos,
            radius: VIEW_RADIUS,
            visible,
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
        let Some(&AiEye { ai, .. }) = world.get::<AiEye>(click.eye) else {
            continue;
        };
        let Some(core) = world.get::<AiCore>(ai) else {
            continue;
        };
        if core.control_disabled
            || world.get::<Stat>(ai) != Some(&Stat::Conscious)
            || super::ai_power::depowered(world, ai)
        {
            continue;
        }
        // tg `can_see(A)`: `GLOB.cameranet.checkTurfVis(get_turf_pixel(A))`; carded, the
        // distance from the card.
        let sees = if carded(world, ai) {
            world
                .get::<TilePos>(click.eye)
                .and_then(|p| p.chebyshev(click.target_pos))
                .is_some_and(|d| d <= CARDED_VIEW)
        } else {
            world.resource::<CameraNet>().visible(click.target_pos)
        };
        if !sees {
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
            } else {
                super::holopad::ai_ctrl_click(world, click.eye, click.target);
            }
        } else if !m.middle && !m.right && click.target == ai {
            // shortcut: the AI HUD's "Show Laws" button.
            if let Some(laws) = world.get::<Laws>(ai).cloned() {
                show_laws(world, click.client, &laws);
            }
        } else if !m.middle && !m.right {
            // tg `/mob/living/silicon/ai/ClickOn` -> `A.attack_ai(src)`.
            super::interact::attack_ai(world, click.client, click.eye, click.target);
        } else if m.right && !m.middle {
            // tg `A.attack_ai_secondary(src)`.
            super::holopad::attack_ai_secondary(world, click.client, click.eye, click.target);
        }
    }
}

pub(super) fn warn(world: &mut World, client: Entity, text: &str) {
    notice(world, client, Span::Warning, text);
}

/// tg airlock `user_allowed` for an AI: `canAIControl()`.
pub(crate) fn can_ai_control(world: &World, door: Entity) -> bool {
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

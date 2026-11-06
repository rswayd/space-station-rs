//! Holopads and the AI's hologram (server), after tg `/obj/machinery/holopad`
//! (`code/game/machinery/hologram.dm`), `/mob/eye/camera/ai/setLoc` and
//! `/mob/living/silicon/ai/proc/holopad_talk`.
//!
//! - Mapped holopads on the network (not the tutorial pad) are [`Holopad`]s.
//! - The AI's right click on a holopad its cameras see (`attack_ai_secondary`) projects its
//!   hologram there (`activate_holo`): the default AI hologram (`ai.dmi` `default`, tinted and
//!   half transparent as `makeHologram`), named "<AI> (Hologram)", its eye moved onto the pad;
//!   the pad shows `holopad1`. Unpowered: "ERROR: Unable to project hologram."; one hologram per
//!   AI ("Image feed in progress."). Right click again, or ctrl-click (`AICtrlClick`), removes
//!   it (`clear_holo`).
//! - The hologram follows the eye (`move_hologram`) while the eye stays within the pad's
//!   `holo_range` (4 + its capacitors' tiers) in the pad's area (`validate_location`); outside,
//!   another pad that covers the tile takes it (`transfer_to_nearby_pad`), else it is removed.
//!   It is removed when the pad loses power or goes, or the AI is no longer conscious, powered,
//!   in its core or played (`process` -> `validate_user`).
//! - What the AI says aloud (not on a radio) the hologram says to those around it (tg
//!   `holopad_talk`); while it projects, the AI hears and sees around its hologram without the
//!   cameras (tg `holopad/Hear` relays what the pad hears).
// shortcut: tg's AI talks through the hologram with `:h`; here `:h` is the shared parser's
// department radio and the AI's plain speech is already said at its eye, so the hologram says
// the plain speech.
// shortcut: the AI perceives everything around its hologram, tg relays only speech the pad
// hears; no holoray, pad light, hologram choice from the preferences or holocolor, no holocalls,
// AI presence requests or holodisks (see ../comms/station-info.md).
// shortcut: holopads built mid-round are not on the network until a map init sees them.

use bevy::prelude::*;
use ss13_chat::server::{
    HEARING_RANGE, Listeners, MaskedSenses, SaySystems, Spoke, Understanding, say_message, tagged,
};
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{Areas, DmObj, IconIndex};
use ss13_net::chat::server::SendChat;
use ss13_net::server::{Networked, SeeInvisible};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, Facing, IconStateId, Layer, MouseOpacity, TilePos};
use ss13_world::SpatialIndex;

use super::ai::{AI_EYE_INVISIBILITY, AiCore, AiEye, carded};
use super::cameras::CAMERANET_MASK;
use crate::health::Stat;
use crate::power::Unpowered;
use crate::server::PlayerMob;
use crate::stock_parts::PartKind;
use crate::tools::tell;

const HOLOPAD_ROOT: &str = "/obj/machinery/holopad";
/// `/obj/machinery/holopad/tutorial`: `on_network = FALSE`.
const TUTORIAL_PAD: &str = "/obj/machinery/holopad/tutorial";
/// `GLOB.ai_hologram_icons[AI_HOLOGRAM_DEFAULT]`, `ai_hologram_icon_state`.
const HOLOGRAM_ICON: (&str, &str) = ("icons/mob/silicon/ai.dmi", "default");
/// `makeHologram()`: `rgb(125, 180, 225, 0.5 * 255)`.
const HOLOGRAM_COLOR: [u8; 4] = [125, 180, 225, 127];
/// `FLY_LAYER`, `ABOVE_GAME_PLANE`.
const HOLOGRAM_LAYER: f32 = 5.0;
const HOLOGRAM_PLANE: i16 = -3;
/// `RefreshParts`: `holograph_range = 4` plus the capacitors' tiers.
const BASE_RANGE: u32 = 4;

/// A holopad on the network (tg `holopads`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Holopad;

/// On the AI mob: its hologram over `pad` (tg `current`, the pad's `masters[ai]`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HoloProjection {
    pub pad: Entity,
    pub hologram: Entity,
}

/// On a hologram (tg `/obj/effect/overlay/holo_pad_hologram`): the AI it shows.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hologram {
    pub ai: Entity,
}

pub struct HolopadPlugin;

impl Plugin for HolopadPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(ss13_world::MapInitLate, init_holopads)
            .add_systems(
                Update,
                (
                    keep_holograms.run_if(any_with_component::<Hologram>),
                    hologram_speech.after(SaySystems),
                ),
            );
    }
}

fn init_holopads(world: &mut World) {
    let mut q =
        world.query_filtered::<(Entity, &DmObj), (Without<Holopad>, With<ss13_world::Mapped>)>();
    let pads: Vec<Entity> = q
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, HOLOPAD_ROOT) && !is_under(&o.path, TUTORIAL_PAD))
        .map(|(e, _)| e)
        .collect();
    for pad in pads {
        world.entity_mut(pad).insert(Holopad);
    }
}

/// tg `holo_range`.
pub fn holo_range(world: &World, pad: Entity) -> u32 {
    BASE_RANGE + crate::stock_parts::ratings(world, pad).sum(PartKind::Capacitor)
}

/// tg `validate_location`: the pad's level and area, within its range.
pub fn validate_location(world: &World, pad: Entity, at: TilePos) -> bool {
    let Some(&pad_pos) = world.get::<TilePos>(pad) else {
        return false;
    };
    let in_range = pad_pos
        .chebyshev(at)
        .is_some_and(|d| d <= holo_range(world, pad));
    let areas = world.get_resource::<Areas>();
    let area = |p: TilePos| areas.and_then(|a| a.area_at(p));
    in_range && area(at) == area(pad_pos)
}

/// tg `/obj/machinery/holopad/attack_ai_secondary`: the AI's right click. True when `target` is
/// a holopad.
pub fn attack_ai_secondary(world: &mut World, client: Entity, eye: Entity, target: Entity) -> bool {
    if world.get::<Holopad>(target).is_none() {
        return false;
    }
    let Some(ai) = world.get::<AiEye>(eye).map(|e| e.ai) else {
        return true;
    };
    if world
        .get::<HoloProjection>(ai)
        .is_some_and(|p| p.pad == target)
    {
        clear_holo(world, ai);
    } else {
        activate_holo(world, client, ai, target);
    }
    true
}

/// tg `/obj/machinery/holopad/AICtrlClick`: removes the AI's hologram over this pad. True when
/// `target` is a holopad.
pub fn ai_ctrl_click(world: &mut World, eye: Entity, target: Entity) -> bool {
    if world.get::<Holopad>(target).is_none() {
        return false;
    }
    if let Some(ai) = world.get::<AiEye>(eye).map(|e| e.ai)
        && world
            .get::<HoloProjection>(ai)
            .is_some_and(|p| p.pad == target)
    {
        clear_holo(world, ai);
    }
    true
}

/// tg `activate_holo` for an AI.
pub fn activate_holo(world: &mut World, client: Entity, ai: Entity, pad: Entity) {
    if world.get::<Unpowered>(pad).is_some() {
        tell(
            world,
            client,
            Span::Danger,
            "ERROR: Unable to project hologram.",
        );
        return;
    }
    if world.get::<HoloProjection>(ai).is_some() {
        tell(
            world,
            client,
            Span::Danger,
            "ERROR: Image feed in progress.",
        );
        return;
    }
    let (Some(&pos), Some(eye)) = (
        world.get::<TilePos>(pad),
        world.get::<AiCore>(ai).map(|c| c.eye),
    ) else {
        return;
    };
    let name = world
        .get::<Name>(ai)
        .map_or_else(String::new, |n| n.as_str().to_owned());
    let icon = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(HOLOGRAM_ICON.0, HOLOGRAM_ICON.1));
    let hologram = world
        .spawn((
            Hologram { ai },
            pos,
            Facing(Dir::South),
            Appearance {
                icon_state: icon.unwrap_or(IconStateId(u32::MAX)),
                layer: Layer(HOLOGRAM_LAYER),
                plane: HOLOGRAM_PLANE,
                color: Some(HOLOGRAM_COLOR),
                pixel_offset: [0, 0],
                rotation: 0,
                mouse_opacity: MouseOpacity::Transparent,
                overlays: vec![],
            },
            Networked,
            Name::new(format!("{name} (Hologram)")),
        ))
        .id();
    world
        .entity_mut(ai)
        .insert(HoloProjection { pad, hologram });
    // `AI.eyeobj.setLoc(get_turf(src))`; the AI hears around the pad (`holopad/Hear`).
    world.entity_mut(eye).insert(pos).remove::<MaskedSenses>();
    pad_look(world, pad);
    let text = format!("A holographic image of {name} flickers to life before your eyes!");
    let clients = crate::obj_damage::clients_in_range(world, pos, HEARING_RANGE, &[]);
    if !clients.is_empty() {
        world.write_message(SendChat {
            clients,
            message: ss13_shared::chat::ChatMessage::simple(
                ss13_shared::chat::ChatChannel::Local,
                Span::Notice,
                text,
            ),
        });
    }
}

/// tg `clear_holo` + `unset_holo`.
pub fn clear_holo(world: &mut World, ai: Entity) {
    let Some(projection) = world.get::<HoloProjection>(ai).copied() else {
        return;
    };
    world.entity_mut(ai).remove::<HoloProjection>();
    world.try_despawn(projection.hologram).ok();
    // A carded AI's eye perceives everything around the card (`super::aicard`).
    if let Some(eye) = world.get::<AiCore>(ai).map(|c| c.eye)
        && !carded(world, ai)
        && let Ok(mut e) = world.get_entity_mut(eye)
    {
        e.insert(MaskedSenses(CAMERANET_MASK));
    }
    pad_look(world, projection.pad);
}

/// tg `update_icon_state`: `holopad1` while a hologram is over the pad.
fn pad_look(world: &mut World, pad: Entity) {
    if world.get_entity(pad).is_err() {
        return;
    }
    let mut q = world.query::<&HoloProjection>();
    let used = q.iter(world).any(|p| p.pad == pad);
    let state = if used { "holopad1" } else { "holopad0" };
    crate::obj_damage::set_icon_state(world, pad, state);
}

/// tg `validate_user` and the pad's `is_operational`.
fn still_valid(world: &World, ai: Entity, pad: Entity) -> bool {
    let Some(core) = world.get::<AiCore>(ai) else {
        return false;
    };
    world.get::<Holopad>(pad).is_some()
        && world.get::<Unpowered>(pad).is_none()
        && world.get::<Stat>(ai) == Some(&Stat::Conscious)
        && !super::ai_power::depowered(world, ai)
        && world.get::<super::ai::AiInside>(ai).is_none()
        && world.get::<PlayerMob>(core.eye).is_some()
}

/// tg `holopad/process` (removes invalid holograms) and `move_hologram` after every eye move;
/// a hologram whose AI is gone goes too. Runs only while a hologram exists.
fn keep_holograms(world: &mut World) {
    let mut q = world.query::<(Entity, &Hologram)>();
    let holograms: Vec<(Entity, Entity)> = q.iter(world).map(|(e, h)| (e, h.ai)).collect();
    for (hologram, ai) in holograms {
        let Some(projection) = world
            .get::<HoloProjection>(ai)
            .copied()
            .filter(|p| p.hologram == hologram)
        else {
            world.try_despawn(hologram).ok();
            continue;
        };
        if !still_valid(world, ai, projection.pad) {
            clear_holo(world, ai);
            continue;
        }
        let Some(eye) = world.get::<AiCore>(ai).map(|c| c.eye) else {
            continue;
        };
        let (Some(&at), Some(&shown)) = (world.get::<TilePos>(eye), world.get::<TilePos>(hologram))
        else {
            continue;
        };
        if at == shown {
            continue;
        }
        if !validate_location(world, projection.pad, at) {
            // `transfer_to_nearby_pad`: holograms can't stray outside of a pad's range.
            let mut pads = world.query_filtered::<Entity, With<Holopad>>();
            let w: &World = world;
            let other = pads
                .iter(w)
                .filter(|&p| p != projection.pad && w.get::<Unpowered>(p).is_none())
                .find(|&p| validate_location(w, p, at));
            let Some(other) = other else {
                clear_holo(world, ai);
                continue;
            };
            world.entity_mut(ai).insert(HoloProjection {
                pad: other,
                hologram,
            });
            pad_look(world, projection.pad);
            pad_look(world, other);
        }
        world.entity_mut(hologram).insert(at);
    }
}

/// tg `holopad_talk`: the hologram says what its AI said aloud, to those who did not hear the
/// (invisible) eye itself.
fn hologram_speech(
    mut spoken: MessageReader<Spoke>,
    eyes: Query<&AiEye>,
    projections: Query<&HoloProjection>,
    holograms: Query<(&TilePos, &Name), With<Hologram>>,
    index: Res<SpatialIndex>,
    listeners: Res<Listeners>,
    sight: Query<&SeeInvisible>,
    understanding: Understanding,
    mut chat: MessageWriter<SendChat>,
) {
    for spoke in spoken.read() {
        if spoke.radio.is_some() || spoke.whisper {
            continue;
        }
        let Some((&pos, name)) = eyes
            .get(spoke.speaker)
            .ok()
            .and_then(|eye| projections.get(eye.ai).ok())
            .and_then(|p| holograms.get(p.hologram).ok())
        else {
            continue;
        };
        let clients: Vec<Entity> = index
            .in_radius(pos, HEARING_RANGE)
            .filter_map(|e| listeners.client_of(e))
            .filter(|&c| c != spoke.client)
            // Those who see the eye heard it already.
            .filter(|&c| sight.get(c).ok().is_none_or(|s| s.0 < AI_EYE_INVISIBILITY))
            .collect();
        for heard in understanding.split(clients, spoke.language, &spoke.text) {
            chat.write(SendChat {
                clients: heard.clients,
                message: tagged(say_message(name.as_str(), &heard.text), heard.tag),
            });
        }
    }
}

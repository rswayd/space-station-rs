//! The pilot's side of an exosuit (server), after tg `mecha_ui.dm`, `mecha_actions.dm`,
//! `toggle_lights` and `process_constant_power_usage` (`_mecha.dm`), `mecha_diagnostic.dm` and the
//! Vim's noise actions (`vehicle_actions.dm`).
//!
//! - Action buttons while piloting: "View Stats" opens the `Mecha` window, "Toggle Lights" on a
//!   mech with `HAS_LIGHTS`, the Vim's "Chime!" and "Buzz." (a sound and a message, 1 s
//!   cooldown).
//! - The `Mecha` window (pilot only, closes on leaving): name, integrity, charge, the
//!   safety, ID lock and lights toggles, the equipment with the sleeper's and the syringe gun's
//!   controls (`equip_act`, [`super::medical`]), and the "Diagnostic" button: once per mech a
//!   holodiagnostic sheet (`/obj/item/mecha_diagnostic`, "mecha holodiagnostic (<mech>)")
//!   appears on its tile; its examine names the mech's type.
//! - Lights: on/off with the cell holding a 2 s drain; while on they drain `0.002 ×
//!   STANDARD_CELL_RATE` W over the capacitor rating each second and go out when the cell cannot
//!   pay (`process_constant_power_usage`). The Vim shows its `vim_headlights` overlay.
//!
//! Cost: views are rebuilt only for mechs with an open window, once per tick; the light drain is
//! one pass over lit mechs.
// shortcut: the window has no camera view, rename, DNA lock, access editor, cabin, internal
// damage, overclock or module selection; the other action buttons (eject, cabin seal, safeties,
// strafe) stay on B, middle click and alt-click; the light shines all around instead of
// `OVERLAY_LIGHT_DIRECTIONAL`'s cone; the cargo bounty for holodiagnostics is the cargo group's.
// shortcut: the cabin is not simulated; the control console reads an enclosed mech's cabin at
// one atmosphere (tg's initial `cabin_air`).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::light::{DmLight, source};
use ss13_mapload::{DmName, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Facing, LightSource, Optics, Overlay, TilePos};
use ss13_tgui::UiStatus;
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiKind, UiParams, UiReject, UiSystems, UiWindow,
};

use super::core::{
    HAS_LIGHTS, ID_LOCK_ON, ItemCell, LIGHTS_ON, Mech, MechaDb, Pilot, charge, name_of, use_energy,
    visible,
};
use super::{
    DIAGNOSTIC, EQUIP_ACT, GEAR_ACTION, MechaData, MechaGear, MechaStatic, MechaUi, REF,
    TOGGLE_ID_LOCK, TOGGLE_LIGHTS, TOGGLE_SAFETY,
};
use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::health::Stat;
use crate::power::chargers::STANDARD_CELL_RATE;
use crate::server::PlayerMob;
use crate::sound::{PlaySound, playsound};
use crate::tools::{client_of, tell};

pub const DIAGNOSTIC_ITEM: &str = "/obj/item/mecha_diagnostic";
const ACTIONS_MECHA_DMI: &str = "icons/mob/actions/actions_mecha.dmi";
const ACTIONS_VEHICLE_DMI: &str = "icons/mob/actions/actions_vehicle.dmi";
const BACKGROUNDS_DMI: &str = "icons/mob/actions/backgrounds.dmi";
/// The ids of the buttons granted here.
const BUTTON: &str = "mecha:";
const VIEW_STATS: &str = "mecha:view_stats";
const LIGHTS: &str = "mecha:toggle_lights";
const CHIME: &str = "mecha:vim_chime";
const BUZZ: &str = "mecha:vim_buzz";
/// `VIM_SOUND_COOLDOWN`.
const VIM_SOUND_COOLDOWN_SECS: f64 = 1.0;
const CHIME_SOUND: &str = "sound/machines/chime.ogg";
const BUZZ_SOUND: &str = "sound/machines/buzz/buzz-sigh.ogg";
const LIGHTS_SOUND: &str = "sound/machines/clockcult/brass_skewer.ogg";
const SAFETY_SOUND: &str = "sound/machines/beep/beep.ogg";
/// `light_power_drain` (`0.002 * STANDARD_CELL_RATE`), watts before the capacitor, and the
/// `SSobj` wait the switch-on check pays for (`power_to_energy(.., scheduler = SSobj)`).
const LIGHT_POWER_DRAIN: f64 = 0.002 * STANDARD_CELL_RATE;
const SSOBJ_WAIT_SECS: f64 = 2.0;
/// `ONE_ATMOSPHERE`.
pub const ONE_ATMOSPHERE: f32 = 101.325;

/// The `Mecha` window's data, rebuilt while a window is open.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct MechView {
    pub statics: MechaStatic,
    pub data: MechaData,
}

/// On a holodiagnostic sheet: `initial(mech_data.name)`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct MechDiagnostic {
    pub mech_name: String,
}

/// The light a lit mech put into its [`Optics`].
#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct MechLight(LightSource);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MechaAct {
    Diagnostic,
    ToggleSafety,
    ToggleIdLock,
    ToggleLights,
    /// `equip_act`: the equipment's bits and its `gear_action`.
    Equip(u64, String),
}

pub struct MechPanelPlugin;

impl Plugin for MechPanelPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<MechaUi>().add_systems(
            Update,
            (
                window_status.in_set(UiSystems::StatusHook),
                (panel_acts, trigger_buttons).in_set(UiSystems::Handle),
                refresh_views
                    .before(UiSystems::Send)
                    .after(UiSystems::Handle),
                (light_drain, light_facing),
            ),
        );
    }
}

impl ServerUi for MechaUi {
    type Source = &'static MechView;
    type UserParam = ();
    type Act = MechaAct;

    fn static_data(view: &MechView) -> MechaStatic {
        view.statics.clone()
    }

    fn data(view: &MechView, _: Entity, _: &()) -> MechaData {
        view.data.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<MechaAct, UiReject> {
        match action {
            DIAGNOSTIC => Ok(MechaAct::Diagnostic),
            TOGGLE_SAFETY => Ok(MechaAct::ToggleSafety),
            TOGGLE_ID_LOCK => Ok(MechaAct::ToggleIdLock),
            TOGGLE_LIGHTS => Ok(MechaAct::ToggleLights),
            EQUIP_ACT => {
                let reference = params
                    .get(REF)
                    .and_then(|r| r.trim().parse::<u64>().ok())
                    .ok_or_else(|| UiReject::BadParams("ref: expected an entity".into()))?;
                Ok(MechaAct::Equip(reference, params.text(GEAR_ACTION)?))
            }
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// `"Left arm"`, `"Right arm"` or the module's category (`MECHA_UTILITY`...).
fn slot_label(world: &World, m: &Mech, equip: Entity) -> String {
    if m.l_arm == Some(equip) {
        return "Left arm".to_owned();
    }
    if m.r_arm == Some(equip) {
        return "Right arm".to_owned();
    }
    super::equipment::def(world, equip)
        .map(|d| d.slot)
        .unwrap_or_default()
}

/// `ui_data`/`ui_static_data` of the mech.
pub fn compute_view(world: &World, mech: Entity) -> Option<MechView> {
    let m = world.get::<Mech>(mech)?;
    let def = &world.resource::<MechaDb>().data.mechs[m.def];
    let cell = m.cell.and_then(|c| world.get::<ItemCell>(c)).map(|c| c.0);
    let equipment = m
        .equipment()
        .map(|e| {
            let mut gear = MechaGear {
                name: crate::obj_damage::strip(&name_of(world, e)).to_owned(),
                slot: slot_label(world, m, e),
                reference: e.to_bits(),
                sleeper: None,
                syringe: None,
            };
            super::medical::gear_data(world, mech, e, &mut gear);
            gear
        })
        .collect();
    Some(MechView {
        // shortcut: no window themes (`ui_theme`, the Vim's cat theme).
        statics: MechaStatic::default(),
        data: MechaData {
            name: crate::obj_damage::strip(&name_of(world, mech)).to_owned(),
            integrity: m.integrity,
            integrity_max: def.max_integrity,
            power_level: cell.map(|c| c.charge),
            power_max: cell.map(|c| c.maxcharge),
            weapons_safety: m.weapons_safety,
            id_lock: m.flags & ID_LOCK_ON != 0,
            has_lights: m.flags & HAS_LIGHTS != 0,
            lights_on: m.flags & LIGHTS_ON != 0,
            diagnostic_status: m.diagnostic_created,
            equipment,
        },
    })
}

fn refresh_view(world: &mut World, mech: Entity) {
    let Some(view) = compute_view(world, mech) else {
        return;
    };
    match world.get_mut::<MechView>(mech) {
        Some(mut old) => {
            old.set_if_neq(view);
        }
        None => {
            world.entity_mut(mech).insert(view);
        }
    }
}

/// Mechs with an open `Mecha` window get fresh data.
fn refresh_views(world: &mut World) {
    let mut q = world.query_filtered::<&UiWindow, With<UiKind<MechaUi>>>();
    let mut mechs: Vec<Entity> = q.iter(world).map(|w| w.source).collect();
    mechs.sort();
    mechs.dedup();
    for mech in mechs {
        refresh_view(world, mech);
    }
}

/// `ui_status`: `ui_status_user_inside` (the pilot of that mech) and conscious.
fn window_status(
    mut windows: Query<&mut UiWindow, With<UiKind<MechaUi>>>,
    pilots: Query<(&Pilot, Option<&Stat>)>,
) {
    for mut window in &mut windows {
        let inside = pilots
            .get(window.actor)
            .is_ok_and(|(p, stat)| p.mech == window.source && stat == Some(&Stat::Conscious));
        if !inside {
            window.cap_status(UiStatus::Close);
        }
    }
}

/// `mech_view_stats`: the pilot opens its mech's window.
pub fn open(world: &mut World, client: Entity, pilot: Entity) {
    let Some(mech) = world.get::<Pilot>(pilot).map(|p| p.mech) else {
        return;
    };
    refresh_view(world, mech);
    let title = crate::obj_damage::strip(&name_of(world, mech)).to_owned();
    world.write_message(OpenUi::new::<MechaUi>(client, pilot, mech, title));
}

fn panel_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<MechaUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, MechaAct)> = acts
        .read(world.resource::<Messages<UiActed<MechaUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, mech, act) in pending {
        // The window's status already checked the pilot; recheck at the moment of effect.
        if world.get::<Pilot>(actor).is_none_or(|p| p.mech != mech) {
            continue;
        }
        match act {
            MechaAct::Diagnostic => diagnostic(world, mech),
            MechaAct::ToggleSafety => set_safety(world, client, mech),
            MechaAct::ToggleIdLock => {
                if let Some(mut m) = world.get_mut::<Mech>(mech) {
                    m.flags ^= ID_LOCK_ON;
                }
            }
            MechaAct::ToggleLights => toggle_lights(world, mech, Some(client)),
            MechaAct::Equip(bits, action) => {
                let equip = Entity::try_from_bits(bits).filter(|&e| {
                    world
                        .get::<Mech>(mech)
                        .is_some_and(|m| m.equipment().any(|x| x == e))
                });
                if let Some(equip) = equip {
                    super::medical::ui_act(world, client, actor, mech, equip, &action);
                }
            }
        }
        refresh_view(world, mech);
    }
}

/// `ui_act("diagnostic")`: one holodiagnostic sheet per mech, on its tile.
pub fn diagnostic(world: &mut World, mech: Entity) {
    let Some(m) = world.get::<Mech>(mech) else {
        return;
    };
    if m.diagnostic_created {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mech) else {
        return;
    };
    let type_name = world.resource::<MechaDb>().data.mechs[m.def].name.clone();
    let mech_name = crate::obj_damage::strip(&name_of(world, mech)).to_owned();
    let Some(sheet) = crate::construction::spawn_obj(world, DIAGNOSTIC_ITEM, pos) else {
        return;
    };
    let name = format!("mecha holodiagnostic ({mech_name})");
    match world.get_mut::<DmName>(sheet) {
        Some(mut n) => n.name = name,
        None => {
            world.entity_mut(sheet).insert(DmName {
                name,
                desc: String::new(),
            });
        }
    }
    world.entity_mut(sheet).insert(MechDiagnostic {
        mech_name: type_name,
    });
    world
        .get_mut::<Mech>(mech)
        .expect("checked above")
        .diagnostic_created = true;
}

/// The holodiagnostic's examine line.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    if let Some(d) = world.get::<MechDiagnostic>(target) {
        lines.push((
            Span::Notice,
            format!(
                "Эта голограмма содержит данные экзокостюма «{}».",
                d.mech_name
            ),
        ));
    }
}

/// tg `set_safety`.
pub fn set_safety(world: &mut World, client: Entity, mech: Entity) {
    let Some(mut m) = world.get_mut::<Mech>(mech) else {
        return;
    };
    m.weapons_safety = !m.weapons_safety;
    let text = if m.weapons_safety {
        "equipment safe"
    } else {
        "equipment ready"
    };
    if let Some(&pos) = world.get::<TilePos>(mech) {
        playsound(world, PlaySound::new(pos, SAFETY_SOUND, 25.0, false));
    }
    tell(world, client, Span::Notice, text);
}

/// The lights' drain with the mech's capacitor (`light_power_drain / capacitor.rating`), watts.
fn light_drain_watts(world: &World, mech: Entity) -> f64 {
    let rating = world
        .get::<Mech>(mech)
        .and_then(|m| m.capacitor)
        .and_then(|c| super::core::path_of(world, c))
        .map_or(1, |p| world.resource::<MechaDb>().rating(&p));
    LIGHT_POWER_DRAIN / f64::from(rating.max(1))
}

/// tg `toggle_lights(user)`.
pub fn toggle_lights(world: &mut World, mech: Entity, client: Option<Entity>) {
    let Some(flags) = world.get::<Mech>(mech).map(|m| m.flags) else {
        return;
    };
    if flags & HAS_LIGHTS == 0 {
        if let Some(client) = client {
            tell(world, client, Span::Warning, "mech has no lights!");
        }
        return;
    }
    if flags & LIGHTS_ON == 0
        && charge(world, mech) < light_drain_watts(world, mech) * SSOBJ_WAIT_SECS
    {
        if let Some(client) = client {
            tell(world, client, Span::Warning, "no power for lights!");
        }
        return;
    }
    set_lights(world, mech, flags & LIGHTS_ON == 0);
    let on = flags & LIGHTS_ON == 0;
    if let Some(client) = world
        .get::<Mech>(mech)
        .and_then(|m| m.occupant)
        .and_then(|p| client_of(world, p))
    {
        tell(
            world,
            client,
            Span::Notice,
            if on { "lights on" } else { "lights off" },
        );
    }
}

/// `mecha_flags ^= LIGHTS_ON`, `set_light_on`, the sound and the Vim's headlights.
fn set_lights(world: &mut World, mech: Entity, on: bool) {
    let Some(mut m) = world.get_mut::<Mech>(mech) else {
        return;
    };
    if on {
        m.flags |= LIGHTS_ON;
    } else {
        m.flags &= !LIGHTS_ON;
    }
    let def = m.def;
    if let Some(&pos) = world.get::<TilePos>(mech) {
        playsound(world, PlaySound::new(pos, LIGHTS_SOUND, 40.0, true));
    }
    let light = on.then(|| light_of(world, mech)).flatten();
    place_light(world, mech, light);
    // `/obj/vehicle/sealed/mecha/vim/update_overlays`.
    let d = &world.resource::<MechaDb>().data.mechs[def];
    if d.path == super::core::VIM {
        let (icon, state) = (d.icon.clone(), format!("{}_headlights", d.base_icon_state));
        if let Some(id) = world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(&icon, &state))
            && let Some(mut look) = world.get_mut::<Appearance>(mech)
        {
            look.overlays.retain(|o| o.icon_state != id);
            if on {
                look.overlays.push(Overlay::new(id));
            }
        }
    }
    refresh_buttons(world, mech);
}

/// The mech's light from its DM light vars, switched on.
fn light_of(world: &World, mech: Entity) -> Option<LightSource> {
    let path = super::core::path_of(world, mech)?;
    let mut light = world.get::<DmLight>(mech)?.clone();
    light.light_on = true;
    let dir = world.get::<Facing>(mech).map_or(2, |f| f.0 as u8);
    source(&path, &light, dir, [0, 0])
}

/// Puts `light` into the mech's [`Optics`] (or takes its light back out).
fn place_light(world: &mut World, mech: Entity, light: Option<LightSource>) {
    let old = world.get::<MechLight>(mech).map(|l| l.0);
    if let Some(old) = old {
        world.entity_mut(mech).remove::<MechLight>();
        if let Some(mut o) = world.get_mut::<Optics>(mech)
            && o.light == Some(old)
        {
            o.light = None;
        }
    }
    let Some(light) = light else {
        return;
    };
    match world.get_mut::<Optics>(mech) {
        Some(mut o) => o.light = Some(light),
        None => {
            world.entity_mut(mech).insert(Optics {
                light: Some(light),
                ..default()
            });
        }
    }
    world.entity_mut(mech).insert(MechLight(light));
}

/// `process_constant_power_usage`: lit mechs pay for their lights, or they go out.
fn light_drain(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs_f64();
    let mut q = world.query::<(Entity, &Mech)>();
    let lit: Vec<Entity> = q
        .iter(world)
        .filter(|(_, m)| m.flags & LIGHTS_ON != 0)
        .map(|(e, _)| e)
        .collect();
    for mech in lit {
        let cost = light_drain_watts(world, mech) * dt;
        if !use_energy(world, mech, cost) {
            set_lights(world, mech, false);
        }
    }
}

/// The light's `dir` follows the mech's facing.
fn light_facing(world: &mut World) {
    let mut q = world.query_filtered::<Entity, (With<MechLight>, Changed<Facing>)>();
    let turned: Vec<Entity> = q.iter(world).collect();
    for mech in turned {
        let light = light_of(world, mech);
        place_light(world, mech, light);
    }
}

// --- Action buttons -----------------------------------------------------------------------

/// The pilot's mech buttons after its other buttons (tg `grant_controller_actions` /
/// `remove_controller_actions`); called when a mob enters or leaves a mech, and for `mech`'s
/// pilot when its lights change.
pub fn refresh_buttons(world: &mut World, mech_or_mob: Entity) {
    let mob = world
        .get::<Mech>(mech_or_mob)
        .map_or(Some(mech_or_mob), |m| m.occupant);
    let Some(mob) = mob else {
        return;
    };
    if world.get::<PlayerMob>(mob).is_none() {
        return;
    }
    let mut buttons: Vec<(ActionButton, f64)> = world
        .get::<ActionButtons>(mob)
        .map(|b| {
            b.0.iter()
                .filter(|(b, _)| !b.id.starts_with(BUTTON))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let mech = world.get::<Pilot>(mob).map(|p| p.mech);
    if let Some(m) = mech.and_then(|m| world.get::<Mech>(m)) {
        let flags = m.flags;
        let vim = world.resource::<MechaDb>().data.mechs[m.def].path == super::core::VIM;
        let icon = |dmi: &str, state: &str| {
            world
                .get_resource::<IconIndex>()
                .and_then(|index| index.get(dmi, state))
        };
        let background = icon(BACKGROUNDS_DMI, "bg_default");
        let button = |id: &str, name: &str, icon| ActionButton {
            id: id.to_owned(),
            name: name.to_owned(),
            icon,
            background,
            border: None,
            available: true,
            cooldown: 0.0,
            active: false,
            maptext: None,
        };
        buttons.push((
            button(
                VIEW_STATS,
                "View Stats",
                icon(ACTIONS_MECHA_DMI, "mech_view_stats"),
            ),
            0.0,
        ));
        if flags & HAS_LIGHTS != 0 {
            let state = if flags & LIGHTS_ON != 0 {
                "mech_lights_on"
            } else {
                "mech_lights_off"
            };
            buttons.push((
                button(LIGHTS, "Toggle Lights", icon(ACTIONS_MECHA_DMI, state)),
                0.0,
            ));
        }
        if vim {
            buttons.push((
                button(CHIME, "Chime!", icon(ACTIONS_VEHICLE_DMI, "vim_chime")),
                0.0,
            ));
            buttons.push((
                button(BUZZ, "Buzz.", icon(ACTIONS_VEHICLE_DMI, "vim_buzz")),
                0.0,
            ));
        }
    }
    if buttons.is_empty() && world.get::<ActionButtons>(mob).is_none() {
        return;
    }
    set_buttons(world, mob, buttons);
}

/// A click on a mech button, checked at the moment of effect: the client controls the pilot,
/// it is conscious and still in a mech.
fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .filter(|a| a.act.starts_with(BUTTON))
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source || client_of(world, actor) != Some(client) {
            continue;
        }
        if world.get::<Stat>(actor) != Some(&Stat::Conscious) {
            continue;
        }
        let Some(mech) = world.get::<Pilot>(actor).map(|p| p.mech) else {
            continue;
        };
        match id.as_str() {
            VIEW_STATS => open(world, client, actor),
            LIGHTS => toggle_lights(world, mech, Some(client)),
            CHIME => noise(world, client, mech, CHIME_SOUND, "chimes!"),
            BUZZ => noise(world, client, mech, BUZZ_SOUND, "buzzes."),
            _ => {}
        }
    }
}

/// `/datum/action/vehicle/sealed/noise/Trigger` (the Vim only).
fn noise(world: &mut World, client: Entity, mech: Entity, sound: &str, message: &str) {
    if !super::core::is_vim(world, mech) {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(ready) = world.get::<Mech>(mech).map(|m| now >= m.next_noise) else {
        return;
    };
    if !ready {
        tell(world, client, Span::Warning, "on cooldown!");
        return;
    }
    world.get_mut::<Mech>(mech).expect("checked").next_noise = now + VIM_SOUND_COOLDOWN_SECS;
    let Some(&pos) = world.get::<TilePos>(mech) else {
        return;
    };
    let name = crate::obj_damage::strip(&name_of(world, mech)).to_owned();
    visible(world, pos, Span::Notice, format!("{name} {message}"), &[]);
    playsound(world, PlaySound::new(pos, sound, 75.0, false));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_drain_is_twenty_watts_per_capacitor_tier() {
        assert_eq!(LIGHT_POWER_DRAIN, 20.0);
        // The switch-on check wants one `SSobj` wait of it.
        assert_eq!(LIGHT_POWER_DRAIN * SSOBJ_WAIT_SECS, 40.0);
    }
}

//! Ballistic gun modes (server): tg `/obj/item/gun/ballistic` `burst_select` (the
//! `/datum/action/item_action/toggle_firemode` button), the revolver's `spin_chamber`
//! (`guns/ballistic/revolver.dm`), the dual-tube shotgun's `toggle_tube` and alt-click rack
//! (`guns/ballistic/shotgun.dm`), and sawing a gun off (`try_sawoff`, `do_sawoff`).
//!
//! - A held gun whose type has the fire mode action shows a "Переключить режим огня" button:
//!   it switches between its burst and single shots (`burst_size = 1`, `fire_delay = 0`).
//! - Alt-click on a held revolver spins its cylinder (`rand(0, max_ammo * 2)` turns, at most
//!   once a second); on the held dual-tube shotgun it racks.
//! - A saw (`TOOL_SAW`) or a switched-on energy blade, double-bladed sword or plasma cutter on a
//!   gun that `can_be_sawn_off` saws it off in 3 seconds: a live round goes off into the user
//!   first and stops it. A sawn-off gun spreads 25° more, kicks harder (`sawoff_bonus_recoil`),
//!   is a normal-size item worn on the belt, not the back, and is named "обрез ...".
//!
//! tg's APS and Regal Condor have the action but start with `burst_fire_selection = FALSE`
//! while firing bursts; here every gun starts in the mode it fires in.
// shortcut: no selector switch or sawn-off overlays beyond the `_sawn` icon state, the sawn-off
// name is "обрез " + the nominative name (no genitive declension in the port); add them with
// declensions.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmGun, DmItem, DmName, DmWeight, IconIndex};
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::protocol::InventorySlot;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::server::{UiActed, UiSystems};

use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::click::AltClick;
use crate::combat::{CombatRng, NextMove};
use crate::guns::Gun;
use crate::health::{COMBAT_MESSAGE_RANGE, Stat};
use crate::inventory::{CarriedBy, Inventory};
use crate::security::{name_of, show};
use crate::server::PlayerMob;
use crate::sound::PlaySound;
use crate::tools::tell;

const BUTTON: &str = "firemode:";
const BUTTON_BACKGROUNDS: &str = "icons/mob/actions/backgrounds.dmi";
/// `burst_select_sound` (`SFX_FIRE_MODE_SWITCH`), `SFX_REVOLVER_SPIN`.
const FIRE_MODE_SWITCH: &str = "fire_mode_switch";
const REVOLVER_SPIN: &str = "revolver_spin";
/// The revolver's `spin_delay`, seconds.
const SPIN_DELAY: f64 = 1.0;
/// `try_sawoff`'s `do_after(user, 3 SECONDS)`, deciseconds; `CLICK_CD_MELEE`, seconds.
pub const SAWOFF_DS: f32 = 30.0;
const CLICK_CD_MELEE: f64 = 0.8;
/// `GLOB.gun_saw_types`.
const SAW_TYPES: [&str; 3] = [
    "/obj/item/gun/energy/plasmacutter",
    "/obj/item/melee/energy",
    "/obj/item/dualsaber",
];
/// `WEIGHT_CLASS_NORMAL`.
const WEIGHT_CLASS_NORMAL: u8 = 3;
const GUNS_LEFTHAND: &str = "icons/mob/inhands/weapons/guns_lefthand.dmi";
const GUNS_RIGHTHAND: &str = "icons/mob/inhands/weapons/guns_righthand.dmi";

/// When the revolver's cylinder may spin again (`recent_spin`).
#[derive(Component, Clone, Copy, Debug)]
struct RecentSpin(f64);

pub(crate) fn plugin(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // A saw on the gun in the other hand.
    app.add_click_handler(ClickPhase::OtherHand, 1483, |w, c| {
        c.held
            .is_some_and(|h| crate::gun_modes::try_sawoff(w, c.client, c.mob, h, c.target))
    });
    // A saw on a gun lying around.
    app.add_click_handler(ClickPhase::HeldOnTarget, 4293, |w, c| {
        c.held
            .is_some_and(|h| crate::gun_modes::try_sawoff(w, c.client, c.mob, h, c.target))
    });
    app.add_systems(
        Update,
        (alt_clicks, refresh_buttons)
            .chain()
            .after(crate::inventory::inventory_intents),
    )
    .add_systems(
        Update,
        trigger_buttons
            .in_set(UiSystems::Handle)
            .run_if(resource_exists::<Messages<UiActed<ActionButtonsUi>>>),
    );
}

// --- Fire mode ----------------------------------------------------------------------------

/// tg `burst_select`: burst or single shots. The balloon goes to the user.
pub fn burst_select(world: &mut World, client: Entity, mob: Entity, gun: Entity) {
    let Some(dm) = world.get::<DmGun>(gun).cloned() else {
        return;
    };
    let single = match world.get_mut::<Gun>(gun) {
        Some(mut state) => {
            state.single_fire = !state.single_fire;
            state.single_fire
        }
        None => return,
    };
    let text = if single {
        "режим - полуавтомат".to_owned()
    } else {
        let n = dm.burst_size;
        let ending = if n % 10 == 1 && n % 100 != 11 {
            "а"
        } else {
            "ов"
        };
        format!("режим - очередь из {n} патрон{ending}")
    };
    tell(world, client, Span::Notice, text);
    if let Some(&at) = world.get::<TilePos>(mob) {
        crate::sound::playsound(world, PlaySound::new(at, FIRE_MODE_SWITCH, 50.0, true));
    }
}

/// `/datum/action/item_action/toggle_firemode` for each held gun that has it.
fn refresh_buttons(world: &mut World) {
    let mut q = world.query_filtered::<Entity, (Changed<Inventory>, With<PlayerMob>)>();
    let mobs: Vec<Entity> = q.iter(world).collect();
    for mob in mobs {
        let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
            continue;
        };
        let icons = world.get_resource::<IconIndex>();
        let mut ours = Vec::new();
        for (hand, item) in inventory.hands.iter().enumerate() {
            let Some(gun) = item.filter(|&g| world.get::<DmGun>(g).is_some_and(|d| d.burst_select))
            else {
                continue;
            };
            ours.push((
                ActionButton {
                    id: format!("{BUTTON}{hand}"),
                    name: "Переключить режим огня".to_owned(),
                    icon: world.get::<Appearance>(gun).map(|a| a.icon_state),
                    background: icons.and_then(|i| i.get(BUTTON_BACKGROUNDS, "bg_default")),
                    border: None,
                    available: true,
                    cooldown: 0.0,
                    active: false,
                    maptext: None,
                },
                0.0,
            ));
        }
        let current = world.get::<ActionButtons>(mob);
        let had = current.is_some_and(|b| b.0.iter().any(|(b, _)| b.id.starts_with(BUTTON)));
        if ours.is_empty() && !had {
            continue;
        }
        let mut buttons: Vec<(ActionButton, f64)> = current
            .map(|b| {
                b.0.iter()
                    .filter(|(b, _)| !b.id.starts_with(BUTTON))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        buttons.extend(ours);
        set_buttons(world, mob, buttons);
    }
}

/// `ui_action_click` -> `burst_select`, checked at the moment of effect: the client's conscious
/// mob still holds a gun with the action in that hand.
fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .filter(|a| a.act.starts_with(BUTTON))
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world.get::<Stat>(actor) != Some(&Stat::Conscious)
        {
            continue;
        }
        let Some(hand) = id
            .strip_prefix(BUTTON)
            .and_then(|n| n.parse::<usize>().ok())
            .filter(|&h| h < 2)
        else {
            continue;
        };
        let Some(gun) = world
            .get::<Inventory>(actor)
            .and_then(|i| i.hands[hand])
            .filter(|&g| world.get::<DmGun>(g).is_some_and(|d| d.burst_select))
        else {
            continue;
        };
        burst_select(world, client, actor, gun);
    }
}

// --- Revolver and dual-tube shotgun -------------------------------------------------------

/// `/obj/item/gun/ballistic/revolver/click_alt` (`spin_chamber`) and
/// `/obj/item/gun/ballistic/shotgun/automatic/dual_tube/click_alt` (`rack`).
// shortcut: both want the gun in a hand (tg: within reach).
fn alt_clicks(world: &mut World, mut cursor: Local<MessageCursor<AltClick>>) {
    let clicks: Vec<AltClick> = cursor
        .read(world.resource::<Messages<AltClick>>())
        .copied()
        .collect();
    for click in clicks {
        if click.loot_only || click.secondary {
            continue;
        }
        let (gun, mob) = (click.target, click.mob);
        let Some(state) = world.get::<Gun>(gun) else {
            continue;
        };
        let dual_tube = state.alt_magazine.is_some();
        let cylinder = state.magazine.as_ref().is_some_and(|m| m.is_cylinder());
        if !dual_tube && !cylinder {
            continue;
        }
        let holding = world.get::<Inventory>(mob).is_some_and(|i| i.holds(gun));
        if !holding || world.get::<Stat>(mob) != Some(&Stat::Conscious) {
            continue;
        }
        if dual_tube {
            crate::guns::rack(world, click.client, gun);
        } else {
            spin_chamber(world, mob, gun);
        }
    }
}

/// `spin_chamber` -> `do_spin`: the cylinder turns `rand(0, max_ammo * 2)` times; the round
/// it stops at is the next to fire.
pub fn spin_chamber(world: &mut World, mob: Entity, gun: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if world.get::<RecentSpin>(gun).is_some_and(|r| now < r.0) {
        return;
    }
    world.entity_mut(gun).insert(RecentSpin(now + SPIN_DELAY));
    let Some(max) = world
        .get::<Gun>(gun)
        .and_then(|g| g.magazine.as_ref())
        .filter(|m| m.is_cylinder())
        .map(|m| m.data.max_ammo)
    else {
        return;
    };
    let turns = world.resource_mut::<CombatRng>().range(0, max * 2) as usize;
    if let Some(mut state) = world.get_mut::<Gun>(gun)
        && let Some(mag) = state.magazine.as_mut()
        && !mag.rounds.is_empty()
    {
        let len = mag.rounds.len();
        mag.rounds.rotate_left(turns % len);
    }
    let Some(&at) = world.get::<TilePos>(mob) else {
        return;
    };
    crate::sound::playsound(world, PlaySound::new(at, REVOLVER_SPIN, 30.0, false));
    let (user, item) = (name_of(world, mob), name_of(world, gun));
    let item = crate::obj_damage::strip(&item).to_owned();
    show(
        world,
        at,
        COMBAT_MESSAGE_RANGE,
        ChatChannel::Info,
        (Span::Notice, format!("{user} прокручивает барабан {item}.")),
        &[(
            mob,
            Span::Notice,
            format!("Вы прокручиваете барабан {item}."),
        )],
    );
}

/// `toggle_tube`: the dual-tube shotgun feeds from its other tube.
pub fn toggle_tube(world: &mut World, client: Entity, gun: Entity) {
    let toggled = {
        let Some(mut state) = world.get_mut::<Gun>(gun) else {
            return;
        };
        let state = &mut *state;
        let Some(alt) = state.alt_magazine.as_mut() else {
            return;
        };
        if let Some(current) = state.magazine.as_mut() {
            std::mem::swap(current, alt);
        }
        state.toggled = !state.toggled;
        state.toggled
    };
    let text = if toggled {
        "переключено на трубку B"
    } else {
        "переключено на трубку A"
    };
    tell(world, client, Span::Notice, text);
}

// --- Sawing off ---------------------------------------------------------------------------

/// Whether `item` saws guns off (`get_sharpness()` and a `gun_saw_types` type or `TOOL_SAW`).
fn is_saw(world: &World, item: Entity) -> bool {
    if world
        .get::<ss13_mapload::construct::DmTool>(item)
        .is_some_and(|t| t.behaviour == "saw")
    {
        return true;
    }
    let Some(path) = world.get::<DmItem>(item).map(|d| d.path.as_str()) else {
        return false;
    };
    SAW_TYPES.iter().any(|t| is_under(path, t)) && crate::weapons::energy::is_on(world, item)
}

/// tg `try_sawoff` up to its `do_after`. True when the click is used.
pub fn try_sawoff(
    world: &mut World,
    client: Entity,
    mob: Entity,
    saw: Entity,
    gun: Entity,
) -> bool {
    if saw == gun
        || !world.get::<DmGun>(gun).is_some_and(|d| d.can_be_sawn_off)
        || !is_saw(world, saw)
    {
        return false;
    }
    if world.get::<Gun>(gun).is_some_and(|g| g.sawn_off) {
        tell(world, client, Span::Warning, "уже обрезано!");
        return true;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(mob).insert(NextMove(now + CLICK_CD_MELEE));
    let Some(&at) = world.get::<TilePos>(mob) else {
        return true;
    };
    let (user, item) = (name_of(world, mob), name_of(world, gun));
    let item = crate::obj_damage::strip(&item).to_owned();
    show(
        world,
        at,
        COMBAT_MESSAGE_RANGE,
        ChatChannel::Info,
        (Span::Notice, format!("{user} пытается обрезать {item}.")),
        &[(
            mob,
            Span::Notice,
            format!("Вы пытаетесь обрезать {item}..."),
        )],
    );
    // "if there's any live ammo inside the gun, makes it go off".
    if crate::guns::blow_up(world, mob, gun, at) {
        let name = ss13_shared::capitalize(&item);
        show(
            world,
            at,
            COMBAT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (Span::Danger, format!("{name} выстреливает!")),
            &[(
                mob,
                Span::Danger,
                format!("{name} выстреливает вам в лицо!"),
            )],
        );
        return true;
    }
    let actor = crate::tools::Actor {
        client,
        mob,
        item: Some(saw),
    };
    // A held gun has no tile: the `do_after` then follows the user, and the gun in the other
    // hand is looked up again when it ends.
    let target = if world.get::<TilePos>(gun).is_some() {
        gun
    } else {
        mob
    };
    crate::tools::do_after(
        world,
        actor,
        target,
        SAWOFF_DS,
        0,
        crate::construction::Job::SawOff,
    );
    true
}

/// `try_sawoff` after its `do_after`.
pub fn complete_sawoff(world: &mut World, mob: Entity, target: Entity) {
    let gun = if target == mob {
        let Some(inv) = world.get::<crate::inventory::Inventory>(mob) else {
            return;
        };
        let Some(gun) = inv.hands[1 - inv.active] else {
            return;
        };
        gun
    } else {
        target
    };
    if world.get::<Gun>(gun).is_none_or(|g| g.sawn_off)
        || !world.get::<DmGun>(gun).is_some_and(|d| d.can_be_sawn_off)
    {
        return;
    }
    if let Some(&at) = world.get::<TilePos>(mob) {
        let (user, item) = (name_of(world, mob), name_of(world, gun));
        let item = crate::obj_damage::strip(&item).to_owned();
        show(
            world,
            at,
            COMBAT_MESSAGE_RANGE,
            ChatChannel::Info,
            (Span::Notice, format!("{user} обрезает {item}!")),
            &[(mob, Span::Notice, format!("Вы обрезаете {item}."))],
        );
    }
    do_sawoff(world, gun);
}

/// tg `do_sawoff`: the gun is a sawn-off (also `spawn_sawn_off` guns at spawn).
pub fn do_sawoff(world: &mut World, gun: Entity) {
    let Some(dm) = world.get::<DmGun>(gun).cloned() else {
        return;
    };
    match world.get_mut::<Gun>(gun) {
        Some(mut state) => state.sawn_off = true,
        None => return,
    }
    let name = crate::obj_damage::strip(&name_of(world, gun)).to_owned();
    let sawn = format!("обрез {name}");
    if let Some(mut dm_name) = world.get_mut::<DmName>(gun) {
        dm_name.name = sawn.clone();
        if !dm.sawn_desc.is_empty() {
            dm_name.desc = dm.sawn_desc.clone();
        }
    }
    if world.get::<Name>(gun).is_some() {
        world.entity_mut(gun).insert(Name::new(sawn));
    }
    world.entity_mut(gun).insert(DmWeight {
        w_class: WEIGHT_CLASS_NORMAL,
    });
    if let Some(mut item) = world.get_mut::<DmItem>(gun) {
        let back = crate::inventory::slot_bit(InventorySlot::Back);
        let belt = crate::inventory::slot_bit(InventorySlot::Belt);
        item.slot_flags = (item.slot_flags & !back) | belt;
        item.lefthand_file = GUNS_LEFTHAND.to_owned();
        item.righthand_file = GUNS_RIGHTHAND.to_owned();
        item.inhand_icon_state = "gun".to_owned();
        item.worn_icon_state = "gun".to_owned();
    }
    // `update_icon_state`: `[base_icon_state || initial(icon_state)]_sawn`.
    if let Some(state) = world
        .get::<ss13_mapload::DmAtom>(gun)
        .map(|a| format!("{}_sawn", a.icon_state))
    {
        crate::obj_damage::set_icon_state(world, gun, &state);
    }
    if let Some(mob) = world.get::<CarriedBy>(gun).map(|c| c.0) {
        crate::inventory::refresh_look(world, mob);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guns::{Magazine, Round};
    use ss13_mapload::{CasingData, MagazineData};

    fn cylinder() -> MagazineData {
        MagazineData {
            path: format!("{}/rev38", crate::guns::CYLINDER),
            caliber: ".38".into(),
            max_ammo: 6,
            ammo: CasingData {
                path: "/obj/item/ammo_casing/c38".into(),
                caliber: ".38".into(),
                projectile: ss13_mapload::ProjectileData {
                    path: "/obj/projectile/bullet/c38".into(),
                    ..default()
                },
                ..default()
            },
            start_empty: true,
        }
    }

    #[test]
    fn a_cylinder_keeps_its_empty_chambers_and_swaps_out_spent_casings() {
        let data = cylinder();
        let mut mag = Magazine::new(&data);
        assert_eq!(mag.rounds.len(), 6);
        assert_eq!(mag.ammo_count(true), 0);
        let live = Round {
            casing: data.ammo.clone(),
            live: true,
        };
        // Into the first empty chamber; nothing falls out.
        assert_eq!(mag.give_round(live.clone()), Some(None));
        assert_eq!(mag.ammo_count(true), 1);
        assert!(mag.rounds[0].live && mag.rounds[1].is_empty());
        // A spent casing gives its chamber up to a new round and comes back out.
        mag.rounds[0].live = false;
        let spent = mag.give_round(live.clone()).expect("a spent chamber");
        assert!(spent.is_some_and(|r| !r.live && !r.is_empty()));
        for _ in 0..5 {
            assert_eq!(mag.give_round(live.clone()), Some(None));
        }
        assert_eq!(mag.ammo_count(false), 6);
        // Full of live rounds: no room.
        assert_eq!(mag.give_round(live), None);
    }
}

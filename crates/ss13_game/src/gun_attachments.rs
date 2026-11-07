//! Gun attachments (server): the under-barrel seclite (`/datum/component/seclite_attachable`),
//! the suppressor (`/obj/item/suppressor`, `/obj/item/gun/ballistic` `install_suppressor` /
//! `click_alt`), the bayonet (`/datum/component/bayonet_attachable`) and the scope
//! (`/datum/component/scope`).
//!
//! - A gun with a light mount (its `add_seclight_point`) takes a seclite clicked on it; the light
//!   goes inside the gun and shines from the gun's holder (or the gun on the floor) while on. A
//!   held gun with a light gets a "Переключить фонарик" action button. Some guns start with
//!   one (the mini energy gun, the Zashch and Wespe pistols).
//! - A ballistic gun that `can_suppress` takes a suppressor while held; its shots are then
//!   heard only nearby (`suppressed_sound`, `SILENCED_SOUND_EXTRARANGE`) and print no shot
//!   message (`crate::guns`). Alt-click on the held gun takes the suppressor back into a hand
//!   unless it is built in (`can_unsuppress = FALSE`).
//! - A gun with a bayonet mount (its `add_bayonet_point`) takes a combat knife; in combat mode a
//!   melee hit with the gun is the bayonet's (`crate::combat`).
//! - A screwdriver on the gun takes the light and the bayonet off, into the hands.
//! - A scoped gun (`AddComponent(/datum/component/scope)`) zooms with a right click far away:
//!   the view moves towards the clicked point, `range_modifier` times as far (tg
//!   `calculate_params`, capped at `range_modifier` half-views); another right click, a step,
//!   dropping the gun or being incapacitated zooms back ([`ss13_shared::protocol::CameraEffect::Scope`]).
// shortcut: no attachment overlays or `-flight` icon states, no examine lines, no EMP on the
// mounted light; the scope keeps the offset of the zooming click (tg follows the cursor every
// tick), has no fullscreen overlay and no scope sound. Add them as the client grows them.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmGun, IconIndex};
use ss13_net::camera::server::SendCameraEffect;
use ss13_shared::chat::Span;
use ss13_shared::protocol::{CameraEffect, ClickModifiers};
use ss13_shared::{Appearance, LightSource, Optics, TilePos};
use ss13_tgui::server::{UiActed, UiSystems};

use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::click::AltClick;
use crate::devices::flashlight::Flashlight;
use crate::guns::Gun;
use crate::health::Stat;
use crate::inventory::{CarriedBy, Inventory};
use crate::security::name_of;
use crate::server::PlayerMob;
use crate::storage::StoredIn;
use crate::tools::{ToolKind, tell, tool_of};

const SECLITE: &str = "/obj/item/flashlight/seclite";
const SUPPRESSOR: &str = "/obj/item/suppressor";
/// `valid_bayonets`.
const BAYONET: &str = "/obj/item/knife/combat";
const BUTTON: &str = "gunlight:";
/// The screwdriver's `usesound`.
const SCREWDRIVER_SOUND: &str = "sound/items/tools/screwdriver.ogg";
const BUTTON_BACKGROUNDS: &str = "icons/mob/actions/backgrounds.dmi";
/// tg's default `view` (`21x15`), half of it in pixels: the scope's cap at range 1.
const HALF_VIEW_PX: [f32; 2] = [21.0 * 16.0, 15.0 * 16.0];
/// `MOUSE_POINTER_OFFSET_MULT`.
const MOUSE_POINTER_OFFSET_MULT: f32 = 1.1;

/// Guns whose `add_seclight_point` adds a light mount (their subtypes inherit it).
const LIGHT_MOUNTS: [&str; 46] = [
    "/obj/item/gun/energy/disabler",
    "/obj/item/gun/energy/e_gun",
    "/obj/item/gun/energy/laser",
    "/obj/item/gun/energy/pulse/carbine",
    "/obj/item/gun/energy/ionrifle",
    "/obj/item/gun/energy/recharge/kinetic_accelerator",
    "/obj/item/gun/energy/eg_14",
    "/obj/item/gun/ballistic/shotgun/riot",
    "/obj/item/gun/ballistic/shotgun/automatic/combat",
    "/obj/item/gun/ballistic/automatic/battle_rifle",
    "/obj/item/gun/ballistic/automatic/ar",
    "/obj/item/gun/ballistic/automatic/mini_uzi",
    "/obj/item/gun/ballistic/automatic/proto",
    "/obj/item/gun/ballistic/automatic/wt550",
    "/obj/item/gun/ballistic/rifle/sniper_rifle",
    "/obj/item/gun/ballistic/automatic/pistol/tp14",
    "/obj/item/gun/ballistic/automatic/fn18",
    "/obj/item/gun/ballistic/automatic/fn4",
    "/obj/item/gun/ballistic/automatic/as32",
    "/obj/item/gun/ballistic/automatic/miecz",
    "/obj/item/gun/ballistic/automatic/pistol/gp9",
    "/obj/item/gun/ballistic/automatic/cm5",
    "/obj/item/gun/ballistic/automatic/sabel",
    "/obj/item/gun/ballistic/automatic/pistol/cm357",
    "/obj/item/gun/ballistic/automatic/cm15",
    "/obj/item/gun/ballistic/automatic/cm82",
    "/obj/item/gun/ballistic/automatic/vityaz",
    "/obj/item/gun/ballistic/automatic/pistol/zashch",
    "/obj/item/gun/ballistic/automatic/f4",
    "/obj/item/gun/ballistic/automatic/pistol/cm70",
    "/obj/item/gun/ballistic/automatic/sindano",
    "/obj/item/gun/ballistic/automatic/pistol/cm23",
    "/obj/item/gun/ballistic/automatic/carwo",
    "/obj/item/gun/ballistic/automatic/pistol/wespe",
    // Duplicates of parents above are harmless; these keep the list greppable against DM.
    "/obj/item/gun/energy/disabler/smg",
    "/obj/item/gun/energy/disabler/smoothbore",
    "/obj/item/gun/energy/laser/thermal",
    "/obj/item/gun/energy/laser/assault",
    "/obj/item/gun/energy/e_gun/mini",
    "/obj/item/gun/energy/ionrifle/carbine",
    "/obj/item/gun/ballistic/shotgun/automatic/combat/compact",
    "/obj/item/gun/ballistic/shotgun/riot/renoster",
    "/obj/item/gun/ballistic/automatic/sabel/auto/modern/bullpup",
    "/obj/item/gun/ballistic/automatic/sabel/auto/upgraded",
    "/obj/item/gun/ballistic/automatic/pistol/zashch",
    "/obj/item/gun/ballistic/automatic/pistol/wespe",
];
/// Guns that come with a seclite (`starting_light = new /obj/item/flashlight/seclite(src)`).
const STARTING_LIGHT: [&str; 3] = [
    "/obj/item/gun/energy/e_gun/mini",
    "/obj/item/gun/ballistic/automatic/pistol/zashch",
    "/obj/item/gun/ballistic/automatic/pistol/wespe",
];
/// Guns whose `add_bayonet_point` adds a bayonet mount.
const BAYONET_MOUNTS: [&str; 10] = [
    "/obj/item/gun/ballistic/rifle/boltaction",
    "/obj/item/gun/ballistic/rifle/sks",
    "/obj/item/gun/ballistic/rifle/krov",
    "/obj/item/gun/ballistic/automatic/c20r",
    "/obj/item/gun/ballistic/automatic/wt550",
    "/obj/item/gun/energy/recharge/ebow",
    "/obj/item/gun/energy/recharge/kinetic_accelerator",
    "/obj/item/gun/energy/laser/musket",
    "/obj/item/gun/energy/laser/thermal",
    // `pipegun` and `mosin` are bolt actions.
    "/obj/item/gun/ballistic/rifle/boltaction/pipegun",
];
/// `AddComponent(/datum/component/scope, range_modifier = ...)` of guns.
const SCOPES: [(&str, f32); 12] = [
    ("/obj/item/gun/ballistic/automatic/battle_rifle", 2.0),
    ("/obj/item/gun/ballistic/rifle/boltaction/prime", 1.5),
    ("/obj/item/gun/ballistic/rifle/rebarxbow/syndie", 2.0),
    ("/obj/item/gun/ballistic/rifle/sniper_rifle", 4.0),
    ("/obj/item/gun/energy/event_horizon", 4.0),
    ("/obj/item/gun/ballistic/rifle/lionhunter", 3.2),
    ("/obj/item/gun/ballistic/rifle/hlrm", 4.0),
    ("/obj/item/gun/ballistic/automatic/lanca", 1.5),
    ("/obj/item/gun/ballistic/rifle/krov", 1.5),
    ("/obj/item/gun/ballistic/automatic/wylom", 2.0),
    ("/obj/item/gun/ballistic/automatic/f90", 4.0),
    ("/obj/item/gun/ballistic/automatic/carwo/marksman", 2.0),
];

/// `seclite_attachable` on a gun: the mounted light.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct LightMount {
    pub light: Option<Entity>,
}

/// `bayonet_attachable` on a gun: the mounted knife.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct BayonetMount {
    pub bayonet: Option<Entity>,
}

/// `/datum/component/scope` on a gun.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Scope {
    pub range_modifier: f32,
}

/// A mob looking through `gun`'s scope from `at` (`TRAIT_USER_SCOPED`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Zoomed {
    pub gun: Entity,
    pub at: TilePos,
}

/// The light a mounted seclite put on `.0` (the gun's holder or the gun).
#[derive(Component, Clone, Copy, Debug)]
struct MountLit(Entity, LightSource);

pub fn plugin(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // A seclite, suppressor, bayonet or screwdriver on the gun in the other hand.
    app.add_click_handler(ClickPhase::OtherHand, 1491, |w, c| {
        c.held
            .is_some_and(|h| crate::gun_attachments::attach(w, c.client, c.mob, h, c.target, true))
    });
    // The same on a gun lying around (not the suppressor: tg wants it held).
    app.add_click_handler(ClickPhase::HeldOnTarget, 4291, |w, c| {
        c.held
            .is_some_and(|h| crate::gun_attachments::attach(w, c.client, c.mob, h, c.target, false))
    });
    app.add_observer(add_mounts)
        .add_observer(unlight)
        .add_systems(
            Update,
            (alt_clicks, sync_lights, refresh_buttons, scope_upkeep)
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

/// The gun's `Initialize`: `add_seclight_point`, `add_bayonet_point`, the scope component.
fn add_mounts(add: On<Add<DmGun>>, guns: Query<&DmGun>, mut commands: Commands) {
    let Ok(dm) = guns.get(add.entity) else {
        return;
    };
    let path = dm.path.as_str();
    let gun = add.entity;
    if LIGHT_MOUNTS.iter().any(|m| is_under(path, m)) {
        commands.entity(gun).insert_if_new(LightMount::default());
        if STARTING_LIGHT.iter().any(|m| is_under(path, m)) {
            commands.queue(move |world: &mut World| {
                let Some(at) = crate::modpc::pda::turf_of(world, gun) else {
                    return;
                };
                if let Some(light) = crate::construction::spawn_obj(world, SECLITE, at) {
                    mount_light(world, gun, light);
                }
            });
        }
    }
    if BAYONET_MOUNTS.iter().any(|m| is_under(path, m)) {
        commands.entity(gun).insert_if_new(BayonetMount::default());
    }
    if let Some(&(_, range_modifier)) = SCOPES.iter().find(|(p, _)| is_under(path, p)) {
        commands.entity(gun).insert_if_new(Scope { range_modifier });
    }
}

fn path_is(world: &World, item: Entity, root: &str) -> bool {
    crate::construction::path_of(world, item).is_some_and(|p| is_under(&p, root))
}

/// Out of the user's hands (the item is about to go inside the gun).
fn release(world: &mut World, mob: Entity, item: Entity) {
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        for hand in &mut inventory.hands {
            if *hand == Some(item) {
                *hand = None;
            }
        }
    }
}

/// `add_light`: the seclite into the gun.
fn mount_light(world: &mut World, gun: Entity, light: Entity) {
    crate::storage::put_inside(world, light, gun);
    if let Some(mut mount) = world.get_mut::<LightMount>(gun) {
        mount.light = Some(light);
    }
}

/// tg `on_attackby` of the mounts and `/obj/item/gun/ballistic/item_interaction` with a
/// suppressor, plus the screwdriver that takes the mounts off. True when the click is used.
pub fn attach(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    gun: Entity,
    in_hand: bool,
) -> bool {
    if world.get::<Gun>(gun).is_none() || held == gun {
        return false;
    }
    // Seclites.
    if path_is(world, held, SECLITE) {
        let Some(mount) = world.get::<LightMount>(gun).copied() else {
            return false;
        };
        if let Some(light) = mount.light {
            let light_name = name_of(world, light);
            tell(
                world,
                client,
                Span::Warning,
                format!("уже имеется {light_name}!"),
            );
            return true;
        }
        let what = name_of(world, held);
        release(world, mob, held);
        mount_light(world, gun, held);
        tell(world, client, Span::Notice, format!("прикрепление {what}"));
        return true;
    }
    // Bayonets.
    if path_is(world, held, BAYONET) {
        let Some(mount) = world.get::<BayonetMount>(gun).copied() else {
            return false;
        };
        if let Some(bayonet) = mount.bayonet {
            let name = name_of(world, bayonet);
            tell(world, client, Span::Warning, format!("уже имеется {name}!"));
            return true;
        }
        release(world, mob, held);
        crate::storage::put_inside(world, held, gun);
        if let Some(mut mount) = world.get_mut::<BayonetMount>(gun) {
            mount.bayonet = Some(held);
        }
        tell(world, client, Span::Notice, "прикрепление");
        return true;
    }
    // Suppressors (ballistic guns only).
    if path_is(world, held, SUPPRESSOR) {
        let Some(dm) = world.get::<DmGun>(gun).cloned() else {
            return false;
        };
        if !dm.ballistic {
            return false;
        }
        let state = world.get::<Gun>(gun).cloned().unwrap_or_default();
        let holding = world.get::<Inventory>(mob).is_some_and(|i| i.holds(gun));
        let refusal = if !dm.can_suppress {
            Some("не подходит!")
        } else if !in_hand || !holding {
            Some("не в руке!")
        } else if state.suppressed > 0 {
            Some("уже имеет глушитель!")
        } else {
            None
        };
        if let Some(text) = refusal {
            tell(world, client, Span::Warning, text);
            return true;
        }
        release(world, mob, held);
        crate::storage::put_inside(world, held, gun);
        if let Some(mut g) = world.get_mut::<Gun>(gun) {
            // `suppressor.suppression`: `SUPPRESSED_QUIET`.
            g.suppressed = 1;
            g.suppressor = Some(held);
        }
        tell(world, client, Span::Notice, "глушитель прикреплён");
        return true;
    }
    // A screwdriver takes the light and the bayonet off (`unscrew_light`, `unscrew_bayonet`).
    if tool_of(world, held).is_some_and(|(kind, _)| kind == ToolKind::Screwdriver) {
        let light = world.get::<LightMount>(gun).and_then(|m| m.light);
        let bayonet = world.get::<BayonetMount>(gun).and_then(|m| m.bayonet);
        if light.is_none() && bayonet.is_none() {
            return false;
        }
        for item in [light, bayonet].into_iter().flatten() {
            let name = name_of(world, item);
            detach(world, mob, gun, item);
            tell(world, client, Span::Notice, format!("отвинчивание {name}"));
        }
        // `play_tool_sound`.
        if let Some(at) = crate::modpc::pda::turf_of(world, gun) {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(at, SCREWDRIVER_SOUND, 50.0, true),
            );
        }
        return true;
    }
    false
}

/// An attachment out of `gun` into `mob`'s hands (or at its feet); the mount forgets it.
fn detach(world: &mut World, mob: Entity, gun: Entity, item: Entity) {
    if let Some(mut mount) = world.get_mut::<LightMount>(gun)
        && mount.light == Some(item)
    {
        mount.light = None;
    }
    if let Some(mut mount) = world.get_mut::<BayonetMount>(gun)
        && mount.bayonet == Some(item)
    {
        mount.bayonet = None;
    }
    if let Some(mut g) = world.get_mut::<Gun>(gun)
        && g.suppressor == Some(item)
    {
        g.suppressor = None;
        g.suppressed = 0;
    }
    let Some(&at) = world.get::<TilePos>(mob) else {
        return;
    };
    crate::storage::take_out(world, item, at);
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, item);
    }
}

/// `/obj/item/gun/ballistic/click_alt`: the suppressor back into a hand.
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
        let Some(suppressor) = world.get::<Gun>(gun).and_then(|g| g.suppressor) else {
            continue;
        };
        let removable = world.get::<DmGun>(gun).is_some_and(|d| d.can_unsuppress);
        let holding = world.get::<Inventory>(mob).is_some_and(|i| i.holds(gun));
        if !removable || !holding || world.get::<Stat>(mob) != Some(&Stat::Conscious) {
            continue;
        }
        let name = name_of(world, suppressor);
        detach(world, mob, gun, suppressor);
        tell(world, click.client, Span::Notice, format!("снятие {name}"));
    }
}

/// The bayonet a combat-mode melee hit with `item` is made with (tg `on_pre_attack`).
pub fn bayonet_of(world: &World, item: Entity) -> Option<Entity> {
    world.get::<BayonetMount>(item).and_then(|m| m.bayonet)
}

/// The mounted light shines from the gun's holder, or the gun itself on the floor, while on.
fn sync_lights(world: &mut World) {
    let mut q = world.query::<(Entity, &LightMount, Option<&MountLit>)>();
    let mounts: Vec<(Entity, Option<Entity>, Option<MountLit>)> = q
        .iter(world)
        .map(|(e, m, lit)| (e, m.light, lit.copied()))
        .collect();
    for (gun, light, lit) in mounts {
        // A light taken out by other means (deleted, moved) is no longer mounted.
        let light = light.filter(|&l| world.get::<StoredIn>(l).is_some_and(|s| s.0 == gun));
        if light.is_none()
            && let Some(mut mount) = world.get_mut::<LightMount>(gun)
            && mount.light.is_some()
        {
            mount.light = None;
        }
        let source = light
            .and_then(|l| world.get::<Flashlight>(l))
            .filter(|f| f.on)
            .and_then(|f| f.light);
        let target = source.and_then(|_| {
            world
                .get::<CarriedBy>(gun)
                .map(|c| c.0)
                .or(world.get::<TilePos>(gun).map(|_| gun))
        });
        let wanted = target.zip(source);
        if lit.map(|l| (l.0, l.1)) == wanted {
            continue;
        }
        if lit.is_some() {
            world.entity_mut(gun).remove::<MountLit>();
        }
        let Some((target, source)) = wanted else {
            continue;
        };
        // One light per entity: another one already there wins (as the flashlight's).
        match world.get::<Optics>(target).map(|o| o.light.is_none()) {
            Some(true) => {
                if let Some(mut o) = world.get_mut::<Optics>(target) {
                    o.light = Some(source);
                }
            }
            Some(false) => continue,
            None => {
                world.entity_mut(target).insert(Optics {
                    light: Some(source),
                    ..default()
                });
            }
        }
        world.entity_mut(gun).insert(MountLit(target, source));
    }
}

/// The light goes out where [`sync_lights`] put it.
fn unlight(
    remove: On<Remove<MountLit>>,
    lit: Query<&MountLit>,
    mut optics: Query<&mut Optics>,
    mut commands: Commands,
) {
    let Ok(&MountLit(target, light)) = lit.get(remove.entity) else {
        return;
    };
    if let Ok(mut o) = optics.get_mut(target)
        && o.light == Some(light)
    {
        if !o.opaque && o.tile.is_none() {
            commands.entity(target).try_remove::<Optics>();
        } else {
            o.light = None;
        }
    }
}

/// `/datum/action/item_action/toggle_seclight` for each held gun with a light.
fn refresh_buttons(world: &mut World) {
    let mut q = world.query_filtered::<Entity, (Changed<Inventory>, With<PlayerMob>)>();
    let mut mobs: Vec<Entity> = q.iter(world).collect();
    // A light mounted or taken off a gun already held.
    let mut changed = world.query_filtered::<Entity, Changed<LightMount>>();
    let guns: Vec<Entity> = changed.iter(world).collect();
    mobs.extend(
        guns.into_iter()
            .filter_map(|g| world.get::<CarriedBy>(g).map(|c| c.0)),
    );
    mobs.sort_unstable();
    mobs.dedup();
    for mob in mobs {
        let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
            continue;
        };
        let icons = world.get_resource::<IconIndex>();
        let mut ours = Vec::new();
        for (hand, item) in inventory.hands.iter().enumerate() {
            let Some(light) = item
                .and_then(|g| world.get::<LightMount>(g))
                .and_then(|m| m.light)
            else {
                continue;
            };
            ours.push((
                ActionButton {
                    id: format!("{BUTTON}{hand}"),
                    name: "Переключить фонарик".to_owned(),
                    icon: world.get::<Appearance>(light).map(|a| a.icon_state),
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

/// `on_action_click` -> `toggle_light`, checked at the moment of effect: the client's conscious
/// mob still holds a gun with a light in that hand.
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
        let gun = world.get::<Inventory>(actor).and_then(|i| i.hands[hand]);
        let Some(light) = gun
            .and_then(|g| world.get::<LightMount>(g))
            .and_then(|m| m.light)
        else {
            continue;
        };
        toggle_mounted(world, client, light);
    }
}

/// `seclite_attachable/toggle_light`: the light's own switch, then the balloon.
pub fn toggle_mounted(world: &mut World, client: Entity, light: Entity) {
    if !crate::devices::flashlight::toggle_light(world, Some(client), light) {
        return;
    }
    let on = world.get::<Flashlight>(light).is_some_and(|f| f.on);
    let name = name_of(world, light);
    let state = if on {
        "включается"
    } else {
        "выключается"
    };
    tell(world, client, Span::Notice, format!("{name} {state}"));
}

/// tg `calculate_params`: the view offset, pixels, for a click `pixels` from the user.
pub fn scope_offset(range_modifier: f32, pixels: [f32; 2]) -> [i16; 2] {
    let axis = |i: usize| {
        let cap = range_modifier * HALF_VIEW_PX[i];
        (range_modifier * pixels[i] * MOUSE_POINTER_OFFSET_MULT)
            .round()
            .clamp(-cap, cap) as i16
    };
    [axis(0), axis(1)]
}

/// `do_secondary_zoom` (a right click far away with a scoped gun): zoom in or out. True when
/// the gun has a scope.
pub fn scope_click(
    world: &mut World,
    mob: Entity,
    gun: Entity,
    target_pos: TilePos,
    modifiers: ClickModifiers,
) -> bool {
    let Some(scope) = world.get::<Scope>(gun).copied() else {
        return false;
    };
    if world.get::<Zoomed>(mob).is_some() {
        stop_zooming(world, mob);
        return true;
    }
    let Some(&at) = world.get::<TilePos>(mob) else {
        return true;
    };
    if world.get::<Stat>(mob) != Some(&Stat::Conscious) {
        return true;
    }
    let pixel = modifiers.pixel.unwrap_or([0, 0]);
    let pixels = [
        ((target_pos.x - at.x) * 32 + i32::from(pixel[0])) as f32,
        ((target_pos.y - at.y) * 32 + i32::from(pixel[1])) as f32,
    ];
    let offset = scope_offset(scope.range_modifier, pixels);
    world.entity_mut(mob).insert(Zoomed { gun, at });
    send_scope(world, mob, offset);
    true
}

fn send_scope(world: &mut World, mob: Entity, offset: [i16; 2]) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        world.write_message(SendCameraEffect {
            client,
            effect: CameraEffect::Scope { offset },
        });
    }
}

/// `stop_zooming`: the view back on the user.
pub fn stop_zooming(world: &mut World, mob: Entity) {
    if world.get::<Zoomed>(mob).is_none() {
        return;
    }
    world.entity_mut(mob).remove::<Zoomed>();
    send_scope(world, mob, [0, 0]);
}

/// `on_move`, `COMSIG_ITEM_DROPPED`/`COMSIG_MOB_SWAP_HANDS`, `on_incapacitated`: zoom out.
fn scope_upkeep(world: &mut World) {
    let mut q = world.query::<(Entity, &Zoomed)>();
    let zoomed: Vec<(Entity, Zoomed)> = q.iter(world).map(|(e, z)| (e, *z)).collect();
    for (mob, zoom) in zoomed {
        let still = world.get::<TilePos>(mob) == Some(&zoom.at)
            && world.get::<Inventory>(mob).and_then(Inventory::active_item) == Some(zoom.gun)
            && world.get::<Stat>(mob) == Some(&Stat::Conscious)
            && !crate::status_effects::incapacitation(world, mob).incapacitated;
        if !still {
            stop_zooming(world, mob);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scope_reaches_out_by_its_range_and_stops_at_its_cap() {
        // Five tiles east at range 2: 2 × 160 × 1.1 = 352.
        assert_eq!(scope_offset(2.0, [160.0, 0.0]), [352, 0]);
        // Far off: capped at range × half a 21×15 view.
        assert_eq!(scope_offset(1.5, [1000.0, -1000.0]), [504, -360]);
    }

    #[test]
    fn mounts_follow_tg_guns() {
        let light = |p: &str| LIGHT_MOUNTS.iter().any(|m| is_under(p, m));
        assert!(light("/obj/item/gun/energy/e_gun/mini"));
        assert!(light("/obj/item/gun/ballistic/automatic/pistol/gp9"));
        assert!(!light("/obj/item/gun/ballistic/automatic/pistol"));
        assert!(
            BAYONET_MOUNTS
                .iter()
                .any(|m| is_under("/obj/item/gun/ballistic/rifle/boltaction/mosin", m))
        );
    }
}

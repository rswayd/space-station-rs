//! AI upgrade modules (server), after tg `/obj/item/ai_module/upgrade`
//! (`code/game/objects/items/AI_modules/ai_upgrades.dm`), `/datum/ai_module/power_apc`
//! (`ai_actions/remote_power.dm`) and `/datum/ai_module/malf/upgrade/eavesdrop`.
//!
//! - An upgrade module counts while it sits in a slot of the AI's base rack or of one of that
//!   rack's broadcasters (tg `silicon_linked_to_installed` on install, link and broadcaster
//!   link; `silicon_unlinked_from_installed` on the reverse). Only AIs take upgrades.
//! - Surveillance (`/upgrade/surveillance`): the unlock text and the eye's `relay_speech`
//!   ([`Eavesdrop`]); an upgrade is never taken back.
//! - Power transfer (`/upgrade/power_transfer`): the unlock text and the "remotely power APC"
//!   button; pressed, the AI's next click on an APC it sees gives the APC's cell
//!   `STANDARD_BATTERY_CHARGE` for 50 of its backup battery ("You do not have the battery to
//!   charge an APC!" at 50 or less); a click on anything else answers "not an APC!". Unlinked,
//!   the button goes unless the AI is malfunctioning.
// shortcut: the AI eye here already hears speech its cameras see for every AI
// (`super::ai`), so `Eavesdrop` only records the upgrade; no unlock sound (`rped.ogg` is
// `playsound_local`, which the port lacks); no ranged mouse pointer or enable/disable texts.
// The combat software module (`/obj/item/ai_module/combat`, the malf picker for a non-malf AI)
// is the malf group's (`super::malf_modules` needs a `MalfAi`).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::Span;
use ss13_tgui::server::{UiActed, UiSystems};

use super::ai::{AiClick, AiCore};
use super::ai_power::AiPower;
use super::chassis::tell_ai;
use super::law_rack::{LawRack, LinkedRack};
use super::malf::MalfAi;
use super::malf_modules::{add_button, remove_button};
use crate::actions::{ActionButtons, ActionButtonsUi};
use crate::power::Apc;
use crate::server::PlayerMob;

pub const SURVEILLANCE: &str = "/obj/item/ai_module/upgrade/surveillance";
pub const POWER_TRANSFER: &str = "/obj/item/ai_module/upgrade/power_transfer";
/// The "remotely power APC" button.
pub const POWER_APC: &str = "power_apc";
/// `ai_clicker.battery -= 50`.
pub const POWER_APC_BATTERY: f64 = 50.0;
/// `STANDARD_BATTERY_CHARGE`, joules.
pub const POWER_APC_CHARGE: f64 = 1_000_000.0;

/// An upgrade a module gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Upgrade {
    Surveillance,
    PowerTransfer,
}

impl Upgrade {
    pub fn of(path: &str) -> Option<Self> {
        if is_under(path, SURVEILLANCE) {
            Some(Self::Surveillance)
        } else if is_under(path, POWER_TRANSFER) {
            Some(Self::PowerTransfer)
        } else {
            None
        }
    }

    fn unlock_text(self) -> &'static str {
        match self {
            Self::Surveillance => {
                "Распространения прошивки через OTA завершено! Камеры улучшены: Система \
                 улучшенного наблюдения активна."
            }
            Self::PowerTransfer => "Remote APC power systems online.",
        }
    }
}

/// On an AI: the upgrades its rack's modules give now.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct AiUpgrades(pub Vec<Upgrade>);

/// tg eye `relay_speech`: the AI took the surveillance upgrade.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Eavesdrop;

/// On the AI's eye: the power transfer button is pressed (tg `set_ranged_ability`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PowerApcAiming;

pub struct AiUpgradesPlugin;

impl Plugin for AiUpgradesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                sync_upgrades.run_if(any_with_component::<AiCore>),
                buttons.in_set(UiSystems::Handle),
            ),
        );
    }
}

/// The upgrades the modules of `rack` (a base rack) and its broadcasters give.
pub fn rack_upgrades(world: &World, rack: Entity) -> Vec<Upgrade> {
    let Some(r) = world.get::<LawRack>(rack) else {
        return Vec::new();
    };
    let mut racks = vec![rack];
    racks.extend(r.linked_racks.iter().copied());
    let mut out = Vec::new();
    for rack in racks {
        let Some(r) = world.get::<LawRack>(rack) else {
            continue;
        };
        for slot in r.slots.iter().flatten() {
            let upgrade = world
                .get::<DmItem>(slot.module)
                .and_then(|i| Upgrade::of(&i.path));
            if let Some(u) = upgrade
                && !out.contains(&u)
            {
                out.push(u);
            }
        }
    }
    out
}

/// What changed between the old and the new upgrades: (gained, lost).
pub fn diff(old: &[Upgrade], new: &[Upgrade]) -> (Vec<Upgrade>, Vec<Upgrade>) {
    let gained = new.iter().copied().filter(|u| !old.contains(u)).collect();
    let lost = old.iter().copied().filter(|u| !new.contains(u)).collect();
    (gained, lost)
}

/// tg `silicon_linked_to_installed` / `silicon_unlinked_from_installed`, reconciled each frame
/// against the AI's rack.
// Cost: per frame, the round's AIs (one or two), a few slots each.
fn sync_upgrades(world: &mut World) {
    let mut q = world.query::<(Entity, Option<&LinkedRack>, Option<&AiUpgrades>, &AiCore)>();
    let ais: Vec<(Entity, Option<Entity>, Vec<Upgrade>)> = q
        .iter(world)
        .map(|(e, rack, ups, _)| {
            (
                e,
                rack.map(|r| r.0),
                ups.map(|u| u.0.clone()).unwrap_or_default(),
            )
        })
        .collect();
    for (ai, rack, old) in ais {
        let new = rack.map(|r| rack_upgrades(world, r)).unwrap_or_default();
        if new == old {
            continue;
        }
        let (gained, lost) = diff(&old, &new);
        world.entity_mut(ai).insert(AiUpgrades(new));
        for upgrade in gained {
            linked(world, ai, upgrade);
        }
        for upgrade in lost {
            unlinked(world, ai, upgrade);
        }
    }
}

fn linked(world: &mut World, ai: Entity, upgrade: Upgrade) {
    tell_ai(world, ai, Span::Notice, upgrade.unlock_text());
    match upgrade {
        Upgrade::Surveillance => {
            world.entity_mut(ai).insert(Eavesdrop);
        }
        Upgrade::PowerTransfer => {
            if let Some(eye) = world.get::<AiCore>(ai).map(|c| c.eye) {
                add_button(
                    world,
                    eye,
                    POWER_APC,
                    "remotely power APC",
                    ("icons/obj/machines/wallmounts.dmi", "apc0"),
                );
            }
        }
    }
    info!("the AI {ai} got the upgrade {upgrade:?}");
}

fn unlinked(world: &mut World, ai: Entity, upgrade: Upgrade) {
    // tg: an upgrade cannot be taken back; a malf AI keeps its hard earned ability.
    if upgrade == Upgrade::Surveillance || world.get::<MalfAi>(ai).is_some() {
        return;
    }
    if let Some(eye) = world.get::<AiCore>(ai).map(|c| c.eye) {
        remove_button(world, eye, POWER_APC);
        world.entity_mut(eye).remove::<PowerApcAiming>();
    }
}

/// The power transfer button toggles the aiming (tg ranged ability).
fn buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if id != POWER_APC
            || actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || !world
                .get::<ActionButtons>(actor)
                .is_some_and(|b| b.0.iter().any(|(b, _)| b.id == POWER_APC))
        {
            continue;
        }
        let Some(ai) = world.get::<super::ai::AiEye>(actor).map(|e| e.ai) else {
            continue;
        };
        if world.get::<PowerApcAiming>(actor).is_some() {
            world.entity_mut(actor).remove::<PowerApcAiming>();
            tell_ai(
                world,
                ai,
                Span::Notice,
                "You stop focusing on powering APCs.",
            );
        } else {
            world.entity_mut(actor).insert(PowerApcAiming);
            tell_ai(
                world,
                ai,
                Span::Notice,
                "You prepare to power any APC you see.",
            );
        }
    }
}

/// tg `/datum/action/innate/ai/ranged/power_apc/do_ability`, from the AI's click handling: true
/// when the click was taken by the aiming (the click does nothing else).
pub fn intercept_click(world: &mut World, click: &AiClick, ai: Entity) -> bool {
    if world.get::<PowerApcAiming>(click.eye).is_none() {
        return false;
    }
    if world.get::<Apc>(click.target).is_none() {
        tell_ai(world, ai, Span::Warning, "not an APC!");
        return true;
    }
    let battery = world.get::<AiPower>(ai).map_or(0.0, |p| p.battery);
    if battery - POWER_APC_BATTERY <= 0.0 {
        tell_ai(
            world,
            ai,
            Span::Warning,
            "You do not have the battery to charge an APC!",
        );
        return true;
    }
    if let Some(mut apc) = world.get_mut::<Apc>(click.target)
        && let Some(cell) = apc.cell.as_mut()
    {
        cell.give(POWER_APC_CHARGE);
    }
    if let Some(mut power) = world.get_mut::<AiPower>(ai) {
        power.battery -= POWER_APC_BATTERY;
    }
    // tg `unset_after_click`.
    world.entity_mut(click.eye).remove::<PowerApcAiming>();
    info!("the AI {ai} remotely powered the APC {}", click.target);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_paths_give_their_upgrades() {
        assert_eq!(Upgrade::of(SURVEILLANCE), Some(Upgrade::Surveillance));
        assert_eq!(Upgrade::of(POWER_TRANSFER), Some(Upgrade::PowerTransfer));
        assert_eq!(Upgrade::of("/obj/item/ai_module/upgrade"), None);
        assert_eq!(
            Upgrade::of("/obj/item/ai_module/law/supplied/freeform"),
            None
        );
    }

    #[test]
    fn diff_finds_gained_and_lost() {
        use Upgrade::*;
        assert_eq!(
            diff(&[Surveillance], &[PowerTransfer]),
            (vec![PowerTransfer], vec![Surveillance])
        );
        assert_eq!(diff(&[], &[]), (vec![], vec![]));
    }

    /// An AI linked to a rack with a power transfer module gets the button; pressed, a click on
    /// an APC charges its cell from the AI's battery; the module out, the button goes.
    #[test]
    fn power_transfer_charges_an_apc() {
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        world.init_resource::<Messages<ss13_tgui::server::OpenUi>>();
        let module = world
            .spawn(DmItem {
                path: POWER_TRANSFER.into(),
                ..default()
            })
            .id();
        let mut rack = LawRack::new(super::super::law_rack::RackKind::Base, 3, true);
        rack.slots[1] = Some(super::super::law_rack::Slot {
            module,
            name: "power transfer upgrade".into(),
            security: super::super::law_rack::Security::Unsecured,
        });
        let rack = world.spawn(rack).id();
        let eye = world.spawn_empty().id();
        let ai = world
            .spawn((
                AiCore {
                    eye,
                    control_disabled: false,
                    connected_robots: Vec::new(),
                },
                AiPower::default(),
                LinkedRack(rack),
            ))
            .id();
        world
            .entity_mut(eye)
            .insert(super::super::ai::AiEye { ai, blind: false });
        sync_upgrades(&mut world);
        assert_eq!(
            world.get::<AiUpgrades>(ai),
            Some(&AiUpgrades(vec![Upgrade::PowerTransfer]))
        );
        assert!(
            world
                .get::<ActionButtons>(eye)
                .is_some_and(|b| b.0.iter().any(|(b, _)| b.id == POWER_APC))
        );
        let apc = world
            .spawn(Apc::new(
                Entity::PLACEHOLDER,
                Some(crate::power::model::Cell {
                    charge: 0.0,
                    maxcharge: 2_500_000.0,
                    chargerate: 0.0,
                }),
            ))
            .id();
        let click = AiClick {
            client: eye,
            eye,
            target: apc,
            target_pos: ss13_shared::TilePos::new(1, 1, 1),
            modifiers: default(),
        };
        // Not aiming: the click is the AI's usual one.
        assert!(!intercept_click(&mut world, &click, ai));
        world.entity_mut(eye).insert(PowerApcAiming);
        assert!(intercept_click(&mut world, &click, ai));
        let charge = world.get::<Apc>(apc).unwrap().cell.as_ref().unwrap().charge;
        assert_eq!(charge, POWER_APC_CHARGE);
        assert_eq!(world.get::<AiPower>(ai).unwrap().battery, 150.0);
        assert!(world.get::<PowerApcAiming>(eye).is_none());
        // The module out of the rack: the button goes.
        world.get_mut::<LawRack>(rack).unwrap().slots[1] = None;
        sync_upgrades(&mut world);
        assert!(
            world
                .get::<ActionButtons>(eye)
                .is_none_or(|b| b.0.iter().all(|(b, _)| b.id != POWER_APC))
        );
    }
}

//! Cable coils (`/obj/item/stack/cable_coil`, `code/modules/power/cable.dm`): laying cable on
//! plating, the layer menu (`attack_self`, tg's radial as a list), cable restraints, repairing
//! burns on a robotic limb; wirecutters and a multitool on a laid cable.
//!
//! Laying goes through the RWD's `lay_cable` (the power grid and the neighbours' sprites).
// shortcut: restraints are always red (tg: the coil's colour), no multilayer and multi-z hubs
// from the menu (a hub needs its own layer radial), no
// shock when laying on a live net or when cutting (`shock`), a repair does one burn step per
// click (tg loops while it can), no suit sensor repair, no custom coil colours beyond the
// three layers.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmCable, DmItem, Turf};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::list_input::{ListInputData, ListInputUi};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use crate::click_handlers::{AddClickHandler, ClickCtx, ClickPhase};
use crate::construction::Job;
use crate::devices::rhd;
use crate::health::{Health, ZoneSelected};
use crate::inventory::CarriedBy;
use crate::recycling::RecyclingJob;
use crate::tools::{Actor, ToolKind, consume, do_after, tell, tool_of, use_stack};
use crate::zone::BodyZone;

const COIL: &str = "/obj/item/stack/cable_coil";
const CUFFS: &str = "/obj/item/restraints/handcuffs/cable";
/// `CABLE_RESTRAINTS_COST`.
const RESTRAINTS_COST: u32 = 15;
/// `burn_heal` of `item_heal`.
const BURN_HEAL: f32 = 15.0;
/// `do_after` of a repair on oneself (on others tg waits 0).
const SELF_REPAIR_DS: f32 = 50.0;
/// `UNDERFLOOR_INTERACTABLE`.
const UNDERFLOOR_INTERACTABLE: u8 = 2;
/// `/obj/structure/cable` `max_integrity`.
const CABLE_MAX_INTEGRITY: f32 = 50.0;

const MENU: [(&str, u8, [u8; 4]); 3] = [
    ("Слой 1", 1, [0xFF, 0x00, 0x00, 0xFF]),
    ("Слой 2", 2, [0xFF, 0xFF, 0x00, 0xFF]),
    ("Слой 3", 4, [0x19, 0x19, 0xC8, 0xFF]),
];
const RESTRAINTS_ENTRY: &str = "Наручники из кабеля";

/// The layer a coil lays (`target_layer`); no component: layer 2.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoilLayer(pub u8);

/// On the mob: the coil whose menu is open.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoilMenu {
    coil: Entity,
}

pub struct CoilMenuWindow;

impl UiInterface for CoilMenuWindow {
    const NAME: &'static str = ListInputUi::NAME;
    type Data = ListInputData;
    type StaticData = ();
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoilAct {
    Pick(String),
    Cancel,
}

impl ServerUi for CoilMenuWindow {
    type Source = &'static CoilMenu;
    type UserParam = ();
    type Act = CoilAct;

    fn static_data(_: &CoilMenu) {}

    fn data(_: &CoilMenu, _: Entity, _: &()) -> ListInputData {
        let mut items: Vec<String> = MENU.iter().map(|(n, ..)| (*n).to_owned()).collect();
        items.push(RESTRAINTS_ENTRY.to_owned());
        ListInputData {
            title: "Моток кабеля".into(),
            message: String::new(),
            init_value: items[1].clone(),
            items,
            timeout: None,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<CoilAct, UiReject> {
        match action {
            "submit" => params
                .get("entry")
                .map(|e| CoilAct::Pick(e.to_owned()))
                .ok_or_else(|| UiReject::BadParams("entry: expected an item".into())),
            "cancel" => Ok(CoilAct::Cancel),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn is_coil(world: &World, item: Entity) -> bool {
    world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, COIL))
}

pub struct CableCoilPlugin;

impl Plugin for CableCoilPlugin {
    fn build(&self, app: &mut App) {
        app.add_click_handler(ClickPhase::AttackSelf, 3295, attack_self);
        app.add_click_handler(ClickPhase::HeldOnTarget, 3995, held_on_target);
        app.register_ui::<CoilMenuWindow>()
            .add_systems(Update, apply_acts.in_set(UiSystems::Handle));
    }
}

fn attack_self(world: &mut World, c: &ClickCtx) -> bool {
    if !is_coil(world, c.target) {
        return false;
    }
    world.entity_mut(c.mob).insert(CoilMenu { coil: c.target });
    world.write_message(OpenUi::new::<CoilMenuWindow>(
        c.client,
        c.mob,
        c.mob,
        "Моток кабеля",
    ));
    true
}

fn apply_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<CoilMenuWindow>>>) {
    let picked: Vec<(Entity, Entity, CoilAct)> = acts
        .read(world.resource::<Messages<UiActed<CoilMenuWindow>>>())
        .filter(|a| a.actor == a.source)
        .map(|a| (a.client, a.actor, a.act.clone()))
        .collect();
    for (client, mob, act) in picked {
        let Some(menu) = world.get::<CoilMenu>(mob).copied() else {
            continue;
        };
        world.entity_mut(mob).remove::<CoilMenu>();
        let CoilAct::Pick(entry) = act else {
            continue;
        };
        // `check_menu`: still the mob's coil in hand.
        if world.get::<CarriedBy>(menu.coil).map(|c| c.0) != Some(mob) {
            continue;
        }
        if entry == RESTRAINTS_ENTRY {
            make_restraints(world, client, mob, menu.coil);
        } else if let Some(&(_, layer, color)) = MENU.iter().find(|(n, ..)| *n == entry) {
            world.entity_mut(menu.coil).insert(CoilLayer(layer));
            if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(menu.coil) {
                look.color = Some(color);
            }
            tell(world, client, Span::Notice, format!("Кабель: {entry}."));
        }
    }
}

fn make_restraints(world: &mut World, client: Entity, mob: Entity, coil: Entity) {
    if crate::tools::stack_amount(world, coil) < RESTRAINTS_COST {
        tell(world, client, Span::Warning, "Нужно как минимум 15 кабеля!");
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    use_stack(world, coil, RESTRAINTS_COST);
    // `user.put_in_hands(restraints)`.
    if let Some(cuffs) = crate::construction::spawn_obj(world, CUFFS, pos)
        && let Some(inventory) = world.get::<crate::inventory::Inventory>(mob).cloned()
    {
        crate::storage::put_in_hands(world, mob, &inventory, cuffs);
    }
}

fn held_on_target(world: &mut World, c: &ClickCtx) -> bool {
    let Some(held) = c.held else {
        return false;
    };
    let actor = Actor {
        client: c.client,
        mob: c.mob,
        item: Some(held),
    };
    if world.get::<Turf>(c.target).is_some() && is_coil(world, held) {
        return lay(world, actor, held, c.pos);
    }
    if world.get::<DmCable>(c.target).is_some_and(|d| d.layer != 0)
        && let Some((kind, _)) = tool_of(world, held)
        && reachable(world, c.pos)
    {
        match kind {
            ToolKind::Wirecutters => {
                // `/obj/structure/cable/wirecutter_act`: `if(shock(user, 50)) return`.
                if world
                    .resource_mut::<crate::combat::CombatRng>()
                    .chance(50.0)
                    && crate::wires::electrocute_from_cable(world, c.mob, c.target, c.target)
                {
                    return true;
                }
                return cut(world, c.client, c.target, c.pos);
            }
            ToolKind::Multitool => return power_info(world, c.client, c.target),
            _ => {}
        }
    }
    if is_coil(world, held)
        && !world
            .get::<crate::combat::CombatMode>(c.mob)
            .is_some_and(|m| m.0)
        && world.get::<Health>(c.target).is_some()
    {
        return repair(world, actor, c.target);
    }
    false
}

fn reachable(world: &World, pos: TilePos) -> bool {
    ss13_mapload::undertile::accessibility(world, pos) >= UNDERFLOOR_INTERACTABLE
}

/// `place_turf`.
fn lay(world: &mut World, actor: Actor, coil: Entity, pos: TilePos) -> bool {
    let layer = world.get::<CoilLayer>(coil).map_or(2, |l| l.0);
    if !rhd::can_lay_cable(world, pos) || !reachable(world, pos) {
        tell(
            world,
            actor.client,
            Span::Warning,
            "Кабель можно проложить только на покрытии и мостках!",
        );
        return true;
    }
    if !rhd::cable_allowed_here(world, pos, layer) {
        tell(world, actor.client, Span::Warning, "Здесь уже есть кабель!");
        return true;
    }
    if rhd::lay_cable(world, pos, layer).is_some() {
        use_stack(world, coil, 1);
    }
    true
}

/// `wirecutter_act`: the cable is cut and one length drops.
fn cut(world: &mut World, client: Entity, cable: Entity, pos: TilePos) -> bool {
    if let Some(mut grid) = world.get_resource_mut::<crate::power::grid::Grid>() {
        grid.remove_cable(cable);
    }
    world.despawn(cable);
    crate::construction::spawn_stack(world, COIL, pos, 1);
    rhd::relink_around(world, pos);
    tell(world, client, Span::Notice, "Вы перерезаете кабель.");
    true
}

/// `/obj/structure/cable` `ex_act` (`take_damage(BOMB)`): a quarter of the damage under a floor
/// tile (`run_atom_armor` with `TRAIT_UNDERFLOOR`); destroyed (`atom_destruction(BOMB)`), it
/// makes the lights of its powernet flicker and drops one length. False when `obj` is not a
/// laid cable.
pub fn ex_act(
    world: &mut World,
    obj: Entity,
    dm: &ss13_mapload::DmObj,
    severity: crate::explosions::Severity,
) -> bool {
    let Some(net) = world
        .get_resource::<crate::power::grid::Grid>()
        .and_then(|g| g.cable_net(obj))
    else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(obj) else {
        return true;
    };
    let mut damage = crate::explosions::ex_act::through_armor(
        crate::explosions::ex_act::bomb_damage(world, severity),
        dm.armor_bomb,
    );
    if ss13_mapload::undertile::accessibility(world, pos) < UNDERFLOOR_INTERACTABLE {
        damage *= 0.25;
    }
    if damage < crate::obj_damage::DAMAGE_PRECISION {
        return true;
    }
    let mut integrity = world
        .get::<crate::obj_damage::Integrity>(obj)
        .copied()
        .unwrap_or(crate::obj_damage::Integrity {
            current: if dm.max_integrity > 0.0 {
                dm.max_integrity
            } else {
                CABLE_MAX_INTEGRITY
            },
            broken: false,
        });
    integrity.current -= damage;
    if integrity.current > 0.0 {
        world.entity_mut(obj).insert(integrity);
        return true;
    }
    super::lighting::propagate_light_flicker(
        world,
        net,
        Some(pos),
        super::lighting::FLICKER_FALLOFF,
    );
    if let Some(mut grid) = world.get_resource_mut::<crate::power::grid::Grid>() {
        grid.remove_cable(obj);
    }
    world.despawn(obj);
    crate::construction::spawn_stack(world, COIL, pos, 1);
    rhd::relink_around(world, pos);
    true
}

/// `multitool_act`.
fn power_info(world: &mut World, client: Entity, cable: Entity) -> bool {
    let text = world
        .get_resource::<crate::power::grid::Grid>()
        .and_then(|g| g.cable_net(cable).and_then(|n| g.net(n)))
        .filter(|n| n.avail > 0.0)
        .map_or_else(
            || "Кабель не под напряжением.".to_owned(),
            |n| {
                format!(
                    "Мощность сети: {:.0} Вт. Нагрузка: {:.0} Вт. Избыток: {:.0} Вт.",
                    n.avail,
                    n.load,
                    n.surplus()
                )
            },
        );
    tell(world, client, Span::Danger, text);
    true
}

/// `try_heal_loop`: a robotic limb's burns.
fn repair(world: &mut World, actor: Actor, patient: Entity) -> bool {
    let Some(zone) = world
        .get::<ZoneSelected>(actor.mob)
        .map(|z| z.0.check_zone())
    else {
        return false;
    };
    if !world
        .get::<crate::medical::wounds::Limbs>(patient)
        .is_some_and(|l| l.limb(zone).robotic)
    {
        return false;
    }
    if !world
        .get::<Health>(patient)
        .is_some_and(|h| h.part(zone).burn > 0.0)
    {
        tell(
            world,
            actor.client,
            Span::Warning,
            "Конечность не повреждена.",
        );
        return true;
    }
    let delay = if actor.mob == patient {
        SELF_REPAIR_DS
    } else {
        0.0
    };
    do_after(
        world,
        actor,
        patient,
        delay,
        0,
        Job::Recycling(RecyclingJob::CoilHeal { zone }),
    );
    true
}

/// `item_heal` after the `do_after`: one length mends 15 burn.
pub fn complete_heal(world: &mut World, action: &crate::tools::DoAfter, zone: BodyZone) {
    let Some(coil) = action.item else {
        return;
    };
    if !is_coil(world, coil) || !consume(world, coil, 1) {
        return;
    }
    if let Some(mut health) = world.get_mut::<Health>(action.target) {
        health.heal_zone(zone, 0.0, BURN_HEAL);
    }
}

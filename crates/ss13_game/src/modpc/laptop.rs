//! Laptops after tg `/obj/item/modular_computer/laptop` (`laptop.dm`, `laptop_presets.dm`).
//!
//! A laptop is a carried modular computer that opens and closes (`screen_on`): it starts open
//! (`start_open`, a buildable one closed). Used in hand, or right-clicked where it lies, a
//! closed laptop opens (`try_toggle_open`: on the floor or in hands, not in a bag); an open one
//! works like any computer (`interact`). Alt-click closes it. A closed laptop's window closes
//! (`ui_status`: `!screen_on`) and it draws its idle power (`crate::modpc::power`). It keeps
//! three programs in the background and runs the software hub plus its preset's programs.
// shortcut: no open slowdown or bulky weight class (`slowdown_open`, `w_class_open`), no
// screen overlays (`icon_state_menu`), no "Toggle Open" verb or drag pickup.

use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmItem, DmName};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::server::AlwaysState;

use super::ntnet::LAPTOP_ROOT;
use super::pda::ModularComputer;
use super::programs::ProgramKind;
use crate::inventory::CarriedBy;

/// `/obj/item/modular_computer/laptop/buildable`: starts closed.
const BUILDABLE: &str = "/obj/item/modular_computer/laptop/buildable";
/// `/obj/item/modular_computer/laptop/preset/civilian`.
const CIVILIAN: &str = "/obj/item/modular_computer/laptop/preset/civilian";
const LAPTOP_DMI: &str = "icons/obj/devices/modular_laptop.dmi";

/// A laptop's lid: `screen_on`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Laptop {
    pub open: bool,
}

pub fn is_laptop(path: &str) -> bool {
    is_under(path, LAPTOP_ROOT)
}

/// `default_programs` + `starting_programs` of a laptop of type `path`.
pub fn laptop_programs(path: &str) -> Vec<ProgramKind> {
    let mut programs = vec![ProgramKind::Downloader];
    if is_under(path, CIVILIAN) {
        programs.push(ProgramKind::ChatClient);
    }
    programs
}

/// `Initialize`: the computer, open unless buildable.
fn attach(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let entity = add.entity;
    if !items.get(entity).is_ok_and(|i| is_laptop(&i.path)) {
        return;
    }
    commands.queue(move |world: &mut World| {
        let Ok(e) = world.get_entity(entity) else {
            return;
        };
        if e.contains::<ModularComputer>() {
            return;
        }
        let path = e
            .get::<DmItem>()
            .map(|i| i.path.clone())
            .unwrap_or_default();
        let name = e
            .get::<DmName>()
            .map(|n| n.name.clone())
            .unwrap_or_default();
        let open = !is_under(&path, BUILDABLE);
        world.entity_mut(entity).insert((
            ModularComputer::console(&path, &name, laptop_programs(&path)),
            Laptop { open },
            AlwaysState,
        ));
    });
}

/// `try_toggle_open` + `toggle_open`: only on a tile or in someone's hands.
pub fn toggle_open(world: &mut World, client: Entity, laptop: Entity) -> bool {
    let Some(&Laptop { open }) = world.get::<Laptop>(laptop) else {
        return false;
    };
    let reachable =
        world.get::<TilePos>(laptop).is_some() || world.get::<CarriedBy>(laptop).is_some();
    if !reachable {
        return true;
    }
    let name = world
        .get::<DmName>(laptop)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    world.write_message(ss13_net::chat::server::SendChat {
        clients: vec![client],
        message: ss13_shared::chat::ChatMessage::simple(
            ss13_shared::chat::ChatChannel::System,
            Span::Notice,
            if open {
                format!("Вы закрываете {name}.")
            } else {
                format!("Вы открываете {name}.")
            },
        ),
    });
    world.entity_mut(laptop).insert(Laptop { open: !open });
    true
}

/// `attack_self`: a closed laptop opens; an open one goes on to `interact`
/// (`crate::modpc::pda::attack_self`).
pub fn attack_self(world: &mut World, client: Entity, laptop: Entity) -> bool {
    match world.get::<Laptop>(laptop) {
        Some(l) if !l.open => toggle_open(world, client, laptop),
        _ => false,
    }
}

/// `attack_hand_secondary`: `attack_self` of a laptop where it lies.
pub fn attack_hand_secondary(
    world: &mut World,
    client: Entity,
    mob: Entity,
    laptop: Entity,
) -> bool {
    if world.get::<Laptop>(laptop).is_none() || world.get::<TilePos>(laptop).is_none() {
        return false;
    }
    attack_self(world, client, laptop) || super::pda::attack_self(world, client, mob, laptop)
}

/// `click_alt`: an open laptop closes; a closed one blocks the click. False for anything else.
pub fn alt_click(world: &mut World, mob: Entity, laptop: Entity) -> bool {
    let Some(l) = world.get::<Laptop>(laptop) else {
        return false;
    };
    if l.open
        && let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client)
    {
        toggle_open(world, client, laptop);
    }
    true
}

/// `update_icon_state`: closed, or open powered or off.
// Cost: the laptops whose lid or computer changed this tick (MetaStation maps two).
fn looks(
    mut laptops: Query<
        (&Laptop, &ModularComputer, &mut Appearance),
        Or<(Changed<Laptop>, Changed<ModularComputer>)>,
    >,
    icons: Option<Res<IconIndex>>,
) {
    let Some(icons) = icons else {
        return;
    };
    for (laptop, computer, mut appearance) in &mut laptops {
        let state = match (laptop.open, computer.enabled) {
            (false, _) => "laptop-closed",
            (true, true) => "laptop",
            (true, false) => "laptop-off",
        };
        if let Some(id) = icons.get(LAPTOP_DMI, state)
            && appearance.icon_state != id
        {
            appearance.icon_state = id;
        }
    }
}

pub fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // A closed laptop in hand opens before it turns on.
    app.add_click_handler(ClickPhase::AttackSelf, 2593, |w, c| {
        crate::modpc::laptop::attack_self(w, c.client, c.target)
    });
    // Right click on a laptop where it lies: open it, or use it.
    app.add_click_handler(ClickPhase::EmptyHand, 2403, |w, c| {
        c.modifiers.right
            && crate::modpc::laptop::attack_hand_secondary(w, c.client, c.mob, c.target)
    });
    app.add_observer(attach).add_systems(Update, looks);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_laptop_opens_and_closes_and_runs_its_preset() {
        assert_eq!(
            laptop_programs(CIVILIAN),
            [ProgramKind::Downloader, ProgramKind::ChatClient]
        );
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        let client = world.spawn_empty().id();
        let laptop = world
            .spawn((
                ModularComputer::console(CIVILIAN, "laptop", laptop_programs(CIVILIAN)),
                Laptop { open: false },
            ))
            .id();
        // In a bag (neither on a tile nor in hands): it stays shut.
        assert!(attack_self(&mut world, client, laptop));
        assert!(!world.get::<Laptop>(laptop).unwrap().open);
        world.entity_mut(laptop).insert(TilePos::new(1, 1, 1));
        assert!(attack_self(&mut world, client, laptop));
        assert!(world.get::<Laptop>(laptop).unwrap().open);
        // Open: the click goes on to `interact`.
        assert!(!attack_self(&mut world, client, laptop));
        assert!(toggle_open(&mut world, client, laptop));
        assert!(!world.get::<Laptop>(laptop).unwrap().open);
        // Three background programs plus the one in front.
        let mut pc = world.get::<ModularComputer>(laptop).unwrap().clone();
        assert_eq!(pc.max_idle_programs(), 3);
        assert!(pc.open_program(ProgramKind::Downloader));
        assert!(pc.open_program(ProgramKind::ChatClient));
        // A closed laptop draws its idle power.
        assert_eq!(
            super::super::power::power_usage(&pc, false),
            1.0 + 2.0 + 1.0
        );
    }
}

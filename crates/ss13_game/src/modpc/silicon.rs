//! Silicon PDAs after tg `/obj/item/modular_computer/pda/silicon` and
//! `/mob/living/silicon/proc/create_modularInterface`.
//!
//! Every AI, cyborg and pAI has a built-in computer ([`ModularInterface`]): not an item, never
//! dropped, imprinted with the silicon's name and "AI", "Cyborg" or "pAI Messenger". The AI's
//! and pAI's run the messenger (the pAI's the chat client too), a cyborg's the file manager,
//! RoboTact (`programs::robotact`, with the journal of [`logevent`]), the gas scanner and the
//! crew list. It needs no power (`check_power_override`); a cyborg's has no
//! NTNet while locked down or with a dead cell, and its signal is the silicon's tile's. The AI
//! opens it with its "Messenger" button, a cyborg with "Modular Interface"
//! (`crate::silicon::ai_hud`), a pAI with its "Digital Messenger" software
//! (`crate::silicon::pai`); the window follows the silicon (`crate::modpc::pda::ntos_status`).
// shortcut: no lamp button and no cyborg tablet screen
// object; it is not renamed when the silicon is.

use bevy::prelude::*;
use ss13_mapload::DmName;
use ss13_tgui::server::{AlwaysState, OpenUi};

use super::NtosUi;
use super::pda::ModularComputer;
use super::power::ComputerCell;
use super::programs::messenger::Messenger;
use super::programs::{ProgramKind, StationProgram};
use crate::silicon::ai::{AiCore, AiEye};
use crate::silicon::cyborg::Cyborg;
use crate::silicon::pai::Pai;

pub const SILICON_PDA: &str = "/obj/item/modular_computer/pda/silicon";
pub const CYBORG_PDA: &str = "/obj/item/modular_computer/pda/silicon/cyborg";
pub const PAI_PDA: &str = "/obj/item/modular_computer/pda/silicon/pai";

/// On a silicon's built-in computer: `silicon_owner`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SiliconPda {
    pub owner: Entity,
}

/// On a cyborg's built-in computer: tg `borglog`, the journal RoboTact shows.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct BorgLog(pub Vec<String>);

/// tg `logevent`: "[round time] - text" in a living cyborg's journal (its computer is made if
/// it has none yet).
pub fn logevent(world: &mut World, silicon: Entity, text: &str) {
    if text.is_empty()
        || world.get::<Cyborg>(silicon).is_none()
        || world.get::<crate::health::Stat>(silicon) == Some(&crate::health::Stat::Dead)
    {
        return;
    }
    let Some(pc) = create(world, silicon, CYBORG_PDA, "Cyborg") else {
        return;
    };
    let stamp = if world.contains_resource::<Time>() {
        crate::records::manifest::round_timestamp(world)
    } else {
        "00:00:00".to_owned()
    };
    let line = format!("{stamp} - {text}");
    match world.get_mut::<BorgLog>(pc) {
        Some(mut log) => log.0.push(line),
        None => {
            world.entity_mut(pc).insert(BorgLog(vec![line]));
        }
    }
}

/// On a silicon: its `modularInterface`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModularInterface(pub Entity);

/// `starting_programs` (`has_pda_programs = FALSE`: nothing else).
pub fn silicon_programs(path: &str) -> Vec<ProgramKind> {
    use ProgramKind::*;
    match path {
        CYBORG_PDA => vec![
            FileManager,
            Station(StationProgram::Robotact),
            Atmosscan,
            CrewManifest,
        ],
        PAI_PDA => vec![Messenger, ChatClient],
        _ => vec![Messenger],
    }
}

/// `create_modularInterface`: once per silicon.
pub fn create(world: &mut World, silicon: Entity, path: &str, job: &str) -> Option<Entity> {
    if let Some(&ModularInterface(pc)) = world.get::<ModularInterface>(silicon) {
        return Some(pc);
    }
    world.get_entity(silicon).ok()?;
    let name = world
        .get::<DmName>(silicon)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    let mut computer = ModularComputer::pda(path, "modular interface");
    computer.programs = silicon_programs(path);
    computer.has_light = false;
    let pc = world
        .spawn((
            computer,
            Messenger::default(),
            // No cell: the silicon powers it (`check_power_override`).
            ComputerCell(None),
            SiliconPda { owner: silicon },
            AlwaysState,
        ))
        .id();
    world.entity_mut(silicon).insert(ModularInterface(pc));
    super::pda::imprint_id(world, pc, Some(&name), Some(job));
    Some(pc)
}

/// `get_ntnet_status` override: false (no signal) for a cyborg locked down or with an empty
/// cell, true when the base status decides.
pub fn has_network(world: &World, owner: Entity) -> bool {
    match world.get::<Cyborg>(owner) {
        Some(borg) => {
            borg.charge > 0.0
                && world
                    .get::<crate::silicon::chassis::LockedDown>(owner)
                    .is_none()
        }
        None => true,
    }
}

/// The silicon owning the built-in computer `pc`, if it is one.
pub fn owner(world: &World, pc: Entity) -> Option<Entity> {
    world.get::<SiliconPda>(pc).map(|s| s.owner)
}

/// `modularInterface.interact(user)` from `actor` (the silicon, or the AI's eye): turns it on
/// and opens its window. False without one.
pub fn interact(world: &mut World, client: Entity, actor: Entity) -> bool {
    let silicon = world.get::<AiEye>(actor).map_or(actor, |e| e.ai);
    let Some(&ModularInterface(pc)) = world.get::<ModularInterface>(silicon) else {
        return false;
    };
    // `turn_on`: a dead silicon's does not start.
    if world.get::<crate::health::Stat>(silicon) == Some(&crate::health::Stat::Dead) {
        return true;
    }
    let title = match world.get_mut::<ModularComputer>(pc) {
        Some(mut c) => {
            c.enabled = true;
            c.display_name()
        }
        None => return false,
    };
    world.write_message(OpenUi::new::<NtosUi>(client, actor, pc, title));
    true
}

/// The interface goes with its silicon.
fn remove_interface(
    remove: On<Remove<ModularInterface>>,
    interfaces: Query<&ModularInterface>,
    mut commands: Commands,
) {
    if let Ok(&ModularInterface(pc)) = interfaces.get(remove.entity) {
        commands.entity(pc).try_despawn();
    }
}

fn queue_create(commands: &mut Commands, silicon: Entity, path: &'static str, job: &'static str) {
    commands.queue(move |world: &mut World| {
        create(world, silicon, path, job);
    });
}

fn ai_created(add: On<Add<AiCore>>, mut commands: Commands) {
    queue_create(&mut commands, add.entity, SILICON_PDA, "AI");
}

fn cyborg_created(add: On<Add<Cyborg>>, mut commands: Commands) {
    queue_create(&mut commands, add.entity, CYBORG_PDA, "Cyborg");
}

fn pai_created(add: On<Add<Pai>>, mut commands: Commands) {
    queue_create(&mut commands, add.entity, PAI_PDA, "pAI Messenger");
}

pub fn build(app: &mut App) {
    app.add_observer(ai_created)
        .add_observer(cyborg_created)
        .add_observer(pai_created)
        .add_observer(remove_interface);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logevent_journals_a_living_cyborgs_events() {
        let mut world = World::new();
        let borg = world
            .spawn(Cyborg {
                model: None,
                model_item: None,
                charge: 0.0,
                max_charge: 0.0,
                low_power: false,
                connected_ai: None,
                lawupdate: true,
            })
            .id();
        logevent(&mut world, borg, "System brought online.");
        let pc = world.get::<ModularInterface>(borg).unwrap().0;
        assert_eq!(
            world.get::<BorgLog>(pc).unwrap().0,
            ["00:00:00 - System brought online."]
        );
        world.entity_mut(borg).insert(crate::health::Stat::Dead);
        logevent(&mut world, borg, "late");
        assert_eq!(world.get::<BorgLog>(pc).unwrap().0.len(), 1);
        let ai = world.spawn_empty().id();
        logevent(&mut world, ai, "not a cyborg");
        assert!(world.get::<ModularInterface>(ai).is_none());
    }

    #[test]
    fn a_cyborg_gets_an_imprinted_interface_that_needs_no_power() {
        let mut world = World::new();
        world.init_resource::<Messages<OpenUi>>();
        let borg = world
            .spawn(DmName {
                name: "RoboCop".into(),
                desc: String::new(),
            })
            .id();
        let pc = create(&mut world, borg, CYBORG_PDA, "Cyborg").unwrap();
        assert_eq!(
            create(&mut world, borg, CYBORG_PDA, "Cyborg"),
            Some(pc),
            "once"
        );
        let computer = world.get::<ModularComputer>(pc).unwrap().clone();
        assert_eq!(computer.saved_identification.as_deref(), Some("RoboCop"));
        assert_eq!(computer.saved_job.as_deref(), Some("Cyborg"));
        assert!(!computer.programs.contains(&ProgramKind::Messenger));
        assert_eq!(silicon_programs(SILICON_PDA), [ProgramKind::Messenger]);
        // No cell, yet it turns on and runs (`check_power_override`).
        assert!(super::super::power::can_turn_on(&mut world, pc));
        let client = world.spawn_empty().id();
        assert!(interact(&mut world, client, borg));
        assert!(world.get::<ModularComputer>(pc).unwrap().enabled);
        super::super::power::tick(&mut world, 2.0);
        assert!(world.get::<ModularComputer>(pc).unwrap().enabled);
        assert_eq!(owner(&world, pc), Some(borg));
        assert!(has_network(&world, borg));
    }
}

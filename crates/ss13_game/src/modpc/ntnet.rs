//! NTNet availability after tg `get_ntnet_status()` and `find_functional_ntnet_relay()`.
//!
//! A console is wired (`NTNET_ETHERNET_SIGNAL`). A carried computer needs a working NTNet relay
//! (`/obj/machinery/ntnet_relay` with power) somewhere, then its tile decides: a station level
//! gives good signal (a laptop ethernet), the mining level low signal, anywhere else none unless
//! the device is `long_ranged` (low). Programs that require NTNet (`PROGRAM_REQUIRES_NTNET`) do
//! not start without signal and crash when it is lost (`event_networkfailure`), checked every
//! `SSobj` tick in `crate::modpc::power`.
//!
//! Cost: [`refresh`] reads the relays (one on MetaStation) and the level layout every frame.
// shortcut: a relay works while powered (no `relay_enabled` toggle, DoS overload or breakage:
// the relay machine itself belongs to telecomms), no network logs, and the downloader's speed
// does not follow the signal (`download_netspeed`).

use std::ops::RangeInclusive;

use bevy::prelude::*;
use ss13_mapload::DmPower;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;

use super::pda::ModularComputer;
use super::programs::ProgramKind;
use crate::power::Unpowered;

/// `NTNET_NO_SIGNAL` .. `NTNET_ETHERNET_SIGNAL`.
pub const NO_SIGNAL: u8 = 0;
pub const LOW_SIGNAL: u8 = 1;
pub const GOOD_SIGNAL: u8 = 2;
pub const ETHERNET_SIGNAL: u8 = 3;

/// `/obj/machinery/ntnet_relay`.
pub const RELAY: &str = "/obj/machinery/ntnet_relay";
/// `/obj/item/modular_computer/laptop` (`PROGRAM_LAPTOP`).
pub const LAPTOP_ROOT: &str = "/obj/item/modular_computer/laptop";
/// The `long_ranged` computers: the nuclear operatives', the curator's and the clear PDA.
const LONG_RANGED: [&str; 3] = [
    "/obj/item/modular_computer/pda/nukeops",
    "/obj/item/modular_computer/pda/crew/curator",
    "/obj/item/modular_computer/pda/clear",
];

/// An NTNet relay.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct NtnetRelay;

/// What `get_ntnet_status` reads besides the computer.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Ntnet {
    /// `find_functional_ntnet_relay()`.
    pub relay_up: bool,
    /// `ZTRAIT_STATION` levels.
    pub station: RangeInclusive<u16>,
    /// `ZTRAIT_MINING`.
    pub mining: Option<u16>,
}

// `station` starts empty (no station level known yet).
#[allow(clippy::reversed_empty_ranges)]
impl Default for Ntnet {
    fn default() -> Self {
        Self {
            relay_up: false,
            station: 1..=0,
            mining: None,
        }
    }
}

impl Ntnet {
    /// `get_ntnet_status()` of a computer of type `path` on `pos` (`None`: nowhere on the map).
    pub fn signal(&self, path: &str, pos: Option<TilePos>) -> u8 {
        if super::power::is_console(path) {
            return ETHERNET_SIGNAL;
        }
        if !self.relay_up {
            return NO_SIGNAL;
        }
        let Some(pos) = pos else {
            return NO_SIGNAL;
        };
        if self.station.contains(&pos.z) {
            if is_under(path, LAPTOP_ROOT) {
                return ETHERNET_SIGNAL;
            }
            return GOOD_SIGNAL;
        }
        if self.mining == Some(pos.z) || LONG_RANGED.iter().any(|r| is_under(path, r)) {
            return LOW_SIGNAL;
        }
        NO_SIGNAL
    }
}

/// `program_flags & PROGRAM_REQUIRES_NTNET`.
pub fn requires_ntnet(program: ProgramKind) -> bool {
    matches!(
        program,
        ProgramKind::CrewManifest
            | ProgramKind::AlarmMonitor
            | ProgramKind::StatusDisplay
            | ProgramKind::Newscaster
            | ProgramKind::Downloader
            | ProgramKind::ChatClient
    )
}

/// `get_ntnet_status()` of the computer `pc`.
pub fn status(world: &World, pc: Entity) -> u8 {
    let Some(computer) = world.get::<ModularComputer>(pc) else {
        return NO_SIGNAL;
    };
    // A silicon's built-in computer: the silicon's tile, unless it cuts the network.
    let pos = match super::silicon::owner(world, pc) {
        Some(owner) if !super::silicon::has_network(world, owner) => return NO_SIGNAL,
        Some(owner) => silicon_turf(world, owner),
        None => super::pda::turf_of(world, pc),
    };
    world
        .get_resource::<Ntnet>()
        .map_or(NO_SIGNAL, |n| n.signal(&computer.path, pos))
}

/// The tile of a silicon: its own, or (an AI in a card or an APC) its holder's.
pub fn silicon_turf(world: &World, silicon: Entity) -> Option<TilePos> {
    super::pda::turf_of(world, silicon).or_else(|| {
        match world.get::<crate::silicon::ai::AiInside>(silicon)? {
            crate::silicon::ai::AiInside::Apc(e)
            | crate::silicon::ai::AiInside::Card(e)
            | crate::silicon::ai::AiInside::Terminal(e)
            | crate::silicon::ai::AiInside::Mod(e) => crate::explosions::turf_of(world, *e),
        }
    })
}

/// `open_program`'s check: false (and the warning to `client`) when `program` needs NTNet the
/// computer does not have.
pub fn can_open(world: &mut World, client: Entity, pc: Entity, program: ProgramKind) -> bool {
    if !requires_ntnet(program) || status(world, pc) != NO_SIGNAL {
        return true;
    }
    let name = world
        .get::<ModularComputer>(pc)
        .map(ModularComputer::display_name)
        .unwrap_or_default();
    world.write_message(ss13_net::chat::server::SendChat {
        clients: vec![client],
        message: ss13_shared::chat::ChatMessage::simple(
            ss13_shared::chat::ChatChannel::System,
            ss13_shared::chat::Span::Danger,
            format!(
                "{name} показывает предупреждение \"Не удаётся подключиться к NTNet. Пожалуйста, \
                 повторите попытку. Если проблема не устраняется, обратитесь к своему системному \
                 администратору.\"."
            ),
        ),
    });
    false
}

/// `process`'s network check: without signal the program in front and every background program
/// that needs NTNet are killed (`event_networkfailure`), with the error on the screen.
pub fn network_check(world: &mut World, pc: Entity) {
    let Some(computer) = world.get::<ModularComputer>(pc).cloned() else {
        return;
    };
    if !computer.enabled {
        return;
    }
    let lost: Vec<(ProgramKind, bool)> = computer
        .active
        .map(|p| (p, false))
        .into_iter()
        .chain(computer.idle.iter().map(|&p| (p, true)))
        .filter(|&(p, _)| requires_ntnet(p))
        .collect();
    if lost.is_empty() || status(world, pc) != NO_SIGNAL {
        return;
    }
    let name = computer.display_name();
    for (program, background) in lost {
        if let Some(mut c) = world.get_mut::<ModularComputer>(pc) {
            c.kill_program(program);
        }
        let text = if background {
            format!(
                "Экран {name} показывает ошибку \"Process {}.PRG (PID {}) terminated - Network \
                 Error\".",
                program.filename(),
                100 + pc.to_bits() % 900
            )
        } else {
            format!(
                "Экран {name} на мгновение зависает и показывает ошибку \"NETWORK ERROR - NTNet \
                 connection lost. Please retry. If problem persists contact your system \
                 administrator.\"."
            )
        };
        if let Some(pos) = super::pda::turf_of(world, pc) {
            for message in crate::pull::visible_message(
                world,
                pos,
                ss13_shared::chat::ChatChannel::Local,
                (ss13_shared::chat::Span::Danger, text),
                &[],
            ) {
                world.write_message(message);
            }
        }
    }
}

/// Keeps [`Ntnet`] current: the relays' power and the level layout.
pub fn refresh(world: &mut World) {
    let mut relays = world.query_filtered::<Has<Unpowered>, With<NtnetRelay>>();
    let relay_up = relays.iter(world).any(|unpowered| !unpowered);
    let next = Ntnet {
        relay_up,
        station: crate::events::controller::station_levels(world),
        mining: ss13_mapload::mining_z(world),
    };
    if world.get_resource::<Ntnet>() != Some(&next) {
        world.insert_resource(next);
    }
}

fn mark_relay(add: On<Add<DmPower>>, power: Query<&DmPower>, mut commands: Commands) {
    if power
        .get(add.entity)
        .is_ok_and(|p| is_under(&p.path, RELAY))
    {
        commands.entity(add.entity).insert_if_new(NtnetRelay);
    }
}

pub fn build(app: &mut App) {
    app.init_resource::<Ntnet>()
        .add_observer(mark_relay)
        .add_systems(Update, refresh);
}

#[cfg(test)]
mod tests {
    use super::*;

    const PDA: &str = "/obj/item/modular_computer/pda/crew";

    #[test]
    fn signal_follows_relays_and_levels() {
        let mut net = Ntnet {
            relay_up: true,
            station: 2..=3,
            mining: Some(5),
        };
        let at = |z| Some(TilePos::new(1, 1, z));
        assert_eq!(net.signal(PDA, at(2)), GOOD_SIGNAL);
        assert_eq!(net.signal(PDA, at(5)), LOW_SIGNAL);
        assert_eq!(net.signal(PDA, at(7)), NO_SIGNAL);
        assert_eq!(
            net.signal("/obj/item/modular_computer/pda/crew/curator", at(7)),
            LOW_SIGNAL
        );
        assert_eq!(net.signal(LAPTOP_ROOT, at(3)), ETHERNET_SIGNAL);
        assert_eq!(net.signal(PDA, None), NO_SIGNAL);
        // The relay is down: only wired consoles stay on.
        net.relay_up = false;
        assert_eq!(net.signal(PDA, at(2)), NO_SIGNAL);
        assert_eq!(
            net.signal(super::super::programs::ID_CONSOLE, None),
            ETHERNET_SIGNAL
        );
    }

    #[test]
    fn losing_the_network_kills_network_programs() {
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        world.insert_resource(Ntnet {
            relay_up: true,
            station: 1..=1,
            mining: None,
        });
        let mut pc = ModularComputer::pda(PDA, "pda");
        pc.enabled = true;
        assert!(pc.open_program(ProgramKind::Notepad));
        assert!(pc.open_program(ProgramKind::CrewManifest));
        let pda = world.spawn((pc, TilePos::new(1, 1, 1))).id();
        let client = world.spawn_empty().id();
        network_check(&mut world, pda);
        assert_eq!(
            world.get::<ModularComputer>(pda).unwrap().active,
            Some(ProgramKind::CrewManifest)
        );
        world.resource_mut::<Ntnet>().relay_up = false;
        assert!(!can_open(&mut world, client, pda, ProgramKind::Downloader));
        assert!(can_open(&mut world, client, pda, ProgramKind::Notepad));
        // No `TilePos` resolution needed for the kill; the message goes nowhere without viewers.
        world.entity_mut(pda).remove::<TilePos>();
        network_check(&mut world, pda);
        let pc = world.get::<ModularComputer>(pda).unwrap();
        assert_eq!(pc.active, None, "the crew list crashed");
        assert_eq!(
            pc.idle,
            [ProgramKind::Notepad],
            "the notepad needs no network"
        );
    }
}

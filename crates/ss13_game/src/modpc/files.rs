//! Files, storage and data disks after tg `computer_files.dm`, `/obj/item/disk/computer`
//! (`computer_disk.dm`, `role_disks.dm`) and the file manager
//! (`/datum/computer_file/program/filemanager`).
//!
//! A computer's files are its programs (`stored_files`): each takes its `size` of the
//! computer's `max_capacity` (`crate::modpc::programs::max_capacity`). A data disk
//! ([`ComputerDisk`]) holds programs of its own up to its capacity; used on a computer it goes
//! into the disk slot (`computer_disk_act`, swapping out the one there) and comes out with
//! "Eject Disk" on the main screen. The file manager lists both drives: a file is deleted
//! (unless `undeletable`; a running program closes first), copied to the disk
//! (`PRG_copytousb`: not there yet and fits) or from it (`PRG_copyfromusb`: `can_store_file`).
// shortcut: programs are the only files (no text or image files, so no printing, renaming or
// alert toggles); no ctrl-shift-click eject; disks of programs the port lacks start empty
// (maintenance, syndicate, black market, virus disks).

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_net::chat::server::SendChat;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
use ss13_tgui::server::{UiParams, UiReject};

use super::pda::ModularComputer;
use super::programs::ProgramKind;
use super::{
    FileEntry, FileManagerData, PRG_COPY_FROM_USB, PRG_COPY_TO_USB, PRG_DELETE_FILE,
    PRG_USB_DELETE_FILE,
};
use crate::inventory::{CarriedBy, Inventory};

/// `/obj/item/disk/computer`.
pub const DISK_ROOT: &str = "/obj/item/disk/computer";
/// `filetype` of a program.
const PROGRAM_FILETYPE: &str = "PRG";

/// What an ordnance data file holds (`/datum/computer_file/data/ordnance`): the record it was
/// printed from, by identity (a doppler array's explosion index, a compressor record's id).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrdnanceRecord {
    /// `ordnance/explosive` (`DOP`): `explosion_record.explosion_identifier`.
    Explosion(u32),
    /// `ordnance/gaseous` (`COM`): the `gas_record`.
    Compressor(u32),
}

/// `ORDNANCE_FILE_SIZE`: `/datum/computer_file/data/ordnance` `size`.
pub const ORDNANCE_FILE_SIZE: u32 = 4;

/// An ordnance data file on a disk.
#[derive(Clone, Debug, PartialEq)]
pub struct OrdnanceFile {
    /// `uid`: unique per round.
    pub uid: u32,
    pub filename: String,
    /// `possible_experiments`: experiment path -> score.
    pub possible: Vec<(String, f64)>,
    pub record: OrdnanceRecord,
}

/// A data disk's drive.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ComputerDisk {
    pub max_capacity: u32,
    pub files: Vec<ProgramKind>,
    /// Data files (the doppler array's and tank compressor's prints).
    // shortcut: data files are not listed in the file manager (no copy or delete there); the
    // printers and NT Frontier read them from the disk.
    pub data: Vec<OrdnanceFile>,
}

impl ComputerDisk {
    /// `Initialize` of a disk of type `path`: its capacity and `starting_programs` (with a
    /// command disk's `command_programs` first).
    pub fn of_type(path: &str) -> Self {
        use ProgramKind::*;
        let max_capacity = if is_under(path, "/obj/item/disk/computer/super") {
            256
        } else if is_under(path, "/obj/item/disk/computer/advanced") {
            64
        } else if is_under(path, "/obj/item/disk/computer/command") {
            32
        } else {
            16
        };
        let role = |r: &str| is_under(path, &format!("{DISK_ROOT}/{r}"));
        let mut programs: Vec<ProgramKind> = Vec::new();
        if role("command") {
            programs.push(StatusDisplay);
        }
        let starting: &[ProgramKind] = if role("command/captain") {
            &[SecRecords, MedRecords]
        } else if role("command/cmo") || role("medical") {
            &[MedRecords]
        } else if role("command/rd") {
            &[Signaler]
        } else if role("command/hos") || role("command/hop") || role("security") {
            &[SecRecords]
        } else if role("command/ce") {
            &[Atmosscan, AlarmMonitor]
        } else {
            &[]
        };
        programs.extend_from_slice(starting);
        let mut disk = Self {
            max_capacity,
            files: Vec::new(),
            data: Vec::new(),
        };
        for p in programs {
            disk.add_file(p);
        }
        disk
    }

    pub fn used_capacity(&self) -> u32 {
        self.files.iter().map(|p| p.size()).sum::<u32>()
            + self.data.len() as u32 * ORDNANCE_FILE_SIZE
    }

    /// `add_file` of a data file: false when it does not fit.
    pub fn add_data(&mut self, file: OrdnanceFile) -> bool {
        if self.used_capacity() + ORDNANCE_FILE_SIZE > self.max_capacity {
            return false;
        }
        self.data.push(file);
        true
    }

    /// `add_file`: fits (and, unlike tg's list, one copy of a program per drive).
    pub fn add_file(&mut self, program: ProgramKind) -> bool {
        if self.files.contains(&program)
            || self.used_capacity() + program.size() > self.max_capacity
        {
            return false;
        }
        self.files.push(program);
        true
    }
}

fn entry(program: ProgramKind) -> FileEntry {
    FileEntry {
        name: program.filename().to_owned(),
        filetype: PROGRAM_FILETYPE.to_owned(),
        size: program.size(),
        undeletable: program.undeletable(),
    }
}

/// `filemanager/ui_data`.
pub fn ui_data(computer: &ModularComputer, disk: Option<&ComputerDisk>) -> FileManagerData {
    FileManagerData {
        files: computer.programs.iter().map(|&p| entry(p)).collect(),
        usbconnected: disk.is_some(),
        usbfiles: disk.map_or_else(Vec::new, |d| d.files.iter().map(|&p| entry(p)).collect()),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileAct {
    Delete(ProgramKind),
    UsbDelete(ProgramKind),
    CopyToUsb(ProgramKind),
    CopyFromUsb(ProgramKind),
}

/// The file manager's acts; `None` for other actions. `name` must name a known program.
pub fn parse(action: &str, params: &UiParams) -> Option<Result<FileAct, UiReject>> {
    let make: fn(ProgramKind) -> FileAct = match action {
        PRG_DELETE_FILE => FileAct::Delete,
        PRG_USB_DELETE_FILE => FileAct::UsbDelete,
        PRG_COPY_TO_USB => FileAct::CopyToUsb,
        PRG_COPY_FROM_USB => FileAct::CopyFromUsb,
        _ => return None,
    };
    Some(
        params
            .get("name")
            .and_then(ProgramKind::from_filename)
            .map(make)
            .ok_or_else(|| UiReject::BadParams("name: expected a file".into())),
    )
}

/// `filemanager/ui_act`: only while the file manager is in front.
pub fn apply(
    world: &mut World,
    pc: Entity,
    computer: &ModularComputer,
    act: FileAct,
) -> Result<(), &'static str> {
    if computer.active != Some(ProgramKind::FileManager) {
        return Err("file manager not in front");
    }
    let disk = computer.disk;
    match act {
        FileAct::Delete(program) => {
            if !computer.programs.contains(&program) || program.undeletable() {
                return Err("no such deletable file");
            }
            // `remove_file`: a running program is killed first.
            if program == ProgramKind::CardMod
                && (computer.active == Some(program) || computer.idle.contains(&program))
            {
                super::programs::card_mod::on_kill(world, pc);
            }
            let mut c = world.get_mut::<ModularComputer>(pc).ok_or("gone")?;
            c.kill_program(program);
            c.programs.retain(|&p| p != program);
        }
        FileAct::UsbDelete(program) => {
            let mut d = disk
                .and_then(|d| world.get_mut::<ComputerDisk>(d))
                .ok_or("no disk")?;
            if program.undeletable() || !d.files.contains(&program) {
                return Err("no such deletable file");
            }
            d.files.retain(|&p| p != program);
        }
        FileAct::CopyToUsb(program) => {
            if !computer.programs.contains(&program) {
                return Err("no such file");
            }
            let mut d = disk
                .and_then(|d| world.get_mut::<ComputerDisk>(d))
                .ok_or("no disk")?;
            if !d.add_file(program) {
                return Err("disk full or file there");
            }
        }
        FileAct::CopyFromUsb(program) => {
            let on_disk = disk
                .and_then(|d| world.get::<ComputerDisk>(d))
                .is_some_and(|d| d.files.contains(&program));
            if !on_disk {
                return Err("no such file on the disk");
            }
            if !super::programs::downloader::install(world, pc, program) {
                return Err("no room");
            }
        }
    }
    Ok(())
}

fn tell(world: &mut World, client: Entity, text: String) {
    world.write_message(SendChat {
        clients: vec![client],
        message: ChatMessage::simple(ChatChannel::System, Span::Notice, text),
    });
}

/// `computer_disk_act`: the disk from `mob`'s hands into the slot; one already there goes to
/// the hands. False when `disk` is not a data disk or `pc` not a computer.
pub fn insert_disk(
    world: &mut World,
    client: Entity,
    mob: Entity,
    disk: Entity,
    pc: Entity,
) -> bool {
    if world.get::<ComputerDisk>(disk).is_none() || world.get::<ModularComputer>(pc).is_none() {
        return false;
    }
    let in_hands = world
        .get::<Inventory>(mob)
        .is_some_and(|inv| inv.hands.contains(&Some(disk)));
    if !in_hands {
        return true;
    }
    world.entity_mut(disk).remove::<CarriedBy>();
    let swapped = eject_disk(world, pc, Some(mob));
    crate::storage::put_inside(world, disk, pc);
    if let Some(mut c) = world.get_mut::<ModularComputer>(pc) {
        c.disk = Some(disk);
    }
    tell(
        world,
        client,
        if swapped {
            "Диски поменяны местами.".to_owned()
        } else {
            "Диск вставлен.".to_owned()
        },
    );
    true
}

/// `PC_Eject_Disk` "Eject Disk": into `user`'s hands when it carries the computer or stands
/// next to it, else onto the computer's tile. False with an empty slot.
pub fn eject_disk(world: &mut World, pc: Entity, user: Option<Entity>) -> bool {
    let Some(disk) = world.get::<ModularComputer>(pc).and_then(|c| c.disk) else {
        return false;
    };
    let at = super::pda::turf_of(world, pc);
    let near = user.filter(|&u| {
        super::pda::holder_mob(world, pc) == Some(u)
            || matches!((at, world.get::<TilePos>(u)), (Some(a), Some(b)) if a.chebyshev(*b).is_some_and(|d| d <= 1))
    });
    // `Exited` (`crate::modpc::pda`) empties the slot.
    match (
        near.and_then(|u| world.get::<Inventory>(u).cloned().map(|i| (u, i))),
        at,
    ) {
        (Some((mob, inventory)), _) => {
            crate::storage::put_in_hands(world, mob, &inventory, disk);
        }
        (None, Some(pos)) => crate::storage::take_out(world, disk, pos),
        (None, None) => return false,
    }
    if let Some(mut c) = world.get_mut::<ModularComputer>(pc)
        && c.disk == Some(disk)
    {
        c.disk = None;
    }
    true
}

/// `Initialize` of a data disk.
fn attach(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if let Ok(item) = items.get(add.entity)
        && is_under(&item.path, DISK_ROOT)
    {
        commands
            .entity(add.entity)
            .insert_if_new(ComputerDisk::of_type(&item.path));
    }
}

pub fn build(app: &mut App) {
    app.add_observer(attach);
}

#[cfg(test)]
mod tests {
    use super::*;

    const PDA: &str = "/obj/item/modular_computer/pda/crew";

    #[test]
    fn role_disks_start_with_their_programs() {
        let ce = ComputerDisk::of_type("/obj/item/disk/computer/command/ce");
        assert_eq!(ce.max_capacity, 32);
        assert_eq!(
            ce.files,
            [
                ProgramKind::StatusDisplay,
                ProgramKind::Atmosscan,
                ProgramKind::AlarmMonitor
            ]
        );
        let mut plain = ComputerDisk::of_type(DISK_ROOT);
        assert_eq!((plain.max_capacity, plain.files.len()), (16, 0));
        assert!(plain.add_file(ProgramKind::CardMod));
        assert!(!plain.add_file(ProgramKind::CardMod), "one copy");
        assert!(plain.add_file(ProgramKind::ChatClient));
        assert!(!plain.add_file(ProgramKind::Notepad), "16 GQ full");
    }

    #[test]
    fn the_file_manager_copies_and_deletes() {
        let mut world = World::new();
        let disk = world
            .spawn(ComputerDisk::of_type("/obj/item/disk/computer/security"))
            .id();
        let mut pc = ModularComputer::pda(PDA, "pda");
        pc.enabled = true;
        pc.disk = Some(disk);
        assert!(pc.open_program(ProgramKind::Notepad));
        let e = world.spawn(pc).id();
        // `apply` with the computer as it is now.
        fn apply_now(world: &mut World, e: Entity, act: FileAct) -> Result<(), &'static str> {
            let computer = world.get::<ModularComputer>(e).unwrap().clone();
            apply(world, e, &computer, act)
        }
        // Not in front: refused.
        assert!(apply_now(&mut world, e, FileAct::CopyToUsb(ProgramKind::Notepad)).is_err());
        world
            .get_mut::<ModularComputer>(e)
            .unwrap()
            .open_program(ProgramKind::FileManager);
        apply_now(&mut world, e, FileAct::CopyToUsb(ProgramKind::Notepad)).unwrap();
        assert!(
            world
                .get::<ComputerDisk>(disk)
                .unwrap()
                .files
                .contains(&ProgramKind::Notepad)
        );
        // The running notepad is deleted: it closes too. The messenger is undeletable.
        apply_now(&mut world, e, FileAct::Delete(ProgramKind::Notepad)).unwrap();
        let pc = world.get::<ModularComputer>(e).unwrap().clone();
        assert!(
            !pc.programs.contains(&ProgramKind::Notepad)
                && !pc.idle.contains(&ProgramKind::Notepad)
        );
        assert!(apply(&mut world, e, &pc, FileAct::Delete(ProgramKind::Messenger)).is_err());
        // Back from the disk, and the records too.
        apply(
            &mut world,
            e,
            &pc,
            FileAct::CopyFromUsb(ProgramKind::Notepad),
        )
        .unwrap();
        apply_now(&mut world, e, FileAct::CopyFromUsb(ProgramKind::SecRecords)).unwrap();
        let data = ui_data(
            &world.get::<ModularComputer>(e).unwrap().clone(),
            world.get::<ComputerDisk>(disk),
        );
        assert!(data.usbconnected);
        assert!(
            data.files
                .iter()
                .any(|f| f.name == "secrecords" && f.filetype == "PRG")
        );
        assert!(
            data.files
                .iter()
                .any(|f| f.name == "filemanager" && f.undeletable)
        );
        apply_now(&mut world, e, FileAct::UsbDelete(ProgramKind::SecRecords)).unwrap();
        assert_eq!(
            world.get::<ComputerDisk>(disk).unwrap().files,
            [ProgramKind::Notepad]
        );
        let p = vec![("name".to_owned(), "../etc".to_owned())];
        assert!(matches!(
            parse(PRG_DELETE_FILE, &UiParams::new(&p)),
            Some(Err(_))
        ));
    }
}

//! NT Software Hub (`/datum/computer_file/program/ntnetdownload`): the NTNet catalog of the
//! programs this port has, by category, with what the computer's disk and ID allow. A download
//! runs while the hub is open (in front or in the background) at the good-signal NTNet speed
//! and is stored on completion if it still fits.
// shortcut: NTNet is always at good signal (no relays, no `TRAIT_MODPC_HALVED_DOWNLOAD_SPEED`),
// no network logs, and no Syndicate catalog (no antagonist programs in the port), so an emag
// changes nothing here.
// tg's `begin_file_download` checks neither the ID's access nor the device's hardware (only the
// UI greys them out); here the server refuses both.

use bevy::prelude::*;
use ss13_tgui::server::{UiParams, UiReject};

use super::ProgramKind;
use crate::id_access::{ACCESS_FLAG_COMMAND, access_flag};
use crate::inventory::IdCard;
use crate::modpc::pda::ModularComputer;
use crate::modpc::{
    DownloadProgress, DownloadableProgram, DownloaderData, PRG_DOWNLOAD_FILE, PRG_RESET_ERROR,
};

/// `NTNETSPEED_HIGHSIGNAL` GQ per `SSobj` tick (2 s).
pub const DOWNLOAD_SPEED: f32 = 1.0 / 2.0;
/// `downloaderror` of a download that no longer fits.
const IO_ERROR: &str = "I/O ERROR - Unable to save file. Check whether you have enough free \
    space on your hard drive and whether your hard drive is properly connected. If the issue \
    persists contact your system administrator for assistance.";

/// The hub's state on its computer.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Download {
    /// `downloaded_file`.
    pub file: Option<ProgramKind>,
    /// `download_completion`, GQ.
    pub completion: f32,
    /// `downloaderror`.
    pub error: Option<String>,
    /// `ui_header = "downloader_finished.gif"`: a download completed since the hub opened.
    pub finished: bool,
}

/// `used_capacity`.
pub fn used_capacity(computer: &ModularComputer) -> u32 {
    computer.programs.iter().map(|p| p.size()).sum()
}

/// `can_store_file`: not stored yet and fits on the disk.
pub fn can_store(computer: &ModularComputer, program: ProgramKind) -> bool {
    !computer.programs.contains(&program)
        && used_capacity(computer) + program.size() <= super::max_capacity(&computer.path)
}

/// `can_run(user, downloading = TRUE, access)`: no requirement, or one of the ID's accesses
/// opens it. No ID in the computer opens nothing that has a requirement.
pub fn can_download(program: ProgramKind, access: Option<&[String]>) -> bool {
    let (required, command) = program.download_access();
    if required.is_empty() && !command {
        return true;
    }
    access.is_some_and(|access| {
        access.iter().any(|a| {
            required.contains(&a.as_str()) || (command && access_flag(a) == ACCESS_FLAG_COMMAND)
        })
    })
}

/// The ID card in the computer's slot, its access.
fn access_of(world: &World, computer: &ModularComputer) -> Option<Vec<String>> {
    computer
        .stored_id
        .and_then(|c| world.get::<IdCard>(c))
        .map(|c| c.access.clone())
}

/// `ui_data`.
pub fn ui_data(
    computer: &ModularComputer,
    download: Option<&Download>,
    access: Option<&[String]>,
) -> DownloaderData {
    let pda = computer.is_pda();
    DownloaderData {
        disk_size: super::max_capacity(&computer.path),
        disk_used: used_capacity(computer),
        downloading: download.and_then(|d| {
            d.file.map(|p| DownloadProgress {
                name: p.filename().to_owned(),
                desc: p.filedesc().to_owned(),
                size: p.size(),
                completion: (d.completion * 10.0).round() / 10.0,
            })
        }),
        error: download.and_then(|d| d.error.clone()),
        emagged: computer.emagged,
        programs: ProgramKind::ALL
            .into_iter()
            .filter(|p| p.on_ntnet_store())
            .map(|p| DownloadableProgram {
                icon: p.icon().to_owned(),
                filename: p.filename().to_owned(),
                filedesc: p.filedesc().to_owned(),
                fileinfo: p.extended_desc().to_owned(),
                category: p.category().to_owned(),
                installed: computer.programs.contains(&p),
                compatible: p.runs_on(pda),
                size: p.size(),
                access: can_download(p, access),
            })
            .collect(),
    }
}

/// The hub's acts, well-formed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloaderAct {
    /// `PRG_downloadfile`: a program of the catalog.
    Download(ProgramKind),
    /// `PRG_reseterror`.
    ResetError,
}

/// `None` for an action that is not the hub's.
pub fn parse(action: &str, params: &UiParams) -> Option<Result<DownloaderAct, UiReject>> {
    Some(match action {
        PRG_DOWNLOAD_FILE => params
            .get("filename")
            .and_then(ProgramKind::from_filename)
            .filter(|p| p.on_ntnet_store())
            .map(DownloaderAct::Download)
            .ok_or_else(|| UiReject::BadParams("filename: not in the catalog".into())),
        PRG_RESET_ERROR => Ok(DownloaderAct::ResetError),
        _ => return None,
    })
}

/// `ui_act` of the hub in front of `pc`.
pub fn apply(
    world: &mut World,
    pc: Entity,
    computer: &ModularComputer,
    act: DownloaderAct,
) -> Result<(), &'static str> {
    if computer.active != Some(ProgramKind::Downloader) {
        return Err("hub not in front");
    }
    let mut download = world.get::<Download>(pc).cloned().unwrap_or_default();
    match act {
        DownloaderAct::ResetError => {
            if download.error.is_some() {
                download.completion = 0.0;
                download.file = None;
                download.error = None;
            }
        }
        DownloaderAct::Download(program) => {
            // `begin_file_download`.
            if download.file.is_some() {
                return Err("already downloading");
            }
            if !program.runs_on(computer.is_pda()) {
                return Err("incompatible hardware");
            }
            if !can_download(program, access_of(world, computer).as_deref()) {
                return Err("access denied");
            }
            if !can_store(computer, program) {
                return Err("cannot store");
            }
            download.file = Some(program);
            download.completion = 0.0;
            if program.size() == 0 {
                world.entity_mut(pc).insert(download);
                complete(world, pc);
                return Ok(());
            }
        }
    }
    world.entity_mut(pc).insert(download);
    Ok(())
}

/// `store_file` of a downloaded program, with the state its program needs.
pub fn install(world: &mut World, pc: Entity, program: ProgramKind) -> bool {
    if !world
        .get::<ModularComputer>(pc)
        .is_some_and(|c| can_store(c, program))
    {
        return false;
    }
    if program == ProgramKind::Minesweeper {
        super::minesweeper::install(world, pc);
    } else if let Some(mut computer) = world.get_mut::<ModularComputer>(pc) {
        computer.programs.push(program);
    }
    true
}

/// `complete_file_download`.
fn complete(world: &mut World, pc: Entity) {
    let Some(program) = world.get::<Download>(pc).and_then(|d| d.file) else {
        return;
    };
    let stored = install(world, pc, program);
    if let Some(mut d) = world.get_mut::<Download>(pc) {
        if !stored {
            d.error = Some(IO_ERROR.to_owned());
        }
        d.file = None;
        d.completion = 0.0;
        d.finished = true;
    }
}

/// `process_tick` of every hub: the download advances while the hub runs on a working computer
/// and is aborted (`kill_program`) when the hub is closed or the computer turned off. It stays
/// at 100% for a moment before it is stored, as tg's does for a cycle.
pub fn process(
    world: &mut World,
    hubs: &mut QueryState<(Entity, &'static ModularComputer, &'static mut Download)>,
) {
    let dt = world.resource::<Time>().delta_secs();
    let mut done = Vec::new();
    for (pc, computer, mut download) in hubs.iter_mut(world) {
        let running = computer.enabled
            && (computer.active == Some(ProgramKind::Downloader)
                || computer.idle.contains(&ProgramKind::Downloader));
        if !running {
            // `kill_program` -> `abort_file_download`, `ui_header = null`.
            if download.file.is_some() || download.finished {
                download.file = None;
                download.completion = 0.0;
                download.finished = false;
            }
            continue;
        }
        let Some(program) = download.file else {
            continue;
        };
        let size = program.size() as f32;
        if download.completion >= size {
            done.push(pc);
        } else {
            download.completion = (download.completion + DOWNLOAD_SPEED * dt).min(size);
        }
    }
    for pc in done {
        complete(world, pc);
    }
}

pub fn build(app: &mut App) {
    app.add_systems(Update, process.run_if(any_with_component::<Download>));
}

#[cfg(test)]
mod tests {
    use super::*;

    const PDA: &str = "/obj/item/modular_computer/pda/crew";

    fn hub_world() -> (World, Entity) {
        let mut world = World::new();
        world.init_resource::<Time>();
        let mut pc = ModularComputer::pda(PDA, "pda");
        pc.enabled = true;
        assert!(pc.open_program(ProgramKind::Downloader));
        let pc = world.spawn(pc).id();
        (world, pc)
    }

    fn tick(world: &mut World, secs: f32) {
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(secs));
        let mut q = world.query::<(Entity, &ModularComputer, &mut Download)>();
        process(world, &mut q);
    }

    #[test]
    fn a_download_takes_its_size_at_ntnet_speed_then_installs() {
        let (mut world, pc) = hub_world();
        let computer = world.get::<ModularComputer>(pc).unwrap().clone();
        apply(
            &mut world,
            pc,
            &computer,
            DownloaderAct::Download(ProgramKind::Atmosscan),
        )
        .unwrap();
        // Busy: a second download is refused.
        assert!(
            apply(
                &mut world,
                pc,
                &computer,
                DownloaderAct::Download(ProgramKind::Minesweeper)
            )
            .is_err()
        );
        // 4 GQ at 0.5 GQ/s: 8 s, then a tick at 100%.
        for _ in 0..7 {
            tick(&mut world, 1.0);
        }
        assert!(
            !world
                .get::<ModularComputer>(pc)
                .unwrap()
                .programs
                .contains(&ProgramKind::Atmosscan)
        );
        tick(&mut world, 1.0);
        tick(&mut world, 1.0);
        let pc_state = world.get::<ModularComputer>(pc).unwrap();
        assert!(pc_state.programs.contains(&ProgramKind::Atmosscan));
        let d = world.get::<Download>(pc).unwrap();
        assert!(d.finished && d.file.is_none() && d.error.is_none());
    }

    #[test]
    fn closing_the_hub_aborts_and_access_and_space_are_checked() {
        let (mut world, pc) = hub_world();
        let computer = world.get::<ModularComputer>(pc).unwrap().clone();
        // No ID: the ID writer needs command access.
        assert_eq!(
            apply(
                &mut world,
                pc,
                &computer,
                DownloaderAct::Download(ProgramKind::CardMod)
            ),
            Err("access denied")
        );
        assert!(can_download(
            ProgramKind::CardMod,
            Some(&["command".to_owned()][..])
        ));
        // The records open to command staff too (tg's `ACCESS_FLAG_COMMAND` intent).
        assert!(can_download(
            ProgramKind::SecRecords,
            Some(&["security".to_owned()][..])
        ));
        assert!(!can_download(
            ProgramKind::SecRecords,
            Some(&["maint_tunnels".to_owned()][..])
        ));
        // Records do not run on a console.
        assert!(!ProgramKind::MedRecords.runs_on(false));
        apply(
            &mut world,
            pc,
            &computer,
            DownloaderAct::Download(ProgramKind::Minesweeper),
        )
        .unwrap();
        tick(&mut world, 1.0);
        world.get_mut::<ModularComputer>(pc).unwrap().minimize();
        tick(&mut world, 1.0);
        assert!(world.get::<Download>(pc).unwrap().completion > 0.5);
        world
            .get_mut::<ModularComputer>(pc)
            .unwrap()
            .kill_program(ProgramKind::Downloader);
        tick(&mut world, 1.0);
        assert_eq!(world.get::<Download>(pc).unwrap().file, None);
        // Installed or no room: not stored.
        assert!(!can_store(&computer, ProgramKind::Notepad));
        let mut full = computer.clone();
        full.programs = vec![ProgramKind::CardMod; 8];
        assert_eq!(used_capacity(&full), 64);
        assert!(!can_store(&full, ProgramKind::StatusDisplay));
        // A 0 GQ program still fits.
        assert!(can_store(&full, ProgramKind::CrewManifest));
        let data = ui_data(&computer, world.get::<Download>(pc), None);
        assert!(data.programs.iter().all(|p| p.filename != "nt_messenger"));
        assert!(
            data.programs
                .iter()
                .any(|p| p.filename == "minesweeper" && p.access && p.compatible)
        );
    }

    #[test]
    fn parse_takes_only_catalog_programs() {
        let params = |v: &str| vec![("filename".to_owned(), v.to_owned())];
        let p = params("minesweeper");
        assert_eq!(
            parse(PRG_DOWNLOAD_FILE, &UiParams::new(&p)),
            Some(Ok(DownloaderAct::Download(ProgramKind::Minesweeper)))
        );
        let p = params("statusdisplay");
        assert!(matches!(
            parse(PRG_DOWNLOAD_FILE, &UiParams::new(&p)),
            Some(Err(_))
        ));
        let none: Vec<(String, String)> = Vec::new();
        assert_eq!(parse("PC_exit", &UiParams::new(&none)), None);
    }
}

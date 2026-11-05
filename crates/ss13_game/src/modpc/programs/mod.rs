//! The programs a PDA runs (`/datum/computer_file/program/*`): their catalog entries here, their
//! state and acts in the submodules.
// shortcut: only programs backed by systems that exist: no NT Pay (no pay tokens or transaction
// history in the economy), navigator (no nanomaps), supermatter and power monitors, theme
// manager, file manager or software downloader.

#[cfg(feature = "server")]
pub mod apps;
#[cfg(feature = "server")]
pub mod card_mod;
#[cfg(feature = "server")]
pub mod messenger;

/// An installed program (an entry of `stored_files`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProgramKind {
    /// `/datum/computer_file/program/messenger`.
    Messenger,
    /// `/datum/computer_file/program/notepad`.
    Notepad,
    /// `/datum/computer_file/program/crew_manifest`.
    CrewManifest,
    /// `/datum/computer_file/program/atmosscan`.
    Atmosscan,
    /// `/datum/computer_file/program/card_mod` (Plexagon Access Management).
    CardMod,
    /// `/datum/computer_file/program/alarm_monitor` (Canary).
    AlarmMonitor,
    /// `/datum/computer_file/program/records/security`.
    SecRecords,
    /// `/datum/computer_file/program/records/medical`.
    MedRecords,
    /// `/datum/computer_file/program/signal_commander`.
    Signaler,
    /// `/datum/computer_file/program/status`.
    StatusDisplay,
    /// `/datum/computer_file/program/newscaster`.
    Newscaster,
}

impl ProgramKind {
    pub const ALL: [Self; 11] = [
        Self::Messenger,
        Self::Notepad,
        Self::CrewManifest,
        Self::Atmosscan,
        Self::CardMod,
        Self::AlarmMonitor,
        Self::SecRecords,
        Self::MedRecords,
        Self::Signaler,
        Self::StatusDisplay,
        Self::Newscaster,
    ];

    /// `filename`.
    pub fn filename(self) -> &'static str {
        match self {
            Self::Messenger => "nt_messenger",
            Self::Notepad => "notepad",
            Self::CrewManifest => "plexagoncrew",
            Self::Atmosscan => "atmosscan",
            Self::CardMod => "plexagonidwriter",
            Self::AlarmMonitor => "alarmmonitor",
            Self::SecRecords => "secrecords",
            Self::MedRecords => "medrecords",
            Self::Signaler => "signaler",
            Self::StatusDisplay => "statusdisplay",
            Self::Newscaster => "newscasterapp",
        }
    }

    pub fn from_filename(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.filename() == name)
    }

    /// `filedesc`.
    pub fn filedesc(self) -> &'static str {
        match self {
            Self::Messenger => "Direct Messenger",
            Self::Notepad => "Notepad",
            Self::CrewManifest => "Plexagon Crew List",
            Self::Atmosscan => "AtmoZphere",
            Self::CardMod => "Plexagon Access Management",
            Self::AlarmMonitor => "Canary",
            Self::SecRecords => "Security Records",
            Self::MedRecords => "Medical Records",
            Self::Signaler => "SignalCommander",
            Self::StatusDisplay => "Status Display",
            Self::Newscaster => "Newscaster",
        }
    }

    /// `program_icon`.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Messenger => "comment-alt",
            Self::Notepad => "book",
            Self::CrewManifest => "clipboard-list",
            Self::Atmosscan => "thermometer-half",
            Self::CardMod => "id-card",
            Self::AlarmMonitor => "bell",
            Self::SecRecords => "clipboard",
            Self::MedRecords => "book-medical",
            Self::Signaler => "satellite-dish",
            Self::StatusDisplay => "signal",
            Self::Newscaster => "newspaper",
        }
    }

    /// `program_flags & PROGRAM_HEADER`: shown above the program list.
    pub fn header_program(self) -> bool {
        self == Self::Messenger
    }

    /// `ui_header`: the header icon while it runs in the background (the alarm monitor's turns
    /// `alarm_red.gif` while there are alarms, `crate::modpc::pda`).
    pub fn ui_header(self) -> Option<&'static str> {
        match self {
            Self::Messenger => Some("ntnrc_idle.gif"),
            Self::AlarmMonitor => Some("alarm_green.gif"),
            _ => None,
        }
    }

    /// Programs that open their own window instead of an NtOS screen (tg's newscaster app runs
    /// a hidden newscaster's `PhysicalNewscaster` interface).
    pub fn launcher(self) -> bool {
        self == Self::Newscaster
    }
}

/// `/obj/item/modular_computer/pda` `pda_programs` this port has, in `stored_files` order.
pub const PDA_PROGRAMS: [ProgramKind; 3] = [
    ProgramKind::Messenger,
    ProgramKind::Notepad,
    ProgramKind::CrewManifest,
];

/// `/obj/machinery/modular_computer/preset/id`: the identification console.
pub const ID_CONSOLE: &str = "/obj/machinery/modular_computer/preset/id";

/// `/obj/machinery/modular_computer/preset/engineering`.
pub const ENGINEERING_CONSOLE: &str = "/obj/machinery/modular_computer/preset/engineering";

/// `starting_programs` of the stationary consoles this port has (`console_presets.dm`), by
/// type: the identification console runs the ID writer and the crew list, the engineering one
/// the alarm monitor.
// shortcut: no chat client or job management program on the ID console, no power or
// supermatter monitor on the engineering one, and no other console presets.
pub fn console_programs(path: &str) -> Option<Vec<ProgramKind>> {
    if is_under_path(path, ID_CONSOLE) {
        return Some(vec![ProgramKind::CardMod, ProgramKind::CrewManifest]);
    }
    is_under_path(path, ENGINEERING_CONSOLE).then(|| vec![ProgramKind::AlarmMonitor])
}

fn is_under_path(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// `/obj/item/modular_computer/pda/crew/heads`: `head_programs` are added after the rest.
const HEADS_PDA: &str = "/obj/item/modular_computer/pda/crew/heads";

/// tg `starting_programs` of the PDA types (`role_tablet_presets.dm`), the ones this port has;
/// a type without an entry inherits its parent's.
const STARTING_PROGRAMS: [(&str, &[ProgramKind]); 22] = {
    use ProgramKind::*;
    [
        (
            "/obj/item/modular_computer/pda/crew/heads/hop",
            &[SecRecords],
        ),
        (
            "/obj/item/modular_computer/pda/crew/heads/hos",
            &[SecRecords],
        ),
        (
            "/obj/item/modular_computer/pda/crew/heads/ce",
            &[Atmosscan, AlarmMonitor],
        ),
        (
            "/obj/item/modular_computer/pda/crew/heads/cmo",
            &[MedRecords],
        ),
        ("/obj/item/modular_computer/pda/crew/heads/rd", &[Signaler]),
        (
            "/obj/item/modular_computer/pda/crew/heads/quartermaster",
            &[],
        ),
        (
            "/obj/item/modular_computer/pda/crew/security",
            &[SecRecords],
        ),
        (
            "/obj/item/modular_computer/pda/crew/detective",
            &[SecRecords],
        ),
        ("/obj/item/modular_computer/pda/crew/warden", &[SecRecords]),
        (
            "/obj/item/modular_computer/pda/crew/engineering",
            &[AlarmMonitor, Atmosscan],
        ),
        (
            "/obj/item/modular_computer/pda/crew/atmos",
            &[AlarmMonitor, Atmosscan],
        ),
        (
            "/obj/item/modular_computer/pda/crew/science",
            &[Atmosscan, Signaler],
        ),
        ("/obj/item/modular_computer/pda/crew/roboticist", &[]),
        (
            "/obj/item/modular_computer/pda/crew/geneticist",
            &[MedRecords],
        ),
        ("/obj/item/modular_computer/pda/crew/medical", &[MedRecords]),
        ("/obj/item/modular_computer/pda/crew/coroner", &[MedRecords]),
        (
            "/obj/item/modular_computer/pda/crew/psychologist",
            &[MedRecords],
        ),
        ("/obj/item/modular_computer/pda/crew/lawyer", &[SecRecords]),
        ("/obj/item/modular_computer/pda/crew/curator", &[Newscaster]),
        (
            "/obj/item/modular_computer/pda/bridge_assistant",
            &[StatusDisplay],
        ),
        (
            "/obj/item/modular_computer/pda/veteran_advisor",
            &[SecRecords],
        ),
        ("/obj/item/modular_computer/pda/crew/cargo", &[]),
    ]
};

/// `starting_programs` of `path`: its own entry or its nearest ancestor's.
fn starting_programs(path: &str) -> &'static [ProgramKind] {
    let mut at = path;
    loop {
        if let Some((_, programs)) = STARTING_PROGRAMS.iter().find(|(p, _)| *p == at) {
            return programs;
        }
        match at.rfind('/') {
            Some(i) if i > 0 => at = &at[..i],
            _ => return &[],
        }
    }
}

/// `install_default_programs` of a PDA of type `path`: `pda_programs` + `starting_programs`,
/// then a head's `head_programs` (`heads/Initialize`).
pub fn pda_programs(path: &str) -> Vec<ProgramKind> {
    let mut programs = PDA_PROGRAMS.to_vec();
    for &p in starting_programs(path) {
        if !programs.contains(&p) {
            programs.push(p);
        }
    }
    if is_under_path(path, HEADS_PDA) && !programs.contains(&ProgramKind::StatusDisplay) {
        programs.push(ProgramKind::StatusDisplay);
    }
    programs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engineering_pdas_get_the_atmos_scanner() {
        assert_eq!(pda_programs("/obj/item/modular_computer/pda/crew").len(), 3);
        assert_eq!(
            pda_programs("/obj/item/modular_computer/pda/crew/atmos").last(),
            Some(&ProgramKind::Atmosscan)
        );
        for p in pda_programs("/obj/item/modular_computer/pda/crew/heads/ce") {
            assert_eq!(ProgramKind::from_filename(p.filename()), Some(p));
        }
    }

    #[test]
    fn role_pdas_get_their_starting_and_head_programs() {
        use ProgramKind::*;
        assert_eq!(
            pda_programs("/obj/item/modular_computer/pda/crew/heads/ce")[3..],
            [Atmosscan, AlarmMonitor, StatusDisplay]
        );
        // The paramedic inherits nothing new: its own list has medical records too.
        assert!(
            pda_programs("/obj/item/modular_computer/pda/crew/medical/paramedic")
                .contains(&MedRecords)
        );
        assert_eq!(
            pda_programs("/obj/item/modular_computer/pda/crew/heads/quartermaster")[3..],
            [StatusDisplay]
        );
        assert_eq!(
            pda_programs("/obj/item/modular_computer/pda/crew/curator").last(),
            Some(&Newscaster)
        );
        assert_eq!(
            console_programs(ENGINEERING_CONSOLE),
            Some(vec![AlarmMonitor])
        );
        for p in ProgramKind::ALL {
            assert_eq!(ProgramKind::from_filename(p.filename()), Some(p));
        }
    }
}

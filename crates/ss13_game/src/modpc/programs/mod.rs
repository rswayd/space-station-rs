//! The programs a PDA runs (`/datum/computer_file/program/*`): their catalog entries here, their
//! state and acts in the submodules.
// shortcut: only programs backed by systems that exist: no NT Pay (no economy in the PDA yet),
// navigator, theme manager, file manager or software downloader; status display skipped.

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
}

impl ProgramKind {
    /// `filename`.
    pub fn filename(self) -> &'static str {
        match self {
            Self::Messenger => "nt_messenger",
            Self::Notepad => "notepad",
            Self::CrewManifest => "plexagoncrew",
            Self::Atmosscan => "atmosscan",
            Self::CardMod => "plexagonidwriter",
        }
    }

    pub fn from_filename(name: &str) -> Option<Self> {
        [
            Self::Messenger,
            Self::Notepad,
            Self::CrewManifest,
            Self::Atmosscan,
            Self::CardMod,
        ]
        .into_iter()
        .find(|p| p.filename() == name)
    }

    /// `filedesc`.
    pub fn filedesc(self) -> &'static str {
        match self {
            Self::Messenger => "Direct Messenger",
            Self::Notepad => "Notepad",
            Self::CrewManifest => "Plexagon Crew List",
            Self::Atmosscan => "AtmoZphere",
            Self::CardMod => "Plexagon Access Management",
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
        }
    }

    /// `program_flags & PROGRAM_HEADER`: shown above the program list.
    pub fn header_program(self) -> bool {
        self == Self::Messenger
    }

    /// `ui_header`: the header icon while it runs in the background.
    pub fn ui_header(self) -> Option<&'static str> {
        (self == Self::Messenger).then_some("ntnrc_idle.gif")
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

/// `starting_programs` of the stationary consoles this port has (`console_presets.dm`), by
/// type: the identification console runs the ID writer and the crew list.
// shortcut: no chat client or job management program on it, and no other console presets.
pub fn console_programs(path: &str) -> Option<Vec<ProgramKind>> {
    is_under_path(path, ID_CONSOLE).then(|| vec![ProgramKind::CardMod, ProgramKind::CrewManifest])
}

fn is_under_path(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// PDA types whose tg `starting_programs` (`role_tablet_presets.dm`) include the atmos scanner.
const ATMOSSCAN_PDAS: [&str; 4] = [
    "/obj/item/modular_computer/pda/crew/heads/ce",
    "/obj/item/modular_computer/pda/crew/engineering",
    "/obj/item/modular_computer/pda/crew/atmos",
    "/obj/item/modular_computer/pda/crew/science",
];

/// `install_default_programs` of a PDA of type `path`: `pda_programs` + `starting_programs`.
pub fn pda_programs(path: &str) -> Vec<ProgramKind> {
    let mut programs = PDA_PROGRAMS.to_vec();
    if ATMOSSCAN_PDAS.contains(&path) {
        programs.push(ProgramKind::Atmosscan);
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
}

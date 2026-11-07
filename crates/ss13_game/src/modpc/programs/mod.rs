//! The programs a PDA runs (`/datum/computer_file/program/*`): their catalog entries here, their
//! state and acts in the submodules.
// shortcut: only programs backed by systems that exist: no navigator (no nanomaps), supermatter
// and power monitors, theme manager or file manager.

#[cfg(feature = "server")]
pub mod apps;
#[cfg(feature = "server")]
pub mod card_mod;
#[cfg(feature = "server")]
pub mod chat;
#[cfg(feature = "server")]
pub mod downloader;
#[cfg(feature = "server")]
pub mod messenger;
#[cfg(feature = "server")]
pub mod minesweeper;
#[cfg(feature = "server")]
pub mod station;
pub mod station_kinds;

pub use station_kinds::StationProgram;

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
    /// BandaStation `/datum/computer_file/program/minesweeper`: installed by an emag or
    /// downloaded.
    Minesweeper,
    /// `/datum/computer_file/program/ntnetdownload` (NT Software Hub).
    Downloader,
    /// `/datum/computer_file/program/chatclient` (NTNet Relay Chat).
    ChatClient,
    /// The programs of `station_kinds` (state in `station`).
    Station(StationProgram),
}

/// `PROGRAM_CATEGORY_*`, the downloader's tabs in `show_categories` order.
pub const CATEGORY_DEVICE: &str = "Инструменты";
pub const CATEGORY_EQUIPMENT: &str = "Оборудование";
pub const CATEGORY_GAMES: &str = "Игры";
pub const CATEGORY_SECURITY: &str = "Безопасность и записи";
pub const CATEGORY_ENGINEERING: &str = "Инженерия";
pub const CATEGORY_SUPPLY: &str = "Снабжение";
pub const CATEGORY_SCIENCE: &str = "Наука";
pub const CATEGORY_SERVICE: &str = "Сервис";
pub const CATEGORIES: [&str; 8] = [
    CATEGORY_DEVICE,
    CATEGORY_EQUIPMENT,
    CATEGORY_GAMES,
    CATEGORY_SECURITY,
    CATEGORY_ENGINEERING,
    CATEGORY_SUPPLY,
    CATEGORY_SCIENCE,
    CATEGORY_SERVICE,
];

impl ProgramKind {
    pub const ALL: [Self; 14 + StationProgram::ALL.len()] = {
        let mut all = [Self::Messenger; 14 + StationProgram::ALL.len()];
        let mut i = 0;
        while i < Self::BASE.len() {
            all[i] = Self::BASE[i];
            i += 1;
        }
        while i < all.len() {
            all[i] = Self::Station(StationProgram::ALL[i - Self::BASE.len()]);
            i += 1;
        }
        all
    };

    const BASE: [Self; 14] = [
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
        Self::Minesweeper,
        Self::Downloader,
        Self::ChatClient,
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
            Self::Minesweeper => "minesweeper",
            Self::Downloader => "ntsoftwarehub",
            Self::ChatClient => "ntnrc_client",
            Self::Station(p) => p.filename(),
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
            Self::Minesweeper => "Сапёр",
            Self::Downloader => "NT Software Hub",
            Self::ChatClient => "Chat Client",
            Self::Station(p) => p.filedesc(),
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
            Self::Minesweeper => "bomb",
            Self::Downloader => "download",
            Self::ChatClient => "comment-alt",
            Self::Station(p) => p.icon(),
        }
    }

    /// `extended_desc`.
    pub fn extended_desc(self) -> &'static str {
        match self {
            Self::Messenger => {
                "Эта программа позволяет общаться с другими модульными устройствами по старинке."
            }
            Self::Notepad => {
                "Запишите свои мысли о том, как безопасно работать и что не стоит делать."
            }
            Self::CrewManifest => "Программа для просмотра и печати списка членов экипажа",
            Self::Atmosscan => {
                "Небольшой встроенный датчик считывает данные об атмосферных изменениях вокруг \
                 устройства."
            }
            Self::CardMod => "Программа для изменения доступов у ID-карт сотрудников.",
            Self::AlarmMonitor => {
                "Эта программа предоставляет визуальный интерфейс для системы тревог станции."
            }
            Self::SecRecords => {
                "Позволяет пользователю просмотреть некоторые основные охранные записи экипажа"
            }
            Self::MedRecords => {
                "Позволяет пользователю просмотреть некоторые основные медицинские записи экипажа."
            }
            Self::Signaler => {
                "Небольшое встроенное частотное приложение, которое подает сигналы при наличии \
                 соответствующего оборудования."
            }
            Self::StatusDisplay => {
                "An app used to change the message on the station status displays."
            }
            Self::Newscaster => {
                "Эта программа позволяет любому пользователю получить доступ к сети новостей из \
                 любого места."
            }
            Self::Minesweeper => {
                "Погрузись в удивительный мир 'Сапёра', где каждое неверное нажатие может \
                 привести к взрыву! Сразись с друзьями и стань мастером разминирования в этой \
                 захватывающей игре!."
            }
            Self::Downloader => {
                "This program allows downloads of software from official NT repositories"
            }
            Self::ChatClient => {
                "Эта программа позволяет осуществлять коммуникацию через сеть NTNRC."
            }
            Self::Station(p) => p.extended_desc(),
        }
    }

    /// `size` in GQ.
    pub fn size(self) -> u32 {
        match self {
            Self::Messenger | Self::CrewManifest => 0,
            Self::StatusDisplay => 1,
            Self::Notepad | Self::Signaler | Self::Newscaster => 2,
            Self::Atmosscan
            | Self::AlarmMonitor
            | Self::SecRecords
            | Self::MedRecords
            | Self::Downloader => 4,
            Self::Minesweeper => 6,
            Self::CardMod | Self::ChatClient => 8,
            Self::Station(p) => p.size(),
        }
    }

    /// `downloader_category`.
    pub fn category(self) -> &'static str {
        match self {
            Self::Atmosscan | Self::AlarmMonitor => CATEGORY_ENGINEERING,
            Self::CardMod | Self::Signaler => CATEGORY_EQUIPMENT,
            Self::SecRecords | Self::MedRecords => CATEGORY_SECURITY,
            Self::Newscaster | Self::Minesweeper => CATEGORY_GAMES,
            Self::Station(p) => p.category(),
            _ => CATEGORY_DEVICE,
        }
    }

    /// `program_flags & PROGRAM_ON_NTNET_STORE`: in the NT Software Hub catalog. The messenger
    /// comes with every PDA, the status display with the heads' and the hub itself with every
    /// computer.
    pub fn on_ntnet_store(self) -> bool {
        match self {
            Self::Station(p) => p.on_ntnet_store(),
            _ => !matches!(
                self,
                Self::Messenger | Self::StatusDisplay | Self::Downloader
            ),
        }
    }

    /// `PROGRAM_ON_SYNDINET_STORE`: offered by the hub of an emagged computer.
    pub fn on_syndinet_store(self) -> bool {
        matches!(self, Self::Station(p) if p.on_syndinet_store())
    }

    /// `can_run_on_flags`: whether it runs on a PDA (`PROGRAM_PDA`) or a console
    /// (`PROGRAM_CONSOLE`).
    pub fn runs_on(self, pda: bool) -> bool {
        match self {
            Self::Messenger => pda,
            // `PROGRAM_PDA | PROGRAM_LAPTOP`.
            Self::SecRecords | Self::MedRecords | Self::Signaler => pda,
            Self::Station(p) => p.runs_on(pda),
            _ => true,
        }
    }

    /// `download_access` (any one of them opens): accesses, and whether any command-flag
    /// access opens too.
    // tg lists `ACCESS_FLAG_COMMAND` (a flag number) among the records' access strings, which
    // never matches; the intent is that command staff may download them.
    pub fn download_access(self) -> (&'static [&'static str], bool) {
        match self {
            Self::CardMod => (&["command"], false),
            Self::MedRecords => (&["medical"], true),
            Self::SecRecords => (&["security"], true),
            Self::Newscaster => (&["library"], false),
            Self::Station(p) => (p.download_access(), false),
            // BandaStation `crew_manifest.download_access = list()`.
            _ => (&[], false),
        }
    }

    /// `undeletable`.
    pub fn undeletable(self) -> bool {
        matches!(self, Self::Messenger | Self::Downloader)
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

/// `/obj/item/modular_computer` `default_programs` + `/obj/item/modular_computer/pda`
/// `pda_programs` this port has, in `stored_files` order.
pub const PDA_PROGRAMS: [ProgramKind; 5] = [
    ProgramKind::Downloader,
    ProgramKind::Messenger,
    ProgramKind::Station(StationProgram::NtPay),
    ProgramKind::Notepad,
    ProgramKind::CrewManifest,
];

/// `/obj/machinery/modular_computer/preset/id`: the identification console.
pub const ID_CONSOLE: &str = "/obj/machinery/modular_computer/preset/id";

/// `/obj/machinery/modular_computer/preset/engineering`.
pub const ENGINEERING_CONSOLE: &str = "/obj/machinery/modular_computer/preset/engineering";

/// `default_programs` + `starting_programs` of the stationary consoles this port has
/// (`console_presets.dm`), by type: every one has the software hub, the identification console
/// runs the chat client, the ID writer and the crew list, the engineering one the alarm monitor.
// shortcut: no job management program on the ID console, no power or
// supermatter monitor on the engineering one, and no other console presets.
pub fn console_programs(path: &str) -> Option<Vec<ProgramKind>> {
    use ProgramKind::*;
    if is_under_path(path, ID_CONSOLE) {
        return Some(vec![Downloader, ChatClient, CardMod, CrewManifest]);
    }
    is_under_path(path, ENGINEERING_CONSOLE).then(|| vec![Downloader, AlarmMonitor])
}

/// `max_capacity` in GQ: a PDA 64, a head's PDA twice that, a console's processor 128.
pub fn max_capacity(path: &str) -> u32 {
    if is_under_path(path, HEADS_PDA) {
        128
    } else if is_under_path(path, "/obj/item/modular_computer/pda") {
        64
    } else {
        128
    }
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
        assert_eq!(pda_programs("/obj/item/modular_computer/pda/crew").len(), 5);
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
            pda_programs("/obj/item/modular_computer/pda/crew/heads/ce")[5..],
            [Atmosscan, AlarmMonitor, StatusDisplay]
        );
        // The paramedic inherits nothing new: its own list has medical records too.
        assert!(
            pda_programs("/obj/item/modular_computer/pda/crew/medical/paramedic")
                .contains(&MedRecords)
        );
        assert_eq!(
            pda_programs("/obj/item/modular_computer/pda/crew/heads/quartermaster")[5..],
            [StatusDisplay]
        );
        assert_eq!(
            pda_programs("/obj/item/modular_computer/pda/crew/curator").last(),
            Some(&Newscaster)
        );
        assert_eq!(
            console_programs(ENGINEERING_CONSOLE),
            Some(vec![Downloader, AlarmMonitor])
        );
        for p in ProgramKind::ALL {
            assert_eq!(ProgramKind::from_filename(p.filename()), Some(p));
            assert!(CATEGORIES.contains(&p.category()));
        }
    }

    #[test]
    fn capacity_follows_the_device() {
        assert_eq!(max_capacity("/obj/item/modular_computer/pda/crew"), 64);
        assert_eq!(
            max_capacity("/obj/item/modular_computer/pda/crew/heads/ce"),
            128
        );
        assert_eq!(max_capacity(ID_CONSOLE), 128);
        // Every computer's defaults fit its disk.
        let used: u32 = pda_programs("/obj/item/modular_computer/pda/crew/heads/ce")
            .iter()
            .map(|p| p.size())
            .sum();
        assert!(used <= 128);
    }
}

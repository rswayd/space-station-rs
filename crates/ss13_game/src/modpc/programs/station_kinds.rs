//! Catalog entries of the station programs whose state lives in [`super::station`]: NT Pay,
//! Lifeline, Custodial Locator, SiliConnect, Bot Remote Controller, Revelation and the
//! maintenance disk programs (Physical Scanner, Spectre-Meter).

use super::{CATEGORY_DEVICE, CATEGORY_EQUIPMENT, CATEGORY_SCIENCE, CATEGORY_SUPPLY};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StationProgram {
    /// `/datum/computer_file/program/nt_pay`.
    NtPay,
    /// `/datum/computer_file/program/radar/lifeline`.
    Lifeline,
    /// `/datum/computer_file/program/radar/custodial_locator`.
    CustodialLocator,
    /// `/datum/computer_file/program/borg_monitor` (SiliConnect).
    BorgMonitor,
    /// `/datum/computer_file/program/robocontrol`.
    RoboControl,
    /// `/datum/computer_file/program/revelation`: Syndicate store only.
    Revelation,
    /// `/datum/computer_file/program/maintenance/phys_scanner`: from a maintenance disk.
    PhysScanner,
    /// `/datum/computer_file/program/maintenance/spectre_meter`: from a maintenance disk.
    SpectreMeter,
    /// `/datum/computer_file/program/budgetorders` (NT Shopping Network): the supply window.
    BudgetOrders,
    /// `/datum/computer_file/program/civilianbounties`.
    CivBounties,
    /// `/datum/computer_file/program/bounty_board`.
    BountyBoard,
    /// `/datum/computer_file/program/science` (Nanotrasen Science Hub): the techweb window.
    ScienceHub,
    /// `/datum/computer_file/program/restock_tracker`.
    RestockTracker,
    /// `/datum/computer_file/program/scipaper_program` (NT Frontier, `crate::science::scipaper`).
    Frontier,
}

impl StationProgram {
    pub const ALL: [Self; 14] = [
        Self::NtPay,
        Self::Lifeline,
        Self::CustodialLocator,
        Self::BorgMonitor,
        Self::RoboControl,
        Self::Revelation,
        Self::PhysScanner,
        Self::SpectreMeter,
        Self::BudgetOrders,
        Self::CivBounties,
        Self::BountyBoard,
        Self::ScienceHub,
        Self::RestockTracker,
        Self::Frontier,
    ];

    pub fn filename(self) -> &'static str {
        match self {
            Self::NtPay => "ntpay",
            Self::Lifeline => "lifeline",
            Self::CustodialLocator => "custodiallocator",
            Self::BorgMonitor => "siliconnect",
            Self::RoboControl => "botkeeper",
            Self::Revelation => "revelation",
            Self::PhysScanner => "phys_scanner",
            Self::SpectreMeter => "spectre_meter",
            Self::BudgetOrders => "orderapp",
            Self::CivBounties => "bountyapp",
            Self::BountyBoard => "bountyboard",
            Self::ScienceHub => "experi_track",
            Self::RestockTracker => "restockapp",
            Self::Frontier => "ntfrontier",
        }
    }

    pub fn filedesc(self) -> &'static str {
        match self {
            Self::NtPay => "Nanotrasen Pay System",
            Self::Lifeline => "Lifeline",
            Self::CustodialLocator => "Custodial Locator",
            Self::BorgMonitor => "SiliConnect",
            Self::RoboControl => "BotKeeper",
            Self::Revelation => "Revelation",
            Self::PhysScanner => "Physical Scanner",
            Self::BudgetOrders => "NT Shopping Network",
            Self::CivBounties => "Civilian Bounties",
            Self::BountyBoard => "Bounty Board Request Network",
            Self::ScienceHub => "Nanotrasen Science Hub",
            Self::RestockTracker => "NT Restock Tracker",
            Self::SpectreMeter => "Spectre-Meter",
            Self::Frontier => "NT Frontier",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::NtPay => "money-bill-wave",
            Self::Lifeline => "heartbeat",
            Self::CustodialLocator => "broom",
            Self::BorgMonitor => "project-diagram",
            Self::RoboControl => "robot",
            Self::Revelation => "magnet",
            Self::PhysScanner => "barcode",
            Self::SpectreMeter => "ghost",
            Self::BudgetOrders => "cart-flatbed",
            Self::CivBounties => "boxes-stacked",
            Self::BountyBoard => "hand-holding-usd",
            Self::ScienceHub => "atom",
            Self::RestockTracker => "cash-register",
            Self::Frontier => "paper-plane",
        }
    }

    pub fn extended_desc(self) -> &'static str {
        match self {
            Self::NtPay => {
                "Приложение, которое локально (в вашем секторе) помогает переводить деньги или \
                 отслеживать расходы и прибыль."
            }
            Self::Lifeline => {
                "Эта программа позволяет отслеживать членов экипажа по датчикам их костюма."
            }
            Self::CustodialLocator => {
                "Эта программа позволяет отслеживать оборудование для уборки помещений."
            }
            Self::BorgMonitor => {
                "Эта программа позволяет удаленно отслеживать киборгов станции и отправлять им \
                 сообщения."
            }
            Self::RoboControl => "Программа для удаленного управления ботами станции.",
            Self::Revelation => {
                "Этот вирус может уничтожить жесткий диск системы, на которой он запущен. Он \
                 может быть замаскирован под другую программу, не являющуюся вредоносной. После \
                 запуска он уничтожит систему при следующем использовании."
            }
            Self::PhysScanner => {
                "Эта программа позволяет планшету сканировать физические объекты и выводить \
                 данные на экран."
            }
            Self::SpectreMeter => {
                "Программа, используемая для того, чтобы каким-то образом обнаружить присутствие \
                 призраков поблизости."
            }
            Self::BudgetOrders => {
                "Nanotrasen Shopping Requisition - сетевой интерфейс для закупки поставок с \
                 использованием счета отдела."
            }
            Self::CivBounties => {
                "Nanotrasen Civilian Bounty Requisition Network interface for displaying wanted \
                 items."
            }
            Self::BountyBoard => {
                "Мультиплатформенная сеть для размещения заявок по всей станции, с возможностью \
                 удаленной оплаты."
            }
            Self::ScienceHub => {
                "Подключитесь к внутреннему научному серверу, чтобы помочь в исследованиях на \
                 станции."
            }
            Self::RestockTracker => {
                "Сеть Nanotrasen IoT, в которой перечислены все торговые автоматы, находящиеся на \
                 станции, и насколько хорошо укомплектован каждый из них. Прибыльно!"
            }
            Self::Frontier => {
                "Программное обеспечение для публикации и навигации по научным статьям."
            }
        }
    }

    pub fn size(self) -> u32 {
        match self {
            Self::NtPay | Self::CustodialLocator | Self::PhysScanner => 2,
            Self::RestockTracker => 4,
            Self::Lifeline | Self::BorgMonitor | Self::CivBounties => 5,
            Self::BudgetOrders | Self::BountyBoard | Self::ScienceHub => 10,
            Self::Frontier => 12,
            Self::RoboControl => 6,
            Self::SpectreMeter => 7,
            Self::Revelation => 13,
        }
    }

    pub fn category(self) -> &'static str {
        match self {
            Self::NtPay | Self::Revelation => CATEGORY_DEVICE,
            Self::BorgMonitor | Self::RoboControl | Self::ScienceHub | Self::Frontier => {
                CATEGORY_SCIENCE
            }
            Self::BudgetOrders | Self::CivBounties | Self::BountyBoard | Self::RestockTracker => {
                CATEGORY_SUPPLY
            }
            Self::Lifeline | Self::CustodialLocator | Self::PhysScanner | Self::SpectreMeter => {
                CATEGORY_EQUIPMENT
            }
        }
    }

    /// `PROGRAM_ON_NTNET_STORE`: maintenance programs come only on disks, Revelation only from
    /// the Syndicate store.
    pub fn on_ntnet_store(self) -> bool {
        !matches!(
            self,
            Self::Revelation | Self::PhysScanner | Self::SpectreMeter
        )
    }

    /// `PROGRAM_ON_SYNDINET_STORE`: in the hub of an emagged computer.
    pub fn on_syndinet_store(self) -> bool {
        self == Self::Revelation
    }

    /// `can_run_on_flags`: the radars, the spectre meter (`PROGRAM_LAPTOP | PROGRAM_PDA`) and the
    /// physical scanner (`PROGRAM_PDA`) need a portable device.
    pub fn runs_on(self, pda: bool) -> bool {
        match self {
            Self::Lifeline
            | Self::CustodialLocator
            | Self::PhysScanner
            | Self::SpectreMeter
            | Self::BudgetOrders
            | Self::CivBounties => pda,
            _ => true,
        }
    }

    /// `download_access`.
    pub fn download_access(self) -> &'static [&'static str] {
        match self {
            Self::Lifeline => &["medical"],
            Self::CustodialLocator => &["janitor"],
            Self::BorgMonitor => &["robotics"],
            Self::RoboControl => &["robotics"],
            Self::ScienceHub => &["research"],
            Self::Frontier => &["ordnance", "science", "away_science", "atmospherics"],
            _ => &[],
        }
    }

    /// `run_access` (any one of them opens).
    pub fn run_access(self) -> &'static [&'static str] {
        match self {
            Self::ScienceHub => &["command", "research"],
            _ => &[],
        }
    }

    /// Programs that open an existing window with the computer as its machine (the supply
    /// console's `Cargo`, the R&D console's `Techweb`) instead of an NtOS screen.
    pub fn launcher(self) -> bool {
        matches!(self, Self::BudgetOrders | Self::ScienceHub)
    }

    /// The `/obj/item/disk/computer/maintenance/*` that carries it.
    pub fn from_maintenance_disk(path: &str) -> Option<Self> {
        match path {
            "/obj/item/disk/computer/maintenance/scanner" => Some(Self::PhysScanner),
            "/obj/item/disk/computer/maintenance/spectre_meter" => Some(Self::SpectreMeter),
            _ => None,
        }
    }
}

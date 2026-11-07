//! The manufacturing sorter's window contract, tgui `ManufacturingSorter.tsx`. The server half
//! is `crate::eng_machines::manufactorio`, the view [`crate::manufactorio_client`].

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// tgui `ManufacturingSorter`.
pub struct ManufacturingSorterUi;

impl UiInterface for ManufacturingSorterUi {
    const NAME: &'static str = "ManufacturingSorter";
    type Data = SorterData;
    type StaticData = ();
}

/// A `filters` entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SorterFilterRow {
    pub name: String,
    /// The `ref` act param.
    pub r#ref: String,
    pub inverted: bool,
    /// BYOND dir bits.
    pub dir: u8,
}

/// `/obj/machinery/power/manufacturing/sorter/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SorterData {
    pub unmet_dir: u8,
    pub filters: Vec<SorterFilterRow>,
}

pub const SORTER_DEL: &str = "del_filter";
pub const SORTER_NEW: &str = "new_filter";
pub const SORTER_ROTATE: &str = "rotate";
pub const SORTER_ROTATE_UNMET: &str = "rotate_unmet";
pub const SORTER_EDIT: &str = "edit";
pub const SORTER_SHIFT: &str = "shift";
pub const SORTER_REF: &str = "ref";
pub const SORTER_AMOUNT: &str = "amount";

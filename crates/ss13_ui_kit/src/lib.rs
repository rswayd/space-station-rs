//! Building blocks for 1:1 ports of tgui windows. Game-agnostic: no protocol types here.
//!
//! Widgets are BSN scene components named after tgui-core: a tgui JSX tree becomes a `bsn!`
//! tree with the same shape (see `examples/gallery.rs`).

use bevy::prelude::*;

pub mod button;
pub mod dropdown;
pub mod icon;
pub mod image_button;
pub mod input;
pub mod labeled_list;
pub mod layout;
pub mod modal;
pub mod notice;
pub mod overlay;
pub mod progress;
pub mod scroll;
pub mod section;
pub mod table;
pub mod tabs;
pub mod theme;
pub mod tooltip;
pub mod virtual_list;
pub mod window;

pub use button::{Act, ActParams, ActScope, Button, UiAct};
pub use dropdown::{Dropdown, DropdownSelected};
pub use icon::Icon;
pub use image_button::ImageButton;
pub use input::{Input, InputChanged, InputSubmitted, NumberInput, NumberInputChanged, TextArea};
pub use labeled_list::{LabeledList, LabeledListDivider, LabeledListItem};
pub use layout::{Divider, Flex, FlexItem, Stack, StackDivider, StackItem, UiBox, half_unit, unit};
pub use modal::{Dimmer, Modal};
pub use notice::NoticeBox;
pub use overlay::Placement;
pub use progress::{AnimatedNumber, ProgressBar};
pub use section::Section;
pub use table::{Table, TableCell, TableRow};
pub use tabs::{Tabs, TabsTab};
pub use theme::UiColor;
pub use tooltip::Tooltip;
pub use virtual_list::{VirtualList, VirtualRow};
pub use window::{Window, WindowClose, WindowContent};

/// `use ss13_ui_kit::prelude::*;` in UI code: Bevy's prelude with the tgui names winning over
/// Bevy's `Window`/`Button`.
pub mod prelude {
    pub use bevy::prelude::*;
    pub use bevy::text::Justify;

    pub use crate::{
        Act, ActParams, ActScope, AnimatedNumber, Button, Dimmer, Divider, Dropdown,
        DropdownSelected, Flex, FlexItem, Icon, ImageButton, Input, InputChanged, InputSubmitted,
        LabeledList, LabeledListDivider, LabeledListItem, Modal, NoticeBox, NumberInput,
        NumberInputChanged, Placement, ProgressBar, Section, Stack, StackDivider, StackItem, Table,
        TableCell, TableRow, Tabs, TabsTab, TextArea, Tooltip, UiAct, UiBox, UiColor, VirtualList,
        VirtualRow, Window, WindowClose, WindowContent, half_unit, unit,
    };
}

/// Systems that read this frame's layout and write `Node` for the next one (label and column
/// widths, scrollbar tracks, overlay placement): a freshly spawned tree needs a frame or two
/// after its first layout to look final. Run after it to see the settled result.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LayoutFeedback;

pub struct UiKitPlugin;

impl Plugin for UiKitPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            PostUpdate,
            LayoutFeedback.after(bevy::ui::UiSystems::Layout),
        );
        app.add_plugins((
            theme::plugin,
            icon::plugin,
            overlay::plugin,
            tooltip::plugin,
            layout::plugin,
            button::plugin,
            image_button::plugin,
            window::plugin,
            section::plugin,
            scroll::plugin,
        ))
        .add_plugins((
            tabs::plugin,
            table::plugin,
            dropdown::plugin,
            progress::plugin,
            virtual_list::plugin,
            labeled_list::plugin,
            input::InputWidgetsPlugin,
        ));
    }
}

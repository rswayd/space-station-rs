//! `JobSelection` (the latejoin menu): `tgui/packages/tgui/interfaces/JobSelection.tsx` and
//! `code/modules/mob/dead/new_player/latejoin_menu.dm`. The server half lives in `ss13_game::jobs`.
//!
//! DM keys departments and jobs by name in associative lists; here both data halves are vectors
//! in the same order (static: names, colors, descriptions; data: slots), which postcard sends
//! without repeating the names every update.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct JobSelectionUi;

/// `departments_static[name].jobs[title]`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct JobStatic {
    pub title: String,
    pub command: bool,
    pub description: String,
}

/// `departments_static[name]`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DepartmentStatic {
    /// English `department_name`; the view translates it.
    pub name: String,
    /// `ui_color`, `#rrggbb`.
    pub color: String,
    pub jobs: Vec<JobStatic>,
}

/// `ui_static_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct JobSelectionStatic {
    pub departments: Vec<DepartmentStatic>,
}

/// `departments[name].jobs[title]`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct JobSlots {
    pub used_slots: u32,
    /// `total_positions`; `None` is DM's `"∞"`.
    pub open_slots: Option<u32>,
    pub prioritized: bool,
    /// Why the job cannot be picked now; `None`: available.
    pub unavailable_reason: Option<String>,
}

/// `departments[name]`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DepartmentSlots {
    /// Free slots summed over the department; `None` is `"∞"`.
    pub open_slots: Option<u32>,
    pub jobs: Vec<JobSlots>,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct JobSelectionData {
    pub departments: Vec<DepartmentSlots>,
    /// `DisplayTimeText(world.time - SSticker.round_start_time, round_seconds_to = 1)`.
    pub round_duration: String,
    pub shuttle_status: String,
}

impl UiInterface for JobSelectionUi {
    const NAME: &'static str = "JobSelection";
    type Data = JobSelectionData;
    type StaticData = JobSelectionStatic;
}

/// The act both halves agree on: `act('select_job', { job })`.
pub const SELECT_JOB: &str = "select_job";
/// `params["job"] == "Random"`.
pub const RANDOM_JOB: &str = "Random";

/// tgui-core `Color.darken(percent)`: each channel scaled by `1 - percent / 100`.
pub fn darken(rgb: [u8; 3], percent: f32) -> [u8; 3] {
    let k = 1.0 - percent / 100.0;
    rgb.map(|c| (f32::from(c) * k).round().clamp(0.0, 255.0) as u8)
}

/// `#rrggbb` (or `#rgb`); `None` for anything else.
pub fn parse_hex(color: &str) -> Option<[u8; 3]> {
    let hex = color.strip_prefix('#')?;
    let channel = |s: &str| u8::from_str_radix(s, 16).ok();
    match hex.len() {
        6 => Some([
            channel(&hex[0..2])?,
            channel(&hex[2..4])?,
            channel(&hex[4..6])?,
        ]),
        3 => {
            let c = |i: usize| channel(&hex[i..=i]).map(|v| v * 17);
            Some([c(0)?, c(1)?, c(2)?])
        }
        _ => None,
    }
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::app::Propagate;
    use bevy::picking::hover::Hovered;
    use bevy::scene::EntityScene;
    use bevy::text::TextColor;
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::theme::{Bold, FontPx};

    use super::{
        DepartmentSlots, DepartmentStatic, JobSelectionData, JobSelectionStatic, JobSelectionUi,
        JobSlots, JobStatic, RANDOM_JOB, SELECT_JOB, darken, parse_hex,
    };
    use crate::client::ClientUi;
    use crate::interfaces::ru_jobs::{department_ru, job_icon, job_ru};

    /// `columns: 20em` in a 1012 px window: three columns.
    const COLUMNS: usize = 3;
    /// `fontSize: 1.1rem`.
    const JOB_FONT: f32 = 13.2;
    /// Height of a job button and of a department title, for balancing the columns.
    const JOB_ROW_PX: f32 = 24.0;
    const TITLE_PX: f32 = 36.0;

    fn rgb(c: [u8; 3]) -> Color {
        Color::srgb_u8(c[0], c[1], c[2])
    }

    /// `'#949494'` / `'#616161'`.
    const UNAVAILABLE_BG: [u8; 3] = [0x94, 0x94, 0x94];
    const UNAVAILABLE_TEXT: [u8; 3] = [0x61, 0x61, 0x61];
    /// `'#16fc0f'`.
    const PRIORITIZED_BG: [u8; 3] = [0x16, 0xfc, 0x0f];

    impl ClientUi for JobSelectionUi {
        const WIDTH: f32 = 1012.0;
        const HEIGHT: f32 = 666.0;

        fn content(st: &JobSelectionStatic, data: &JobSelectionData) -> impl Scene {
            let header = format!("It is currently {} into the shift.", data.round_duration);
            // CSS columns fill top to bottom with `break-inside: avoid`: split departments in
            // order where the running height passes each column's share.
            let heights: Vec<f32> = st
                .departments
                .iter()
                .map(|d| TITLE_PX + JOB_ROW_PX * d.jobs.len() as f32)
                .collect();
            let total: f32 = heights.iter().sum();
            let mut columns: Vec<Vec<usize>> = vec![Vec::new(); COLUMNS];
            let mut running = 0.0;
            for (i, h) in heights.iter().enumerate() {
                let column = ((running + h / 2.0) / (total / COLUMNS as f32)) as usize;
                columns[column.min(COLUMNS - 1)].push(i);
                running += h;
            }
            let empty = DepartmentSlots::default();
            let columns: Vec<_> = columns
                .into_iter()
                .map(|indexes| {
                    let departments: Vec<_> = indexes
                        .into_iter()
                        .map(|i| {
                            let slots = data.departments.get(i).unwrap_or(&empty);
                            department(&st.departments[i], slots)
                        })
                        .collect();
                    bsn! {
                        Node {
                            flex_direction: FlexDirection::Column,
                            flex_basis: px(0),
                            flex_grow: 1.0,
                            min_width: px(0),
                        }
                        Children [ {departments} ]
                    }
                })
                .collect();
            let shuttle = (!data.shuttle_status.is_empty()).then(|| {
                let status = data.shuttle_status.clone();
                EntityScene(bsn! { @NoticeBox { @color: UiColor::Blue } Children [ Text(status) ] })
            });
            bsn! {
                @WindowContent
                Children [
                    {shuttle}
                    --
                    @Section {
                        @title: header,
                        @fill: true,
                        @scrollable: true,
                        @buttons: bsn! {
                            @Button {
                                @content: "Случайная профессия!",
                                @tooltip: "Случайно выбрать профессию. Вы можете повторно выбирать случайную профессию или отказаться от этого.",
                            }
                            Act({SELECT_JOB.to_owned()})
                            ActParams({vec![("job".to_owned(), RANDOM_JOB.to_owned())]})
                        },
                    }
                    Children [
                        Node { flex_direction: FlexDirection::Row, column_gap: px(12) }
                        Children [ {columns} ]
                    ]
                ]
            }
        }
    }

    /// `DepartmentEntry`: a `StyleableSection` in the department color.
    fn department(st: &DepartmentStatic, slots: &DepartmentSlots) -> impl Scene + use<> {
        let color = parse_hex(&st.color).unwrap_or([0x96, 0x89, 0xdb]);
        let name = department_ru(&st.name).unwrap_or(&st.name).to_owned();
        let open = slots
            .open_slots
            .map_or_else(|| "∞".to_owned(), |n| n.to_string());
        let open = format!("позиций доступно: {open}");
        let empty = JobSlots::default();
        let jobs: Vec<_> = st
            .jobs
            .iter()
            .enumerate()
            .map(|(i, job)| job_entry(job, slots.jobs.get(i).unwrap_or(&empty), color))
            .collect();
        let (title_border, slots_color, text) = (
            rgb(darken(color, 50.0)),
            rgb(darken(color, 60.0)),
            rgb(darken(color, 80.0)),
        );
        bsn! {
            Node {
                flex_direction: FlexDirection::Column,
                margin: UiRect::bottom(px(12)),
            }
            BackgroundColor(rgb(color))
            Propagate::<TextColor>(TextColor(text))
            Children [
                Node {
                    padding: UiRect::all(px(6)),
                    border: UiRect::bottom(px(2)),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                }
                BorderColor::all(title_border)
                Children [
                    Node
                    Propagate::<FontPx>(FontPx(14.0))
                    Propagate::<Bold>(Bold(true))
                    Children [ Text(name) ]
                    --
                    Node
                    Propagate::<FontPx>(FontPx(12.0))
                    Propagate::<TextColor>(TextColor(slots_color))
                    Children [ Text(open) ]
                ]
                --
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(2),
                    padding: UiRect::all(px(6)),
                }
                Children [ {jobs} ]
            ]
        }
    }

    /// `JobEntry`: a fluid button in the darkened department color.
    fn job_entry(job: &JobStatic, slots: &JobSlots, color: [u8; 3]) -> impl Scene + use<> {
        let unavailable = slots.unavailable_reason.is_some();
        let (bg, fg) = if unavailable {
            (UNAVAILABLE_BG, UNAVAILABLE_TEXT)
        } else if slots.prioritized {
            (PRIORITIZED_BG, darken(color, 90.0))
        } else {
            (darken(color, 10.0), darken(color, 90.0))
        };
        let tooltip = match &slots.unavailable_reason {
            Some(reason) => reason.clone(),
            None if slots.prioritized => {
                format!(
                    "The HoP wants more people in this job!\n{}",
                    job.description
                )
            }
            None => job.description.clone(),
        };
        let name = job_ru(&job.title).unwrap_or(&job.title).to_owned();
        let total = slots
            .open_slots
            .map_or_else(|| "∞".to_owned(), |n| n.to_string());
        let counts = format!("{} / {total}", slots.used_slots);
        let icon = job_icon(&job.title).map(|icon| {
            EntityScene(bsn! {
                Node { margin: UiRect::right(px(4)), min_width: px(16), justify_content: JustifyContent::Center }
                Children [ @Icon { @name: icon } ]
            })
        });
        let bold = job.command;
        let act = (!unavailable).then(|| {
            let params = vec![("job".to_owned(), job.title.clone())];
            bsn! { Act({SELECT_JOB.to_owned()}) ActParams(params) }
        });
        let entry = bsn! {
            Node {
                min_height: px(20),
                padding: UiRect::horizontal(px(6)),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(px(2)),
            }
            bevy::ui_widgets::Button
            Hovered
            Name({job.title.clone()})
            BackgroundColor(rgb(bg))
            Propagate::<TextColor>(TextColor(rgb(fg)))
            Propagate::<FontPx>(FontPx(JOB_FONT))
            Tooltip { content: tooltip, position: Placement::Top }
            Children [
                {icon}
                --
                Node { flex_grow: 1.0 }
                Propagate::<Bold>(Bold(bold))
                Children [ Text(name) ]
                --
                Node
                Children [ Text(counts) ]
            ]
        };
        (entry, act)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_follow_tgui_core() {
        assert_eq!(parse_hex("#d46a78"), Some([0xd4, 0x6a, 0x78]));
        assert_eq!(parse_hex("#fff"), Some([255, 255, 255]));
        assert_eq!(parse_hex("red"), None);
        assert_eq!(darken([200, 100, 0], 10.0), [180, 90, 0]);
        assert_eq!(darken([200, 100, 0], 90.0), [20, 10, 0]);
    }
}

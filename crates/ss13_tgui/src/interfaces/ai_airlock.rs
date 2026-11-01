//! `AiAirlock` (an airlock's silicon control window): `tgui/packages/tgui/interfaces/
//! AiAirlock.jsx` and `/obj/machinery/door/airlock/ui_data` (`code/game/machinery/doors/
//! airlock.dm`). The server half lives with the game (`ss13_game::silicon::airlock`).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct AiAirlockUi;

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AiAirlockData {
    pub power: AirlockPower,
    /// 2 safe, 0 electrified.
    pub shock: u8,
    /// `secondsElectrified`: seconds left, -1 permanent, 0 not electrified.
    pub shock_timeleft: i32,
    pub id_scanner: bool,
    pub emergency: bool,
    pub locked: bool,
    pub feedback: bool,
    pub safe: bool,
    pub speed: bool,
    pub welded: bool,
    pub opened: bool,
    pub wires: AirlockWires,
}

/// `power`: 2 online, 0 offline; seconds until it comes back.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct AirlockPower {
    pub main: u8,
    pub main_timeleft: i32,
    pub backup: u8,
    pub backup_timeleft: i32,
}

/// `wires`: true while intact.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct AirlockWires {
    pub main_1: bool,
    pub main_2: bool,
    pub backup_1: bool,
    pub backup_2: bool,
    pub shock: bool,
    pub id_scanner: bool,
    pub bolts: bool,
    pub feedback: bool,
    pub safe: bool,
    pub timing: bool,
}

impl UiInterface for AiAirlockUi {
    const NAME: &'static str = "AiAirlock";
    type Data = AiAirlockData;
    type StaticData = ();
}

/// `dangerMap` (sic: 2 is "good"/Offline, 0 is "bad"/Optimal in the JSX; only the color is
/// shown).
pub fn danger_color(level: u8) -> &'static str {
    match level {
        2 => "good",
        1 => "average",
        _ => "bad",
    }
}

/// The text after "Online"/"Offline": cut wires, else the seconds left.
pub fn power_note(intact: bool, timeleft: i32) -> String {
    if !intact {
        "[Wires have been cut!]".to_owned()
    } else if timeleft > 0 {
        format!("[{timeleft}s]")
    } else {
        String::new()
    }
}

/// The door control note: `[Door is bolted and welded!]`.
pub fn door_note(locked: bool, welded: bool) -> String {
    if !(locked || welded) {
        return String::new();
    }
    format!(
        "[Door is {}{}{}!]",
        if locked { "bolted" } else { "" },
        if locked && welded { " and " } else { "" },
        if welded { "welded" } else { "" }
    )
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;

    use super::{AiAirlockData, AiAirlockUi, danger_color, door_note, power_note};
    use crate::client::ClientUi;

    fn color(name: &str) -> UiColor {
        match name {
            "good" => UiColor::Good,
            "average" => UiColor::Average,
            _ => UiColor::Bad,
        }
    }

    /// A toggle row: label (bad color), an Enabled/Disabled button, a cut-wire note.
    fn toggle(
        label: &'static str,
        on: bool,
        wire: bool,
        action: &'static str,
        icons: (&'static str, &'static str),
        texts: (&'static str, &'static str),
    ) -> Box<dyn Scene> {
        let (icon, content) = if on {
            (icons.0, texts.0)
        } else {
            (icons.1, texts.1)
        };
        let note = if wire {
            String::new()
        } else {
            "[Wires have been cut!]".to_owned()
        };
        Box::new(bsn! {
            @LabeledListItem {
                @label: label,
                @color: UiColor::Bad,
                @buttons: bsn! {
                    @Button { @icon: icon, @content: content }
                    Button { selected: on, disabled: {!wire} }
                    Act(action)
                },
            }
            Children [ Text(note) ]
        })
    }

    impl ClientUi for AiAirlockUi {
        const WIDTH: f32 = 500.0;
        const HEIGHT: f32 = 390.0;

        fn content(_: &(), data: &AiAirlockData) -> impl Scene {
            let w = data.wires;
            let p = data.power;
            let main_color = color(danger_color(p.main));
            let backup_color = color(danger_color(p.backup));
            let shock_color = color(danger_color(data.shock));
            let main_text = format!(
                "{} {}",
                if p.main != 0 { "Online" } else { "Offline" },
                power_note(w.main_1 && w.main_2, p.main_timeleft)
            );
            let backup_text = format!(
                "{} {}",
                if p.backup != 0 { "Online" } else { "Offline" },
                power_note(w.backup_1 && w.backup_2, p.backup_timeleft)
            );
            let shock_note = if !w.shock {
                "[Wires have been cut!]".to_owned()
            } else if data.shock_timeleft > 0 {
                format!("[{}s]", data.shock_timeleft)
            } else if data.shock_timeleft == -1 {
                "[Permanent]".to_owned()
            } else {
                String::new()
            };
            let shock_text = format!(
                "{} {shock_note}",
                if data.shock == 2 {
                    "Safe"
                } else {
                    "Electrified"
                }
            );
            let (main_off, backup_off) = (p.main == 0, p.backup == 0);
            let restore_off = !(w.shock && data.shock == 0);
            let shock_cut = !w.shock;
            let id_scan = toggle(
                "ID Scan",
                data.id_scanner,
                w.id_scanner,
                "idscan-toggle",
                ("power-off", "times"),
                ("Enabled", "Disabled"),
            );
            let (emergency_icon, emergency_text) = if data.emergency {
                ("power-off", "Enabled")
            } else {
                ("times", "Disabled")
            };
            let emergency = data.emergency;
            let bolts = toggle(
                "Door Bolts",
                data.locked,
                w.bolts,
                "bolt-toggle",
                ("lock", "unlock"),
                ("Lowered", "Raised"),
            );
            let feedback = toggle(
                "Door Feedback",
                data.feedback,
                w.feedback,
                "light-toggle",
                ("power-off", "times"),
                ("Enabled", "Disabled"),
            );
            let safe = toggle(
                "Door Force Sensors",
                data.safe,
                w.safe,
                "safe-toggle",
                ("power-off", "times"),
                ("Enabled", "Disabled"),
            );
            let speed = toggle(
                "Door Timing Safety",
                data.speed,
                w.timing,
                "speed-toggle",
                ("power-off", "times"),
                ("Enabled", "Disabled"),
            );
            let (door_icon, door_text) = if data.opened {
                ("sign-out-alt", "Open")
            } else {
                ("sign-in-alt", "Closed")
            };
            let opened = data.opened;
            let door_blocked = data.locked || data.welded;
            let door_text_note = door_note(data.locked, data.welded);
            bsn! {
                @WindowContent
                Children [
                    @Section { @title: "Power Status" }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem {
                                @label: "Main",
                                @color: main_color,
                                @buttons: bsn! {
                                    @Button { @icon: "lightbulb-o", @content: "Disrupt" }
                                    Button { disabled: main_off }
                                    Act("disrupt-main")
                                },
                            }
                            Children [ Text(main_text) ]
                            --
                            @LabeledListItem {
                                @label: "Backup",
                                @color: backup_color,
                                @buttons: bsn! {
                                    @Button { @icon: "lightbulb-o", @content: "Disrupt" }
                                    Button { disabled: backup_off }
                                    Act("disrupt-backup")
                                },
                            }
                            Children [ Text(backup_text) ]
                            --
                            @LabeledListItem {
                                @label: "Electrify",
                                @color: shock_color,
                                @buttons: bsn_list! {
                                    @Button { @icon: "wrench", @content: "Restore" }
                                    Button { disabled: restore_off }
                                    Act("shock-restore")
                                    --
                                    @Button { @icon: "bolt", @content: "Temporary" }
                                    Button { disabled: shock_cut }
                                    Act("shock-temp")
                                    --
                                    @Button { @icon: "bolt", @content: "Permanent" }
                                    Button { disabled: shock_cut }
                                    Act("shock-perm")
                                },
                            }
                            Children [ Text(shock_text) ]
                        ]
                    ]
                    --
                    @Section { @title: "Access and Door Control" }
                    Children [
                        @LabeledList
                        Children [
                            {EntityScene(id_scan)}
                            --
                            @LabeledListItem {
                                @label: "Emergency Access",
                                @buttons: bsn! {
                                    @Button { @icon: emergency_icon, @content: emergency_text }
                                    Button { selected: emergency }
                                    Act("emergency-toggle")
                                },
                            }
                            --
                            @LabeledListDivider
                            --
                            {EntityScene(bolts)}
                            --
                            {EntityScene(feedback)}
                            --
                            {EntityScene(safe)}
                            --
                            {EntityScene(speed)}
                            --
                            @LabeledListDivider
                            --
                            @LabeledListItem {
                                @label: "Door Control",
                                @color: UiColor::Bad,
                                @buttons: bsn! {
                                    @Button { @icon: door_icon, @content: door_text }
                                    Button { selected: opened, disabled: door_blocked }
                                    Act("open-close")
                                },
                            }
                            Children [ Text(door_text_note) ]
                        ]
                    ]
                ]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_follow_the_jsx() {
        assert_eq!(power_note(false, 10), "[Wires have been cut!]");
        assert_eq!(power_note(true, 10), "[10s]");
        assert_eq!(power_note(true, 0), "");
        assert_eq!(door_note(true, true), "[Door is bolted and welded!]");
        assert_eq!(door_note(false, true), "[Door is welded!]");
        assert_eq!(door_note(false, false), "");
        assert_eq!(danger_color(2), "good");
    }
}

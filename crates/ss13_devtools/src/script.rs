//! The script format: one step per line, `#` starts a comment, arguments are words or
//! double-quoted strings (`\"` and `\\` escape inside quotes). Waits take an optional trailing
//! `timeout=<secs>`.
//!
//! ```text
//! wait_connected timeout=30
//! wait_until controlled
//! wait_until entities 500
//! wait_until chat "Привет"
//! wait_secs 1.5
//! key_down W
//! key_up W
//! tap Enter
//! type "Привет, станция!"
//! click_world 120 80
//! click_tile 1 0
//! click_tile 1 0 2 29      # at icon pixel (2, 29) of the tile, from its bottom-left corner
//! rclick_tile -1 0          # right click on the tile west of the own mob
//! click_ui "Сообщение"
//! drag_tile 1 0 0 0        # press on the tile east of the own mob, release on its own tile
//! screenshot "before.png"
//! log_state
//! quit
//! ```

use bevy::input::keyboard::{Key, KeyCode, NativeKeyCode};

pub const DEFAULT_CONNECT_TIMEOUT: f64 = 30.0;
pub const DEFAULT_WAIT_TIMEOUT: f64 = 10.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    WaitConnected {
        timeout: f64,
    },
    WaitSecs(f64),
    WaitUntil {
        cond: Condition,
        timeout: f64,
    },
    KeyDown(KeyName),
    KeyUp(KeyName),
    /// Pressed for one frame, released the next.
    Tap(KeyName),
    /// One key press per character, with the character as the key's text (what a keyboard
    /// layout produces), all in one frame.
    Type(String),
    /// Absolute tile on the own mob's z-level, at `pixel` (from its bottom-left corner;
    /// default the center, 16 16).
    ClickWorld {
        x: i32,
        y: i32,
        pixel: [i32; 2],
    },
    /// Tile relative to the own mob, at `pixel` as above.
    ClickTile {
        dx: i32,
        dy: i32,
        pixel: [i32; 2],
    },
    /// A right click on the center of a tile relative to the own mob.
    RightClickTile {
        dx: i32,
        dy: i32,
    },
    /// The first visible UI node whose `Text` contains, or whose `Name` equals, the string.
    ClickUi(String),
    /// Left button pressed at the center of tile `from`, moved, released at the center of `to`
    /// (both relative to the own mob): a click-drag (BYOND `MouseDrop`).
    DragTile {
        from: [i32; 2],
        to: [i32; 2],
    },
    /// Mouse wheel over that node: `lines` down (negative: up), as a wheel scrolls a list.
    ScrollUi {
        needle: String,
        lines: i32,
    },
    Screenshot(String),
    LogState,
    Quit,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Condition {
    /// This client controls a mob.
    Controlled,
    /// At least this many replicated entities.
    Entities(usize),
    /// A chat entry contains the text.
    Chat(String),
}

/// A key as both the physical code (`ButtonInput<KeyCode>`) and the logical key plus text
/// (`ButtonInput<Key>`, text fields).
#[derive(Clone, Debug, PartialEq)]
pub struct KeyName {
    pub code: KeyCode,
    pub logical: Key,
    pub text: Option<String>,
}

impl KeyName {
    /// The key a US layout types `c` with; other characters as an unidentified key with that
    /// text, like a non-Latin layout key whose physical position the script does not know.
    pub fn for_char(c: char) -> Self {
        let text = c.to_string();
        let code = match c {
            ' ' => {
                return Self {
                    code: KeyCode::Space,
                    logical: Key::Space,
                    text: Some(text),
                };
            }
            'a'..='z' | 'A'..='Z' => letter_code(c.to_ascii_uppercase()),
            '0'..='9' => digit_code(c),
            _ => KeyCode::Unidentified(NativeKeyCode::Unidentified),
        };
        Self {
            code,
            logical: Key::Character(text.as_str().into()),
            text: Some(text),
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        let named = |code, logical| {
            Some(Self {
                code,
                logical,
                text: None,
            })
        };
        let mut chars = name.chars();
        if let (Some(c), None) = (chars.next(), chars.next())
            && c.is_ascii_alphanumeric()
        {
            return Some(Self::for_char(c.to_ascii_lowercase()));
        }
        let lower = name.to_ascii_lowercase();
        if let Some(digit) = lower.strip_prefix("numpad")
            && let Ok(n @ 0..=9) = digit.parse::<u8>()
        {
            use KeyCode::*;
            const NUMPAD: [KeyCode; 10] = [
                Numpad0, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8,
                Numpad9,
            ];
            return Some(Self {
                code: NUMPAD[usize::from(n)],
                logical: Key::Character(n.to_string().as_str().into()),
                text: None,
            });
        }
        if let Some(n) = lower.strip_prefix('f')
            && let Ok(n @ 1..=12) = n.parse::<usize>()
        {
            use KeyCode::*;
            const F: [(KeyCode, Key); 12] = [
                (F1, Key::F1),
                (F2, Key::F2),
                (F3, Key::F3),
                (F4, Key::F4),
                (F5, Key::F5),
                (F6, Key::F6),
                (F7, Key::F7),
                (F8, Key::F8),
                (F9, Key::F9),
                (F10, Key::F10),
                (F11, Key::F11),
                (F12, Key::F12),
            ];
            let (code, key) = F[n - 1].clone();
            return named(code, key);
        }
        match lower.as_str() {
            "enter" | "return" => named(KeyCode::Enter, Key::Enter),
            "escape" | "esc" => named(KeyCode::Escape, Key::Escape),
            "space" => Some(Self::for_char(' ')),
            "tab" => named(KeyCode::Tab, Key::Tab),
            "backspace" => named(KeyCode::Backspace, Key::Backspace),
            "delete" => named(KeyCode::Delete, Key::Delete),
            "up" | "arrowup" => named(KeyCode::ArrowUp, Key::ArrowUp),
            "down" | "arrowdown" => named(KeyCode::ArrowDown, Key::ArrowDown),
            "left" | "arrowleft" => named(KeyCode::ArrowLeft, Key::ArrowLeft),
            "right" | "arrowright" => named(KeyCode::ArrowRight, Key::ArrowRight),
            "shift" => named(KeyCode::ShiftLeft, Key::Shift),
            "ctrl" | "control" => named(KeyCode::ControlLeft, Key::Control),
            "alt" => named(KeyCode::AltLeft, Key::Alt),
            _ => None,
        }
    }
}

fn letter_code(c: char) -> KeyCode {
    use KeyCode::*;
    const LETTERS: [KeyCode; 26] = [
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO,
        KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
    ];
    LETTERS[(c as u8 - b'A') as usize]
}

fn digit_code(c: char) -> KeyCode {
    use KeyCode::*;
    const DIGITS: [KeyCode; 10] = [
        Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
    ];
    DIGITS[(c as u8 - b'0') as usize]
}

/// A parsed step and its 1-based source line, for error messages.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub number: usize,
    pub text: String,
    pub step: Step,
}

pub fn parse(source: &str) -> Result<Vec<Line>, String> {
    let mut lines = Vec::new();
    for (i, raw) in source.lines().enumerate() {
        let number = i + 1;
        let words = split(raw).map_err(|e| format!("line {number}: {e}"))?;
        if words.is_empty() {
            continue;
        }
        let step = parse_step(&words).map_err(|e| format!("line {number}: {e}: {}", raw.trim()))?;
        lines.push(Line {
            number,
            text: raw.trim().to_string(),
            step,
        });
    }
    Ok(lines)
}

/// Words and quoted strings up to an unquoted `#`.
fn split(line: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '#' {
            break;
        } else if c == '"' {
            chars.next();
            let mut word = String::new();
            loop {
                match chars.next() {
                    Some('"') => break,
                    Some('\\') => match chars.next() {
                        Some(c @ ('"' | '\\')) => word.push(c),
                        Some('n') => word.push('\n'),
                        other => return Err(format!("bad escape \\{}", other.unwrap_or(' '))),
                    },
                    Some(c) => word.push(c),
                    None => return Err("unterminated string".into()),
                }
            }
            words.push(word);
        } else {
            let mut word = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                word.push(c);
                chars.next();
            }
            words.push(word);
        }
    }
    Ok(words)
}

fn parse_step(words: &[String]) -> Result<Step, String> {
    let (name, mut args) = (words[0].as_str(), words[1..].to_vec());
    // `timeout=N` may end any wait.
    let mut timeout = None;
    if let Some(last) = args.last()
        && let Some(secs) = last.strip_prefix("timeout=")
    {
        timeout = Some(secs_arg(secs)?);
        args.pop();
    }
    let arity = |n: usize| {
        if args.len() == n {
            Ok(())
        } else {
            Err(format!(
                "`{name}` takes {n} argument(s), got {}",
                args.len()
            ))
        }
    };
    let key = |s: &str| KeyName::parse(s).ok_or_else(|| format!("unknown key `{s}`"));
    let int = |s: &str| {
        s.parse::<i32>()
            .map_err(|_| format!("not an integer: `{s}`"))
    };
    // `click_world`/`click_tile`: 2 words, or 4 with an icon pixel of the tile.
    let click_pixel = |args: &[String]| match args {
        [_, _] => Ok([16, 16]),
        [_, _, px, py] => {
            let pixel = [int(px)?, int(py)?];
            if pixel.iter().all(|p| (0..32).contains(p)) {
                Ok(pixel)
            } else {
                Err(format!("`{name}`: pixel 0..=31, got {pixel:?}"))
            }
        }
        _ => Err(format!(
            "`{name}` takes 2 or 4 arguments, got {}",
            args.len()
        )),
    };
    if timeout.is_some() && !matches!(name, "wait_connected" | "wait_until") {
        return Err(format!("`{name}` takes no timeout"));
    }
    let step = match name {
        "wait_connected" => {
            arity(0)?;
            Step::WaitConnected {
                timeout: timeout.unwrap_or(DEFAULT_CONNECT_TIMEOUT),
            }
        }
        "wait_secs" => {
            arity(1)?;
            Step::WaitSecs(secs_arg(&args[0])?)
        }
        "wait_until" => {
            let cond = match args.first().map(String::as_str) {
                Some("controlled") => {
                    arity(1)?;
                    Condition::Controlled
                }
                Some("entities") => {
                    arity(2)?;
                    Condition::Entities(
                        args[1]
                            .parse()
                            .map_err(|_| format!("not a count: `{}`", args[1]))?,
                    )
                }
                Some("chat") => {
                    arity(2)?;
                    Condition::Chat(args[1].clone())
                }
                other => {
                    return Err(format!(
                        "unknown condition {other:?}: expected controlled, entities N, chat \"text\""
                    ));
                }
            };
            Step::WaitUntil {
                cond,
                timeout: timeout.unwrap_or(DEFAULT_WAIT_TIMEOUT),
            }
        }
        "key_down" => {
            arity(1)?;
            Step::KeyDown(key(&args[0])?)
        }
        "key_up" => {
            arity(1)?;
            Step::KeyUp(key(&args[0])?)
        }
        "tap" => {
            arity(1)?;
            Step::Tap(key(&args[0])?)
        }
        "type" => {
            arity(1)?;
            Step::Type(args[0].clone())
        }
        "click_world" => {
            let pixel = click_pixel(&args)?;
            Step::ClickWorld {
                x: int(&args[0])?,
                y: int(&args[1])?,
                pixel,
            }
        }
        "click_tile" => {
            let pixel = click_pixel(&args)?;
            Step::ClickTile {
                dx: int(&args[0])?,
                dy: int(&args[1])?,
                pixel,
            }
        }
        "rclick_tile" => {
            arity(2)?;
            Step::RightClickTile {
                dx: int(&args[0])?,
                dy: int(&args[1])?,
            }
        }
        "drag_tile" => {
            arity(4)?;
            Step::DragTile {
                from: [int(&args[0])?, int(&args[1])?],
                to: [int(&args[2])?, int(&args[3])?],
            }
        }
        "click_ui" => {
            arity(1)?;
            Step::ClickUi(args[0].clone())
        }
        "scroll_ui" => {
            arity(2)?;
            Step::ScrollUi {
                needle: args[0].clone(),
                lines: int(&args[1])?,
            }
        }
        "screenshot" => {
            arity(1)?;
            let name = &args[0];
            // Files land in the output directory only.
            if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
                return Err(format!(
                    "screenshot name must be a plain file name: `{name}`"
                ));
            }
            Step::Screenshot(name.clone())
        }
        "log_state" => {
            arity(0)?;
            Step::LogState
        }
        "quit" => {
            arity(0)?;
            Step::Quit
        }
        _ => return Err(format!("unknown step `{name}`")),
    };
    Ok(step)
}

fn secs_arg(s: &str) -> Result<f64, String> {
    match s.parse::<f64>() {
        Ok(v) if v.is_finite() && v >= 0.0 => Ok(v),
        _ => Err(format!("not a duration in seconds: `{s}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps(source: &str) -> Vec<Step> {
        parse(source).unwrap().into_iter().map(|l| l.step).collect()
    }

    #[test]
    fn parses_every_step() {
        let source = r#"
            # comment line
            wait_connected
            wait_connected timeout=5
            wait_until controlled timeout=2.5
            wait_until entities 100
            wait_until chat "Привет \"мир\"" # trailing comment
            wait_secs 1.5
            key_down W
            key_up up
            tap Enter
            type "Привет, станция!"
            click_world 10 -3
            click_tile 1 0
            click_tile 0 1 2 29
            click_ui "Сообщение"
            scroll_ui "Смотритель" 5
            screenshot "a.png"
            log_state
            quit
        "#;
        let w = KeyName::parse("w").unwrap();
        assert_eq!(w.code, KeyCode::KeyW);
        assert_eq!(w.logical, Key::Character("w".into()));
        assert_eq!(KeyName::parse("Numpad8").unwrap().code, KeyCode::Numpad8);
        assert_eq!(KeyName::parse("F1").unwrap().code, KeyCode::F1);
        assert!(KeyName::parse("F13").is_none());
        assert!(KeyName::parse("Numpad10").is_none());
        assert_eq!(
            steps(source),
            [
                Step::WaitConnected {
                    timeout: DEFAULT_CONNECT_TIMEOUT
                },
                Step::WaitConnected { timeout: 5.0 },
                Step::WaitUntil {
                    cond: Condition::Controlled,
                    timeout: 2.5
                },
                Step::WaitUntil {
                    cond: Condition::Entities(100),
                    timeout: DEFAULT_WAIT_TIMEOUT
                },
                Step::WaitUntil {
                    cond: Condition::Chat("Привет \"мир\"".into()),
                    timeout: DEFAULT_WAIT_TIMEOUT
                },
                Step::WaitSecs(1.5),
                Step::KeyDown(w),
                Step::KeyUp(KeyName::parse("ArrowUp").unwrap()),
                Step::Tap(KeyName::parse("enter").unwrap()),
                Step::Type("Привет, станция!".into()),
                Step::ClickWorld {
                    x: 10,
                    y: -3,
                    pixel: [16, 16]
                },
                Step::ClickTile {
                    dx: 1,
                    dy: 0,
                    pixel: [16, 16]
                },
                Step::ClickTile {
                    dx: 0,
                    dy: 1,
                    pixel: [2, 29]
                },
                Step::ClickUi("Сообщение".into()),
                Step::ScrollUi {
                    needle: "Смотритель".into(),
                    lines: 5
                },
                Step::Screenshot("a.png".into()),
                Step::LogState,
                Step::Quit,
            ]
        );
    }

    #[test]
    fn errors_name_the_line() {
        for (source, needle) in [
            ("quit\nfly", "line 2: unknown step `fly`"),
            ("key_down Hyper", "unknown key `Hyper`"),
            ("type \"open", "line 1: unterminated string"),
            ("wait_secs -1", "not a duration"),
            ("wait_until mob", "unknown condition"),
            ("tap Enter timeout=3", "takes no timeout"),
            ("screenshot ../x.png", "plain file name"),
            ("click_tile 1", "takes 2 or 4 arguments, got 1"),
            ("click_tile 1 0 32 0", "pixel 0..=31"),
        ] {
            let err = parse(source).unwrap_err();
            assert!(err.contains(needle), "{source:?}: {err}");
        }
    }

    #[test]
    fn characters_map_to_layout_keys() {
        let a = KeyName::for_char('A');
        assert_eq!(a.code, KeyCode::KeyA);
        assert_eq!(a.text.as_deref(), Some("A"));
        let cyr = KeyName::for_char('ж');
        assert_eq!(cyr.code, KeyCode::Unidentified(NativeKeyCode::Unidentified));
        assert_eq!(cyr.logical, Key::Character("ж".into()));
        assert_eq!(KeyName::for_char(' ').logical, Key::Space);
    }
}

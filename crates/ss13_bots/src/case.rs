//! Case files (`tests/cases/*.case`): one scenario per file in a line format like the visual
//! check scripts (`scripts/visual/*.txt`). Syntax reference: docs/RUNNING.md, "Functional test
//! bots".

use std::path::{Path, PathBuf};

use ss13_shared::Dir;
use ss13_shared::protocol::{ClickModifiers, InventorySlot};

use crate::cond::{self, Cond};

/// `until` timeout when a case sets none.
pub const DEFAULT_TIMEOUT: f64 = 10.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Case {
    /// File stem: what `--filter` matches and reports name.
    pub name: String,
    pub file: PathBuf,
    pub map: MapSpec,
    /// Runs on a server of its own (global or round state).
    pub exclusive: bool,
    pub seed: Option<u64>,
    pub time_scale: Option<f32>,
    pub timeout: f64,
    pub bots: Vec<BotSpec>,
    /// Steps before the first `test`, run once after the bots are placed.
    pub setup: Vec<Step>,
    pub tests: Vec<TestSpec>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MapSpec {
    /// A `.dmm` next to the case file (absolute here).
    File(PathBuf),
    /// A station of the BYOND checkout (`MetaStation`): always exclusive.
    Station(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct BotSpec {
    pub name: String,
    pub pos: (i32, i32),
    pub job: Option<String>,
    pub give: Vec<String>,
    /// An antagonist role given after the job and items (an admin's "make antag"): `brother`,
    /// `vampire`, `changeling`.
    pub antag: Option<String>,
    /// Listed as an admin with these rights keywords (`admin_ranks.txt` `Include`, e.g.
    /// `"ADMIN BAN"`, `EVERYTHING`) before it connects.
    pub admin: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TestSpec {
    pub name: String,
    pub line: usize,
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub line: usize,
    pub text: String,
    pub kind: StepKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StepKind {
    Wait(f64),
    Until(Cond, f64),
    Expect(Cond),
    Spawn {
        path: String,
        pos: (i32, i32),
    },
    /// Dev: forces a random event by its tg name (the admin Force Event).
    Event(String),
    Bot {
        bot: usize,
        action: Action,
    },
    /// The action every `period` seconds (first at once) until the condition holds.
    Repeat {
        period: f64,
        bot: usize,
        action: Action,
        until: Cond,
        timeout: f64,
    },
    /// Remembers the value at a condition's left side as `$name`.
    Save {
        name: String,
        value: Cond,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Move(Vec<Dir>),
    Click {
        target: Target,
        modifiers: ClickModifiers,
    },
    Slot(InventorySlot),
    AltSlot(InventorySlot),
    /// A click-drag from one end onto the other (`ClientIntent::Drag`).
    Drag {
        from: DragTarget,
        to: DragTarget,
    },
    Say(String),
    Act {
        interface: String,
        action: String,
        params: Vec<(String, String)>,
    },
    Close(String),
    Swap,
    Drop,
    Combat,
    Throw,
    Resist,
    StopPull,
    LateJoin,
    Zone(String),
    Teleport((i32, i32)),
    /// dev: damage to the bot's own mob (`brute`, `burn`, `tox`, `oxy`).
    Hurt(String, f32),
    /// An admin verb or adminhelp (`crate::admin`); bot names in `args` become ckeys.
    Admin {
        verb: String,
        args: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// The topmost clickable thing the bot sees on the tile.
    Tile(i32, i32),
    /// The first thing on the tile whose DM path starts with the text (without a leading `/`:
    /// whose name contains it).
    Match(i32, i32, String),
    /// A bot's mob (the clicking bot itself included).
    Bot(usize),
}

/// One end of a `drag`: a thing as a click names it, or a HUD slot (`slot:<name>`).
#[derive(Clone, Debug, PartialEq)]
pub enum DragTarget {
    Thing(Target),
    Slot(InventorySlot),
}

#[derive(Debug, PartialEq)]
pub struct ParseError {
    pub file: PathBuf,
    pub line: usize,
    pub message: String,
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}:{}: {}", self.file.display(), self.line, self.message)
    }
}

/// Whitespace-separated tokens; `#` starts a comment; a token starting with `"` runs to the next
/// unescaped `"` (`\"`, `\\`) and comes out marked with a leading NUL so values know it was
/// quoted. Quotes inside a token are kept as they are.
pub fn tokenize(line: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '#' {
            break;
        } else if c == '"' {
            chars.next();
            let mut s = String::from('\0');
            loop {
                match chars.next() {
                    Some('\\') => s.push(chars.next().ok_or("dangling `\\`")?),
                    Some('"') => break,
                    Some(c) => s.push(c),
                    None => return Err("unterminated string".into()),
                }
            }
            tokens.push(s);
        } else {
            let mut s = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                s.push(c);
                chars.next();
            }
            tokens.push(s);
        }
    }
    Ok(tokens)
}

/// A token without its quote mark.
fn unq(token: &str) -> &str {
    token.strip_prefix('\0').unwrap_or(token)
}

pub fn parse_file(path: &Path) -> Result<Case, ParseError> {
    let text = std::fs::read_to_string(path).map_err(|e| ParseError {
        file: path.to_owned(),
        line: 0,
        message: e.to_string(),
    })?;
    parse(path, &text)
}

pub fn parse(file: &Path, text: &str) -> Result<Case, ParseError> {
    let err = |line: usize, message: String| ParseError {
        file: file.to_owned(),
        line,
        message,
    };
    let dir = file.parent().unwrap_or(Path::new("."));
    let mut case = Case {
        name: file
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned()),
        file: file.to_owned(),
        map: MapSpec::File(PathBuf::new()),
        exclusive: false,
        seed: None,
        time_scale: None,
        timeout: DEFAULT_TIMEOUT,
        bots: Vec::new(),
        setup: Vec::new(),
        tests: Vec::new(),
    };
    let mut has_map = false;
    // Steps are parsed after the header: bot names must be known.
    let mut pending: Vec<(Option<usize>, usize, String, Vec<String>)> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let tokens = tokenize(raw).map_err(|e| err(line, e))?;
        let Some(head) = tokens.first() else {
            continue;
        };
        let in_tests = !case.tests.is_empty();
        let arg = |n: usize| -> Result<&str, ParseError> {
            tokens
                .get(n)
                .map(|t| unq(t))
                .ok_or_else(|| err(line, format!("`{head}` needs an argument")))
        };
        match head.as_str() {
            "test" => {
                let name = arg(1)?.to_owned();
                if case.tests.iter().any(|t| t.name == name) {
                    return Err(err(line, format!("duplicate test {name:?}")));
                }
                case.tests.push(TestSpec {
                    name,
                    line,
                    steps: Vec::new(),
                });
            }
            "map" | "exclusive" | "seed" | "time_scale" | "timeout" | "bot" if in_tests => {
                return Err(err(line, format!("`{head}` belongs before the first test")));
            }
            "map" => {
                let m = arg(1)?;
                case.map = if m.ends_with(".dmm") {
                    MapSpec::File(dir.join(m))
                } else {
                    MapSpec::Station(m.to_owned())
                };
                has_map = true;
            }
            "exclusive" => case.exclusive = true,
            "seed" => case.seed = Some(arg(1)?.parse().map_err(|_| err(line, "bad seed".into()))?),
            "time_scale" => {
                let s: f32 = arg(1)?
                    .parse()
                    .map_err(|_| err(line, "bad time scale".into()))?;
                if !(0.1..=10.0).contains(&s) {
                    return Err(err(line, "time scale must be in 0.1..=10".into()));
                }
                case.time_scale = Some(s);
            }
            "timeout" => {
                case.timeout = arg(1)?
                    .parse()
                    .map_err(|_| err(line, "bad timeout".into()))?
            }
            "bot" => {
                let name = arg(1)?.to_owned();
                if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    || name.starts_with(|c: char| c.is_ascii_digit())
                {
                    return Err(err(line, format!("bad bot name {name:?}")));
                }
                if case.bots.iter().any(|b| b.name == name) {
                    return Err(err(line, format!("duplicate bot {name:?}")));
                }
                let pos = cond::parse_tile(arg(2)?)
                    .ok_or_else(|| err(line, "bot: expected `bot <name> x,y`".into()))?;
                let mut bot = BotSpec {
                    name,
                    pos,
                    job: None,
                    give: Vec::new(),
                    antag: None,
                    admin: None,
                };
                let mut rest = tokens[3..].iter();
                while let Some(key) = rest.next() {
                    let value = rest
                        .next()
                        .map(|t| unq(t).to_owned())
                        .ok_or_else(|| err(line, format!("`{key}` needs a value")))?;
                    match key.as_str() {
                        "job" => bot.job = Some(value),
                        "give" => bot.give.push(value),
                        "antag" => bot.antag = Some(value),
                        "admin" => {
                            let mut warnings = Vec::new();
                            ss13_admin::rights::keywords(&value, None, &mut warnings);
                            if let Some(w) = warnings.first() {
                                return Err(err(line, format!("bot admin: {w}")));
                            }
                            bot.admin = Some(value);
                        }
                        other => return Err(err(line, format!("unknown bot option {other:?}"))),
                    }
                }
                case.bots.push(bot);
            }
            _ => pending.push((
                case.tests.len().checked_sub(1),
                line,
                raw.trim().into(),
                tokens,
            )),
        }
    }
    if !has_map {
        return Err(err(1, "no `map`".into()));
    }
    if case.bots.is_empty() {
        return Err(err(1, "no `bot`: server queries go through a bot".into()));
    }
    if case.tests.is_empty() {
        return Err(err(1, "no `test`".into()));
    }
    if matches!(case.map, MapSpec::Station(_)) || case.seed.is_some() {
        case.exclusive = true;
    }
    let names: Vec<String> = case.bots.iter().map(|b| b.name.clone()).collect();
    let bot = |name: &str| names.iter().position(|n| n == name);
    for (test, line, text, tokens) in pending {
        let kind = parse_step(&tokens, &bot, case.timeout).map_err(|e| err(line, e))?;
        let step = Step { line, text, kind };
        match test {
            Some(t) => case.tests[t].steps.push(step),
            None => case.setup.push(step),
        }
    }
    Ok(case)
}

fn parse_step(
    tokens: &[String],
    bot: &dyn Fn(&str) -> Option<usize>,
    default_timeout: f64,
) -> Result<StepKind, String> {
    let head = tokens[0].as_str();
    let rest = &tokens[1..];
    let secs = |t: Option<&String>| -> Result<f64, String> {
        t.and_then(|t| unq(t).parse::<f64>().ok())
            .filter(|s| s.is_finite() && *s >= 0.0)
            .ok_or_else(|| format!("`{head}` needs seconds"))
    };
    let with_timeout = |tokens: &'_ [String]| -> Result<(Vec<String>, f64), String> {
        match tokens.split_last() {
            Some((last, init)) if last.starts_with("timeout=") => {
                let t = last["timeout=".len()..].to_owned();
                Ok((init.to_vec(), secs(Some(&t))?))
            }
            _ => Ok((tokens.to_vec(), default_timeout)),
        }
    };
    match head {
        "wait" => return Ok(StepKind::Wait(secs(rest.first())?)),
        "until" => {
            let (cond_tokens, timeout) = with_timeout(rest)?;
            return Ok(StepKind::Until(cond::parse(&cond_tokens, bot)?, timeout));
        }
        "repeat" => {
            let period = secs(rest.first())?;
            let split = rest
                .iter()
                .position(|t| t == "until")
                .ok_or("expected `repeat <secs> <bot> <action...> until <condition>`")?;
            let StepKind::Bot { bot: who, action } =
                parse_step(&rest[1..split], bot, default_timeout)?
            else {
                return Err("repeat: the repeated step must be a bot action".into());
            };
            let (cond_tokens, timeout) = with_timeout(&rest[split + 1..])?;
            return Ok(StepKind::Repeat {
                period,
                bot: who,
                action,
                until: cond::parse(&cond_tokens, bot)?,
                timeout,
            });
        }
        "save" => {
            let [name, lhs] = rest else {
                return Err("expected `save <name> <subject>.<path>`".into());
            };
            let tokens = [lhs.clone(), "exists".to_owned()];
            return Ok(StepKind::Save {
                name: unq(name).to_owned(),
                value: cond::parse(&tokens, bot)?,
            });
        }
        "expect" => return Ok(StepKind::Expect(cond::parse(rest, bot)?)),
        "event" => {
            let [name] = rest else {
                return Err("expected `event \"<event name>\"`".into());
            };
            return Ok(StepKind::Event(unq(name).to_owned()));
        }
        "spawn" => {
            let [path, pos] = rest else {
                return Err("expected `spawn <path> x,y`".into());
            };
            let pos = cond::parse_tile(unq(pos)).ok_or("spawn: bad tile")?;
            return Ok(StepKind::Spawn {
                path: unq(path).to_owned(),
                pos,
            });
        }
        _ => {}
    }
    let index = bot(head).ok_or_else(|| format!("unknown step or bot {head:?}"))?;
    let (verb, args) = rest
        .split_first()
        .ok_or_else(|| format!("`{head}` needs an action"))?;
    let no_args = |action: Action| {
        if args.is_empty() {
            Ok(action)
        } else {
            Err(format!("`{verb}` takes no arguments"))
        }
    };
    let action = match verb.as_str() {
        "move" => {
            if args.is_empty() {
                return Err("move: which way? (N S E W NE NW SE SW)".into());
            }
            let dirs = args
                .iter()
                .map(|d| parse_dir(unq(d)).ok_or_else(|| format!("move: bad direction {d:?}")))
                .collect::<Result<_, _>>()?;
            Action::Move(dirs)
        }
        "click" => {
            let (target, mods) = args.split_first().ok_or("click: what?")?;
            let target_text = unq(target);
            let mut mods = mods.iter().map(|m| unq(m));
            let target = if let Some((x, y)) = cond::parse_tile(target_text) {
                match mods.clone().next().filter(|m| !is_modifier(m)) {
                    Some(m) => {
                        mods.next();
                        Target::Match(x, y, m.to_owned())
                    }
                    None => Target::Tile(x, y),
                }
            } else if target_text == "self" {
                Target::Bot(index)
            } else {
                Target::Bot(bot(target_text).ok_or_else(|| {
                    format!("click: target {target_text:?} is not x,y, self or a bot")
                })?)
            };
            let mut modifiers = ClickModifiers::default();
            for m in mods {
                match m {
                    "shift" => modifiers.shift = true,
                    "ctrl" => modifiers.ctrl = true,
                    "alt" => modifiers.alt = true,
                    "right" => modifiers.right = true,
                    "middle" => modifiers.middle = true,
                    other => return Err(format!("click: unknown modifier {other:?}")),
                }
            }
            Action::Click { target, modifiers }
        }
        "drag" => {
            let mut rest = args.iter().map(|a| unq(a)).peekable();
            let mut end = |what: &str| -> Result<DragTarget, String> {
                let text = rest.next().ok_or_else(|| {
                    format!("drag: {what}? (x,y [path], self, a bot, slot:<name>)")
                })?;
                if let Some(slot) = text.strip_prefix("slot:") {
                    return parse_slot(slot)
                        .map(DragTarget::Slot)
                        .ok_or_else(|| format!("unknown slot {slot:?}"));
                }
                if let Some((x, y)) = cond::parse_tile(text) {
                    let matcher = rest.next_if(|m| {
                        cond::parse_tile(m).is_none()
                            && *m != "self"
                            && bot(m).is_none()
                            && !m.starts_with("slot:")
                    });
                    return Ok(DragTarget::Thing(match matcher {
                        Some(m) => Target::Match(x, y, m.to_owned()),
                        None => Target::Tile(x, y),
                    }));
                }
                if text == "self" {
                    return Ok(DragTarget::Thing(Target::Bot(index)));
                }
                bot(text)
                    .map(|b| DragTarget::Thing(Target::Bot(b)))
                    .ok_or_else(|| format!("drag: {text:?} is not x,y, self, a bot or slot:<name>"))
            };
            let from = end("from")?;
            let to = end("onto")?;
            if rest.next().is_some() {
                return Err("drag: expected two ends".into());
            }
            Action::Drag { from, to }
        }
        "slot" | "altslot" => {
            let [slot] = args else {
                return Err(format!("`{verb}` needs one slot"));
            };
            let slot = parse_slot(unq(slot)).ok_or_else(|| format!("unknown slot {slot:?}"))?;
            if verb == "slot" {
                Action::Slot(slot)
            } else {
                Action::AltSlot(slot)
            }
        }
        "say" => {
            let [text] = args else {
                return Err("say: one quoted text".into());
            };
            Action::Say(unq(text).to_owned())
        }
        "act" => {
            let [interface, action, params @ ..] = args else {
                return Err("expected `act <Interface> <action> [key=value...]`".into());
            };
            let params = params
                .iter()
                .map(|p| {
                    let p = unq(p);
                    p.split_once('=')
                        .map(|(k, v)| (k.to_owned(), v.to_owned()))
                        .ok_or_else(|| format!("act: expected key=value, got {p:?}"))
                })
                .collect::<Result<_, _>>()?;
            Action::Act {
                interface: unq(interface).to_owned(),
                action: unq(action).to_owned(),
                params,
            }
        }
        "close" => {
            let [interface] = args else {
                return Err("close: which interface?".into());
            };
            Action::Close(unq(interface).to_owned())
        }
        "zone" => {
            let [zone] = args else {
                return Err("zone: which zone?".into());
            };
            Action::Zone(unq(zone).to_owned())
        }
        "teleport" => {
            let [pos] = args else {
                return Err("teleport: x,y".into());
            };
            Action::Teleport(cond::parse_tile(unq(pos)).ok_or("teleport: bad tile")?)
        }
        "hurt" => {
            let [kind, amount] = args else {
                return Err("hurt: <brute|burn|tox|oxy> <amount>".into());
            };
            let kind = unq(kind).to_owned();
            if !["brute", "burn", "tox", "oxy"].contains(&kind.as_str()) {
                return Err(format!("hurt: unknown damage type {kind:?}"));
            }
            let amount: f32 = unq(amount).parse().map_err(|_| "hurt: bad amount")?;
            Action::Hurt(kind, amount)
        }
        "admin" => {
            let (verb, rest) = args.split_first().ok_or("admin: which verb?")?;
            let args: Vec<String> = rest.iter().map(|a| unq(a).to_owned()).collect();
            let verb = unq(verb).to_owned();
            crate::admin::request(
                &verb,
                &args,
                &|name| bot(name).map(|_| name.to_owned()),
                &|_| Some(0),
            )?;
            Action::Admin { verb, args }
        }
        "swap" => no_args(Action::Swap)?,
        "drop" => no_args(Action::Drop)?,
        "combat" => no_args(Action::Combat)?,
        "throw" => no_args(Action::Throw)?,
        "resist" => no_args(Action::Resist)?,
        "stop_pull" => no_args(Action::StopPull)?,
        "latejoin" => no_args(Action::LateJoin)?,
        other => return Err(format!("unknown action {other:?}")),
    };
    Ok(StepKind::Bot { bot: index, action })
}

fn is_modifier(m: &str) -> bool {
    matches!(m, "shift" | "ctrl" | "alt" | "right" | "middle")
}

fn parse_dir(d: &str) -> Option<Dir> {
    Some(match d.to_ascii_uppercase().as_str() {
        "N" => Dir::North,
        "S" => Dir::South,
        "E" => Dir::East,
        "W" => Dir::West,
        "NE" => Dir::NorthEast,
        "NW" => Dir::NorthWest,
        "SE" => Dir::SouthEast,
        "SW" => Dir::SouthWest,
        _ => return None,
    })
}

pub fn parse_slot(name: &str) -> Option<InventorySlot> {
    ss13_game::inventory::ALL_SLOTS
        .into_iter()
        .find(|&s| ss13_debug::test_hooks::slot_name(s) == name)
}

/// Every `*.case` under `paths` (files or directories, recursively), sorted.
pub fn discover(paths: &[PathBuf]) -> std::io::Result<Vec<PathBuf>> {
    fn walk(path: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
        if path.is_dir() {
            for entry in std::fs::read_dir(path)? {
                walk(&entry?.path(), out)?;
            }
        } else if path.extension().is_some_and(|e| e == "case") {
            out.push(path.to_owned());
        }
        Ok(())
    }
    let mut out = Vec::new();
    for path in paths {
        if path.is_file() {
            out.push(path.clone());
        } else {
            walk(path, &mut out)?;
        }
    }
    // Map paths resolve against the case file; the server loads them by absolute path.
    let mut out = out
        .into_iter()
        .map(std::path::absolute)
        .collect::<std::io::Result<Vec<_>>>()?;
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CASE: &str = r#"
# A door.
map door.dmm
timeout 5
time_scale 2
bot alice 3,3
bot bob 5,3 job "Security Officer" give /obj/item/toolbox give "/obj/item/screwdriver"
spawn /obj/item/crowbar 4,4

test walk   # trailing comment
  alice move E N
  until alice.pos == 4,4 timeout=2.5
  alice click 5,3 /obj/machinery/door ctrl
  alice click 5,3 shift
  bob click alice right
  alice say "Привет, \"станция\"!"
  alice act Vending vend ref=0 premium=0
  alice slot back
  alice altslot suit_storage
  wait 0.5
test second
  expect tile(4,4).entities[].path ~ crowbar
  bob zone head
"#;

    #[test]
    fn parses_a_case() {
        let case = parse(Path::new("/c/doors.case"), CASE).unwrap();
        assert_eq!(case.name, "doors");
        assert_eq!(case.map, MapSpec::File("/c/door.dmm".into()));
        assert!(!case.exclusive);
        assert_eq!(case.timeout, 5.0);
        assert_eq!(case.time_scale, Some(2.0));
        assert_eq!(
            case.bots[1],
            BotSpec {
                name: "bob".into(),
                pos: (5, 3),
                job: Some("Security Officer".into()),
                give: vec!["/obj/item/toolbox".into(), "/obj/item/screwdriver".into()],
                antag: None,
                admin: None,
            }
        );
        assert_eq!(
            case.setup[0].kind,
            StepKind::Spawn {
                path: "/obj/item/crowbar".into(),
                pos: (4, 4)
            }
        );
        let steps = &case.tests[0].steps;
        assert_eq!(steps.len(), 10);
        assert_eq!(steps[0].line, 11);
        assert_eq!(
            steps[0].kind,
            StepKind::Bot {
                bot: 0,
                action: Action::Move(vec![Dir::East, Dir::North])
            }
        );
        let StepKind::Until(cond, timeout) = &steps[1].kind else {
            panic!("{:?}", steps[1]);
        };
        assert_eq!((cond.text.as_str(), *timeout), ("alice.pos == 4,4", 2.5));
        let click = |i: usize| match &steps[i].kind {
            StepKind::Bot {
                action: Action::Click { target, modifiers },
                ..
            } => (target.clone(), *modifiers),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            click(2),
            (
                Target::Match(5, 3, "/obj/machinery/door".into()),
                ClickModifiers {
                    ctrl: true,
                    ..default_mods()
                }
            )
        );
        assert_eq!(
            click(3),
            (
                Target::Tile(5, 3),
                ClickModifiers {
                    shift: true,
                    ..default_mods()
                }
            )
        );
        assert_eq!(
            click(4),
            (
                Target::Bot(0),
                ClickModifiers {
                    right: true,
                    ..default_mods()
                }
            )
        );
        assert_eq!(
            steps[5].kind,
            StepKind::Bot {
                bot: 0,
                action: Action::Say("Привет, \"станция\"!".into())
            }
        );
        assert_eq!(
            steps[6].kind,
            StepKind::Bot {
                bot: 0,
                action: Action::Act {
                    interface: "Vending".into(),
                    action: "vend".into(),
                    params: vec![("ref".into(), "0".into()), ("premium".into(), "0".into())],
                }
            }
        );
        assert_eq!(
            steps[8].kind,
            StepKind::Bot {
                bot: 0,
                action: Action::AltSlot(InventorySlot::SuitStorage)
            }
        );
        assert_eq!(steps[9].kind, StepKind::Wait(0.5));
        assert_eq!(case.tests[1].steps.len(), 2);
        let case = parse(
            Path::new("r.case"),
            "map a.dmm\nbot alice 1,1\nbot bob 2,1\ntest t\n  save hp mob(bob).health.health\n  repeat 0.8 alice click bob until mob(bob).health.health < $hp-5 timeout=20\n",
        )
        .unwrap();
        let StepKind::Save { name, value } = &case.tests[0].steps[0].kind else {
            panic!()
        };
        assert_eq!(
            (name.as_str(), &value.subject),
            ("hp", &crate::cond::Subject::Mob(1))
        );
        let StepKind::Repeat {
            period,
            bot,
            action,
            until,
            timeout,
        } = &case.tests[0].steps[1].kind
        else {
            panic!()
        };
        assert_eq!((*period, *bot, *timeout), (0.8, 0, 20.0));
        assert_eq!(
            action,
            &Action::Click {
                target: Target::Bot(1),
                modifiers: ClickModifiers::default()
            }
        );
        assert_eq!(until.value, serde_json::json!("$hp-5"));
    }

    fn default_mods() -> ClickModifiers {
        ClickModifiers::default()
    }

    #[test]
    fn reports_errors_with_lines() {
        let e = |text: &str| parse(Path::new("x.case"), text).unwrap_err();
        let base = "map a.dmm\nbot alice 1,1\ntest t\n";
        assert_eq!(e(&format!("{base}  carol move E")).line, 4);
        assert!(
            e(&format!("{base}  carol move E"))
                .message
                .contains("carol")
        );
        assert!(
            e(&format!("{base}  alice move Q"))
                .message
                .contains("direction")
        );
        assert!(
            e(&format!("{base}  alice fly"))
                .message
                .contains("unknown action")
        );
        assert!(e(&format!("{base}bot bob 1,1")).message.contains("before"));
        assert!(
            e(&format!("{base}  alice say \"open"))
                .message
                .contains("unterminated")
        );
        assert!(e("bot alice 1,1\ntest t\n").message.contains("map"));
        assert!(e("map a.dmm\ntest t\n").message.contains("bot"));
        assert!(
            e("map a.dmm\nbot a 1,1\nbot a 2,2\ntest t")
                .message
                .contains("duplicate")
        );
        assert!(
            e(&format!("{base}  until alice.pos == 1,1 timeout=x"))
                .message
                .contains("seconds")
        );
    }

    #[test]
    fn parses_drags() {
        let case = parse(
            Path::new("d.case"),
            "map a.dmm\nbot alice 1,1\nbot bob 2,1\ntest t\n  alice drag bob self\n  \
             alice drag slot:back 2,3\n  alice drag 4,4 /obj/structure/closet 3,3 table\n",
        )
        .unwrap();
        let drags: Vec<Action> = case.tests[0]
            .steps
            .iter()
            .map(|s| match &s.kind {
                StepKind::Bot { action, .. } => action.clone(),
                other => panic!("{other:?}"),
            })
            .collect();
        let thing = DragTarget::Thing;
        assert_eq!(
            drags,
            [
                Action::Drag {
                    from: thing(Target::Bot(1)),
                    to: thing(Target::Bot(0)),
                },
                Action::Drag {
                    from: DragTarget::Slot(InventorySlot::Back),
                    to: thing(Target::Tile(2, 3)),
                },
                Action::Drag {
                    from: thing(Target::Match(4, 4, "/obj/structure/closet".into())),
                    to: thing(Target::Match(3, 3, "table".into())),
                },
            ]
        );
        let e = |line: &str| {
            parse(
                Path::new("d.case"),
                &format!("map a.dmm\nbot alice 1,1\ntest t\n  {line}\n"),
            )
            .unwrap_err()
            .message
        };
        assert!(e("alice drag self").contains("onto?"));
        assert!(e("alice drag slot:nose self").contains("unknown slot"));
        assert!(e("alice drag self 1,1 x 2,2").contains("two ends"));
    }

    #[test]
    fn station_and_seed_make_a_case_exclusive() {
        let case = parse(Path::new("s.case"), "map MetaStation\nbot a 1,1\ntest t\n").unwrap();
        assert_eq!(case.map, MapSpec::Station("MetaStation".into()));
        assert!(case.exclusive);
        let case = parse(
            Path::new("s.case"),
            "map a.dmm\nseed 3\nbot a 1,1\ntest t\n",
        )
        .unwrap();
        assert!(case.exclusive);
    }

    #[test]
    fn parses_forced_events() {
        let case = parse(
            Path::new("e.case"),
            "map a.dmm\nbot a 1,1\ntest t\n  event \"Grid Check\"\n",
        )
        .unwrap();
        assert_eq!(
            case.tests[0].steps[0].kind,
            StepKind::Event("Grid Check".into())
        );
        let e = parse(
            Path::new("e.case"),
            "map a.dmm\nbot a 1,1\ntest t\n  event\n",
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("expected `event"), "{e}");
    }
}

//! Every constant path the game spawns is in `ss13_mapload::runtime::runtime_types`, so
//! `ss13-mapload convert` emits its prototype even when no map places the type (fixture tests
//! declare their own types and would pass without it).
//!
//! Reads the game's source: the second argument of each spawn helper call (after `world`), its
//! string literals and its `UPPER_CASE` consts, resolved through the `&str` consts of
//! `ss13_game` and `ss13_mapload`. Paths computed at runtime (`&dm.trash_type`, table entries)
//! are the converter closures' business; the spawn tables have their own unit tests.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Game helpers that spawn their second argument's type from its prototype.
const SPAWNERS: [&str; 16] = [
    "spawn_obj",
    "spawn_detached",
    "spawn_stack",
    "spawn_template",
    "spawn_blood",
    "spawn_armed",
    "spawn_implant",
    "spawn_any",
    "spawn_organ",
    "spawn_into",
    "spawn_obj_or_rune",
    "spawn_mob",
    "spawn_in",
    "spawn_inside",
    "spawn_wreckage",
    "spawn_seed_like",
];

struct Source {
    file: PathBuf,
    /// Non-test code with `//` comments removed.
    text: String,
}

fn read_sources(dir: &Path, out: &mut Vec<Source>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            read_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path).unwrap();
            let code = text.split("#[cfg(test)]").next().unwrap_or_default();
            let text = code
                .lines()
                .map(strip_comment)
                .collect::<Vec<_>>()
                .join("\n");
            out.push(Source { file: path, text });
        }
    }
}

/// `line` up to a `//` outside a string literal.
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_string = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_string => i += 1,
            b'"' => in_string = !in_string,
            b'/' if !in_string && bytes.get(i + 1) == Some(&b'/') => return &line[..i],
            _ => {}
        }
        i += 1;
    }
    line
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// `UPPER_CASE` identifiers of `text`, in order.
fn upper_idents(text: &str) -> Vec<&str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| {
            w.len() >= 2
                && w.as_bytes()[0].is_ascii_uppercase()
                && w.bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        })
        .collect()
}

/// `"/..."` literals of `text`.
fn path_literals(text: &str) -> Vec<&str> {
    text.split('"')
        .skip(1)
        .step_by(2)
        .filter(|s| s.starts_with('/'))
        .collect()
}

/// `const NAME: &str = VALUE;` (or `&'static str`) by name: (source index, value).
fn str_consts(sources: &[Source]) -> BTreeMap<&str, Vec<(usize, &str)>> {
    let mut out: BTreeMap<&str, Vec<(usize, &str)>> = BTreeMap::new();
    for (i, src) in sources.iter().enumerate() {
        let text = src.text.as_str();
        let mut at = 0;
        while let Some(found) = text[at..].find("const ") {
            let keyword = at + found;
            let start = keyword + "const ".len();
            at = start;
            if keyword > 0 && is_ident(text.as_bytes()[keyword - 1]) {
                continue;
            }
            let rest = &text[start..];
            let name_len = rest.bytes().take_while(|&b| is_ident(b)).count();
            let name = &rest[..name_len];
            let Some(rest) = rest[name_len..].trim_start().strip_prefix(':') else {
                continue;
            };
            let Some((ty, value)) = rest.split_once('=') else {
                continue;
            };
            if !matches!(ty.trim(), "&str" | "&'static str") {
                continue;
            }
            let Some(end) = value.find(';') else {
                continue;
            };
            out.entry(name).or_default().push((i, value[..end].trim()));
        }
    }
    out
}

struct Resolver<'a> {
    consts: BTreeMap<&'a str, Vec<(usize, &'a str)>>,
}

impl<'a> Resolver<'a> {
    /// The paths const `name` (seen from source `from`) may hold; `Err` when unresolvable.
    /// A definition in `from` wins over others of the same name; `skip` is a source whose
    /// definition is being resolved (`const X: &str = other::X;`).
    fn resolve(
        &self,
        name: &str,
        from: usize,
        skip: Option<usize>,
        depth: u32,
    ) -> Result<Vec<&'a str>, String> {
        let defs: Vec<&(usize, &str)> = self
            .consts
            .get(name)
            .into_iter()
            .flatten()
            .filter(|(i, _)| Some(*i) != skip)
            .collect();
        let local: Vec<_> = defs.iter().copied().filter(|(i, _)| *i == from).collect();
        let defs = if local.is_empty() { defs } else { local };
        if defs.is_empty() || depth > 8 {
            return Err(name.to_owned());
        }
        let mut out = Vec::new();
        for &(i, value) in defs {
            if let Some(path) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
                out.push(path);
            } else {
                let Some(&next) = upper_idents(value).last() else {
                    return Err(format!("{name} = {value}"));
                };
                let skip = (next == name).then_some(i);
                out.extend(self.resolve(next, i, skip, depth + 1)?);
            }
        }
        Ok(out)
    }
}

/// The second argument (after `world`) of every spawn helper call: (source index, line, text).
fn spawn_args(sources: &[Source]) -> Vec<(usize, usize, &str)> {
    let mut out = Vec::new();
    for (i, src) in sources.iter().enumerate() {
        let text = src.text.as_str();
        let bytes = text.as_bytes();
        for name in SPAWNERS {
            let pattern = format!("{name}(");
            for (at, _) in text.match_indices(&pattern) {
                if at > 0 && is_ident(bytes[at - 1]) {
                    continue;
                }
                let args = &text[at + pattern.len()..];
                let Some(rest) = args.trim_start().strip_prefix("world") else {
                    continue;
                };
                let Some(rest) = rest.trim_start().strip_prefix(',') else {
                    continue;
                };
                let mut depth = 0i32;
                let mut end = rest.len();
                for (j, c) in rest.char_indices() {
                    match c {
                        '(' | '[' | '{' => depth += 1,
                        ')' | ']' | '}' if depth == 0 => {
                            end = j;
                            break;
                        }
                        ')' | ']' | '}' => depth -= 1,
                        ',' if depth == 0 => {
                            end = j;
                            break;
                        }
                        _ => {}
                    }
                }
                let line = text[..at].matches('\n').count() + 1;
                out.push((i, line, rest[..end].trim()));
            }
        }
    }
    out
}

#[test]
fn constant_spawn_paths_are_runtime_types() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut sources = Vec::new();
    read_sources(&crates.join("ss13_game/src"), &mut sources);
    let game_sources = sources.len();
    read_sources(&crates.join("ss13_mapload/src"), &mut sources);
    let resolver = Resolver {
        consts: str_consts(&sources),
    };
    let registry = ss13_mapload::runtime::runtime_types();
    let mut checked = BTreeSet::new();
    let mut missing = BTreeSet::new();
    let mut unresolved = BTreeSet::new();
    for (i, line, arg) in spawn_args(&sources[..game_sources]) {
        let at = format!("{}:{line}", sources[i].file.display());
        let mut paths: Vec<&str> = path_literals(arg);
        for name in upper_idents(arg) {
            match resolver.resolve(name, i, None, 0) {
                Ok(found) => paths.extend(found),
                Err(what) => {
                    unresolved.insert(format!("{at}: {what}"));
                }
            }
        }
        for path in paths {
            checked.insert(path);
            if !registry.contains(path) {
                missing.insert(format!("{path} ({at})"));
            }
        }
    }
    assert!(
        unresolved.is_empty(),
        "spawn arguments naming no `&str` const:\n  {}",
        unresolved.into_iter().collect::<Vec<_>>().join("\n  ")
    );
    assert!(
        missing.is_empty(),
        "spawned by a constant path but not in ss13_mapload::runtime (add them there so \
         `ss13-mapload convert` emits their prototypes):\n  {}",
        missing.into_iter().collect::<Vec<_>>().join("\n  ")
    );
    // The scan found the spawn calls at all (a renamed helper would make it vacuous).
    assert!(
        checked.len() > 50,
        "only {} constant spawn paths",
        checked.len()
    );
}

/// Every `"…/….ogg"` literal in the game and admin code that BandaStation ships is in the
/// converted assets (`assets/generated/sound/<path>`): a sound only the code names is converted
/// when `ss13_mapload::sound::referenced` lists it (`files::GAME_CODE` and its neighbours).
#[test]
#[ignore = "needs generated assets and SS13_SOURCE; run with --ignored"]
fn game_sound_literals_are_converted() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let source = PathBuf::from(env!("SS13_SOURCE"));
    let assets =
        PathBuf::from(std::env::var("SS13_ASSETS").unwrap_or_else(|_| "assets/generated".into()));
    let mut sources = Vec::new();
    read_sources(&crates.join("ss13_game/src"), &mut sources);
    read_sources(&crates.join("ss13_admin/src"), &mut sources);
    let mut missing = BTreeSet::new();
    for s in &sources {
        for (start, _) in s.text.match_indices('"') {
            let rest = &s.text[start + 1..];
            let Some(end) = rest.find('"') else { continue };
            let lit = &rest[..end];
            let rooted = lit.starts_with("sound/") || lit.starts_with("modular_bandastation/");
            if rooted
                && lit.ends_with(".ogg")
                && !lit.contains(' ')
                && source.join(lit).is_file()
                && !assets.join("sound").join(lit).is_file()
            {
                missing.insert(lit.to_owned());
            }
        }
    }
    assert!(
        missing.is_empty(),
        "played by the code but not converted (add them to ss13_mapload::sound::files): {missing:#?}"
    );
}

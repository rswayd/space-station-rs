//! Paper blanks (server): the form templates a photocopier prints, after tg
//! `init_paper_blanks` and `make_blank_print` (`code/modules/paperwork/photocopier.dm`) and
//! BandaStation's `replace_text_keys` (`modular_bandastation/paperwork/code/paperwork.dm`).
//!
//! - `blanks.json` in the config directory's `blanks/` (`SS13_CONFIG_DIR`, default `config`)
//!   lists the forms: code, category, name, `info` (a file under `blanks/`, or the text as a
//!   list of lines) and an optional `required_access`. A missing or broken file means no forms,
//!   as in tg; a form whose file is missing is left out.
//! - Printing puts the form's text on a sheet named after it, with the `[key]`s of the
//!   printing user filled in: name, age, signature, time, date, station, bank account, job,
//!   department, species, gender.
// shortcut: the text keeps tg's markdown but not its `advanced_html` (the sheet sanitises
// HTML like any other); the bank account is the one on the user's ID card, not the one they
// remember; the age is the crew record's.

use std::path::{Path, PathBuf};

use bevy::prelude::*;

use crate::inventory::Inventory;

/// One `GLOB.paper_blanks` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blank {
    pub code: String,
    pub category: String,
    pub name: String,
    pub info: String,
    pub required_access: Option<String>,
}

/// `GLOB.paper_blanks`, in the file's order.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct PaperBlanks(pub Vec<Blank>);

/// `config/blanks` under `SS13_CONFIG_DIR` (default `config`).
fn blanks_dir() -> PathBuf {
    let config = std::env::var("SS13_CONFIG_DIR")
        .ok()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "config".to_owned());
    Path::new(&config).join("blanks")
}

/// Startup: reads the blanks unless a test inserted its own.
pub(super) fn load_blanks(world: &mut World) {
    if world.get_resource::<PaperBlanks>().is_some() {
        return;
    }
    let dir = blanks_dir();
    let blanks = std::fs::read_to_string(dir.join("blanks.json"))
        .ok()
        .map(|json| parse(&json, |file| std::fs::read_to_string(dir.join(file)).ok()))
        .unwrap_or_default();
    if !blanks.is_empty() {
        info!("{} paper blanks loaded", blanks.len());
    }
    world.insert_resource(PaperBlanks(blanks));
}

/// `init_paper_blanks`: the entries of `json` whose text is a list of lines or a file `read`
/// finds.
pub fn parse(json: &str, read: impl Fn(&str) -> Option<String>) -> Vec<Blank> {
    let Ok(serde_json::Value::Array(entries)) = serde_json::from_str::<serde_json::Value>(json)
    else {
        return Vec::new();
    };
    let text = |v: Option<&serde_json::Value>| match v {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    let mut out: Vec<Blank> = Vec::new();
    for entry in &entries {
        let info = match entry.get("info") {
            Some(serde_json::Value::Array(lines)) => {
                lines.iter().filter_map(|l| l.as_str()).collect::<String>()
            }
            Some(serde_json::Value::String(file)) => {
                // `fexists` under `BLANKS_FOLDER`: no climbing out of it.
                if file.contains("..") {
                    continue;
                }
                let Some(info) = read(file) else {
                    continue;
                };
                info
            }
            _ => continue,
        };
        let blank = Blank {
            code: text(entry.get("code")),
            category: text(entry.get("category")),
            name: text(entry.get("name")),
            info,
            required_access: entry
                .get("required_access")
                .and_then(|a| a.as_str())
                .map(str::to_owned),
        };
        // `parsed_blanks[code] = ...`: a later entry with the same code replaces it.
        match out.iter_mut().find(|b| b.code == blank.code) {
            Some(old) => *old = blank,
            None => out.push(blank),
        }
    }
    out
}

/// The forms `access` may print (`ui_static_data`'s `required_access` filter).
pub fn printable<'a>(blanks: &'a PaperBlanks, access: &[String]) -> Vec<&'a Blank> {
    blanks
        .0
        .iter()
        .filter(|b| {
            b.required_access
                .as_ref()
                .is_none_or(|a| access.iter().any(|have| have == a))
        })
        .collect()
}

/// `replace_text_keys`: every `[key]` with a known key replaced by `value(key)`; the rest
/// (`[input_field]` and the like) stays.
pub fn replace_text_keys(text: &str, mut value: impl FnMut(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let key_len = after
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        let key = &after[..key_len];
        let closed = after[key_len..].starts_with(']');
        match (closed && !key.is_empty()).then(|| value(key)).flatten() {
            Some(replacement) => {
                out.push_str(&replacement);
                rest = &after[key_len + 1..];
            }
            None => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `GLOB.paper_replacements[key].get_replacement(user)`; `None` for an unknown key.
pub fn replacement(world: &World, user: Option<Entity>, key: &str) -> Option<String> {
    let unknown = || "неизвестный".to_owned();
    let none = || "отсутствует".to_owned();
    let name = || {
        user.map(|u| crate::chem::name_of(world, u))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(unknown)
    };
    let human = user.filter(|&u| world.get::<crate::human::HumanLook>(u).is_some());
    // `get_assignment(if_no_id = null, if_no_job = null) || mind.assigned_role.title`.
    let job = || {
        let human = human?;
        let card = world.get::<Inventory>(human)?.id_card(world);
        card.and_then(|c| world.get::<crate::id_access::CardAccess>(c))
            .and_then(|a| a.trim.as_ref())
            .map(|t| t.assignment.clone())
            .filter(|a| !a.is_empty())
            .or_else(|| super::server::job_title(world, human))
    };
    Some(match key {
        "name" => name(),
        "age" => human
            .and_then(|_| {
                let manifest = world.get_resource::<crate::records::manifest::Manifest>()?;
                Some(manifest.find(&name())?.age.to_string())
            })
            .unwrap_or_else(unknown),
        "sign" => format!("<font face=\"Segoe Script\"><i>{}</i></font>", name()),
        "time" => timestamp().1,
        "date" => timestamp().0,
        "datetime" => {
            let (date, time) = timestamp();
            format!("{date} {time}")
        }
        "station_name" => crate::round::station_name(world),
        "bank_id" => user
            .and_then(|u| world.get::<Inventory>(u)?.id_card(world))
            .and_then(|card| crate::cargo::economy::account_of(world, card))
            .map_or_else(none, |id| id.to_string()),
        "job" => job().unwrap_or_else(none),
        "department" => {
            let title = human.and_then(|h| super::server::job_title(world, h));
            title
                .and_then(|title| {
                    let jobs = world.get_resource::<crate::jobs::Jobs>()?;
                    let index = jobs.data.job(&title)? as u16;
                    let department = jobs
                        .data
                        .departments
                        .iter()
                        .find(|d| d.jobs.contains(&index))?;
                    Some(department.name.clone())
                })
                .unwrap_or_else(none)
        }
        "species" => user
            .filter(|&u| world.get::<crate::human::HumanLook>(u).is_some())
            .map_or_else(
                || "неизвестная".to_owned(),
                |u| crate::species::def_of(world, u).name.to_owned(),
            ),
        "gender" => user.map_or_else(unknown, |u| match world.get::<crate::human::HumanLook>(u) {
            Some(look) if look.female => "female".to_owned(),
            Some(_) => "male".to_owned(),
            None => "neuter".to_owned(),
        }),
        _ => return None,
    })
}

/// `"DD/MM/<station year>"` and `server_timestamp()` (`hh:mm:ss`, UTC).
fn timestamp() -> (String, String) {
    let secs = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let d = crate::events::holidays::Date::at(secs, 0.0);
    let day = secs.rem_euclid(86_400);
    // `CURRENT_STATION_YEAR`: BandaStation's `STATION_YEAR_OFFSET` is 544.
    (
        format!("{:02}/{:02}/{}", d.day, d.month, d.year + 544),
        format!("{:02}:{:02}:{:02}", day / 3600, day % 3600 / 60, day % 60),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_replaced_and_fields_kept() {
        let value = |key: &str| (key == "name").then(|| "Иван".to_owned());
        assert_eq!(
            replace_text_keys("Я, [name], прошу [input_field] [name][unknown] [", value),
            "Я, Иван, прошу [input_field] Иван[unknown] ["
        );
        assert_eq!(replace_text_keys("[name", value), "[name");
    }

    #[test]
    fn blanks_parse_like_init_paper_blanks() {
        let json = r#"[
            { "code": "A", "category": "C", "name": "Файл", "info": "a.md" },
            { "code": "B", "category": "C", "name": "Строки", "info": ["x", "y"],
              "required_access": "cent_general" },
            { "code": "C", "category": "C", "name": "Нет файла", "info": "missing.md" },
            { "code": "D", "category": "C", "name": "Вверх", "info": "../secret" }
        ]"#;
        let blanks = parse(json, |f| (f == "a.md").then(|| "текст".to_owned()));
        assert_eq!(blanks.len(), 2);
        assert_eq!(blanks[0].info, "текст");
        assert_eq!(blanks[1].info, "xy");
        let blanks = PaperBlanks(blanks);
        assert_eq!(printable(&blanks, &[]).len(), 1);
        assert_eq!(printable(&blanks, &["cent_general".to_owned()]).len(), 2);
        assert!(parse("{}", |_| None).is_empty());
    }
}

//! The station charter (tg `/obj/item/station_charter`, `code/game/objects/items/charter.dm`):
//! used in hand within the first five minutes of the round, it asks for a new station name. A
//! name in the standard format (`strings/station_*.txt`, greek letters, the phonetic alphabet,
//! numbers) is approved at once; any other goes to the admins and is approved after 60 s unless
//! one rejects it ([`reject_proposed`]). Once approved the station is renamed, announced, and the
//! charter is used up (the admin charter never is). The banner variant names a planet.
// shortcut: the admins get the proposal as a request line (`CommsRequest`); the REJECT link needs
// an admin request in `ss13_shared::admin` (coordinator contract), so [`reject_proposed`] has no
// caller in the panel yet. No game log or blackbox.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmName, MapLoadPlugin};
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::UiSystems;

use crate::inventory::Inventory;
use crate::tools::tell;

pub const CHARTER: &str = "/obj/item/station_charter";
const ADMIN_CHARTER: &str = "/obj/item/station_charter/admin";
const BANNER: &str = "/obj/item/station_charter/banner";
/// `STATION_RENAME_TIME_LIMIT`, seconds.
const RENAME_TIME_LIMIT: f64 = 300.0;
/// `approval_time` (600 deciseconds).
const APPROVAL_TIME: f64 = 60.0;
/// `MAX_CHARTER_LEN`.
const MAX_CHARTER_LEN: u32 = 80;

/// tg `set_station_name`: the station's name once renamed (`crate::round::station_name`).
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct StationName(pub String);

/// The word lists of tg's `standard_station_regex` (`GLOB.station_prefixes`, `station_names`,
/// `station_suffixes`, `station_numerals`).
#[derive(Resource, Clone, Debug, Default)]
pub struct StationNameParts {
    pub prefixes: Vec<String>,
    pub names: Vec<String>,
    pub suffixes: Vec<String>,
    pub numerals: Vec<String>,
}

impl StationNameParts {
    /// `^(([prefixes]) )?(([names]) ?)([suffixes]) ([numerals])$`.
    pub fn is_standard(&self, name: &str) -> bool {
        self.numerals.iter().any(|n| {
            name.strip_suffix(n.as_str())
                .and_then(|rest| rest.strip_suffix(' '))
                .is_some_and(|rest| {
                    self.prefixed_name_suffix(rest)
                        || self
                            .prefixes
                            .iter()
                            .filter_map(|p| rest.strip_prefix(p.as_str())?.strip_prefix(' '))
                            .any(|r| self.prefixed_name_suffix(r))
                })
        })
    }

    /// `(([names]) ?)([suffixes])`.
    fn prefixed_name_suffix(&self, text: &str) -> bool {
        self.names
            .iter()
            .filter_map(|n| text.strip_prefix(n.as_str()))
            .any(|rest| {
                let spaced = rest.strip_prefix(' ');
                self.suffixes
                    .iter()
                    .any(|s| rest == s || spaced == Some(s.as_str()))
            })
    }
}

/// A charter's live vars: `used`, and its proposal waiting for the admins
/// (`response_timer_id`).
#[derive(Component, Clone, Debug, Default)]
pub struct Charter {
    pub used: bool,
    pub pending: Option<Proposal>,
}

#[derive(Clone, Debug)]
pub struct Proposal {
    pub name: String,
    /// `user.name` (the description), `user.real_name` (the announcement).
    pub user: String,
    /// When it is approved by itself, `Time` seconds.
    pub at: f64,
}

/// On a name prompt: the charter it was asked from.
#[derive(Component, Clone, Copy, Debug)]
struct CharterAsk(Entity);

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    app.init_resource::<StationName>()
        .add_systems(Startup, load_parts)
        .add_systems(
            Update,
            (
                answers
                    .in_set(UiSystems::Handle)
                    .after(PromptSystems::Answer),
                approve_pending,
            ),
        );
    // `station_charter/attack_self`.
    app.add_click_handler(ClickPhase::AttackSelf, 3227, |w, c| {
        attack_self(w, c.client, c.mob, c.target)
    });
}

/// `strings/station_prefixes.txt`, `station_names.txt`, `station_suffixes.txt`,
/// `greek_letters.txt`, `phonetic_alphabet.txt`, `numbers_as_words.txt` and
/// `generate_number_strings()` (1..=99, arabic and roman).
fn load_parts(world: &mut World) {
    let source = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.source.clone());
    let read = |file: &str| -> Vec<String> {
        source
            .as_ref()
            .and_then(|s| ss13_data::read_to_string(&s.join("strings").join(file)).ok())
            .map(|t| {
                t.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut numerals = read("greek_letters.txt");
    numerals.extend(read("phonetic_alphabet.txt"));
    numerals.extend(read("numbers_as_words.txt"));
    for i in 1..=99 {
        numerals.push(i.to_string());
        numerals.push(roman(i));
    }
    world.insert_resource(StationNameParts {
        prefixes: read("station_prefixes.txt"),
        names: read("station_names.txt"),
        suffixes: read("station_suffixes.txt"),
        numerals,
    });
}

/// BYOND `"\Roman[n]"`.
fn roman(mut n: u32) -> String {
    const DIGITS: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (value, digits) in DIGITS {
        while n >= value {
            out.push_str(digits);
            n -= value;
        }
    }
    out
}

fn path_of(world: &World, item: Entity) -> Option<&str> {
    world.get::<DmItem>(item).map(|i| i.path.as_str())
}

fn is_charter(world: &World, item: Entity) -> bool {
    path_of(world, item).is_some_and(|p| is_under(p, CHARTER))
}

/// `name_type`: what the charter names.
fn name_type(world: &World, item: Entity) -> &'static str {
    if path_of(world, item).is_some_and(|p| is_under(p, BANNER)) {
        "планету"
    } else {
        "станцию"
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// Why the charter cannot be used now: `used`, the time limit, a proposal waiting.
fn refusal(world: &World, item: Entity) -> Option<String> {
    let charter = world.get::<Charter>(item).cloned().unwrap_or_default();
    let what = name_type(world, item);
    if charter.used {
        return Some(format!("Вы уже назвали {what}!"));
    }
    let admin = path_of(world, item).is_some_and(|p| is_under(p, ADMIN_CHARTER));
    let elapsed = world
        .get_resource::<crate::round::Ticker>()
        .map_or(0.0, |t| t.round_duration(now(world)));
    if !admin && elapsed > RENAME_TIME_LIMIT {
        return Some(format!(
            "Экипаж уже обжился на смене. Переименовывать {what} сейчас — не лучшая идея."
        ));
    }
    charter.pending.is_some().then(|| {
        "Вы всё ещё ждёте ответа работодателей на предложенное название, лучше подождать."
            .to_owned()
    })
}

/// `station_charter/attack_self`: the name question. True when `item` is a charter.
fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if !is_charter(world, item) {
        return false;
    }
    if let Some(text) = refusal(world, item) {
        tell(world, client, Span::Warning, text);
        return true;
    }
    let message = format!(
        "Как вы хотите назвать {}? Учтите, что совсем неудачные названия работодатели могут \
         отклонить, а названия в стандартном формате будут приняты автоматически.",
        crate::round::station_name(world)
    );
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Название станции",
            message,
            PromptKind::Text {
                default: String::new(),
                max_length: MAX_CHARTER_LEN,
                multiline: false,
            },
        ),
    );
    world.entity_mut(prompt).insert(CharterAsk(item));
    true
}

/// The name answers, re-checked at the moment of effect: the charter still in the user's hands,
/// still usable.
fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in answers {
        let Some(&CharterAsk(item)) = world.get::<CharterAsk>(prompt) else {
            continue;
        };
        let Some(PromptAnswer::Text(name)) = answer else {
            continue;
        };
        propose(world, client, mob, item, name.trim());
    }
}

/// The rest of `attack_self` after `tgui_input_text`.
fn propose(world: &mut World, client: Entity, mob: Entity, item: Entity, name: &str) {
    let held = world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.hands.contains(&Some(item)));
    if !held || !is_charter(world, item) {
        return;
    }
    if let Some(text) = refusal(world, item) {
        tell(world, client, Span::Warning, text);
        return;
    }
    if name.is_empty() {
        return;
    }
    let user = crate::medical::name_of(world, mob);
    let standard = world
        .get_resource::<StationNameParts>()
        .is_some_and(|p| p.is_standard(name));
    if standard {
        tell(
            world,
            client,
            Span::Notice,
            "Ваше название одобрено автоматически.",
        );
        rename_station(world, item, name, &user);
        return;
    }
    tell(
        world,
        client,
        Span::Notice,
        "Ваше название отправлено работодателям на одобрение.",
    );
    let at = now(world) + APPROVAL_TIME;
    charter_mut(world, item).pending = Some(Proposal {
        name: name.to_owned(),
        user: user.clone(),
        at,
    });
    if world.contains_resource::<Messages<crate::comms_server::CommsRequest>>() {
        world.write_message(crate::comms_server::CommsRequest {
            kind: crate::comms_server::RequestKind::StationRename,
            client,
            sender: user,
            text: format!(
                "предлагает переименовать {} в {name} (одобрится само через {} с)",
                name_type(world, item),
                APPROVAL_TIME as u32
            ),
        });
    }
}

/// The `addtimer(rename_station)` of the proposals nobody rejected.
// Cost: per tick, one pass over the charters (one or two per round).
fn approve_pending(world: &mut World) {
    let t = now(world);
    let mut charters = world.query::<(Entity, &Charter)>();
    let due: Vec<(Entity, Proposal)> = charters
        .iter(world)
        .filter_map(|(e, c)| c.pending.clone().filter(|p| p.at <= t).map(|p| (e, p)))
        .collect();
    for (item, proposal) in due {
        if let Some(mut charter) = world.get_mut::<Charter>(item) {
            charter.pending = None;
        }
        rename_station(world, item, &proposal.name, &proposal.user);
    }
}

/// `station_charter/reject_proposed`: an admin rejected the waiting name. False when nothing
/// was waiting.
pub fn reject_proposed(world: &mut World, item: Entity) -> bool {
    let Some(mut charter) = world.get_mut::<Charter>(item) else {
        return false;
    };
    if charter.pending.take().is_none() {
        return false;
    }
    // `T.visible_message` on the charter's turf.
    if let Some(pos) = crate::explosions::turf_of(world, item) {
        let clients = crate::obj_damage::clients_in_range(world, pos, 7, &[]);
        for client in clients {
            tell(
                world,
                client,
                Span::Warning,
                "Предложенные изменения исчезают с хартии — похоже, их отклонили.",
            );
        }
    }
    true
}

/// `station_charter/rename_station` (and the banner's).
fn rename_station(world: &mut World, item: Entity, designation: &str, user: &str) {
    world.insert_resource(StationName(designation.to_owned()));
    let banner = path_of(world, item).is_some_and(|p| is_under(p, BANNER));
    let (title, text, name, desc) = if banner {
        (
            "Флаг капитана",
            format!("{user} переименовал планету в {designation}"),
            format!("banner of {designation}"),
            format!(
                "The banner bears the official coat of arms of Nanotrasen, signifying that \
                 {designation} has been claimed by Captain {user} in the name of the company."
            ),
        )
    } else {
        (
            "Капитанская хартия",
            format!("{user} переименовал вашу станцию в {designation}"),
            format!("station charter for {designation}"),
            format!(
                "An official document entrusting the governance of {designation} and \
                 surrounding space to Captain {user}."
            ),
        )
    };
    crate::security_level::minor_announce(world, title, &text, false);
    let unlimited = path_of(world, item).is_some_and(|p| is_under(p, ADMIN_CHARTER));
    world.entity_mut(item).insert(Name::new(name));
    if let Some(mut dm) = world.get_mut::<DmName>(item) {
        dm.desc = desc;
    }
    if !unlimited {
        charter_mut(world, item).used = true;
    }
}

/// `item`'s [`Charter`], made on first use.
fn charter_mut(world: &mut World, item: Entity) -> Mut<'_, Charter> {
    if world.get::<Charter>(item).is_none() {
        world.entity_mut(item).insert(Charter::default());
    }
    world.get_mut::<Charter>(item).expect("inserted just above")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts() -> StationNameParts {
        StationNameParts {
            prefixes: vec!["Imperial".into()],
            names: vec!["Aegis".into(), "Space".into()],
            suffixes: vec!["Station".into(), "Outpost".into()],
            numerals: vec!["13".into(), roman(13), "Alpha".into()],
        }
    }

    #[test]
    fn standard_names_follow_the_tg_regex() {
        let p = parts();
        assert_eq!(roman(13), "XIII");
        assert_eq!(roman(99), "XCIX");
        assert!(p.is_standard("Aegis Station 13"));
        assert!(p.is_standard("Imperial Aegis Outpost XIII"));
        assert!(
            p.is_standard("AegisStation Alpha"),
            "the space after the name is optional"
        );
        assert!(!p.is_standard("Station 13"), "a name is required");
        assert!(!p.is_standard("Aegis Station"), "a numeral is required");
        assert!(!p.is_standard("Imperial  Aegis Station 13"));
        assert!(!p.is_standard("Butt Station 13"));
        assert!(!p.is_standard("aegis station 13"), "case-sensitive");
    }
}

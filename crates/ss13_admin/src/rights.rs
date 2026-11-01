//! tg admin ranks and the admin list from text config (`config/admin_ranks.txt`,
//! `config/admins.txt`, BandaStation format; tg `load_admin_ranks`, `load_admins` with
//! `admin_legacy_system`). Pure parsing: no Bevy.

use std::collections::HashMap;

use ss13_shared::admin::rights::*;

/// tg `R_DEFAULT`: what a rank has before its keywords.
pub const R_DEFAULT: u32 = R_AUTOADMIN;

/// tg `ckey()`: lowercase, only `a-z`, `0-9` and `@` kept.
pub fn ckey(key: &str) -> String {
    key.chars()
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '@')
        .collect()
}

/// tg `/datum/admin_rank`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rank {
    pub name: String,
    /// What the rank can use: `include` without `exclude`.
    pub rights: u32,
    pub include: u32,
    pub exclude: u32,
    /// `can_edit_rights` (used by the permissions panel, which is not ported).
    pub edit: u32,
}

/// One keyword of `process_keyword`; `None` for an unknown one.
pub fn keyword(word: &str) -> Option<u32> {
    Some(match word {
        "BUILD" => R_BUILD,
        "ADMIN" => R_ADMIN,
        "BAN" => R_BAN,
        "FUN" => R_FUN,
        "SERVER" => R_SERVER,
        "DEBUG" => R_DEBUG,
        "PERMISSIONS" => R_PERMISSIONS,
        "POSSESS" => R_POSSESS,
        "STEALTH" => R_STEALTH,
        "POLL" => R_POLL,
        "VAREDIT" => R_VAREDIT,
        "EVERYTHING" => R_EVERYTHING,
        "SOUND" => R_SOUND,
        "SPAWN" => R_SPAWN,
        "AUTOADMIN" => R_AUTOADMIN,
        "MENTOR" => R_MENTOR,
        "DBRANKS" => R_DBRANKS,
        _ => return None,
    })
}

/// Rights flags as tg's keywords (`rights2text`), for logs and the panel.
pub fn rights_text(rights: u32) -> String {
    const NAMES: [(u32, &str); 16] = [
        (R_BUILD, "BUILD"),
        (R_ADMIN, "ADMIN"),
        (R_BAN, "BAN"),
        (R_FUN, "FUN"),
        (R_SERVER, "SERVER"),
        (R_DEBUG, "DEBUG"),
        (R_POSSESS, "POSSESS"),
        (R_PERMISSIONS, "PERMISSIONS"),
        (R_STEALTH, "STEALTH"),
        (R_POLL, "POLL"),
        (R_VAREDIT, "VAREDIT"),
        (R_SOUND, "SOUND"),
        (R_SPAWN, "SPAWN"),
        (R_AUTOADMIN, "AUTOADMIN"),
        (R_DBRANKS, "DBRANKS"),
        (R_MENTOR, "MENTOR"),
    ];
    NAMES
        .iter()
        .filter(|(flag, _)| rights & flag != 0)
        .map(|(_, name)| *name)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Keywords separated by spaces as an `Include` line takes them; `@` is `previous` (the rank
/// above's flags of the same line). Unknown keywords are reported in `warnings`.
pub fn keywords(text: &str, previous: Option<u32>, warnings: &mut Vec<String>) -> u32 {
    let mut flags = 0;
    for word in text.split_whitespace() {
        match (word, keyword(word)) {
            ("@", _) => flags |= previous.unwrap_or(0),
            (_, Some(flag)) => flags |= flag,
            (_, None) => warnings.push(format!("unknown rights keyword {word:?}")),
        }
    }
    flags
}

/// `admin_ranks.txt`: blocks of `Name =`, `Include =`, `Exclude =`, `Edit =` lines in that order
/// (blank lines between them allowed, `#` comments skipped). A block out of order is skipped
/// with a warning.
pub fn parse_ranks(text: &str) -> (Vec<Rank>, Vec<String>) {
    const FIELDS: [&str; 4] = ["Name", "Include", "Exclude", "Edit"];
    let mut ranks: Vec<Rank> = Vec::new();
    let mut warnings = Vec::new();
    let mut block: Vec<String> = Vec::new();
    // After a broken block: its remaining lines are skipped up to the next `Name =`.
    let mut skipping = false;
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            warnings.push(format!("admin_ranks.txt:{}: not `Key = value`", i + 1));
            block.clear();
            skipping = true;
            continue;
        };
        let key = key.trim();
        if key == "Name" {
            if !block.is_empty() {
                warnings.push(format!("admin_ranks.txt:{}: incomplete rank before", i + 1));
            }
            block.clear();
            skipping = false;
        }
        if skipping {
            continue;
        }
        if FIELDS.get(block.len()) != Some(&key) {
            skipping = true;
            warnings.push(format!(
                "admin_ranks.txt:{}: expected `{} =`, got `{key} =`",
                i + 1,
                FIELDS.get(block.len()).unwrap_or(&"Name")
            ));
            block.clear();
            continue;
        }
        block.push(value.trim().to_owned());
        if block.len() == FIELDS.len() {
            let previous = ranks.last();
            let include = keywords(&block[1], previous.map(|r| r.include), &mut warnings);
            let exclude = keywords(&block[2], previous.map(|r| r.exclude), &mut warnings);
            let edit = keywords(&block[3], previous.map(|r| r.edit), &mut warnings);
            if block[0].is_empty() {
                warnings.push(format!("admin_ranks.txt:{}: rank without a name", i + 1));
            } else {
                ranks.push(Rank {
                    name: block[0].clone(),
                    rights: (R_DEFAULT | include) & !exclude,
                    include: R_DEFAULT | include,
                    exclude,
                    edit,
                });
            }
            block.clear();
        }
    }
    (ranks, warnings)
}

/// tg `/datum/admins` as loaded: the ranks joined by `+` and their rights.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Holder {
    pub rank: String,
    pub rights: u32,
}

/// `admins.txt`: `key = Rank` (or `Rank1+Rank2`) per line, `#` comments. A key whose ranks are
/// all unknown is not made an admin (a warning); unknown ranks of several are dropped.
pub fn parse_admins(text: &str, ranks: &[Rank]) -> (HashMap<String, Holder>, Vec<String>) {
    let mut admins = HashMap::new();
    let mut warnings = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        if raw.starts_with('#') || raw.trim().is_empty() {
            continue;
        }
        // tg `^(?!#)(.+?)\s+=\s+(.+)`: whitespace on both sides of the first such `=`.
        let Some((key, rank)) = raw.split_once(" = ").or_else(|| raw.split_once("\t=\t")) else {
            warnings.push(format!("admins.txt:{}: not `key = rank`", i + 1));
            continue;
        };
        let (key, rank) = (ckey(key.trim()), rank.trim());
        if key.is_empty() {
            continue;
        }
        let found: Vec<&Rank> = rank
            .split('+')
            .filter_map(|name| {
                let found = ranks.iter().find(|r| r.name == name.trim());
                if found.is_none() {
                    warnings.push(format!("admins.txt:{}: unknown rank {name:?}", i + 1));
                }
                found
            })
            .collect();
        if found.is_empty() {
            continue;
        }
        admins.insert(
            key,
            Holder {
                rank: found
                    .iter()
                    .map(|r| r.name.as_str())
                    .collect::<Vec<_>>()
                    .join("+"),
                rights: found.iter().fold(0, |acc, r| acc | r.rights),
            },
        );
    }
    (admins, warnings)
}

/// tg `check_rights_for`: every flag of `required` (any of them with `any`).
pub fn has(rights: u32, required: u32) -> bool {
    rights & required == required
}

/// tg `check_if_greater_rights_than`: whether `actor` may act against an admin with `target`
/// rights (kick, ban, mute, smite): `R_EVERYTHING`, or a strict superset of the target's.
pub fn greater_rights(actor: u32, target: u32) -> bool {
    actor == R_EVERYTHING || (actor & target == target && actor != target)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The head of BandaStation's `config/admin_ranks.txt`.
    const RANKS: &str = "#comment
Name = Admin Observer
Include =
Exclude = AUTOADMIN
Edit =

Name = Moderator
Include = ADMIN
Exclude =
Edit =

Name = Admin Candidate
Include = @
Exclude =
Edit =

Name = Trial Admin
Include = @ SPAWN VAREDIT BAN
Exclude =
Edit =

Name = Game Master
Include = EVERYTHING
Exclude =
Edit = EVERYTHING

Name = Coder
Include = DEBUG VAREDIT SERVER SPAWN POLL
Exclude = AUTOADMIN
Edit =
";

    fn rank<'a>(ranks: &'a [Rank], name: &str) -> &'a Rank {
        ranks.iter().find(|r| r.name == name).unwrap()
    }

    #[test]
    fn ranks_parse_with_inheritance_and_exclusion() {
        let (ranks, warnings) = parse_ranks(RANKS);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(ranks.len(), 6);
        assert_eq!(rank(&ranks, "Admin Observer").rights, 0);
        assert_eq!(rank(&ranks, "Moderator").rights, R_ADMIN | R_AUTOADMIN);
        // `@` takes the rank above's includes.
        assert_eq!(
            rank(&ranks, "Admin Candidate").rights,
            R_ADMIN | R_AUTOADMIN
        );
        let trial = rank(&ranks, "Trial Admin").rights;
        assert_eq!(
            trial,
            R_ADMIN | R_AUTOADMIN | R_SPAWN | R_VAREDIT | R_BAN,
            "{}",
            rights_text(trial)
        );
        assert_eq!(rank(&ranks, "Game Master").rights, R_EVERYTHING);
        assert_eq!(rank(&ranks, "Game Master").edit, R_EVERYTHING);
        let coder = rank(&ranks, "Coder").rights;
        assert!(has(coder, R_DEBUG | R_SPAWN) && !has(coder, R_ADMIN));
        assert!(!has(coder, R_AUTOADMIN));
    }

    #[test]
    fn broken_blocks_are_skipped_with_a_warning() {
        let (ranks, warnings) = parse_ranks(
            "Name = A\nExclude =\nEdit =\n\nName = B\nInclude = FUN NOPE\nExclude =\nEdit =\n",
        );
        assert_eq!(ranks.len(), 1);
        assert_eq!(ranks[0].rights, R_FUN | R_AUTOADMIN);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
    }

    #[test]
    fn admins_match_ranks_by_ckey() {
        let (ranks, _) = parse_ranks(RANKS);
        let text = "# comment\nOptimumtact = Game Master\nSome Body = Moderator+Coder\n\
                    ghost = No Such Rank\nbad line\n";
        let (admins, warnings) = parse_admins(text, &ranks);
        assert_eq!(admins.len(), 2);
        assert_eq!(admins["optimumtact"].rights, R_EVERYTHING);
        let both = &admins["somebody"];
        assert_eq!(both.rank, "Moderator+Coder");
        assert!(has(both.rights, R_ADMIN | R_DEBUG | R_AUTOADMIN));
        assert_eq!(warnings.len(), 2, "{warnings:?}");
    }

    #[test]
    fn ckey_keeps_letters_digits_and_at() {
        assert_eq!(ckey("Gun Hog"), "gunhog");
        assert_eq!(ckey("events-Alice_2"), "eventsalice2");
        assert_eq!(ckey("Guest@Home"), "guest@home");
    }

    #[test]
    fn greater_rights_needs_a_strict_superset() {
        assert!(greater_rights(R_EVERYTHING, R_EVERYTHING));
        assert!(greater_rights(R_ADMIN | R_BAN, R_ADMIN));
        assert!(!greater_rights(R_ADMIN, R_ADMIN));
        assert!(!greater_rights(R_ADMIN | R_BAN, R_ADMIN | R_FUN));
        assert!(greater_rights(R_ADMIN, 0));
    }
}

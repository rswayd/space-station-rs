//! Silicon laws, after tg `/datum/ai_laws` (`code/datums/ai_laws/ai_laws.dm`): the round's
//! default lawset from the config, the law list as `show_laws` prints it, and a cyborg's copy of
//! its AI's laws (`ai_to_cyborg`, `lawsync`).
// shortcut: no law boards, upload console or ion laws events; laws change only by lawsync.

use bevy::prelude::*;
use ss13_mapload::silicon::{Lawset, SiliconData};

/// BandaStation `config/game_options.txt`: `DEFAULT_LAWS 4` (`CONFIG_SPECIFIED`) with
/// `SPECIFIED_LAWS crewsimovpp`.
pub const SPECIFIED_LAWS: &[&str] = &["crewsimovpp"];
/// tg `AI_LAWS_ASIMOV`: what a missing lawset falls back to.
// shortcut: tg picks a random `/datum/ai_laws/default` subtype when no specified law exists;
// here Asimov.
pub const FALLBACK_LAWS: &str = "asimov";

/// A silicon's laws (tg `laws`).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct Laws {
    pub name: String,
    pub zeroth: Option<String>,
    pub zeroth_borg: Option<String>,
    /// (law, ioned).
    pub inherent: Vec<(String, bool)>,
    pub supplied: Vec<(String, bool)>,
    pub hacked: Vec<String>,
}

impl Laws {
    pub fn from_lawset(set: &Lawset) -> Self {
        Self {
            name: set.name.clone(),
            zeroth: set.zeroth.clone(),
            zeroth_borg: set.zeroth_borg.clone(),
            inherent: set.inherent.iter().map(|l| (l.clone(), false)).collect(),
            supplied: Vec::new(),
            hacked: Vec::new(),
        }
    }

    /// tg `ai_to_cyborg`: the AI's laws as its cyborg gets them (`zeroth_borg || zeroth`).
    pub fn for_cyborg(&self) -> Self {
        Self {
            name: self.name.clone(),
            zeroth: self.zeroth_borg.clone().or_else(|| self.zeroth.clone()),
            zeroth_borg: None,
            inherent: self.inherent.clone(),
            supplied: self.supplied.clone(),
            hacked: self.hacked.clone(),
        }
    }

    /// tg `get_law_list(include_zeroth, show_numbers = TRUE, render_html = FALSE)`.
    pub fn law_list(&self, include_zeroth: bool) -> Vec<String> {
        let mut out = Vec::new();
        if include_zeroth && let Some(z) = &self.zeroth {
            out.push(format!("0: {z}"));
        }
        for law in self.hacked.iter().filter(|l| !l.is_empty()) {
            out.push(format!("{}: {law}", ion_num()));
        }
        let mut number = 1;
        for (law, ioned) in self.inherent.iter().chain(&self.supplied) {
            if law.is_empty() {
                continue;
            }
            if *ioned {
                out.push(format!("{}: {law}", ion_num()));
            } else {
                out.push(format!("{number}: {law}"));
                number += 1;
            }
        }
        out
    }
}

/// tg `ion_num()` is random `"[pick(...)]#!"`-style garbage; a fixed one keeps the list stable.
// shortcut: no randomized ion numbers (there are no ion laws yet).
fn ion_num() -> &'static str {
    "#"
}

/// tg `setup_round_default_laws()` with `CONFIG_SPECIFIED`.
pub fn round_default(data: &SiliconData) -> Laws {
    SPECIFIED_LAWS
        .iter()
        .find_map(|id| data.lawset(id))
        .or_else(|| data.lawset(FALLBACK_LAWS))
        .map(Laws::from_lawset)
        .unwrap_or_else(|| Laws {
            name: "Азимов".into(),
            inherent: [
                "Вы не можете причинить вред человеку или своим бездействием допустить, чтобы человеку был причинён вред.",
                "Вы должны повиноваться всем приказам, которые даёт человек, кроме тех случаев, когда эти приказы противоречат Первому Закону.",
                "Вы должны заботиться о своей безопасности в той мере, в которой это не противоречит Первому или Второму Законам.",
            ]
            .map(|l| (l.to_owned(), false))
            .to_vec(),
            ..default()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> SiliconData {
        SiliconData {
            lawsets: vec![
                Lawset {
                    id: "asimov".into(),
                    name: "Азимов".into(),
                    inherent: vec!["a1".into(), "a2".into()],
                    ..default()
                },
                Lawset {
                    id: "crewsimovpp".into(),
                    name: "Крюзимов++".into(),
                    inherent: vec!["c1".into(), "c2".into(), "c3".into()],
                    ..default()
                },
            ],
            ..default()
        }
    }

    #[test]
    fn default_lawset_is_the_specified_one() {
        let laws = round_default(&data());
        assert_eq!(laws.name, "Крюзимов++");
        assert_eq!(laws.law_list(true), ["1: c1", "2: c2", "3: c3"]);
        let mut only_asimov = data();
        only_asimov.lawsets.remove(1);
        assert_eq!(round_default(&only_asimov).name, "Азимов");
        assert_eq!(round_default(&SiliconData::default()).inherent.len(), 3);
    }

    #[test]
    fn law_list_numbers_like_tg() {
        let laws = Laws {
            zeroth: Some("z".into()),
            zeroth_borg: Some("zb".into()),
            inherent: vec![
                ("i1".into(), false),
                ("ion".into(), true),
                ("i2".into(), false),
            ],
            supplied: vec![("s1".into(), false)],
            hacked: vec!["h".into()],
            ..default()
        };
        assert_eq!(
            laws.law_list(true),
            ["0: z", "#: h", "1: i1", "#: ion", "2: i2", "3: s1"]
        );
        assert_eq!(laws.law_list(false)[0], "#: h");
        let borg = laws.for_cyborg();
        assert_eq!(borg.zeroth.as_deref(), Some("zb"));
        assert_eq!(borg.inherent, laws.inherent);
    }
}

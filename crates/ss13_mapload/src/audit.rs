//! `ss13-mapload audit`: the copied assets (sounds, `sound/SOURCES.txt`) and the source `.dmi`
//! files of the atlases (`ICON-SOURCES.txt`) whose attribution/license files name terms other
//! than BandaStation's default CC BY-SA 3.0, written as `docs/ASSET-AUDIT.md`.
//!
//! The terms come from [`RULES`], written by reading the attribution files (they are free text,
//! so nothing here parses licenses). An asset whose file name an attribution file on its path
//! mentions but no rule covers is reported as not reviewed, so new upstream entries surface.
//! Nothing is removed or replaced: this is a report.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use ss13_assets::import::{ICON_SOURCES_FILE, LICENSES_DIR};
use ss13_assets::license::is_attribution_file;

use crate::sound::SOUND_DIR;

/// `glob \t terms \t risk \t attribution file` per line; `#` comments. The first rule whose glob
/// (`*` matches any run of characters, `/` included) matches an asset's source path wins; risk
/// `-` means reviewed, nothing to report.
pub const RULES: &str = include_str!("asset_terms.tsv");

/// Risk labels in report order.
pub const RISKS: [&str; 4] = ["high", "medium", "low", "unreviewed"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub glob: String,
    pub terms: String,
    pub risk: String,
    pub file: String,
}

pub fn parse_rules(text: &str) -> anyhow::Result<Vec<Rule>> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|(i, l)| {
            let f: Vec<&str> = l.split('\t').collect();
            let [glob, terms, risk, file] = f[..] else {
                anyhow::bail!("asset_terms.tsv:{}: expected 4 tab-separated fields", i + 1);
            };
            if risk != "-" && !RISKS[..3].contains(&risk) {
                anyhow::bail!("asset_terms.tsv:{}: unknown risk {risk:?}", i + 1);
            }
            Ok(Rule {
                glob: glob.into(),
                terms: terms.into(),
                risk: risk.into(),
                file: file.into(),
            })
        })
        .collect()
}

/// `*` matches any run of characters, `/` included.
pub fn glob_match(glob: &str, path: &str) -> bool {
    match glob.split_once('*') {
        None => glob == path,
        Some((head, rest)) => {
            let Some(path) = path.strip_prefix(head) else {
                return false;
            };
            (0..=path.len())
                .filter(|&i| path.is_char_boundary(i))
                .any(|i| glob_match(rest, &path[i..]))
        }
    }
}

/// One reported asset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// Source path in BandaStation.
    pub asset: String,
    pub file: String,
    pub terms: String,
    pub risk: String,
}

/// An attribution file mentions `name` (the file name, or its stem when that is a word of at
/// least 3 characters with a letter) as a whole word.
fn mentions(text: &str, name: &str) -> bool {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
    let needle = if stem.len() >= 3 && stem.chars().any(|c| c.is_ascii_alphabetic()) {
        stem
    } else {
        name
    };
    let (text, needle) = (text.to_lowercase(), needle.to_lowercase());
    text.match_indices(&needle).any(|(i, m)| {
        !text[..i].chars().next_back().is_some_and(word)
            && !text[i + m.len()..].chars().next().is_some_and(word)
    })
}

/// The attribution files on `asset`'s path inside `root` (the copied tree), nearest first.
fn attribution_files(root: &Path, asset: &str) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    let mut dir = Path::new(asset).parent();
    while let Some(d) = dir {
        if let Ok(entries) = std::fs::read_dir(root.join(d)) {
            let mut names: Vec<_> = entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| is_attribution_file(n))
                .collect();
            names.sort();
            for n in names {
                found.push((d.join(&n).display().to_string(), root.join(d).join(n)));
            }
        }
        dir = d.parent();
    }
    found
}

/// Classifies `assets` (source paths; each with the root of its copied attribution files).
pub fn audit<'a>(
    rules: &[Rule],
    assets: impl IntoIterator<Item = (&'a str, &'a Path)>,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (asset, root) in assets {
        if let Some(rule) = rules.iter().find(|r| glob_match(&r.glob, asset)) {
            if rule.risk != "-" {
                findings.push(Finding {
                    asset: asset.into(),
                    file: rule.file.clone(),
                    terms: rule.terms.clone(),
                    risk: rule.risk.clone(),
                });
            }
            continue;
        }
        let name = asset.rsplit('/').next().unwrap_or(asset);
        let mentioned = attribution_files(root, asset)
            .into_iter()
            .find(|(_, path)| {
                std::fs::read_to_string(path).is_ok_and(|text| mentions(&text, name))
            });
        if let Some((file, _)) = mentioned {
            findings.push(Finding {
                asset: asset.into(),
                file,
                terms: "named in an attribution file, no rule in asset_terms.tsv yet".into(),
                risk: RISKS[3].into(),
            });
        }
    }
    findings
}

/// Source paths of the copied sounds and of the atlases' `.dmi` files, with the roots of their
/// copied attribution files.
pub fn copied_assets(assets: &Path) -> anyhow::Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    let sound_root = assets.join(SOUND_DIR);
    let sources = sound_root.join("SOURCES.txt");
    let text = std::fs::read_to_string(&sources)
        .with_context(|| format!("{} (run `ss13-mapload convert`)", sources.display()))?;
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        if let Some((rel, _)) = line.split_once('\t') {
            out.push((rel.to_owned(), sound_root.clone()));
        }
    }
    let icons = assets.join(ICON_SOURCES_FILE);
    let text = std::fs::read_to_string(&icons)
        .with_context(|| format!("{} (run `ss13-import dmi`)", icons.display()))?;
    let icon_root = assets.join(LICENSES_DIR);
    let mut dmis: Vec<&str> = text
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split('\t').nth(1))
        .filter(|s| *s != "?")
        .collect();
    dmis.sort_unstable();
    dmis.dedup();
    out.extend(dmis.into_iter().map(|d| (d.to_owned(), icon_root.clone())));
    Ok(out)
}

/// `docs/ASSET-AUDIT.md`.
pub fn report_markdown(findings: &[Finding], total: usize, commit: &str) -> String {
    let mut md = String::from(
        "# Asset license audit\n\n\
         Generated by `ss13-mapload audit`; do not edit by hand (the terms live in \
         `crates/ss13_mapload/src/asset_terms.tsv`).\n\n",
    );
    let _ = writeln!(
        md,
        "{total} copied assets checked (the sounds in `sound/SOURCES.txt` and the source `.dmi` \
         files of the atlases in `ICON-SOURCES.txt`), from BandaStation commit `{commit}`. Listed \
         below: those whose attribution/license files name terms other than CC BY-SA 3.0 (the \
         default for BandaStation assets) or a restrictive or unknown source. Nothing was \
         removed or replaced.\n"
    );
    md.push_str("| Risk | Assets |\n| --- | --- |\n");
    for risk in RISKS {
        let n = findings.iter().filter(|f| f.risk == risk).count();
        let _ = writeln!(md, "| {risk} | {n} |");
    }
    md.push_str(
        "\nRisk: **high**: non-commercial or closed terms (NC, BBC, personal use only), not \
         compatible with CC BY-SA 3.0 / AGPL distribution; **medium**: a third-party source with \
         no stated license, or a site license that limits redistribution; **low**: a compatible \
         license (CC0, public domain, CC BY), attribution only; **unreviewed**: named in an \
         attribution file, but no rule covers it yet.\n\n",
    );
    md.push_str("| File | Source | Stated terms | Risk |\n| --- | --- | --- | --- |\n");
    let mut sorted: Vec<&Finding> = findings.iter().collect();
    sorted.sort_by_key(|f| {
        (
            RISKS
                .iter()
                .position(|r| *r == f.risk)
                .unwrap_or(RISKS.len()),
            f.asset.clone(),
        )
    });
    for f in sorted {
        let _ = writeln!(
            md,
            "| `{}` | `{}` | {} | {} |",
            f.asset,
            f.file,
            f.terms.replace('|', "\\|"),
            f.risk
        );
    }
    md
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_parse_and_classify_with_unreviewed_mentions() {
        let rules = parse_rules(RULES).unwrap();
        assert!(glob_match("sound/a/visor_*", "sound/a/visor_up1.ogg"));
        assert!(glob_match("sound/*/x.ogg", "sound/a/b/x.ogg"));
        assert!(!glob_match("sound/*/x.ogg", "sound/a/bx.ogg"));
        assert!(mentions("bulletflyby2:https://", "bulletflyby2.ogg"));
        assert!(!mentions("bulletflyby2:https://", "bulletflyby.ogg"));
        assert!(!mentions("CC BY 4.0", "4.ogg"));

        let dir = std::env::temp_dir().join(format!("ss13-audit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sound/items/handling/helmet")).unwrap();
        std::fs::create_dir_all(dir.join("sound/new")).unwrap();
        std::fs::write(dir.join("sound/new/attribution.txt"), "fresh.ogg - NC").unwrap();
        let assets = [
            "sound/items/handling/helmet/visor_up1.ogg",
            "sound/new/fresh.ogg",
            "sound/new/other.ogg",
            "sound/items/handling/cloth/cloth_drop2.ogg",
        ];
        let findings = audit(&rules, assets.iter().map(|a| (*a, dir.as_path())));
        let got: Vec<(&str, &str)> = findings
            .iter()
            .map(|f| (f.asset.as_str(), f.risk.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("sound/items/handling/helmet/visor_up1.ogg", "high"),
                ("sound/new/fresh.ogg", "unreviewed"),
            ]
        );
        assert_eq!(findings[1].file, "sound/new/attribution.txt");
        assert!(report_markdown(&findings, 4, "abc").contains("| high | 1 |"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

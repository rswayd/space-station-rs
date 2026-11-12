//! Scars, after tg `/datum/scar` (`code/datums/wounds/scars/_scars.dm`) and the scar strings
//! (`strings/wounds/flesh_scar_desc.json`, `scar_loc.json`, translated).
//!
//! A wound that heals (tg `remove_wound` when not replaced or destroyed: [`heal_wound`]) leaves
//! a scar of its `scar_keyword` on the limb, at the worst severity a cut was at before it
//! demoted. The description and the spot on the limb are rolled on the next `Life()` tick
//! ([`roll_pending`]). Scars show in the double examine (`carbon/examine_more`,
//! [`examine_more_lines`]) to viewers within their visibility (2, 3, 5 tiles by severity), one
//! less per piece of clothing over the limb; a head's scars hide behind a covered face.
// shortcut: flesh descriptions only (bone-only limbs are not modelled); no persistent scars
// (SQL/preferences come last), no fake scars (admin smite, monkeys), no amputation scars.

use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::{Limb, Limbs, Wound, WoundKind, remove_wound};
use crate::combat::CombatRng;
use crate::zone::BodyZone;

/// A healed wound's mark on a limb.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scar {
    /// `scar_keyword` of the wound.
    pub keyword: &'static str,
    /// `WOUND_SEVERITY_MODERATE` 1 .. `CRITICAL` 3.
    pub severity: u8,
    /// Empty until rolled.
    pub description: String,
    /// `precise_location`, a prepositional phrase ("на левом локте").
    pub location: String,
}

/// tg `scar_keyword` of a wound type.
pub fn keyword(kind: WoundKind) -> &'static str {
    match kind {
        WoundKind::SlashModerate => "slashmoderate",
        WoundKind::SlashSevere => "slashsevere",
        WoundKind::SlashCritical => "slashcritical",
        WoundKind::PierceModerate => "piercemoderate",
        WoundKind::PierceSevere => "piercesevere",
        WoundKind::PierceCritical => "piercecritical",
        WoundKind::Dislocation => "dislocate",
        WoundKind::Hairline => "bluntsevere",
        WoundKind::Compound => "bluntcritical",
        WoundKind::BurnModerate => "burnmoderate",
        WoundKind::BurnSevere => "burnsevere",
        WoundKind::BurnCritical => "burncritical",
    }
}

/// `strings/wounds/flesh_scar_desc.json`.
fn descriptions(keyword: &str) -> &'static [&'static str] {
    match keyword {
        "dislocate" | "bluntmoderate" => &[
            "лёгкое обесцвечивание",
            "лёгкий синеватый оттенок",
            "слегка омертвевший оттенок",
        ],
        "bluntsevere" => &[
            "выцветший синяк размером с кулак",
            "смутно треугольный шрам от содранной кожи",
        ],
        "bluntcritical" => &[
            "участок неровных линий кожи и плохо заживших шрамов",
            "большое пятно кожи неровного цвета",
            "скопление мозолей",
        ],
        "slashmoderate" => &[
            "светлые выцветшие линии",
            "мелкие следы порезов",
            "маленький выцветший разрез",
            "череда мелких шрамов",
        ],
        "slashsevere" => &[
            "кривая линия выцветших рубцов",
            "узловатый серповидный шрам от пореза",
        ],
        "slashcritical" => &[
            "извилистая дорожка очень плохо зажившей рубцовой ткани",
            "череда бугров и впадин вдоль жуткой линии рубцовой ткани",
            "гротескная змейка вмятин и следов от швов",
        ],
        "piercemoderate" => &[
            "маленький выцветший синяк",
            "небольшой завиток сросшейся кожи",
            "колотый шрам размером с большой палец",
        ],
        "piercesevere" => &[
            "клякса рубцовой ткани",
            "давно поблёкшая колотая рана",
            "рваное колотое отверстие со следами выцветших швов",
        ],
        "piercecritical" => &[
            "расходящаяся волна рубцовой ткани",
            "широкое рассеянное облако следов от осколков",
            "жуткий многолучевой колотый шрам",
        ],
        "burnmoderate" => &[
            "мелкие амёбообразные отметины на коже",
            "выцветшая полоса впалой кожи",
        ],
        "burnsevere" => &[
            "большое зазубренное пятно выцветшей кожи",
            "беспорядочные пятна блестящей гладкой кожи",
            "пятна натянутой огрубевшей кожи",
        ],
        "burncritical" => &[
            "огромные уродующие келоидные рубцы",
            "несколько длинных полос сильно обесцвеченной и деформированной кожи",
            "безошибочно узнаваемые пятна омертвевшей ткани от сильных ожогов",
        ],
        _ => &["общее обезображивание"],
    }
}

/// `@pick(inner_outer)`, `@pick(upper_lower)`, `@pick(free_move)` as prefixes of the part's
/// genitive ("" keeps the plain phrase).
const INNER_OUTER: &[&str] = &["на внутренней стороне ", "на внешней стороне ", "", ""];
const UPPER_LOWER: &[&str] = &["в верхней части ", "в нижней части ", "", ""];
const FREE_MOVE: &[&str] = &[
    "на внутренней стороне ",
    "на внешней стороне ",
    "в верхней части ",
    "в нижней части ",
    "",
    "",
];

/// One `scar_loc.json` entry: the plain phrase, or (plain, genitive, modifiers).
type Loc = (&'static str, &'static str, &'static [&'static str]);

/// `strings/wounds/scar_loc.json` by zone.
fn locations(zone: BodyZone) -> &'static [Loc] {
    match zone {
        BodyZone::Head => &[
            ("на брови", "брови", &["на левой ", "на правой "]),
            ("на скуле", "скуле", &["на левой ", "на правой "]),
            ("на носу", "", &[]),
            ("на шее", "", &[]),
            ("на горле", "", &[]),
            ("на линии челюсти", "", &[]),
            ("по всему лицу", "", &[]),
        ],
        BodyZone::Chest => &[
            ("в верхней части груди", "", &[]),
            ("на животе", "живота", UPPER_LOWER),
            ("на середине туловища", "", &[]),
            ("на ключице", "", &[]),
            ("на пояснице", "", &[]),
        ],
        BodyZone::LArm => &[
            ("на левом предплечье", "левого предплечья", INNER_OUTER),
            ("на левом запястье", "левого запястья", INNER_OUTER),
            ("на левом локте", "", &[]),
            ("на левом бицепсе", "", &[]),
            ("на левом плече", "", &[]),
            ("на левой кисти", "", &[]),
        ],
        BodyZone::RArm => &[
            ("на правом предплечье", "правого предплечья", INNER_OUTER),
            ("на правом запястье", "правого запястья", INNER_OUTER),
            ("на правом локте", "", &[]),
            ("на правом бицепсе", "", &[]),
            ("на правом плече", "", &[]),
            ("на правой кисти", "", &[]),
        ],
        BodyZone::LLeg => &[
            ("на левом бедре", "левого бедра", INNER_OUTER),
            ("на левой икре", "левой икры", FREE_MOVE),
            ("на левом боку", "левого бока", FREE_MOVE),
            ("на левом колене", "", &[]),
            ("на левой голени", "левой голени", FREE_MOVE),
        ],
        BodyZone::RLeg => &[
            ("на правом бедре", "правого бедра", INNER_OUTER),
            ("на правой икре", "правой икры", FREE_MOVE),
            ("на правом боку", "правого бока", FREE_MOVE),
            ("на правом колене", "", &[]),
            ("на правой голени", "правой голени", FREE_MOVE),
        ],
    }
}

/// tg `remove_wound` of a wound that went away by itself or by treatment: it scars the limb
/// (`can_scar`; robotic limbs are not `scarrable`).
pub fn heal_wound(limb: &mut Limb, index: usize) -> Wound {
    let wound = remove_wound(limb, index);
    if !limb.robotic {
        limb.scars.push(Scar {
            keyword: keyword_at(wound.kind, wound.scar_severity),
            severity: wound.scar_severity,
            ..default()
        });
    }
    wound
}

/// The keyword of `kind`'s series at `severity` (a demoted cut keeps its worst scar).
fn keyword_at(kind: WoundKind, severity: u8) -> &'static str {
    let worst = match (kind, severity) {
        (WoundKind::SlashModerate | WoundKind::SlashSevere, 3) => WoundKind::SlashCritical,
        (WoundKind::SlashModerate, 2) => WoundKind::SlashSevere,
        _ => kind,
    };
    keyword(worst)
}

/// `generate`: the description (`pick_list`) and the spot (`pick_list_replacements`) of the
/// limb's new scars.
pub fn roll_pending(rng: &mut CombatRng, limb: &mut Limb, zone: BodyZone) {
    let mut pick = |n: usize| rng.range(0, n as u32 - 1) as usize;
    for scar in limb.scars.iter_mut().filter(|s| s.description.is_empty()) {
        let list = descriptions(scar.keyword);
        scar.description = list[pick(list.len())].to_owned();
        let locs = locations(zone);
        let (plain, genitive, mods) = locs[pick(locs.len())];
        scar.location = if mods.is_empty() {
            plain.to_owned()
        } else {
            match mods[pick(mods.len())] {
                "" => plain.to_owned(),
                prefix => format!("{prefix}{genitive}"),
            }
        };
    }
}

/// `visibility` by severity.
fn visibility(severity: u8) -> i32 {
    match severity {
        1 => 2,
        2 => 3,
        _ => 5,
    }
}

/// `is_visible`: within the scar's visibility, less per piece of clothing over the limb; a
/// face behind a mask or helmet hides the head's scars. Ghosts and the scarred one see all.
fn is_visible(world: &World, viewer: Entity, mob: Entity, zone: BodyZone, scar: &Scar) -> bool {
    let (Some(&a), Some(&b)) = (world.get::<TilePos>(viewer), world.get::<TilePos>(mob)) else {
        return false;
    };
    let dist = (a.x - b.x).abs().max((a.y - b.y).abs());
    let range = visibility(scar.severity);
    if a.z != b.z || dist > range {
        return false;
    }
    if viewer == mob
        || world.get::<crate::ghost::Ghost>(viewer).is_some()
        || world.get::<crate::human::HumanLook>(mob).is_none()
    {
        return true;
    }
    if zone == BodyZone::Head {
        return !crate::clothing::cover::face_obscured(world, mob);
    }
    let covers = clothing_on_part(world, mob, zone);
    covers + dist < range
}

/// `get_clothing_on_part`: worn clothing whose `body_parts_covered` has the limb's part.
pub(crate) fn clothing_on_part(world: &World, mob: Entity, zone: BodyZone) -> i32 {
    let Some(inventory) = world.get::<crate::inventory::Inventory>(mob) else {
        return 0;
    };
    let bit = crate::chem::body_part_bit(zone);
    inventory
        .equipped()
        .filter(|&item| {
            inventory.hands.iter().all(|h| *h != Some(item))
                && world
                    .get::<ss13_mapload::DmClothing>(item)
                    .is_some_and(|c| c.body_parts_covered & bit != 0)
        })
        .count() as i32
}

/// `carbon/examine_more`: the scars `viewer` can see on `mob`.
pub fn examine_more_lines(
    world: &World,
    viewer: Entity,
    mob: Entity,
    lines: &mut Vec<(Span, String)>,
) {
    let Some(limbs) = world.get::<Limbs>(mob) else {
        return;
    };
    for zone in BodyZone::ALL {
        for scar in &limbs.limb(zone).scars {
            if scar.description.is_empty() || !is_visible(world, viewer, mob, zone, scar) {
                continue;
            }
            let span = if scar.severity >= 3 {
                Span::BoldNotice
            } else {
                Span::Notice
            };
            lines.push((
                span,
                format!("У него {} {}.", scar.description, scar.location),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Wound, apply_wound};
    use super::*;

    #[test]
    fn a_healed_cut_scars_at_its_worst() {
        let mut limb = Limb::default();
        apply_wound(&mut limb, WoundKind::SlashCritical);
        // Demoted twice: the scar stays critical.
        apply_wound(&mut limb, WoundKind::SlashSevere);
        apply_wound(&mut limb, WoundKind::SlashModerate);
        heal_wound(&mut limb, 0);
        assert_eq!(limb.scars.len(), 1);
        assert_eq!(limb.scars[0].keyword, "slashcritical");
        assert_eq!(limb.scars[0].severity, 3);
        let mut rng = CombatRng::default();
        roll_pending(&mut rng, &mut limb, BodyZone::LArm);
        assert!(descriptions("slashcritical").contains(&limb.scars[0].description.as_str()));
        assert!(
            limb.scars[0].location.contains("лев"),
            "{:?}",
            limb.scars[0]
        );
    }

    #[test]
    fn robotic_limbs_and_replacements_do_not_scar() {
        let mut limb = Limb {
            robotic: true,
            ..default()
        };
        limb.wounds.push(Wound::new(WoundKind::BurnSevere));
        heal_wound(&mut limb, 0);
        assert!(limb.scars.is_empty());
        let mut flesh = Limb::default();
        apply_wound(&mut flesh, WoundKind::PierceModerate);
        apply_wound(&mut flesh, WoundKind::PierceSevere);
        assert!(flesh.scars.is_empty(), "a worse wound replaces, no scar");
    }
}

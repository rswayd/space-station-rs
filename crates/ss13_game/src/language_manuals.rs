//! Language manuals (server), after tg `code/modules/language/_language_manuals.dm`: read in
//! hand (`attack_self`) a manual teaches its language (spoken and understood, any block on it
//! lifted, and `TRAIT_TOWER_OF_BABEL`), or hit on someone's head (`attack`) teaches them. Every
//! lesson takes a charge; the last one turns the manual into a random book manual.
//!
//! - Codespeak (1 charge, the deluxe one unlimited), Captain Pete's pirate lingo (5), the
//!   dronespeak manual (unlimited, drones and silicons only), the plain manual (Galactic
//!   Common).
//! - A roundstart species manual picks one of `GLOB.uncommon_roundstart_languages` when it
//!   appears and is named after it (the extended one has 5 charges, the deluxe one unlimited;
//!   the Draconic one teaches Draconic). tg declares their `language` as Galactic Common, so
//!   its `if(!language)` never picks: every one of them teaches Common; that is not ported.

use bevy::prelude::*;
use ss13_chat::language::{
    self, FULL, LanguageHolder, LanguageId, LanguageSource, UNDERSTOOD, basic_understanding,
    edit_holder,
};
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmName};
use ss13_shared::chat::{ChatChannel, Span};

use crate::combat::CombatRng;
use crate::health::{DEFAULT_MESSAGE_RANGE, Stat};
use crate::security::name_of;
use crate::sound::{PlaySound, playsound};
use crate::tools::tell;

pub const MANUAL: &str = "/obj/item/language_manual";
const DRONESPEAK: &str = "/obj/item/language_manual/dronespeak_manual";
/// `SFX_PUNCH`.
const PUNCH: &str = "punch";

/// A manual type: what it teaches (`None`: a roundstart species language picked when it
/// appears), `charges` (`None`: `INFINITY`), `flavour_text`, and the name a species manual
/// gets.
struct Kind {
    root: &'static str,
    language: Option<LanguageId>,
    charges: Option<u32>,
    flavour: &'static str,
    /// `"[initial(language.name)] manual"` and its deluxe, extended forms.
    species_name: Option<&'static str>,
}

const fn kind(
    root: &'static str,
    language: Option<LanguageId>,
    charges: Option<u32>,
    flavour: &'static str,
    species_name: Option<&'static str>,
) -> Kind {
    Kind {
        root,
        language,
        charges,
        flavour,
        species_name,
    }
}

const CODESPEAK_FLAVOUR: &str = "внезапно ваш разум наполняется кодовыми словами и ответами";
/// `"you feel empowered with a mastery over [language]"`; `{}` is the language.
const SPECIES_FLAVOUR: &str = "вы чувствуете, что овладели языком {}";

/// Most specific first.
const KINDS: [Kind; 9] = [
    kind(
        "/obj/item/language_manual/codespeak_manual/unlimited",
        Some(LanguageId::Codespeak),
        None,
        CODESPEAK_FLAVOUR,
        None,
    ),
    kind(
        "/obj/item/language_manual/codespeak_manual",
        Some(LanguageId::Codespeak),
        Some(1),
        CODESPEAK_FLAVOUR,
        None,
    ),
    kind(
        "/obj/item/language_manual/roundstart_species/unlimited",
        None,
        None,
        SPECIES_FLAVOUR,
        Some("роскошный учебник языка {}"),
    ),
    kind(
        "/obj/item/language_manual/roundstart_species/five",
        None,
        Some(5),
        SPECIES_FLAVOUR,
        Some("расширенный учебник языка {}"),
    ),
    kind(
        "/obj/item/language_manual/roundstart_species/draconic",
        Some(LanguageId::Draconic),
        Some(1),
        SPECIES_FLAVOUR,
        Some("учебник языка {}"),
    ),
    kind(
        "/obj/item/language_manual/roundstart_species",
        None,
        Some(1),
        SPECIES_FLAVOUR,
        Some("учебник языка {}"),
    ),
    kind(
        "/obj/item/language_manual/piratespeak",
        Some(LanguageId::Piratespeak),
        Some(5),
        "Тысяча чертей! Я больше не чувствую себя сухопутной крысой.",
        None,
    ),
    kind(
        DRONESPEAK,
        Some(LanguageId::Drone),
        None,
        "внезапно стрекотание дронов обретает смысл",
        None,
    ),
    kind(
        MANUAL,
        Some(LanguageId::Common),
        Some(1),
        "внезапно ваш разум наполняется кодовыми словами и ответами",
        None,
    ),
];

fn kind_of(path: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|k| is_under(path, k.root))
}

/// A manual's live vars.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct LanguageManual {
    pub language: LanguageId,
    /// `None`: `INFINITY`.
    pub charges: Option<u32>,
    pub flavour: String,
}

pub struct LanguageManualsPlugin;

impl Plugin for LanguageManualsPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_systems(Update, init_manuals);
        // `language_manual/attack_self`.
        app.add_click_handler(ClickPhase::AttackSelf, 3396, |w, c| {
            is_manual(w, c.target) && {
                attack_self(w, c.client, c.mob, c.target);
                true
            }
        });
        // `language_manual/attack`: on a living mob, before the slapper's neighbours hit it.
        app.add_click_handler(ClickPhase::HeldOnTarget, 236, |w, c| {
            c.held.is_some_and(|book| {
                is_manual(w, book) && w.get::<Stat>(c.target).is_some() && {
                    attack(w, c.client, c.mob, book, c.target);
                    true
                }
            })
        });
    }
}

fn is_manual(world: &World, item: Entity) -> bool {
    world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, MANUAL))
}

/// `Initialize`: the language, charges and, for a species manual, the picked language and its
/// name. Cost: per new item, one path check.
fn init_manuals(world: &mut World) {
    let mut added = world.query_filtered::<(Entity, &DmItem), Added<DmItem>>();
    let new: Vec<(Entity, &'static Kind)> = added
        .iter(world)
        .filter_map(|(e, item)| Some((e, kind_of(&item.path)?)))
        .collect();
    for (book, kind) in new {
        if world.get::<LanguageManual>(book).is_some() {
            continue;
        }
        let language = kind.language.unwrap_or_else(|| {
            let pool = crate::species::uncommon_roundstart_languages();
            if pool.is_empty() {
                return LanguageId::Common;
            }
            let i = world
                .get_resource_or_init::<CombatRng>()
                .range(0, pool.len() as u32 - 1);
            pool[i as usize]
        });
        let lang_name = language.def().name;
        if let Some(name) = kind.species_name
            && let Some(mut dm) = world.get_mut::<DmName>(book)
        {
            dm.name = name.replace("{}", lang_name);
            dm.desc = format!(
                "На обложке написано: «{lang_name} для ксеносов — выучите общегалактические \
                 языки за секунды»."
            );
        }
        world.entity_mut(book).insert(LanguageManual {
            language,
            charges: kind.charges,
            flavour: kind.flavour.replace("{}", lang_name),
        });
    }
}

/// The manual's vars, made now if it appeared before this plugin saw it.
fn manual(world: &mut World, book: Entity) -> Option<LanguageManual> {
    if let Some(m) = world.get::<LanguageManual>(book) {
        return Some(m.clone());
    }
    let kind = kind_of(&world.get::<DmItem>(book)?.path)?;
    let language = kind.language.unwrap_or(LanguageId::Common);
    let m = LanguageManual {
        language,
        charges: kind.charges,
        flavour: kind.flavour.replace("{}", language.def().name),
    };
    world.entity_mut(book).insert(m.clone());
    Some(m)
}

/// `has_language(language)`: understood (tg's default `UNDERSTOOD_LANGUAGE`).
fn knows(world: &World, mob: Entity, language: LanguageId) -> bool {
    world.get::<LanguageHolder>(mob).map_or_else(
        || basic_understanding(language) >= FULL,
        |h| h.has(language, UNDERSTOOD),
    )
}

/// `isdrone(M) || issilicon(M)`.
fn drone_or_silicon(world: &World, mob: Entity) -> bool {
    world.get::<crate::silicon::drones::Drone>(mob).is_some()
        || world.get::<crate::silicon::Silicon>(mob).is_some()
}

fn show(
    world: &mut World,
    at: Entity,
    others: (Span, String),
    personal: &[(Entity, Span, String)],
) {
    if let Some(pos) = crate::chem::map_pos(world, at) {
        crate::security::show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            others,
            personal,
        );
    }
}

/// `language_manual/attack_self` (and the dronespeak manual's refusal).
fn attack_self(world: &mut World, client: Entity, mob: Entity, book: Entity) {
    let Some(m) = manual(world, book) else {
        return;
    };
    if world
        .get::<DmItem>(book)
        .is_some_and(|i| is_under(&i.path, DRONESPEAK))
        && !drone_or_silicon(world, mob)
    {
        tell(
            world,
            client,
            Span::Danger,
            "Вы бьёте себя книгой по голове!",
        );
        return;
    }
    let lang_name = m.language.def().name;
    if knows(world, mob, m.language) {
        tell(
            world,
            client,
            Span::BoldWarning,
            format!("Вы начинаете листать книгу, но вы уже знаете {lang_name}."),
        );
        return;
    }
    tell(
        world,
        client,
        Span::Danger,
        format!("Вы начинаете листать книгу, и {}.", m.flavour),
    );
    edit_holder(world, mob, |h| {
        h.grant(m.language, language::ALL, LanguageSource::Atom);
        h.unblock(m.language, language::ALL, None);
        // `ADD_TRAIT(user.mind, TRAIT_TOWER_OF_BABEL, MAGIC_TRAIT)`.
        h.babel = true;
    });
    use_charge(world, client, mob, book);
}

/// `language_manual/attack(M, user)` on a living mob (and the dronespeak manual's override).
fn attack(world: &mut World, client: Entity, user: Entity, book: Entity, target: Entity) {
    if target == user {
        attack_self(world, client, user, book);
        return;
    }
    let Some(m) = manual(world, book) else {
        return;
    };
    let (user_name, target_name) = (name_of(world, user), name_of(world, target));
    let beat = |world: &mut World| {
        show(
            world,
            target,
            (
                Span::Danger,
                format!("{user_name} бьёт {target_name} книгой по голове!"),
            ),
            &[(
                target,
                Span::Danger,
                format!("{user_name} бьёт вас книгой по голове!"),
            )],
        );
    };
    let dronespeak = world
        .get::<DmItem>(book)
        .is_some_and(|i| is_under(&i.path, DRONESPEAK));
    if dronespeak && !drone_or_silicon(world, target) {
        beat(world);
        return;
    }
    if let Some(pos) = crate::chem::map_pos(world, book) {
        playsound(
            world,
            PlaySound::new(pos, PUNCH, 25.0, true).extrarange(-1.0),
        );
    }
    if world.get::<Stat>(target) == Some(&Stat::Dead) {
        show(
            world,
            target,
            (
                Span::Danger,
                format!("{user_name} шлёпает книгой безжизненное тело {target_name}."),
            ),
            &[(
                target,
                Span::Danger,
                format!("{user_name} шлёпает книгой ваше безжизненное тело."),
            )],
        );
    } else if knows(world, target, m.language) {
        beat(world);
    } else {
        show(
            world,
            target,
            (
                Span::Notice,
                format!("{user_name} обучает {target_name}, колотя книгой по голове!"),
            ),
            &[(
                target,
                Span::BoldNotice,
                format!("Когда {user_name} бьёт вас книгой, {}.", m.flavour),
            )],
        );
        edit_holder(world, target, |h| {
            h.grant(m.language, language::ALL, LanguageSource::Mind);
        });
        use_charge(world, client, user, book);
    }
}

/// `use_charge`: the last charge turns the manual into a random book manual at its turf.
fn use_charge(world: &mut World, client: Entity, user: Entity, book: Entity) {
    let left = {
        let Some(mut m) = world.get_mut::<LanguageManual>(book) else {
            return;
        };
        let Some(charges) = m.charges.as_mut() else {
            return;
        };
        *charges = charges.saturating_sub(1);
        *charges
    };
    if left > 0 {
        return;
    }
    let user_name = name_of(world, user);
    show(
        world,
        user,
        (
            Span::Notice,
            format!("Обложка книги в руках {user_name} начинает меняться! Книга выпадает из рук!"),
        ),
        &[],
    );
    tell(
        world,
        client,
        Span::Warning,
        "Обложка и содержимое книги начинают меняться! Она выскальзывает у вас из рук!",
    );
    let pos = crate::chem::map_pos(world, book);
    world.despawn(book);
    if let Some(pos) = pos {
        let manuals = crate::holodeck::MANUALS;
        let i = world
            .get_resource_or_init::<CombatRng>()
            .range(0, manuals.len() as u32 - 1);
        crate::construction::spawn_obj(world, manuals[i as usize], pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_follow_the_type_tree() {
        let k = kind_of("/obj/item/language_manual/codespeak_manual/unlimited").unwrap();
        assert_eq!((k.language, k.charges), (Some(LanguageId::Codespeak), None));
        let k = kind_of("/obj/item/language_manual/roundstart_species/five").unwrap();
        assert_eq!((k.language, k.charges), (None, Some(5)));
        let k = kind_of("/obj/item/language_manual/roundstart_species/draconic").unwrap();
        assert_eq!(k.language, Some(LanguageId::Draconic));
        let k = kind_of("/obj/item/language_manual/piratespeak").unwrap();
        assert_eq!(
            (k.language, k.charges),
            (Some(LanguageId::Piratespeak), Some(5))
        );
        assert_eq!(
            kind_of("/obj/item/language_manual").unwrap().language,
            Some(LanguageId::Common)
        );
        assert!(kind_of("/obj/item/book/manual/wiki/chemistry").is_none());
    }
}

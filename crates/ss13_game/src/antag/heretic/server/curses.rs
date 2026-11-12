//! Heretic curses (`heretic_curses.dm`, cast with the Codex Morbus: `forbidden_book.dm`
//! `interact_with_atom_secondary`).
//!
//! A heretic right-clicks a transmutation rune with the Codex Morbus: it picks a curse, then
//! (adjacent to the rune, a catalyst in the other hand) a living crew human whose fingerprints or
//! blood are on the catalyst (`recipe_snowflake_check`), confirms a self-curse, picks a species
//! for transmutation; the target must be on the caster's z-level within 96 tiles, not another
//! heretic (who is warned of), not magic-resistant (who feels a chill). The book and the
//! catalyst are scrubbed.
//!
//! - Paralysis (5 min): both legs paralysed ([`ParalysisCurse`], `crate::medical::traumas`);
//!   nothing if no leg is usable.
//! - Corrosion (3 min, [`CorrosionCurse`]): every second one of vomiting, dizziness and jitter,
//!   liver/heart/lungs (up to 90), stomach, eye, ear or brain (up to 190) damage, confusion.
//! - Transmutation (no timer, [`RaceSwap`]): the chosen species until the book that cast it is
//!   gone.
//! - Indulgence (8 min): the desire painting's curse and starving nutrition.
// shortcut: no curse outline filter; blood reagents in a catalyst do not count (only its
// forensic blood DNA); the species list is the roundstart one (tg `RACE_SWAP`); the off-z check
// is the same z only (no multi-z stacks).

use bevy::ecs::message::MessageCursor;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind};

use super::*;
use crate::medical::organs::{Slot, adjust_organ_loss, adjust_organ_loss_up_to};
use crate::species::SpeciesId;

pub const CODEX_MORBUS: &str = "/obj/item/codex_cicatrix/morbus";
/// `max_range * 1.5`.
const CURSE_RANGE: u32 = 96;
const PARALYSIS_SECS: f64 = 5.0 * 60.0;
const CORROSION_SECS: f64 = 3.0 * 60.0;
const INDULGENCE_SECS: f64 = 8.0 * 60.0;

/// The curses in `subtypesof(/datum/heretic_knowledge/curse)` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurseKind {
    Paralysis,
    Corrosion,
    Transmutation,
    Indulgence,
}

impl CurseKind {
    pub const ALL: [Self; 4] = [
        Self::Paralysis,
        Self::Corrosion,
        Self::Transmutation,
        Self::Indulgence,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Paralysis => "Проклятие паралича",
            Self::Corrosion => "Проклятие коррозии",
            Self::Transmutation => "Проклятие преображения",
            Self::Indulgence => "Curse of Indulgence",
        }
    }
}

/// `TRAIT_PARALYSIS_L_LEG` and `_R_LEG` from the curse, until then.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ParalysisCurse {
    pub until: f64,
}

/// `/datum/status_effect/corrosion_curse`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct CorrosionCurse {
    pub until: f64,
    pub next_tick: f64,
}

/// `/datum/status_effect/race_swap`: the species before, and the book whose loss undoes it.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct RaceSwap {
    pub old: SpeciesId,
    pub book: Entity,
}

/// One cast in progress, carried through its questions.
#[derive(Clone, Debug)]
struct Cast {
    mob: Entity,
    rune: Entity,
    book: Entity,
    curse: CurseKind,
    catalyst: Option<Entity>,
    target: Option<Entity>,
}

/// What a prompt of a cast asks.
#[derive(Component, Clone, Debug)]
enum CursePrompt {
    Curse {
        mob: Entity,
        rune: Entity,
        book: Entity,
    },
    Target {
        cast: Cast,
        targets: Vec<Entity>,
    },
    Myself {
        cast: Cast,
    },
    Species {
        cast: Cast,
        species: Vec<SpeciesId>,
    },
}

/// The Codex Morbus right-clicked on a transmutation rune by a heretic.
pub(super) fn morbus_on_rune(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<TransmutationRune>(target).is_none()
        || !path_of(world, held).is_some_and(|p| knowledge::is_type(&p, CODEX_MORBUS))
        || !is_heretic(world, mob)
    {
        return false;
    }
    let items = CurseKind::ALL.iter().map(|c| c.name().to_owned()).collect();
    ask(
        world,
        client,
        mob,
        "Выберите проклятие!",
        "Наложить любое проклятие",
        PromptKind::List {
            items,
            default: None,
        },
        CursePrompt::Curse {
            mob,
            rune: target,
            book: held,
        },
    );
    true
}

fn ask(
    world: &mut World,
    client: Entity,
    mob: Entity,
    title: &str,
    message: &str,
    kind: PromptKind,
    state: CursePrompt,
) {
    let prompt = ss13_tgui::prompt::ask_world(world, Ask::new(client, mob, title, message, kind));
    world.entity_mut(prompt).insert(state);
}

fn holds(world: &World, mob: Entity, item: Entity) -> bool {
    world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.hands.contains(&Some(item)))
}

/// The answers of the casts' questions, each checked again at its moment.
pub(super) fn curse_prompts(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.answer.clone()))
        .collect();
    for (prompt, client, answer) in pending {
        let Some(state) = world.get::<CursePrompt>(prompt).cloned() else {
            continue;
        };
        match (state, answer) {
            (CursePrompt::Curse { mob, rune, book }, Some(PromptAnswer::Item(i))) => {
                let Some(&curse) = CurseKind::ALL.get(i) else {
                    continue;
                };
                pick_target(
                    world,
                    client,
                    Cast {
                        mob,
                        rune,
                        book,
                        curse,
                        catalyst: None,
                        target: None,
                    },
                );
            }
            (CursePrompt::Target { mut cast, targets }, Some(PromptAnswer::Item(i))) => {
                let Some(&target) = targets.get(i) else {
                    continue;
                };
                if world.get_entity(target).is_err() {
                    tell(
                        world,
                        client,
                        Span::Warning,
                        "ритуал провален, некорректный выбор цели!",
                    );
                    continue;
                }
                cast.target = Some(target);
                if target == cast.mob {
                    ask(
                        world,
                        client,
                        cast.mob,
                        cast.curse.name(),
                        "Вы уверены что хотите проклясть самого себя?",
                        PromptKind::Alert {
                            buttons: vec!["Да".into(), "Нет".into()],
                            autofocus: true,
                        },
                        CursePrompt::Myself { cast },
                    );
                    continue;
                }
                ask_for_input(world, client, cast);
            }
            (CursePrompt::Myself { cast }, Some(PromptAnswer::Button(0))) => {
                ask_for_input(world, client, cast);
            }
            (CursePrompt::Species { cast, species }, Some(PromptAnswer::Item(i))) => {
                if let Some(&chosen) = species.get(i) {
                    finish(world, client, cast, Some(chosen));
                }
            }
            _ => {}
        }
    }
}

/// After the curse is picked: the catalyst's traces and the crew they match.
fn pick_target(world: &mut World, client: Entity, mut cast: Cast) {
    let mob = cast.mob;
    if !is_heretic(world, mob) || !holds(world, mob, cast.book) {
        return;
    }
    // `user.Adjacent(interacting_with)`.
    let adjacent = match (world.get::<TilePos>(mob), world.get::<TilePos>(cast.rune)) {
        (Some(a), Some(b)) => a.chebyshev(*b).is_some_and(|d| d <= 1),
        _ => false,
    };
    if !adjacent {
        return;
    }
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return;
    };
    let Some(catalyst) = inventory.hands[1 - inventory.active.min(1)] else {
        tell(world, client, Span::Warning, "нет катализатора!");
        return;
    };
    cast.catalyst = Some(catalyst);
    let (prints, blood): (Vec<String>, Vec<String>) = world
        .get::<crate::forensics::server::Forensics>(catalyst)
        .map(|f| {
            (
                f.fingerprints.clone(),
                f.blood_dna.iter().map(|(dna, _)| dna.clone()).collect(),
            )
        })
        .unwrap_or_default();
    let mut crew: Vec<(String, Entity)> = world
        .query_filtered::<(Entity, &crate::genetics::Dna, &Stat), (With<HumanLook>, With<PlayerJob>)>()
        .iter(world)
        .filter(|(_, _, s)| **s != Stat::Dead)
        .filter(|(_, dna, _)| {
            prints.contains(&crate::forensics::server::fingerprint_of(&dna.unique_identity))
                || blood.contains(&dna.unique_enzymes)
        })
        .map(|(e, ..)| e)
        .collect::<Vec<_>>()
        .into_iter()
        .map(|e| (name_of(world, e), e))
        .collect();
    crew.sort_by(|a, b| a.0.cmp(&b.0));
    if crew.is_empty() {
        return;
    }
    let (names, targets): (Vec<String>, Vec<Entity>) = crew.into_iter().unzip();
    let title = cast.curse.name();
    ask(
        world,
        client,
        mob,
        title,
        "Выберите цель для проклятия.",
        PromptKind::List {
            items: names,
            default: None,
        },
        CursePrompt::Target { cast, targets },
    );
}

/// `ask_for_input`: transmutation picks a species first.
fn ask_for_input(world: &mut World, client: Entity, cast: Cast) {
    if cast.curse != CurseKind::Transmutation {
        finish(world, client, cast, None);
        return;
    }
    let species: Vec<SpeciesId> = SpeciesId::roundstart().collect();
    let items = species.iter().map(|s| s.def().name.to_owned()).collect();
    ask(
        world,
        client,
        cast.mob,
        "Выберите расу, в которую превратите свою жертву",
        "Выберите расу",
        PromptKind::List {
            items,
            default: None,
        },
        CursePrompt::Species { cast, species },
    );
}

/// The rest of `on_finished_recipe`: range, the heretic and antimagic checks, the curse, the
/// scrub.
fn finish(world: &mut World, client: Entity, cast: Cast, species: Option<SpeciesId>) {
    let Some(target) = cast.target else {
        return;
    };
    if !is_heretic(world, cast.mob)
        || !holds(world, cast.mob, cast.book)
        || world.get::<HumanLook>(target).is_none()
        || world.get::<Stat>(target).is_none_or(|s| *s == Stat::Dead)
    {
        return;
    }
    let near = match (world.get::<TilePos>(target), world.get::<TilePos>(cast.mob)) {
        (Some(a), Some(b)) => a.chebyshev(*b).is_some_and(|d| d <= CURSE_RANGE),
        _ => false,
    };
    if !near {
        tell(
            world,
            client,
            Span::Warning,
            "ритуал провален, слишком далеко!",
        );
        return;
    }
    let victim = name_of(world, target);
    if is_heretic(world, target) && target != cast.mob {
        tell(
            world,
            client,
            Span::Warning,
            format!("Связь с Мансусом слишком сильна. Вы не можете проклясть {victim}."),
        );
        return;
    }
    use crate::spells::{MAGIC_RESISTANCE, MAGIC_RESISTANCE_HOLY, can_block_magic};
    if can_block_magic(world, target, MAGIC_RESISTANCE | MAGIC_RESISTANCE_HOLY) {
        crate::tools::tell_mob(
            world,
            target,
            Span::Warning,
            "На мгновение тебя охватывает жуткий озноб, но потом он проходит.",
        );
        return;
    }
    info!(
        "{} cursed {target} via heretic ritual ({})",
        cast.mob,
        cast.curse.name()
    );
    curse(world, target, cast.curse, cast.book, species);
    tell(
        world,
        client,
        Span::Mansus,
        format!("Вы накладываете {} на {victim}.", cast.curse.name()),
    );
    for item in [Some(cast.book), cast.catalyst].into_iter().flatten() {
        crate::janitor::wash(world, item, crate::janitor::CLEAN_SCRUB);
    }
}

/// Usable legs: present and not paralysed.
fn usable_legs(world: &World, mob: Entity) -> usize {
    use crate::zone::BodyZone;
    world
        .get::<crate::medical::wounds::Limbs>(mob)
        .map_or(2, |l| {
            [BodyZone::LLeg, BodyZone::RLeg]
                .into_iter()
                .filter(|&z| !l.limb(z).missing && !l.limb(z).paralyzed)
                .count()
        })
}

/// `curse(chosen_mob)` of each curse.
pub fn curse(
    world: &mut World,
    mob: Entity,
    kind: CurseKind,
    book: Entity,
    species: Option<SpeciesId>,
) {
    let t = now(world);
    let say = |world: &mut World, span, text: &str| crate::tools::tell_mob(world, mob, span, text);
    match kind {
        CurseKind::Paralysis => {
            let legs = usable_legs(world, mob);
            if legs == 0 {
                say(
                    world,
                    Span::Notice,
                    "На мгновение вы чувствуете лёгкую боль, но она вскоре проходит. Странно.",
                );
                return;
            }
            let which = if legs == 1 { "ноге" } else { "ногах" };
            say(
                world,
                Span::Danger,
                &format!("Вы внезапно теряете чувствительность в {which}!"),
            );
            world.entity_mut(mob).insert(ParalysisCurse {
                until: t + PARALYSIS_SECS,
            });
            crate::medical::traumas::refresh(world, mob);
        }
        CurseKind::Corrosion => {
            say(world, Span::Danger, "Вы чувствуете себя очень плохо...");
            say(
                world,
                Span::Danger,
                "Ваше тело начинает распадаться на части!",
            );
            world.entity_mut(mob).insert(CorrosionCurse {
                until: t + CORROSION_SECS,
                next_tick: t + 1.0,
            });
        }
        CurseKind::Transmutation => {
            let Some(species) = species else {
                return;
            };
            let current = crate::species::species_of(world, mob);
            if current == species {
                say(
                    world,
                    Span::Warning,
                    "Вы чувствуете как ваше тело превращается... в себя?",
                );
                return;
            }
            // `STATUS_EFFECT_REPLACE`: the first species is kept.
            let old = world.get::<RaceSwap>(mob).map_or(current, |r| r.old);
            crate::species::set_species(world, mob, species);
            world.entity_mut(mob).insert(RaceSwap { old, book });
            say(
                world,
                Span::Danger,
                "Вы чувствуете, как ваше тело приобретает новую форму",
            );
        }
        CurseKind::Indulgence => {
            world.entity_mut(mob).insert(PaintingCurse {
                kind: PaintingKind::Desire,
                until: t + INDULGENCE_SECS,
                next_tick: t,
            });
            if let Some(mut hunger) = world.get_mut::<crate::hunger::Hunger>(mob) {
                hunger.nutrition = crate::hunger::NUTRITION_LEVEL_STARVING;
            }
        }
    }
}

/// The curses' timers and ticks, and transmutations whose book is gone.
// Cost: per tick, the cursed mobs (a handful).
pub(super) fn tick_heretic_curses(world: &mut World) {
    let t = now(world);
    let paralysed: Vec<(Entity, f64)> = world
        .query::<(Entity, &ParalysisCurse)>()
        .iter(world)
        .map(|(e, c)| (e, c.until))
        .collect();
    for (mob, until) in paralysed {
        if t < until {
            continue;
        }
        world.entity_mut(mob).remove::<ParalysisCurse>();
        crate::medical::traumas::refresh(world, mob);
        if usable_legs(world, mob) > 1 {
            crate::tools::tell_mob(
                world,
                mob,
                Span::Notice,
                "Вы снова начинаете чувствовать ваши ноги!",
            );
        }
    }
    let corroding: Vec<(Entity, CorrosionCurse)> = world
        .query::<(Entity, &CorrosionCurse)>()
        .iter(world)
        .map(|(e, c)| (e, *c))
        .collect();
    for (mob, mut curse) in corroding {
        if t >= curse.until {
            world.entity_mut(mob).remove::<CorrosionCurse>();
            crate::tools::tell_mob(world, mob, Span::Notice, "Вам начинает становиться лучше.");
            continue;
        }
        while curse.next_tick <= t {
            curse.next_tick += 1.0;
            let roll = world
                .get_resource_mut::<CombatRng>()
                .map_or(100, |mut r| r.range(0, 100));
            corrosion_tick(world, mob, roll);
        }
        if let Ok(mut e) = world.get_entity_mut(mob) {
            e.insert(curse);
        }
    }
    let swapped: Vec<(Entity, RaceSwap)> = world
        .query::<(Entity, &RaceSwap)>()
        .iter(world)
        .map(|(e, r)| (e, *r))
        .collect();
    for (mob, swap) in swapped {
        // `codex_cicatrix/morbus/atom_destruction`: the transmutations it cast are undone.
        if world.get_entity(swap.book).is_err() {
            world.entity_mut(mob).remove::<RaceSwap>();
            crate::species::set_species(world, mob, swap.old);
        }
    }
}

/// `corrosion_curse/tick`: `rand(0, 100)` picks the effect (the gaps do nothing).
fn corrosion_tick(world: &mut World, mob: Entity, roll: u32) {
    if world.get::<HumanLook>(mob).is_none() || world.get_entity(mob).is_err() {
        return;
    }
    match roll {
        0..=10 => crate::medical::diseases::effects::vomit(world, mob, 10.0, true, false, 1),
        20..=30 => {
            use crate::status_effects::{Timed, set_timed};
            set_timed(world, mob, Timed::Dizziness, 100.0, true);
            set_timed(world, mob, Timed::Jitter, 100.0, true);
        }
        31..=40 => adjust_organ_loss_up_to(world, mob, Slot::Liver, 10.0, 90.0),
        41..=50 => adjust_organ_loss_up_to(world, mob, Slot::Heart, 10.0, 90.0),
        51..=60 => adjust_organ_loss(world, mob, Slot::Stomach, 10.0),
        61..=70 => adjust_organ_loss(world, mob, Slot::Eyes, 5.0),
        71..=80 => adjust_organ_loss(world, mob, Slot::Ears, 10.0),
        81..=90 => adjust_organ_loss_up_to(world, mob, Slot::Lungs, 10.0, 90.0),
        91..=95 => adjust_organ_loss_up_to(world, mob, Slot::Brain, 20.0, 190.0),
        96..=100 => crate::status_effects::adjust_timed(
            world,
            mob,
            crate::status_effects::Timed::Confusion,
            12.0,
            Some(24.0),
        ),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrosion_wears_off_and_ticks_once_a_second() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let ending = world
            .spawn(CorrosionCurse {
                until: 0.5,
                next_tick: 10.0,
            })
            .id();
        let running = world
            .spawn(CorrosionCurse {
                until: 60.0,
                next_tick: 0.0,
            })
            .id();
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_millis(2500));
        tick_heretic_curses(&mut world);
        assert!(world.get::<CorrosionCurse>(ending).is_none());
        // Ticks at 0, 1 and 2 s ran: the next is at 3 s.
        assert_eq!(world.get::<CorrosionCurse>(running).unwrap().next_tick, 3.0);
    }

    #[test]
    fn the_curse_list_follows_tg_order() {
        let names: Vec<&str> = CurseKind::ALL.iter().map(|c| c.name()).collect();
        assert_eq!(
            names,
            [
                "Проклятие паралича",
                "Проклятие коррозии",
                "Проклятие преображения",
                "Curse of Indulgence"
            ]
        );
    }
}

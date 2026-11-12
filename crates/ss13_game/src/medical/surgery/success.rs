//! What a finished operation does (tg `on_success`).

use super::*;

pub(super) fn refresh_patient(world: &mut World, mob: Entity) {
    let Some(health) = world.get::<Health>(mob).cloned() else {
        return;
    };
    if let Some(mut stat) = world.get_mut::<Stat>(mob) {
        let mut new = *stat;
        update_stat(&health, &mut new);
        stat.set_if_neq(new);
    }
}

pub(super) fn with_limbs(world: &mut World, mob: Entity, f: impl FnOnce(&mut Limbs)) {
    if let Some(mut limbs) = world.get::<Limbs>(mob).cloned() {
        f(&mut limbs);
        world.entity_mut(mob).insert(limbs);
    }
}

/// `organ/repair/on_success`: the organ set to `heal_to` of its max health, marked operated on.
pub(super) fn repair_organ(world: &mut World, mob: Entity, slot: Slot, heal_to: f32) {
    organs::set_organ_damage(world, mob, slot, slot.spec().max_health * heal_to);
    if let Some(organ) = organs::organ(world, mob, slot)
        && let Some(mut state) = world.get_mut::<organs::Organ>(organ)
    {
        state.operated = true;
        // `operation_organ_repair`: an EMPed organ works again.
        state.emped = false;
    }
}

/// An operation's own `can_loop`: blood filtering goes on while there is something to filter
/// (`has_filterable_chems`, the filter beeps when done).
pub(super) fn can_loop(world: &mut World, op: Op, mob: Entity) -> bool {
    if op != Op::FilterBlood {
        return true;
    }
    let any = world
        .get::<crate::chem::MobReagents>(mob)
        .is_some_and(|r| !r.blood.reagents.is_empty());
    if !any {
        let target = name_of(world, mob);
        let text = format!(
            "Фильтр крови сигнализирует о том, что в крови у {target} не обнаружено химических \
             веществ."
        );
        super::super::visible(world, mob, &text, &text, Span::Notice);
    }
    any
}

/// `filter_blood/on_success`: each reagent in the blood loses 22% (0.4 to 10 units, rounded
/// to 0.2).
// shortcut: no whitelist (the filter's own UI), no chemical scan with an analyzer in hand.
pub(super) fn filter_blood(world: &mut World, mob: Entity) {
    let Some(mut reagents) = world.get_mut::<crate::chem::MobReagents>(mob) else {
        return;
    };
    let amounts: Vec<_> = reagents
        .blood
        .reagents
        .iter()
        .map(|r| {
            (
                r.id,
                ((r.volume * 0.22 / 0.2).round() * 0.2).clamp(0.4, 10.0),
            )
        })
        .collect();
    for (id, amount) in amounts {
        reagents.blood.remove_reagent(id, amount);
    }
}

/// The new trauma of a lobotomy (75% on success, always on failure): a mild, severe or special
/// one that only magic cures.
pub(super) fn lobotomy_trauma(
    world: &mut World,
    brain: Entity,
    rng: &mut crate::combat::CombatRng,
) {
    use super::super::traumas::{Category, Resilience, gain_type};
    let category = match rng.range(1, 3) {
        1 => Category::Mild,
        2 => Category::Severe,
        _ => Category::Special,
    };
    gain_type(world, brain, category, Some(Resilience::Magic), false, rng);
}

/// A failed brain surgery or pacification: a severe trauma that only a lobotomy cures.
pub(super) fn severe_lobotomy_trauma(world: &mut World, mob: Entity) {
    use super::super::traumas::{Category, Resilience, gain_type};
    if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
        let mut rng = super::super::fork_rng(world);
        gain_type(
            world,
            brain,
            Category::Severe,
            Some(Resilience::Lobotomy),
            false,
            &mut rng,
        );
    }
}

/// `organ/repair/on_failure`: `failure_damage_percent` of the organ's max health.
pub(super) fn fail_organ(world: &mut World, mob: Entity, slot: Slot, share: f32) {
    let max = slot.spec().max_health;
    organs::adjust_organ_loss(world, mob, slot, max * share);
}

pub(super) fn damage_limb(
    world: &mut World,
    mob: Entity,
    zone: BodyZone,
    kind: DamageType,
    amount: f32,
) {
    if let Some(mut health) = world.get::<Health>(mob).cloned() {
        health.damage_zone(zone, kind, amount);
        world.entity_mut(mob).insert(health);
    }
}

pub(super) fn heal_limb(world: &mut World, mob: Entity, zone: BodyZone, brute: f32) {
    if let Some(mut health) = world.get::<Health>(mob).cloned() {
        health.heal_zone(zone, brute, 0.0);
        world.entity_mut(mob).insert(health);
    }
}

/// tend wounds' `show_stats`: the surgeon holds a health analyzer, or an operating computer
/// next to the patient has it on its table.
pub(super) fn shows_stats(world: &World, surgeon: Entity, mob: Entity) -> bool {
    let analyzer = world.get::<Inventory>(surgeon).is_some_and(|inv| {
        inv.hands.iter().flatten().any(|&i| {
            world
                .get::<DmItem>(i)
                .is_some_and(|d| is_under(&d.path, super::super::ANALYZER_ROOT))
        })
    });
    analyzer
        || table_and_computer(world, mob).is_some_and(|(_, computer)| {
            let near = |c: Entity| match (world.get::<TilePos>(c), world.get::<TilePos>(mob)) {
                (Some(a), Some(b)) => a.chebyshev(*b).is_some_and(|d| d <= 1),
                _ => false,
            };
            computer.is_some_and(near)
        })
}

/// tend wounds' `get_progress`.
pub(super) fn tend_progress(
    world: &World,
    surgeon: Entity,
    mob: Entity,
    heal: Heal,
    brute: f32,
    burn: f32,
) -> String {
    let Some(health) = world.get::<Health>(mob) else {
        return String::new();
    };
    let mut text = String::new();
    if shows_stats(world, surgeon, mob) {
        if brute > 0.0 && health.brute > 0.0 {
            text += &format!(". Оставшиеся раны: {}", health.brute.round());
        }
        if burn > 0.0 && health.burn > 0.0 {
            text += &format!(". Оставшиеся ожоги: {}", health.burn.round());
        }
        return text;
    }
    let mut steps: f32 = 0.0;
    if brute > 0.0 {
        steps = (health.brute / brute).max(0.0);
    }
    if burn > 0.0 {
        steps = steps.max(health.burn / burn);
    }
    let pick = |combo: &'static str, b: &'static str, f: &'static str| match heal {
        Heal::Combo => combo,
        Heal::Brute => b,
        Heal::Burn => f,
    };
    if steps <= 1.0 {
        String::new()
    } else if steps <= 3.0 {
        format!(
            ", заканчиваю работу над последними {}",
            pick("признаками повреждения", "ссадинами", "ожогами")
        )
    } else if steps <= 6.0 {
        format!(
            ", считаю последние {}",
            pick("очаги повреждений", "гематомы", "ожоговые пузыри")
        )
    } else if steps <= 9.0 {
        format!(
            ", продолжаю методично обрабатывать его обширные {}",
            pick("травмы", "разрывы", "ожоги")
        )
    } else if steps <= 12.0 {
        ", настраиваясь на долгую операцию впереди".into()
    } else if steps <= 15.0 {
        format!(
            ", хотя всё ещё больше напоминает {}, чем на человека",
            pick("мясное пюре", "говяжий фарш", "обугленный стейк")
        )
    } else {
        format!(
            ", хотя кажется, будто ваши усилия почти не сокращают объём {} тела",
            pick("искореженного", "размозжённого", "обугленного")
        )
    }
}

/// debride's `get_progress`.
pub(super) fn debride_progress(infection: f32) -> &'static str {
    let steps = infection / DEBRIDE_INFECTION_REMOVED;
    if infection <= 0.0 || steps <= 1.0 {
        ""
    } else if steps <= 2.0 {
        ", подготовка к удалению последних остатков инфекции"
    } else if steps <= 4.0 {
        ", неуклонно сокращая количество оставшихся очагов инфекции"
    } else if steps >= 5.0 {
        ", хотя ещё довольно много подлежит удалению"
    } else {
        ""
    }
}

/// debride's `infestation_removed`, `sanitization_added`.
pub(super) const DEBRIDE_INFECTION_REMOVED: f32 = 4.0;
pub(super) const DEBRIDE_SANITIZATION_ADDED: f32 = 0.5;

/// tg `on_success` of each operation.
pub(super) fn success(world: &mut World, actor: Actor, mob: Entity, op: Op, zone: TargetZone) {
    let body = zone.check_zone();
    let (user, target) = (name_of(world, actor.mob), name_of(world, mob));
    let tool = item_name(world, actor.item);
    let place = super::super::wounds::zone_prepositional(body);
    let mut own = "Вы преуспеваете.".to_owned();
    let mut others = format!("{user} преуспел!");
    match op {
        Op::InciseSkin => with_limbs(world, mob, |l| {
            l.limb_mut(body)
                .add_surgery(state::SKIN_CUT | state::VESSELS_UNCLAMPED);
        }),
        Op::RetractSkin => with_limbs(world, mob, |l| {
            let limb = l.limb_mut(body);
            let vessels = if limb.has_state(state::VESSELS_CLAMPED) {
                0
            } else {
                state::VESSELS_UNCLAMPED
            };
            limb.add_surgery(state::SKIN_OPEN | vessels);
            limb.remove_surgery(state::SKIN_CUT);
        }),
        Op::CloseSkin => {
            let sawed = world
                .get::<Limbs>(mob)
                .is_some_and(|l| l.limb(body).has_state(state::BONE_SAWED));
            if sawed {
                heal_limb(world, mob, body, 40.0);
            }
            with_limbs(world, mob, |l| {
                l.limb_mut(body).remove_surgery(state::UNSET_ON_CLOSE)
            });
        }
        Op::ClampBleeders => {
            let sawed = world
                .get::<Limbs>(mob)
                .is_some_and(|l| l.limb(body).has_state(state::BONE_SAWED));
            if sawed {
                heal_limb(world, mob, body, 20.0);
            }
            with_limbs(world, mob, |l| {
                let limb = l.limb_mut(body);
                limb.add_surgery(state::VESSELS_CLAMPED);
                limb.remove_surgery(state::VESSELS_UNCLAMPED);
            });
        }
        Op::UnclampBleeders => with_limbs(world, mob, |l| {
            let limb = l.limb_mut(body);
            limb.add_surgery(state::VESSELS_UNCLAMPED);
            limb.remove_surgery(state::VESSELS_CLAMPED);
        }),
        Op::SawBones => {
            with_limbs(world, mob, |l| {
                l.limb_mut(body).add_surgery(state::BONE_SAWED)
            });
            damage_limb(world, mob, body, DamageType::Brute, 50.0);
            own = format!("Вы распилили кости в {place} {target}.");
            others = format!("{user} распилил кости в {place} у {target}!");
        }
        Op::FixBones => {
            with_limbs(world, mob, |l| {
                l.limb_mut(body).remove_surgery(state::ALL_BONE)
            });
            heal_limb(world, mob, body, 40.0);
        }
        Op::DrillBones => with_limbs(world, mob, |l| {
            l.limb_mut(body).add_surgery(state::BONE_DRILLED)
        }),
        Op::InciseOrgans => {
            with_limbs(world, mob, |l| {
                l.limb_mut(body).add_surgery(state::ORGANS_CUT)
            });
            damage_limb(world, mob, body, DamageType::Brute, 10.0);
        }
        Op::MechanicalIncision => with_limbs(world, mob, |l| {
            l.limb_mut(body).add_surgery(state::SKIN_CUT);
        }),
        // Skin and vessels at once.
        Op::MechanicalOpen => with_limbs(world, mob, |l| {
            let limb = l.limb_mut(body);
            limb.add_surgery(state::SKIN_OPEN | state::VESSELS_CLAMPED);
            limb.remove_surgery(state::SKIN_CUT);
        }),
        Op::MechanicalClose => with_limbs(world, mob, |l| {
            l.limb_mut(body).remove_surgery(state::UNSET_ON_CLOSE);
        }),
        Op::PrepareElectronics => with_limbs(world, mob, |l| {
            l.limb_mut(body).add_surgery(state::ORGANS_CUT);
        }),
        Op::MechanicUnwrench => with_limbs(world, mob, |l| {
            l.limb_mut(body).add_surgery(state::BONE_SAWED);
        }),
        Op::MechanicWrench => with_limbs(world, mob, |l| {
            l.limb_mut(body).remove_surgery(state::BONE_SAWED);
        }),
        Op::TendWounds(tend, heal) => {
            let (brute, burn) = tend_amounts(world, mob, tend, heal);
            if let Some(mut health) = world.get::<Health>(mob).cloned() {
                heal_bodypart_damage(&mut health, brute, burn);
                world.entity_mut(mob).insert(health);
            }
            let covered = patient(world, mob)
                .is_some_and(|p| p.covered & crate::chem::body_part_bit(BodyZone::Chest) != 0);
            let clothes = if covered {
                " насколько это возможно, пока на нём одежда"
            } else {
                ""
            };
            let progress = tend_progress(world, actor.mob, mob, heal, brute, burn);
            own = format!("Вам удается залечить некоторые раны {target}{clothes}{progress}.");
            others = format!("{user} залечивает некоторые раны {target}{clothes}.");
        }
        Op::RepairDislocation | Op::RepairHairline | Op::RepairCompound => {
            with_limbs(world, mob, |l| {
                let limb = l.limb_mut(body);
                if let Some(i) = limb
                    .wounds
                    .iter()
                    .position(|w| w.def().series == Series::Bone)
                {
                    super::super::wounds::heal_wound(limb, i);
                }
            })
        }
        Op::ResetCompound => with_limbs(world, mob, |l| {
            if let Some(w) = l
                .limb_mut(body)
                .wounds
                .iter_mut()
                .find(|w| w.kind == WoundKind::Compound)
            {
                w.reset = true;
            }
        }),
        Op::RemoveOrgan(slot) => {
            let organ = slot.spec().ru.0;
            visible_range(
                world,
                actor.mob,
                &format!("Вы успешно извлекаете {organ} у {target}."),
                &format!("{user} успешно извлекает {organ} у {target}!"),
                Span::Notice,
                1,
            );
            pain(
                world,
                mob,
                &format!(
                    "Ваша {} пульсирует от боли, вы больше не чувствуете свой {organ}!",
                    body.ru().0
                ),
            );
            if let (Some(removed), Some(&pos)) =
                (organs::organ(world, mob, slot), world.get::<TilePos>(mob))
            {
                organs::remove(world, removed, false);
                crate::storage::take_out(world, removed, pos);
            }
            return;
        }
        Op::RemoveImplant(slot) => {
            let Some(implant) = cyberimps::implant_in(world, mob, slot) else {
                return;
            };
            let name = name_of(world, implant);
            visible_range(
                world,
                actor.mob,
                &format!("Вы успешно извлекаете {name} у {target}."),
                &format!("{user} успешно извлекает {name} у {target}!"),
                Span::Notice,
                1,
            );
            pain(
                world,
                mob,
                &format!(
                    "Ваша {} пульсирует от боли, вы больше не чувствуете свой {name}!",
                    body.ru().0
                ),
            );
            cyberimps::remove(world, implant);
            if let Some(&pos) = world.get::<TilePos>(mob) {
                crate::storage::take_out(world, implant, pos);
            }
            return;
        }
        Op::InsertOrgan(_) | Op::InsertImplant => {
            let Some(organ) = actor.item else {
                return;
            };
            let inserted = if op == Op::InsertImplant {
                let path = world
                    .get::<DmItem>(organ)
                    .map(|i| i.path.clone())
                    .unwrap_or_default();
                cyberimps::slot_for(&path, zone)
                    .is_some_and(|slot| cyberimps::insert(world, organ, mob, slot))
            } else {
                organs::insert(world, organ, mob)
            };
            if !inserted {
                return;
            }
            own = format!("Вы успешно вставляете {tool} в {target}.");
            others = format!("{user} успешно вставляет {tool} в {target}.");
            pain(
                world,
                mob,
                &format!(
                    "Ваша {} пульсирует от боли, пока орган начинает приживаться!",
                    body.ru().0
                ),
            );
        }
        Op::Amputate | Op::AmputateMechanic => {
            let part = body.ru().1;
            own = format!("Вы успешно ампутировали {part} у {target}!");
            others = format!("{user} успешно ампутировал {part} у {target}!");
            pain(world, mob, &format!("Вы больше не чувствуете свою {part}!"));
            super::super::bodyparts::drop_limb(world, mob, body);
        }
        Op::AttachLimb => {
            let Some(limb) = actor.item else {
                return;
            };
            // `OPERATION_REJECTION_DAMAGE`: 10 for a flesh limb of the same species, none for
            // a robotic one.
            let robotic = world
                .get::<DmItem>(limb)
                .is_some_and(|i| super::super::bodyparts::is_robotic_path(&i.path));
            if !robotic {
                super::super::hurt_limb(world, mob, BodyZone::Chest, DamageType::Tox, 10.0);
            }
            if !super::super::bodyparts::attach_limb(world, mob, limb) {
                return;
            }
            let part = body.ru().1;
            own = format!("Вам удалось заменить {part} у {target}.");
            others = format!("{user} успешно заменяет {part} у {target}!");
            pain(
                world,
                mob,
                &format!(
                    "Вы ощущаете синтетическое чувство, которое распространяется от вашей {}, \
                     снова чувствуя её!",
                    super::super::wounds::zone_prepositional(body)
                ),
            );
        }
        Op::Revival => {
            visible_range(
                world,
                actor.mob,
                &format!("Вам удаётся успешно ударить мозг {target} разрядом с помощью {tool}..."),
                &format!("{user} посылает мощный разряд в мозг {target} с помощью {tool}..."),
                Span::Notice,
                1,
            );
            revive_by_shock(world, mob);
            return;
        }
        Op::CoronaryBypass => {
            repair_organ(world, mob, Slot::Heart, REPAIR_HEAL_TO);
            own = format!("Вы успешно установили шунт на сердце {target}.");
            others = format!("{user} успешно установил шунт на сердце {target}.");
            pain(
                world,
                mob,
                "Боль в груди пульсирует, но сердце чувствует себя лучше, чем когда-либо!",
            );
        }
        Op::BrainSurgery => {
            if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
                organs::damage_organ(
                    world,
                    brain,
                    -organs::BRAIN_DAMAGE_DEATH * BRAIN_HEAL,
                    organs::BRAIN_DAMAGE_DEATH,
                );
            }
            own = format!("Вам удалось прооперировать мозг {target}.");
            others = format!("{user} успешно оперирует мозг {target}!");
            pain(
                world,
                mob,
                "Боль в голове отступает, думать становится немного легче!",
            );
            if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
                super::super::traumas::cure_all(
                    world,
                    brain,
                    super::super::traumas::Resilience::Surgery,
                );
            }
            crate::antag::brainwashed::remove_brainwashed(world, mob);
            let brain = organs::state(world, mob, Slot::Brain).map_or(0.0, |o| o.damage);
            if brain > organs::BRAIN_DAMAGE_DEATH * 0.1 {
                tell(
                    world,
                    actor.client,
                    Span::Notice,
                    format!("Мозг {target} выглядит так, будто его можно вылечить еще немного."),
                );
            }
        }
        Op::Bioware(kind) => {
            super::super::bioware::gain(world, mob, kind);
            let [_, _, done, did, _, hurt] = bioware_lines(kind, &target, &user);
            own = done;
            others = did;
            pain(world, mob, &hurt);
        }
        Op::FilterBlood => {
            filter_blood(world, mob);
            let tool = capitalize(&tool);
            own = format!("{tool} завершает цикл фильтрации крови у {target}.");
            others = format!("{tool} гудит, фильтруя кровь у {target}.");
        }
        Op::Lobotomy => {
            own = format!("Вы успешно провели лоботомию {target}!");
            others = format!("{user} успешно проводит лоботомию {target}!");
            pain(
                world,
                mob,
                "Ваша голова на мгновение полностью немеет, боль просто невыносима!",
            );
            crate::antag::brainwashed::remove_brainwashed(world, mob);
            if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
                super::super::traumas::cure_all(
                    world,
                    brain,
                    super::super::traumas::Resilience::Lobotomy,
                );
                let mut rng = super::super::fork_rng(world);
                if rng.chance(75.0) {
                    lobotomy_trauma(world, brain, &mut rng);
                }
            }
        }
        Op::Brainwash | Op::BrainwashSleeper => {
            if crate::antag::brainwashed::surgical_brainwash(
                world,
                actor.mob,
                mob,
                op == Op::BrainwashSleeper,
            ) {
                own = format!("Вы успешно промыли мозги {target}!");
                others = format!("{user} успешно промыл мозги {target}!");
            }
        }
        Op::Pacify => {
            own = format!("Вам удалось пацифицировать {target}.");
            others = format!("{user} успешно оперирует мозг {target}!");
            pain(
                world,
                mob,
                "Ваша голова пульсирует... мысль о насилии вспыхивает в вашем разуме, и вас \
                 едва не выворачивает наизнанку!",
            );
            if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
                super::super::traumas::gain(
                    world,
                    brain,
                    super::super::traumas::Kind::Pacifism,
                    Some(super::super::traumas::Resilience::Lobotomy),
                );
            }
        }
        Op::Lobectomy | Op::Hepatectomy | Op::Gastrectomy => {
            let (slot, heal_to, organ, pain_text) = match op {
                Op::Lobectomy => (
                    Slot::Lungs,
                    REPAIR_HEAL_TO,
                    "наиболее повреждённую долю легкого",
                    "",
                ),
                Op::Hepatectomy => (
                    Slot::Liver,
                    HEPATECTOMY_HEAL_TO,
                    "повреждённую часть печени",
                    "Боль немного отступает!",
                ),
                _ => (
                    Slot::Stomach,
                    GASTRECTOMY_HEAL_TO,
                    "повреждённую часть желудка",
                    "Боль в животе немного утихает!",
                ),
            };
            repair_organ(world, mob, slot, heal_to);
            own = format!("Вы успешно удаляете {organ} {target}.");
            others = format!("{user} успешно удаляет {organ} {target}.");
            pain(world, mob, pain_text);
        }
        Op::EarSurgery | Op::EyeSurgery => {
            let ears = op == Op::EarSurgery;
            let slot = if ears { Slot::Ears } else { Slot::Eyes };
            repair_organ(world, mob, slot, 0.0);
            if !ears {
                crate::vision::cure_temp_blindness(world, mob);
                // "This will fix itself slowly."
                crate::vision::set_eye_blur_if_lower(world, mob, 70.0);
            }
            if ears
                && let Some(organ) = organs::organ(world, mob, slot)
                && let Some(mut state) = world.get_mut::<organs::Organ>(organ)
            {
                state.deafness = EAR_SURGERY_DEAFNESS;
            }
            let (what, text) = if ears {
                (
                    "уши",
                    "Голова идет кругом, но, кажется, слух начинает возвращаться!",
                )
            } else {
                (
                    "глаза",
                    "Зрение затуманено, но кажется, теперь вы видите немного лучше!",
                )
            };
            own = format!("Вы успешно вылечили {what} {target}.");
            others = format!("{user} успешно вылечил {what} {target}.");
            pain(world, mob, text);
        }
        Op::Debride => {
            damage_limb(world, mob, body, DamageType::Brute, 3.0);
            let mut left = 0.0;
            with_limbs(world, mob, |l| {
                if let Some(w) = l
                    .limb_mut(body)
                    .wounds
                    .iter_mut()
                    .find(|w| w.def().series == Series::Burn)
                {
                    w.infection -= DEBRIDE_INFECTION_REMOVED;
                    w.sanitization += DEBRIDE_SANITIZATION_ADDED;
                    left = w.infection;
                }
            });
            let progress = debride_progress(left);
            own = format!(
                "Вы успешно удалили часть зараженной плоти с {place}{progress} у {target}."
            );
            others = format!(
                "{user} успешно удалил часть зараженной плоти с {place} у {target} с помощью {tool}!"
            );
        }
        Op::RepairPuncture | Op::SealVeins => {
            let seal = op == Op::SealVeins;
            if !seal {
                damage_limb(world, mob, body, DamageType::Brute, 3.0);
            }
            let mut closed = false;
            with_limbs(world, mob, |l| {
                let limb = l.limb_mut(body);
                let Some(i) = limb
                    .wounds
                    .iter()
                    .position(|w| w.def().series == Series::Pierce)
                else {
                    return;
                };
                let by = if seal { -0.5 } else { -0.25 };
                if super::super::wounds::adjust_pierce(limb, i, by) {
                    limb.wounds[i].mend_state = !seal;
                } else {
                    closed = true;
                }
            });
            let (verb_own, verb_others) = if seal {
                ("прижгли", "прижигает")
            } else {
                ("соединили", "соединяет")
            };
            if closed {
                own = format!(
                    "Вы успешно {verb_own} последние разорванные кровеносные сосуды в {place} у \
                     {target}."
                );
                others = format!(
                    "{user} успешно {verb_others} последние разорванные кровеносные сосуды в \
                     {place} у {target} с помощью {tool}!"
                );
            } else {
                own = format!(
                    "Вы успешно {verb_own} часть кровеносных сосудов в {place} у {target}."
                );
                others = format!(
                    "{user} успешно {verb_others} часть кровеносных сосудов в {place} у {target} \
                     с помощью {tool}!"
                );
            }
        }
    }
    visible_range(world, actor.mob, &own, &others, Span::Notice, 1);
}

/// revival's `on_success` after the shock: the ghost back, 50 oxygen healed, `revive()`; brain
/// damage either way (15 up to 180 alive, 50 up to 199 not).
pub(super) fn revive_by_shock(world: &mut World, mob: Entity) {
    crate::ghost::return_to_body(world, mob);
    if let Some(mut health) = world.get::<Health>(mob).cloned() {
        health.adjust(DamageType::Oxy, -50.0);
        world.entity_mut(mob).insert(health);
    }
    let target = name_of(world, mob);
    if super::super::defib::revive(world, mob) {
        let text = format!("...{target} приходит в себя - живой и в сознании!");
        super::super::visible(world, mob, &text, &text, Span::Notice);
        organs::adjust_organ_loss_up_to(world, mob, Slot::Brain, 15.0, 180.0);
    } else {
        let text = format!("...{target} дёргается в конвульсиях и замирает.");
        super::super::visible(world, mob, &text, &text, Span::Warning);
        organs::adjust_organ_loss_up_to(world, mob, Slot::Brain, 50.0, 199.0);
    }
}

/// tend wounds' healing: the flat amount plus a share of the patient's damage (×0.2 dead,
/// ×0.55 through clothes on the chest).
pub(super) fn tend_amounts(world: &World, mob: Entity, tend: Tend, heal: Heal) -> (f32, f32) {
    let (amount, share) = tend.healing();
    let Some(health) = world.get::<Health>(mob) else {
        return (0.0, 0.0);
    };
    let dead = if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        0.2
    } else {
        1.0
    };
    let covered = patient(world, mob)
        .is_some_and(|p| p.covered & crate::chem::body_part_bit(BodyZone::Chest) != 0);
    let access = if covered { 0.55 } else { 1.0 };
    let round = |x: f32| (x * 10.0).round() / 10.0;
    let brute = amount + round(health.brute * share * dead * access);
    let burn = amount + round(health.burn * share * dead * access);
    match heal {
        Heal::Brute => (brute, 0.0),
        Heal::Burn => (0.0, burn),
        Heal::Combo => (brute, burn),
    }
}

/// tg `heal_bodypart_damage`: over the damaged parts (`heal_overall_damage`).
pub(super) fn heal_bodypart_damage(health: &mut Health, brute: f32, burn: f32) {
    if brute > 0.0 {
        health.adjust(DamageType::Brute, -brute);
    }
    if burn > 0.0 {
        health.adjust(DamageType::Burn, -burn);
    }
}

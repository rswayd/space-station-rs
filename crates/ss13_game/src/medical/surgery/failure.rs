//! What a failed operation does (tg `on_failure`).

use super::*;

/// tg `on_failure` (the operations' own replace the generic message where tg does not call the
/// parent).
pub(super) fn failure(
    world: &mut World,
    actor: Actor,
    mob: Entity,
    op: Op,
    zone: TargetZone,
    speed: f32,
) {
    let body = zone.check_zone();
    let place = super::super::wounds::zone_prepositional(body);
    let (user, target) = (name_of(world, actor.mob), name_of(world, mob));
    let tool = item_name(world, actor.item);
    let say = |world: &mut World, own: String, others: String| {
        visible_range(world, actor.mob, &own, &others, Span::Warning, 1);
    };
    match op {
        Op::TendWounds(tend, heal) => {
            say(world, "Вы ошибаетесь!".into(), format!("{user} ошибается!"));
            // tg: 80% of the flat amount and half the share, as damage.
            let (amount, share) = tend.healing();
            let (brute, burn) = match world.get::<Health>(mob) {
                Some(h) => {
                    let round = |x: f32| (x * 10.0).round() / 10.0;
                    let brute = amount * 0.8 + round(h.brute * share * 0.5);
                    let burn = amount * 0.8 + round(h.burn * share * 0.5);
                    match heal {
                        Heal::Brute => (brute, 0.0),
                        Heal::Burn => (0.0, burn),
                        Heal::Combo => (brute, burn),
                    }
                }
                None => (0.0, 0.0),
            };
            if let Some(mut health) = world.get::<Health>(mob).cloned() {
                if brute > 0.0 {
                    health.adjust(DamageType::Brute, brute);
                }
                if burn > 0.0 {
                    health.adjust(DamageType::Burn, burn);
                }
                world.entity_mut(mob).insert(health);
            }
            return;
        }
        Op::Revival => {
            say(
                world,
                format!(
                    "Вы бьёте мозг {target} разрядом с помощью {tool}, но он никак не реагирует."
                ),
                format!(
                    "{user} бьёт мозг {target} разрядом с помощью {tool}, но он никак не реагирует."
                ),
            );
            return;
        }
        Op::CoronaryBypass => {
            fail_organ(world, mob, Slot::Heart, REPAIR_FAILURE_DAMAGE);
            say(
                world,
                "Вы ошибаетесь при креплении шунта, и он отрывается, повреждая часть сердца!"
                    .into(),
                format!(
                    "{user} совершает ошибку, из-за чего поток крови обильно хлещет из груди {target}!"
                ),
            );
            pain(
                world,
                mob,
                "Грудь горит; кажется, вы сходите с ума от боли!",
            );
            return;
        }
        Op::BrainSurgery => {
            fail_organ(world, mob, Slot::Brain, BRAIN_FAILURE_DAMAGE);
            say(
                world,
                "Вы совершаете ошибку, нанося еще больше повреждений!".into(),
                format!("{user} совершает ошибку, вызывая повреждение мозга!"),
            );
            pain(
                world,
                mob,
                "Ваша голова пульсирует от ужасной боли; даже думать больно!",
            );
            severe_lobotomy_trauma(world, mob);
            return;
        }
        // `cortex_imprint/on_failure`: 60 brain damage and a severe trauma (lobotomy-level).
        Op::Bioware(Bioware::CortexImprint) if organs::organ(world, mob, Slot::Brain).is_some() => {
            say(
                world,
                "Вы ошибаетесь, повреждая мозг!".into(),
                format!("{user} ошибается, нанеся повреждения мозгу!"),
            );
            pain(
                world,
                mob,
                "Голова раскалывается от ужасной боли; от одной мысли об этом уже начинает болеть \
                 голова!",
            );
            organs::adjust_organ_loss(world, mob, Slot::Brain, 60.0);
            severe_lobotomy_trauma(world, mob);
            return;
        }
        Op::FilterBlood => {
            say(
                world,
                format!("Вы ошибаетесь, оставляя синяк на груди у {target}!"),
                format!("{user} ошибается, оставляя синяк на груди у {target}!"),
            );
            damage_limb(world, mob, BodyZone::Chest, DamageType::Brute, 5.0);
            return;
        }
        Op::Brainwash | Op::BrainwashSleeper => {
            world
                .entity_mut(actor.mob)
                .remove::<crate::antag::brainwashed::BrainwashDirective>();
            say(
                world,
                "Вы облажались, повредив мозговую ткань!".into(),
                format!("{user} облажался, что привело к повреждению мозга!"),
            );
            pain(world, mob, "Ваша голова пульсирует от ужасной боли!");
            if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
                organs::damage_organ(world, brain, 40.0, organs::BRAIN_DAMAGE_DEATH);
            }
            return;
        }
        Op::Lobotomy => {
            say(
                world,
                "Вы удалили не ту часть, нанеся еще больше повреждений!".into(),
                format!("{user} безуспешно пытается провести лоботомию {target}!"),
            );
            pain(
                world,
                mob,
                "Боль в вашей голове, кажется, только усиливается!",
            );
            if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
                organs::damage_organ(world, brain, 80.0, organs::BRAIN_DAMAGE_DEATH);
                let mut rng = super::super::fork_rng(world);
                lobotomy_trauma(world, brain, &mut rng);
            }
            return;
        }
        Op::Pacify => {
            say(
                world,
                format!("Вы совершаете ошибку, перепутав все связи в мозгу {target}..."),
                format!("{user} совершает ошибку, вызывая повреждение мозга!"),
            );
            pain(
                world,
                mob,
                "Ваша голова пульсирует, и кажется, становится только хуже!",
            );
            severe_lobotomy_trauma(world, mob);
            return;
        }
        Op::Lobectomy => {
            fail_organ(world, mob, Slot::Lungs, LOBECTOMY_FAILURE_DAMAGE);
            if let Some(mut resp) = world.get_mut::<crate::breath::Respiration>(mob) {
                resp.losebreath += 4.0;
            }
            say(
                world,
                format!(
                    "Вы совершаете ошибку, не сумев удалить повреждённую долю легкого {target}!"
                ),
                format!("{user} совершает ошибку!"),
            );
            pain(
                world,
                mob,
                "Вы чувствуете резкий укол в груди; у вас перехватывает дыхание, и каждый вдох \
                 причиняет боль!",
            );
            return;
        }
        Op::Hepatectomy | Op::Gastrectomy => {
            let (slot, what) = if op == Op::Hepatectomy {
                (Slot::Liver, "печени")
            } else {
                (Slot::Stomach, "желудка")
            };
            fail_organ(world, mob, slot, CUT_FAILURE_DAMAGE);
            say(
                world,
                format!("Вы отрезали не ту часть {what} {target}!"),
                format!("{user} отрезал не ту часть {what} {target}!"),
            );
            pain(world, mob, "Боль в животе усиливается!");
            return;
        }
        // tg damages the ears or eyes themselves here, against its own lines (and the older
        // surgeries) that hit the brain; the brain it is.
        Op::EarSurgery | Op::EyeSurgery => {
            match organs::organ(world, mob, Slot::Brain) {
                Some(brain) => {
                    say(
                        world,
                        format!("Вы случайно тычите инструментом прямо в мозг {target}!"),
                        format!("{user} случайно тычет инструментом прямо в мозг {target}!"),
                    );
                    pain(
                        world,
                        mob,
                        "Вы чувствуете пронзающую боль, проходящую сквозь голову прямо в мозг!",
                    );
                    organs::damage_organ(
                        world,
                        brain,
                        SENSE_FAILURE_BRAIN_DAMAGE,
                        organs::BRAIN_DAMAGE_DEATH,
                    );
                }
                None => say(
                    world,
                    format!(
                        "Вы случайно тыкаете туда, где должен быть мозг {target}! Хорошо, что \
                         его там нет."
                    ),
                    format!(
                        "{user} случайно тыкает инструментом прямо в мозг {target}! Или сделал \
                         бы это, будь у пациента мозг."
                    ),
                ),
            }
            return;
        }
        Op::Debride => {
            let hurt = super::super::fork_rng(world).range(4, 8) as f32;
            damage_limb(world, mob, body, DamageType::Brute, hurt);
            say(
                world,
                format!("Вы отрезаете немного здоровой плоти с {place} у {target}."),
                format!(
                    "{user} отрезает немного здоровой плоти с {place} у {target} с помощью {tool}!"
                ),
            );
            return;
        }
        Op::RepairPuncture => {
            // shortcut: the damage rolls no new wound (tg `wound_bonus = 10`, edged).
            let hurt = super::super::fork_rng(world).range(4, 8) as f32;
            damage_limb(world, mob, body, DamageType::Brute, hurt);
            say(
                world,
                format!("Вы случайно разрываете кровеносные сосуды в {place} у {target}."),
                format!(
                    "{user} разрывает кровеносные сосуды в {place} у {target} с помощью {tool}!"
                ),
            );
            return;
        }
        _ => {}
    }
    let screwed = if speed >= 5.0 {
        " В данных условиях это практически невозможно..."
    } else if speed >= 4.0 {
        " В таких условиях это трудно сделать правильно..."
    } else if (2.5..3.0).contains(&speed) {
        " А ведь у вас почти получилось..."
    } else {
        ""
    };
    say(
        world,
        format!("Вы облажались!{screwed}"),
        format!("{user} облажался!"),
    );
    if op == Op::RepairDislocation {
        damage_limb(world, mob, body, DamageType::Brute, 25.0);
        pain(world, mob, "Боль в конечности усиливается!");
    }
    let _ = CANT_WOUND;
}

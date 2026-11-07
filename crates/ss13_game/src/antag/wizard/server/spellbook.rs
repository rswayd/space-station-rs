//! The spellbook: buying and refunding entries, and the rituals.

use super::*;

/// The book is on `mob` (in its hands, worn or in a bag it carries).
pub(super) fn holds(world: &World, mob: Entity, item: Entity) -> bool {
    let mut e = item;
    for _ in 0..4 {
        if world.get::<CarriedBy>(e).is_some_and(|c| c.0 == mob) {
            return true;
        }
        match world.get::<StoredIn>(e) {
            Some(s) => e = s.0,
            None => return false,
        }
    }
    false
}

/// `can_buy` + `buy_spell` + `purchase_entry` for `mob` with `book`. True when bought.
pub fn purchase(world: &mut World, mob: Entity, book: Entity, id: &str) -> bool {
    let Some((i, def)) = entry(id) else {
        return false;
    };
    let Some(state) = world.get::<Spellbook>(book).cloned() else {
        return false;
    };
    let times = state.times.get(i).copied().unwrap_or(0);
    if !state.offers(i) || state.uses < def.cost || def.limit().is_some_and(|l| times >= l) {
        return false;
    }
    let bought = match def.kind {
        EntryKind::Spell(spell) => {
            let before = world
                .get::<Spells>(mob)
                .and_then(|s| s.get(spell))
                .map(|k| k.name());
            match crate::spells::learn(world, mob, spell) {
                Learned::New => {
                    chat_to(
                        world,
                        mob,
                        Span::Notice,
                        format!("Вы выучили - {}.", spell.def().name),
                    );
                    true
                }
                Learned::Leveled(_) => {
                    let after = world
                        .get::<Spells>(mob)
                        .and_then(|s| s.get(spell))
                        .map(|k| k.name())
                        .unwrap_or_default();
                    chat_to(
                        world,
                        mob,
                        Span::Notice,
                        format!(
                            "Вы улучшили {} в {after}.",
                            before.unwrap_or_else(|| spell.def().name.to_owned())
                        ),
                    );
                    true
                }
                Learned::MaxLevel => {
                    chat_to(
                        world,
                        mob,
                        Span::Warning,
                        "Это заклинание не может быть болeе улучшено!",
                    );
                    false
                }
            }
        }
        EntryKind::Item(path) => summon_item(world, mob, path),
        EntryKind::Ritual(Ritual::SummonGuns) => {
            // `summon_guns(user, 10)`.
            crate::antag::survivalist::summon_guns(world, Some(mob), 10.0);
            if let Some(pos) = crate::explosions::turf_of(world, mob) {
                crate::sound::playsound(
                    world,
                    crate::sound::PlaySound::new(
                        pos,
                        "sound/effects/magic/castsummon.ogg",
                        50.0,
                        true,
                    ),
                );
            }
            true
        }
        EntryKind::Ritual(Ritual::SummonEvents) => {
            let intensified = crate::events::controller::summon_events(world);
            chat_to(
                world,
                mob,
                Span::Warning,
                if intensified {
                    "You have intensified summon events, causing them to occur more often!"
                } else {
                    "You have cast summon events!"
                },
            );
            if let Some(pos) = crate::explosions::turf_of(world, mob) {
                crate::sound::playsound(
                    world,
                    crate::sound::PlaySound::new(
                        pos,
                        "sound/effects/magic/castsummon.ogg",
                        50.0,
                        true,
                    ),
                );
            }
            true
        }
    };
    if !bought {
        return false;
    }
    if let Some(mut book) = world.get_mut::<Spellbook>(book) {
        if let Some(t) = book.times.get_mut(i) {
            *t += 1;
        }
        book.uses -= def.cost;
    }
    if let Some(mut wizard) = world.get_mut::<Wizard>(mob) {
        match wizard.purchases.iter_mut().find(|(n, _)| *n == def.name) {
            Some((_, n)) => *n += 1,
            None => wizard.purchases.push((def.name, 1)),
        }
    }
    info!("{mob} bought {} from the spellbook", def.name);
    true
}

/// `/datum/spellbook_entry/item/buy_spell` + `try_equip_item`.
pub(super) fn summon_item(world: &mut World, mob: Entity, path: &str) -> bool {
    let Some(item) = crate::construction::spawn_detached(world, path) else {
        warn!("no prototype for {path}");
        return false;
    };
    let name = name_of(world, item);
    if path == WAND_BELT {
        fill_wand_belt(world, item);
        let free = world
            .get::<Inventory>(mob)
            .is_some_and(|i| i.belt.is_none());
        if free {
            world.entity_mut(item).insert((Networked, CarriedBy(mob)));
            if let Some(mut inv) = world.get_mut::<Inventory>(mob) {
                inv.belt = Some(item);
            }
            crate::inventory::refresh_look(world, mob);
        } else {
            let pos = world.get::<TilePos>(mob).copied().unwrap_or_default();
            world.entity_mut(item).insert((Networked, pos));
            crate::construction::tile_changed(world, pos);
        }
        let place = if free {
            "на ваш пояс"
        } else {
            "у ваших ног"
        };
        chat_to(
            world,
            mob,
            Span::Notice,
            format!("{} призывается {place}.", capitalize(&name)),
        );
        return true;
    }
    crate::spells::magic_guns::adopt_full(world, item);
    if path == CONTRACT {
        world.entity_mut(item).insert(ApprenticeContract::default());
    }
    let in_hands = give(world, mob, item);
    let place = if in_hands {
        "в ваши руки"
    } else {
        "у ваших ног"
    };
    chat_to(
        world,
        mob,
        Span::Notice,
        format!("Приобретенный {name} призывается {place}."),
    );
    true
}

/// `wands/full/PopulateContents`: the wands, "in the best possible condition".
pub(super) fn fill_wand_belt(world: &mut World, belt: Entity) {
    let inside: Vec<Entity> = world
        .get::<Stored>(belt)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    let wands = if inside.is_empty() {
        BELT_WANDS
            .iter()
            .filter_map(|&p| into_pack(world, belt, p))
            .collect()
    } else {
        inside
    };
    for wand in wands {
        crate::spells::magic_guns::adopt_full(world, wand);
    }
}

/// `can_refund` + `refund_spell`: only spells, only in the den. True when refunded.
pub fn refund(world: &mut World, mob: Entity, book: Entity, id: &str) -> bool {
    let Some((i, def)) = entry(id) else {
        return false;
    };
    let EntryKind::Spell(spell) = def.kind else {
        return false;
    };
    let allowed = world
        .get::<Spellbook>(book)
        .is_some_and(|b| b.refunds_allowed);
    if !allowed || !crate::spells::knows(world, mob, spell) {
        return false;
    }
    let in_den = world
        .get::<TilePos>(mob)
        .is_some_and(|&p| crate::spells::in_wizard_den(world, p));
    if !in_den {
        chat_to(
            world,
            mob,
            Span::Warning,
            "Вы можете вернуть очки за заклинания только в логове волшебника!",
        );
        return false;
    }
    let level = crate::spells::forget(world, mob, spell);
    let amount = u32::from(level) * def.cost;
    if amount == 0 {
        return false;
    }
    if let Some(mut book) = world.get_mut::<Spellbook>(book) {
        if let Some(t) = book.times.get_mut(i) {
            *t = 0;
        }
        book.uses += amount;
    }
    if let Some(mut wizard) = world.get_mut::<Wizard>(mob) {
        wizard.purchases.retain(|(n, _)| *n != def.name);
    }
    info!("{mob} refunded {} for {amount} points", def.name);
    true
}

pub(super) fn book_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<SpellbookUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, SpellbookAct)> = acts
        .read(world.resource::<Messages<UiActed<SpellbookUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, book, act) in pending {
        // At the moment of effect: the owner's own body, holding the book.
        if world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world.get::<Spellbook>(book).and_then(|b| b.owner) != Some(actor)
            || !holds(world, actor, book)
            || world.get::<Stat>(actor) != Some(&Stat::Conscious)
        {
            continue;
        }
        if world.get::<HumanLook>(actor).is_none() {
            tell(
                world,
                client,
                Span::Warning,
                "Книга, похоже, не слушает низшие формы жизни.",
            );
            continue;
        }
        match act {
            SpellbookAct::Purchase(id) => purchase(world, actor, book, &id),
            SpellbookAct::Refund(id) => refund(world, actor, book, &id),
        };
    }
}

/// `attack_self` of the wizard's items: the spellbook binds or opens, the scroll and the
/// contract ask. `false` for other items.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if let Some(book) = world.get::<Spellbook>(item).cloned() {
        let name = name_of(world, item);
        match book.owner {
            None => {
                tell(
                    world,
                    client,
                    Span::Notice,
                    format!("Вы привязываете {name} к себе."),
                );
                let owner_name = name_of(world, mob);
                if let Some(mut b) = world.get_mut::<Spellbook>(item) {
                    b.owner = Some(mob);
                    b.owner_name = owner_name;
                }
            }
            Some(owner) if owner != mob => {
                let apprentice = world.get::<Wizard>(mob).is_some_and(|w| w.master.is_some());
                let text = if apprentice {
                    "Если бы вас поймают за подглядыванием в книгу заклинаний вашего учителя, то, \
                     скорее всего, вас отчислять из Академии волшебников. Лучше не стоит."
                        .to_owned()
                } else {
                    format!("Вы не признаетесь владельцем {name}, и не собирается открываться!")
                };
                tell(world, client, Span::Warning, text);
            }
            Some(_) => {
                world.write_message(OpenUi::new::<SpellbookUi>(
                    client,
                    mob,
                    item,
                    "Книга заклинаний",
                ));
            }
        }
        return true;
    }
    if let Some(scroll) = world.get::<TeleportScroll>(item).copied() {
        if scroll.uses > 0 {
            crate::spells::open_teleport_prompt(world, client, mob, Some(item));
        }
        return true;
    }
    if let Some(contract) = world.get::<ApprenticeContract>(item).copied() {
        if !contract.used {
            let items: Vec<String> = School::ALL.iter().map(|s| s.key().to_owned()).collect();
            let ask = Ask::new(
                client,
                mob,
                "Контракт",
                "Выберите школу ученика.",
                PromptKind::List {
                    items,
                    default: None,
                },
            );
            let prompt = ask_world(world, ask);
            world
                .entity_mut(prompt)
                .insert(WizardPrompt::Contract { contract: item });
        }
        return true;
    }
    false
}

pub(super) fn prompt_answers(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in pending {
        let Some(kind) = world.get::<WizardPrompt>(prompt).copied() else {
            continue;
        };
        if world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client) {
            continue;
        }
        match kind {
            WizardPrompt::Rename => {
                let default = world
                    .get::<RenameDefault>(prompt)
                    .map(|d| d.0.clone())
                    .unwrap_or_default();
                // `sanitize_name(reject_bad_text(...))`, else the random name.
                let entered = match answer {
                    Some(PromptAnswer::Text(text)) => {
                        ss13_chat::server::sanitize_say(&text).unwrap_or_default()
                    }
                    _ => String::new(),
                };
                let name: String = entered.trim().chars().take(MAX_NAME_LEN as usize).collect();
                let name = if name.is_empty() { default } else { name };
                if world.get::<Wizard>(actor).is_some() && !name.is_empty() {
                    world.entity_mut(actor).insert(Name::new(name));
                }
            }
            WizardPrompt::Contract { contract } => {
                let Some(PromptAnswer::Item(i)) = answer else {
                    continue;
                };
                let Some(&school) = School::ALL.get(i) else {
                    continue;
                };
                call_apprentice(world, actor, contract, school);
            }
        }
    }
}

/// `poll_for_student` + `spawn_antag` of the contract: the first ghost who wants to be a
/// wizard (midround) becomes the apprentice beside its teacher.
pub(super) fn call_apprentice(
    world: &mut World,
    teacher: Entity,
    contract: Entity,
    school: School,
) {
    let usable = world
        .get::<ApprenticeContract>(contract)
        .is_some_and(|c| !c.used)
        && holds(world, teacher, contract)
        && world.get::<Stat>(teacher) == Some(&Stat::Conscious);
    if !usable {
        return;
    }
    let mut ghosts: Vec<(Entity, Entity)> = world
        .query_filtered::<(Entity, &PlayerMob), With<crate::ghost::Ghost>>()
        .iter(world)
        .map(|(g, p)| (g, p.client))
        .collect();
    ghosts.sort();
    let chosen = ghosts.into_iter().map(|(_, c)| c).find(|&c| {
        world
            .get::<crate::antag::dynamic::AntagPreferences>(c)
            .is_some_and(|p| p.wants(ROLE_WIZARD_MIDROUND))
    });
    let Some(client) = chosen else {
        chat_to(
            world,
            teacher,
            Span::Warning,
            "Unable to reach your apprentice! You can either attack the spellbook with the \
             contract to refund your points, or wait and try again later.",
        );
        return;
    };
    let Some(&pos) = world.get::<TilePos>(teacher) else {
        return;
    };
    if let Some(mut c) = world.get_mut::<ApprenticeContract>(contract) {
        c.used = true;
    }
    let name = random_name(world);
    let mob = crate::server::spawn_mob(world, client, pos, name);
    add_apprentice(world, mob, teacher, school);
}

/// `/datum/action/antag_info/Trigger` of a wizard.
pub(super) fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if let Some(wizard) = world.get::<Wizard>(source) {
            let title = wizard.antag_name();
            world.write_message(OpenUi::new::<WizardInfoWindow>(
                client, actor, source, title,
            ));
        }
    }
}

use ss13_chat::capitalize;

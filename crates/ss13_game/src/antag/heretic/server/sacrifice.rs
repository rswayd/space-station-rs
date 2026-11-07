//! Sacrifices and ascension.

use super::*;

/// `obtain_targets`: a head of staff, a security officer, someone of the heretic's department,
/// then random crew up to five; living humans, not the heretic, not sacrificed before.
pub(super) fn obtain_targets(world: &mut World, mob: Entity, silent: bool) -> bool {
    let blacklist = world.resource::<SacrificeBlacklist>().0.clone();
    let mut q = world
        .query_filtered::<(Entity, &Stat), (With<PlayerMob>, With<PlayerJob>, With<HumanLook>)>();
    let mut valid: Vec<Entity> = q
        .iter(world)
        .filter(|(e, s)| *e != mob && **s != Stat::Dead && !blacklist.contains(e))
        .map(|(e, _)| e)
        .collect();
    valid.sort();
    if valid.is_empty() {
        if !silent {
            chat_to(world, mob, Span::Mansus, "Не удалось найти ни одной цели!");
        }
        return false;
    }
    let departments = world
        .get_resource::<Jobs>()
        .map(|j| j.data.departments.clone())
        .unwrap_or_default();
    let job_of = |world: &World, e: Entity| world.get::<PlayerJob>(e).map(|j| j.0 as u16);
    let in_department = |world: &World, e: Entity, name: &str| {
        job_of(world, e).is_some_and(|j| {
            departments
                .iter()
                .any(|d| d.name == name && d.jobs.contains(&j))
        })
    };
    let own: Vec<String> = job_of(world, mob)
        .map(|j| {
            departments
                .iter()
                .filter(|d| d.jobs.contains(&j))
                .map(|d| d.name.clone())
                .collect()
        })
        .unwrap_or_default();
    let mut finals: Vec<Entity> = Vec::new();
    let take = |world: &mut World, valid: &mut Vec<Entity>, f: &dyn Fn(&World, Entity) -> bool| {
        let mut shuffled = valid.clone();
        for i in (1..shuffled.len()).rev() {
            let j = rand(world, i + 1);
            shuffled.swap(i, j);
        }
        if let Some(&e) = shuffled.iter().find(|&&e| f(world, e)) {
            valid.retain(|&v| v != e);
            Some(e)
        } else {
            None
        }
    };
    if let Some(e) = take(world, &mut valid, &|w, e| is_head(w, e)) {
        finals.push(e);
    }
    if let Some(e) = take(world, &mut valid, &|w, e| in_department(w, e, "Security")) {
        finals.push(e);
    }
    if let Some(e) = take(world, &mut valid, &|w, e| {
        own.iter().any(|d| in_department(w, e, d))
    }) {
        finals.push(e);
    }
    let mut sanity = 0;
    while finals.len() < NUM_TARGETS && valid.len() > NUM_TARGETS && sanity < 25 {
        let i = rand(world, valid.len());
        finals.push(valid.remove(i));
        sanity += 1;
    }
    if !silent {
        chat_to(
            world,
            mob,
            Span::Danger,
            "Ваши цели были определены. Ваше живое сердце позволит вам отслеживать их положение. \
             Идите и принесите их в жертву!",
        );
    }
    for target in finals {
        let name = name_of(world, target);
        let title = job_title(world, target).unwrap_or_default();
        if let Some(mut h) = world.get_mut::<Heretic>(mob) {
            if !h.sac_targets.contains(&target) {
                h.sac_targets.push(target);
            }
            h.all_sac_targets.push(name.clone());
        }
        if !silent {
            chat_to(world, mob, Span::Danger, format!("{name}, {title}."));
        }
    }
    true
}

/// `sacrifice_process`: the points (2, a head 3, a cultist 1), the target off every heretic's
/// list, then the victim's journey.
pub(super) fn sacrifice_process(world: &mut World, mob: Entity, victim: Entity) {
    world.resource_mut::<SacrificeBlacklist>().0.push(victim);
    let mut q = world.query::<&mut Heretic>();
    for mut h in q.iter_mut(world) {
        h.sac_targets.retain(|&t| t != victim);
    }
    let head = is_head(world, victim);
    let cultist = crate::antag::cult::server::is_cultist(world, victim);
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        h.total_sacrifices += 1;
        if head {
            h.high_value_sacrifices += 1;
        }
    }
    recharge_grasp(world, mob);
    let mut feedback = "Твои покровители принимают твоё подношение".to_owned();
    if head {
        adjust_points(world, mob, 3);
        feedback.push_str(" с великодушием");
    }
    if cultist {
        // shortcut: the cultist is dusted (gibbed here) without the reward item.
        adjust_points(world, mob, 1);
        chat_to(
            world,
            mob,
            Span::Hypnophrase,
            "A servant of the Sanguine Apostate!",
        );
        chat_to(world, mob, Span::Mansus, "Your patrons are rapturous!");
        crate::health::gib(world, victim);
        info!("heretic {mob} sacrificed the cultist {victim}");
        return;
    }
    adjust_points(world, mob, 2);
    chat_to(world, mob, Span::Hypnophrase, format!("{feedback}."));
    info!("heretic {mob} sacrificed {victim} (head: {head})");
    if !begin_sacrifice(world, mob, victim) {
        disembowel(world, victim);
    }
}

/// `begin_sacrifice`: cult shackles, back from the dead at 50 health, paralysed.
// shortcut: no shadow realm (tg's lazy template map and its hands minigame, the cursed organs
// and the heretic curse): the victim lies paralysed on the rune for SACRIFICE_SLEEP_DURATION,
// then wakes up uncuffed at a random safe station tile as a survivor, with tg's after-effects
// (confusion, knockdown, stamina damage), and the heretic hears where.
pub(super) fn begin_sacrifice(world: &mut World, mob: Entity, victim: Entity) -> bool {
    let name = name_of(world, victim);
    visible(
        world,
        victim,
        "Тёмные усики начинают затягивать вас в воздух!",
        &format!(
            "{} начинает сильно дрожать, когда темные усики начинают затягивать их в воздух!",
            capitalize(&name)
        ),
        Span::Danger,
    );
    if world.get::<crate::restraints::Handcuffed>(victim).is_none()
        && let Some(&pos) = world.get::<TilePos>(victim)
        && let Some(cuffs) = crate::construction::spawn_obj(world, CULT_CUFFS, pos)
    {
        crate::restraints::apply_cuffs(world, cuffs, victim);
    }
    if !heal_and_revive(world, victim, 50.0) {
        return false;
    }
    chat_to(
        world,
        victim,
        Span::Hypnophrase,
        "Ваш разум словно разрывается на части, когда вы погружаетесь в неглубокую дрему...",
    );
    let secs = SACRIFICE_SLEEP_SECS as f32 * 1.2;
    crate::status_effects::apply(world, victim, Incap::Paralyze, secs);
    crate::status_effects::apply(world, victim, Incap::Immobilize, secs);
    let t = now(world);
    world.entity_mut(victim).insert(Sacrificed {
        heretic: mob,
        wake_at: t + SACRIFICE_SLEEP_SECS,
    });
    true
}

/// `heal_and_revive(heal_to)`: a dead body is healed up to `heal_to` health and revived.
pub(super) fn heal_and_revive(world: &mut World, mob: Entity, heal_to: f32) -> bool {
    let Some(mut health) = world.get::<Health>(mob).cloned() else {
        return false;
    };
    health.oxy = (health.oxy - 100.0).max(0.0);
    if world.get::<Stat>(mob) != Some(&Stat::Dead) {
        world.entity_mut(mob).insert(health);
        return true;
    }
    let mut guard = 0;
    while health.health() < heal_to && guard < 50 {
        for kind in [
            DamageType::Brute,
            DamageType::Burn,
            DamageType::Tox,
            DamageType::Oxy,
        ] {
            health.adjust(kind, -5.0);
        }
        guard += 1;
    }
    world.entity_mut(mob).insert(health);
    let revived = crate::medical::defib::revive(world, mob);
    if revived {
        let name = name_of(world, mob);
        visible(
            world,
            mob,
            "Ваше сердце начинает биться с нечестивой силой!",
            &format!(
                "Сердце {name} начинает биться с нечестивой силой, когда оно возвращается из \
                 мертвых!"
            ),
            Span::Danger,
        );
        let ghost = {
            let mut q = world.query::<(Entity, &Ghost)>();
            q.iter(world)
                .find(|(_, g)| g.body == Some(mob))
                .map(|(e, _)| e)
        };
        if ghost.is_some() {
            crate::ghost::return_to_body(world, mob);
        }
    }
    revived
}

/// `return_target` + `after_return_live_target`.
pub(super) fn return_sacrifice(world: &mut World, victim: Entity, chain: Sacrificed) {
    world.entity_mut(victim).remove::<Sacrificed>();
    if let Some(&pos) = world.get::<TilePos>(victim) {
        crate::restraints::uncuff(world, victim, pos);
    }
    let dest = crate::events::electrical_storm::random_station_turf(world);
    if let Some(dest) = dest {
        world.entity_mut(victim).insert(dest);
    }
    let dead = world.get::<Stat>(victim) == Some(&Stat::Dead);
    if dead {
        chat_to(
            world,
            victim,
            Span::Hypnophrase,
            "Вы не смогли противостоять ужасам Мансуса! Ваше разрушенное тело было возвращено на \
             станцию.",
        );
    } else {
        chat_to(
            world,
            victim,
            Span::Hypnophrase,
            "Борьба окончена, но высокой ценой. Вы вернулись на станцию невредимыми.",
        );
        chat_to(
            world,
            victim,
            Span::Hypnophrase,
            "Вы не помните ничего из того, что предшествовало этому событию - все, о чем вы можете \
             думать, это те ужасные руки...",
        );
        if let Some(mut e) = effects_mut(world, victim) {
            e.confusion = e.confusion.max(60.0);
        }
        crate::status_effects::knockdown(world, victim, 8.0, 0.0);
        crate::status_effects::adjust_stamina(world, victim, 120.0);
    }
    let name = name_of(world, victim);
    let area = world
        .get::<TilePos>(victim)
        .map(|&p| area_name(world, p))
        .unwrap_or_default();
    if world.get_entity(chain.heretic).is_ok() {
        let state = if dead {
            "мертвой. "
        } else {
            "живой, но с разбитым сознанием. "
        };
        chat_to(
            world,
            chain.heretic,
            Span::Notice,
            format!(
                "Ваша жертва, {name}, была возвращена на станцию - {state}Вы слышите шепот... \
                 {area}"
            ),
        );
    }
}

/// `disembowel_target`: 250 brute and death.
pub(super) fn disembowel(world: &mut World, victim: Entity) {
    hurt(world, victim, DamageType::Brute, 250.0);
    world.entity_mut(victim).insert(Stat::Dead);
    world.entity_mut(victim).remove::<Sacrificed>();
    let name = name_of(world, victim);
    visible(
        world,
        victim,
        "Ваши органы жестоко вытаскиваются из груди теневыми руками!",
        &format!("Органы {name} вытаскиваются из их груди теневыми руами!"),
        Span::Danger,
    );
}

// ---------------------------------------------------------------------------------------------
// Ascension

/// `ultimate/on_finished_recipe`: ascended, CentCom's announcement, the shuttle; the Ash Lord's
/// spells.
// shortcut: no physiology (brute/burn halved), no environment immunity traits, no achievement.
pub(super) fn ascend(world: &mut World, mob: Entity, def: &'static KnowledgeDef) {
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        h.ascended = true;
        if def.path == ASH_FINAL {
            h.ash_ascended_spells = true;
        }
    }
    let name = world
        .get::<Name>(mob)
        .map(|n| n.to_string())
        .unwrap_or_else(|| name_of(world, mob));
    let text = ascension_text(def.announcement, &name, || spooky(world));
    let title = spooky(world);
    crate::events::priority_announce(world, &title, &text);
    info!("heretic {mob} ascended ({})", def.path);
    if let Some(shuttle) = world.get_resource::<crate::shuttle::EmergencyShuttle>()
        && matches!(
            shuttle.state.mode,
            crate::shuttle::ShuttleMode::Idle | crate::shuttle::ShuttleMode::Recall
        )
    {
        crate::shuttle::call_shuttle(
            world,
            "Сканерами дальнего действия зафиксирован критический разрыв в реальности, вызванный \
             потусторонними силами. Вероятность массовых потерь экипажа и уничтожения станции \
             превышает допустимые рассчётные вероятности. Запущен приоритетный эвакуационный \
             шаттл.",
            None,
            1.0,
        );
    }
    if let Some(mut shuttle) = world.get_resource_mut::<crate::shuttle::EmergencyShuttle>() {
        shuttle.state.no_recall = true;
    }
    crate::antag::heretic::paths::on_ascend(world, mob, def);
    refresh(world, mob);
}

/// `generate_heretic_text()`: 25 random symbols.
pub(super) fn spooky(world: &mut World) -> String {
    const SYMBOLS: [&str; 10] = ["!", "$", "^", "@", "&", "#", "*", "(", ")", "?"];
    (0..25).map(|_| pick_str(world, &SYMBOLS)).collect()
}

/// The announcement with `%NAME%` and each `%SPOOKY%` replaced.
pub fn ascension_text(template: &str, name: &str, mut spooky: impl FnMut() -> String) -> String {
    let named = template.replace("%NAME%", name);
    let mut out = String::new();
    let mut rest = named.as_str();
    while let Some(i) = rest.find("%SPOOKY%") {
        out.push_str(&rest[..i]);
        out.push_str(&spooky());
        rest = &rest[i + "%SPOOKY%".len()..];
    }
    out.push_str(rest);
    out
}

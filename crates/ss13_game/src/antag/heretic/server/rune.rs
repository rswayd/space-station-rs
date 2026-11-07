//! The transmutation rune: drawing it and its rituals.

use super::*;

/// `try_draw_rune`: open, non-space floor around, no rune within 3 tiles, not already drawing.
pub(super) fn try_draw_rune(
    world: &mut World,
    client: Entity,
    mob: Entity,
    pen: Entity,
    turf: Entity,
    pos: TilePos,
) {
    for dx in -1..=1 {
        for dy in -1..=1 {
            let p = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
            let ok = crate::construction::turf_at(world, p)
                .and_then(|t| path_of(world, t))
                .is_some_and(|t| {
                    knowledge::is_type(&t, "/turf/open")
                        && ![
                            "/turf/open/space",
                            "/turf/open/openspace",
                            "/turf/open/lava",
                            "/turf/open/chasm",
                        ]
                        .iter()
                        .any(|b| knowledge::is_type(&t, b))
                });
            if !ok {
                tell(world, client, Span::Warning, "плохое место для руны!");
                return;
            }
        }
    }
    let near = {
        let index = world.resource::<ss13_world::SpatialIndex>();
        index
            .in_rect(pos.z, (pos.x - 3, pos.y - 3), (pos.x + 3, pos.y + 3))
            .any(|e| world.get::<TransmutationRune>(e).is_some())
    };
    if near {
        tell(
            world,
            client,
            Span::Warning,
            "слишком близко к другой руне!",
        );
        return;
    }
    if world.get::<Heretic>(mob).is_some_and(|h| h.drawing_rune) {
        tell(world, client, Span::Warning, "уже рисуете руну!");
        return;
    }
    tell(world, client, Span::Notice, "создание руны...");
    let actor = Actor {
        client,
        mob,
        item: Some(pen),
    };
    if do_after(
        world,
        actor,
        turf,
        RUNE_DRAW_DS,
        0,
        Job::Heretic(HereticJob::DrawRune),
    ) && let Some(mut h) = world.get_mut::<Heretic>(mob)
    {
        h.drawing_rune = true;
    }
}

/// [`Job::Heretic`] completions.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: HereticJob) {
    match job {
        HereticJob::DrawRune => {
            if let Some(mut h) = world.get_mut::<Heretic>(mob) {
                h.drawing_rune = false;
            }
            if !is_heretic(world, mob) || !grasp_in_offhand(world, mob) {
                return;
            }
            let route = world.get::<Heretic>(mob).and_then(|h| h.route);
            spawn_rune(world, action.target_pos, route);
            tell(world, action.client, Span::Notice, "руна создана");
        }
        HereticJob::Drain { points } => finish_drain(world, mob, action, points),
        HereticJob::FireBlast => cast_fire_blast(world, mob),
    }
}

/// An interrupted job: tg resets `drawing_rune` and `being_drained`.
pub fn interrupted(world: &mut World, mob: Entity, action: &DoAfter, job: HereticJob) {
    match job {
        HereticJob::DrawRune => {
            if let Some(mut h) = world.get_mut::<Heretic>(mob) {
                h.drawing_rune = false;
            }
        }
        HereticJob::Drain { .. } => {
            if let Some(mut i) = world.get_mut::<Influence>(action.target) {
                i.being_drained = false;
            }
        }
        HereticJob::FireBlast => return,
    }
    tell(world, action.client, Span::Warning, "прервано!");
}

/// `new /obj/effect/heretic_rune/big(turf, path colour)`.
pub fn spawn_rune(world: &mut World, pos: TilePos, route: Option<&str>) -> Option<Entity> {
    let rune = crate::construction::spawn_obj(world, RUNE, pos)?;
    let [r, g, b] = path_color(route);
    let mut e = world.entity_mut(rune);
    e.insert(TransmutationRune::default());
    if let Some(mut appearance) = e.get_mut::<ss13_shared::Appearance>() {
        appearance.color = Some([r, g, b, 255]);
        // `pixel_x = -30`, `pixel_y = 18`, `pixel_z = -48`.
        appearance.pixel_offset = [-30, -30];
    }
    Some(rune)
}

/// An admin's or a test's spawn: a rune gets its state like a drawn one.
pub fn spawn_obj_or_rune(world: &mut World, path: &str, pos: TilePos) -> Option<Entity> {
    if knowledge::is_type(path, "/obj/effect/heretic_rune") {
        return spawn_rune(world, pos, None);
    }
    if path == INFLUENCE {
        return spawn_influence(world, pos);
    }
    crate::antag::cult::server::spawn_obj_or_rune(world, path, pos)
}

/// `can_be_invoked` of a researched knowledge.
pub(super) fn can_be_invoked(heretic: &Heretic, r: &Researched) -> bool {
    let def = r.def();
    match def.kind {
        _ if def.path == FEAST_OF_OWLS => !heretic.feast_of_owls,
        _ if def.path == LIVING_HEART => !heretic.living_heart,
        Kind::Spell => !def.required.is_empty() && r.charges < def.max_charges,
        Kind::Ultimate => !heretic.ascended && heretic.can_ascend().is_ok(),
        _ => !def.required.is_empty(),
    }
}

/// `get_rituals`, by priority.
pub(super) fn rituals(heretic: &Heretic) -> Vec<&'static KnowledgeDef> {
    let mut out: Vec<&'static KnowledgeDef> = heretic
        .researched
        .iter()
        .filter(|r| can_be_invoked(heretic, r))
        .map(Researched::def)
        .collect();
    out.sort_by_key(|k| std::cmp::Reverse(k.priority));
    out
}

// shortcut: the type's last path segment, not its `initial(name)` (no prototype name lookup).
pub(super) fn type_name(_: &World, path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).replace('_', " ")
}

pub(super) fn requirements_text(world: &World, def: &KnowledgeDef) -> String {
    let parts: Vec<String> = def
        .required
        .iter()
        .map(|(types, n)| {
            types
                .iter()
                .map(|t| knowledge::describe_required(t, *n, |p| type_name(world, p)))
                .collect::<Vec<_>>()
                .join(" or ")
        })
        .collect();
    english_list(&parts)
}

pub(super) fn english_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} и {last}", rest.join(", ")),
    }
}

/// `try_rituals`: the list of rituals (tg's radial).
pub(super) fn try_rituals(world: &mut World, client: Entity, mob: Entity, rune: Entity) {
    let Some(heretic) = world.get::<Heretic>(mob) else {
        return;
    };
    let list = rituals(heretic);
    if list.is_empty() {
        tell(world, client, Span::Warning, "нет доступных ритуалов!");
        return;
    }
    let items: Vec<String> = list.iter().map(|k| k.name.to_owned()).collect();
    let message = list
        .iter()
        .filter(|k| !k.required.is_empty())
        .map(|k| {
            let needs = if k.transmute_text.is_empty() {
                format!("Требуется: {}", requirements_text(world, k))
            } else {
                k.transmute_text.to_owned()
            };
            format!("{}: {needs}", k.name)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let paths = list.iter().map(|k| k.path).collect();
    if let Some(mut r) = world.get_mut::<TransmutationRune>(rune) {
        r.in_use = true;
    }
    open_prompt(
        world,
        client,
        mob,
        HereticPrompt {
            kind: PromptKind::Ritual { rune },
            title: "Ритуалы".into(),
            message,
            items,
            paths,
            targets: Vec::new(),
        },
    );
}

/// The atoms a ritual may use: what stands on and around the rune, visible, not the user
/// (`do_ritual`'s `range(1, src)`).
pub(super) fn atoms_near(world: &World, rune: Entity, user: Entity) -> Vec<Candidate<Entity>> {
    let Some(&pos) = world.get::<TilePos>(rune) else {
        return Vec::new();
    };
    let index = world.resource::<ss13_world::SpatialIndex>();
    let mut out = Vec::new();
    for dx in -1..=1 {
        for dy in -1..=1 {
            for &e in index.on_tile(TilePos::new(pos.x + dx, pos.y + dy, pos.z)) {
                if e == user || e == rune || world.get::<Invisibility>(e).is_some_and(|i| i.0 > 0) {
                    continue;
                }
                if crate::construction::turf_at(world, TilePos::new(pos.x + dx, pos.y + dy, pos.z))
                    == Some(e)
                {
                    continue;
                }
                let path = if is_living(world, e) && world.get::<HumanLook>(e).is_some() {
                    Some("/mob/living/carbon/human".to_owned())
                } else {
                    path_of(world, e).or_else(|| {
                        world
                            .get::<ss13_mapload::DmCleanable>(e)
                            .map(|c| c.path.clone())
                    })
                };
                let Some(path) = path else {
                    continue;
                };
                let stack = world.get::<crate::tools::Stack>(e).map(|s| s.amount);
                out.push(Candidate {
                    thing: e,
                    path,
                    stack,
                });
            }
        }
    }
    out
}

/// `do_ritual`: the snowflake check, the matching, the ritual, the cleanup.
pub(super) fn do_ritual(
    world: &mut World,
    client: Entity,
    mob: Entity,
    rune: Entity,
    path: &'static str,
) {
    let Some(def) = knowledge::def(path) else {
        return;
    };
    let mut atoms = atoms_near(world, rune, mob);
    let loc = world.get::<TilePos>(rune).copied();
    let Some(loc) = loc else {
        return;
    };
    if !snowflake_check(world, client, mob, def, &mut atoms) {
        return;
    }
    let matched = match knowledge::match_recipe(def.required, &atoms) {
        Ok(m) => m,
        Err(missing) => {
            tell(
                world,
                client,
                Span::Warning,
                "ритуал провален, нехватает компонентов!",
            );
            let parts: Vec<String> = missing
                .iter()
                .map(|&(i, n)| {
                    def.required[i]
                        .0
                        .iter()
                        .map(|t| knowledge::describe_required(t, n, |p| type_name(world, p)))
                        .collect::<Vec<_>>()
                        .join(" or ")
                })
                .collect();
            tell(
                world,
                client,
                Span::Mansus,
                format!(
                    "Вам не хватает {} для завершения ритуала \"{}\".",
                    english_list(&parts),
                    def.name
                ),
            );
            return;
        }
    };
    let done = finish_recipe(world, client, mob, rune, def, &matched.atoms, loc);
    if !done {
        return;
    }
    // `cleanup_atoms`: everything but the living goes.
    for &e in &matched.atoms {
        if def.kind == Kind::Ultimate && is_living(world, e) {
            crate::health::gib(world, e);
            continue;
        }
        if !is_living(world, e) {
            despawn(world, e);
        }
    }
    for &(stack, amount) in &matched.stacks {
        crate::tools::use_stack(world, stack, amount);
    }
    tell(world, client, Span::Notice, "ритуал завершен");
    update_ui(world, mob);
}

/// `recipe_snowflake_check`: may drop atoms or refuse.
pub(super) fn snowflake_check(
    world: &mut World,
    client: Entity,
    mob: Entity,
    def: &'static KnowledgeDef,
    atoms: &mut Vec<Candidate<Entity>>,
) -> bool {
    let Some(heretic) = world.get::<Heretic>(mob).cloned() else {
        return false;
    };
    match def.kind {
        _ if def.path == HUNT_AND_SACRIFICE => {
            if !heretic.living_heart {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "ритуал провален, нет живого сердца!",
                );
                return false;
            }
            if heretic.sac_targets.is_empty() {
                atoms.push(Candidate {
                    thing: mob,
                    path: "/mob/living/carbon/human".into(),
                    stack: None,
                });
                return true;
            }
            atoms.retain(|c| {
                if c.path != "/mob/living/carbon/human" {
                    return true;
                }
                let stat = world.get::<Stat>(c.thing).copied().unwrap_or_default();
                stat >= Stat::SoftCrit
                    && (heretic.sac_targets.contains(&c.thing)
                        || crate::antag::cult::server::is_cultist(world, c.thing))
            });
            if atoms.iter().any(|c| c.path == "/mob/living/carbon/human") {
                return true;
            }
            tell(
                world,
                client,
                Span::Warning,
                "ритуал провален, нет жертвы для приношения!",
            );
            false
        }
        _ if def.path == LIVING_HEART => {
            if heretic.living_heart {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "ритуал провален, у вас уже есть Живое сердце!",
                );
                return false;
            }
            true
        }
        Kind::Ultimate => {
            if heretic.ascended || heretic.can_ascend().is_err() {
                return false;
            }
            atoms.retain(|c| {
                c.path != "/mob/living/carbon/human"
                    || valid_ascension_sacrifice(world, def, c.thing)
            });
            true
        }
        Kind::Starting | Kind::Limited => limit_check(world, client, mob, def),
        _ => true,
    }
}

/// `ultimate/is_valid_sacrifice`: dead; the Ash Lord wants them burning or husked.
pub(super) fn valid_ascension_sacrifice(world: &World, def: &KnowledgeDef, body: Entity) -> bool {
    if world.get::<Stat>(body) != Some(&Stat::Dead) {
        return false;
    }
    if def.path == ASH_FINAL {
        return crate::fire::on_fire(world, body)
            || world.get::<HumanLook>(body).is_some_and(|l| l.husk);
    }
    true
}

/// `limited_amount/recipe_snowflake_check` with BandaStation's blade renouncing.
pub(super) fn limit_check(
    world: &mut World,
    client: Entity,
    mob: Entity,
    def: &'static KnowledgeDef,
) -> bool {
    let Some(h) = world.get::<Heretic>(mob) else {
        return false;
    };
    let blades = def
        .results
        .iter()
        .any(|r| knowledge::is_type(r, SICKLY_BLADE));
    if h.unlimited_blades && blades {
        return true;
    }
    let created: Vec<Entity> = h
        .get(def.path)
        .map(|r| r.created.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|&e| world.get_entity(e).is_ok())
        .collect();
    let exploded = h.exploded_blades;
    if let Some(mut h) = world.get_mut::<Heretic>(mob)
        && let Some(r) = h.get_mut(def.path)
    {
        r.created = created.clone();
    }
    if (created.len() as u32) < def.limit.max(1) {
        return true;
    }
    if def.kind != Kind::Starting {
        tell(
            world,
            client,
            Span::Warning,
            "ритуал провален - превышен лимит!",
        );
        return false;
    }
    // BandaStation: the farthest blade (10+ tiles) may be renounced once.
    let Some(&at) = world.get::<TilePos>(mob) else {
        return false;
    };
    let farthest = created
        .iter()
        .filter_map(|&e| {
            let pos = crate::modpc::pda::turf_of(world, e)?;
            (pos.z == at.z).then(|| (e, (pos.x - at.x).abs().max((pos.y - at.y).abs())))
        })
        .max_by_key(|(_, d)| *d);
    let Some((item, distance)) = farthest.filter(|(_, d)| *d >= MIN_DISTANCE_TO_EXPLODE) else {
        tell(world, client, Span::Warning, "ritual failed, at limit!");
        return false;
    };
    let _ = distance;
    if exploded >= 1 {
        tell(
            world,
            client,
            Span::Warning,
            "ritual failed, at limit, we can't renounce our blades anymore!",
        );
        return false;
    }
    tell(
        world,
        client,
        Span::Warning,
        "we renounce the furthest blade!",
    );
    if let Some(pos) = crate::modpc::pda::turf_of(world, item) {
        crate::explosions::explosion(
            world,
            pos,
            crate::explosions::Explosion::new(0, 0, 1).cause("heretic blade"),
        );
    }
    if let Some(holder) = world.get::<CarriedBy>(item).map(|c| c.0) {
        despawn_held(world, holder, item);
    } else {
        despawn(world, item);
    }
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        h.exploded_blades += 1;
        if let Some(r) = h.get_mut(def.path) {
            r.created.retain(|&e| e != item);
        }
    }
    true
}

/// `on_finished_recipe`: true when the atoms are consumed.
pub(super) fn finish_recipe(
    world: &mut World,
    client: Entity,
    mob: Entity,
    rune: Entity,
    def: &'static KnowledgeDef,
    atoms: &[Entity],
    loc: TilePos,
) -> bool {
    match def.kind {
        _ if def.path == HUNT_AND_SACRIFICE => {
            let Some(&victim) = atoms.first() else {
                return false;
            };
            let no_targets = world
                .get::<Heretic>(mob)
                .is_some_and(|h| h.sac_targets.is_empty());
            if no_targets && !crate::antag::cult::server::is_cultist(world, victim) {
                if obtain_targets(world, mob, false) {
                    return true;
                }
                tell(
                    world,
                    client,
                    Span::Warning,
                    "ритуал провален, цель не обнаружена!",
                );
                return false;
            }
            sacrifice_process(world, mob, victim);
            true
        }
        _ if def.path == LIVING_HEART => {
            if let Some(mut h) = world.get_mut::<Heretic>(mob) {
                h.living_heart = true;
            }
            chat_to(
                world,
                mob,
                Span::Warning,
                "Вы чувствуете, как ваше сердце начинает пульсировать все быстрее и быстрее по \
                 мере того, как оно пробуждается!",
            );
            refresh(world, mob);
            true
        }
        _ if def.path == FEAST_OF_OWLS => {
            open_prompt(
                world,
                client,
                mob,
                HereticPrompt {
                    kind: PromptKind::ConfirmFeast { rune },
                    title: "Пир для Сов".into(),
                    message: "Вы действительно хотите отказаться от своего вознесения? Это \
                              действие невозможно отменить."
                        .into(),
                    items: vec!["Да, я уверен".into(), "Нет".into()],
                    paths: Vec::new(),
                    targets: Vec::new(),
                },
            );
            false
        }
        Kind::Spell => {
            // `spell/on_finished_recipe`: `recharge_amount` (1.0) of the max charges.
            if let Some(mut h) = world.get_mut::<Heretic>(mob)
                && let Some(r) = h.get_mut(def.path)
            {
                r.charges = r.def().max_charges;
            }
            refresh(world, mob);
            true
        }
        Kind::Ultimate => {
            ascend(world, mob, def);
            true
        }
        Kind::Summon => {
            // shortcut: heretic monsters need basic mobs and ghost polls (not ported).
            tell(
                world,
                client,
                Span::Warning,
                "ритуал провален, нет призраков!",
            );
            false
        }
        _ => {
            if def.results.is_empty() {
                return false;
            }
            let mut made = Vec::new();
            for result in def.results {
                match crate::construction::spawn_obj(world, result, loc) {
                    Some(e) => made.push(e),
                    None => warn!("heretic: no prototype for {result}"),
                }
            }
            if made.is_empty() {
                return false;
            }
            if matches!(def.kind, Kind::Starting | Kind::Limited)
                && let Some(mut h) = world.get_mut::<Heretic>(mob)
                && let Some(r) = h.get_mut(def.path)
            {
                r.created.extend(made);
            }
            true
        }
    }
}

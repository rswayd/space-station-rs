//! The list prompts and the research window.

use super::*;

pub(super) fn open_prompt(world: &mut World, client: Entity, mob: Entity, prompt: HereticPrompt) {
    let title = prompt.title.clone();
    world.entity_mut(mob).insert(prompt);
    world.write_message(OpenUi::new::<HereticListWindow>(client, mob, mob, &title));
}

impl ServerUi for HereticListWindow {
    type Source = &'static HereticPrompt;
    type UserParam = ();
    type Act = Option<String>;

    fn static_data(_: &HereticPrompt) {}

    fn data(prompt: &HereticPrompt, _: Entity, _: &()) -> ListInputData {
        ListInputData {
            title: prompt.title.clone(),
            message: prompt.message.clone(),
            init_value: prompt.items.first().cloned().unwrap_or_default(),
            items: prompt.items.clone(),
            timeout: None,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<Option<String>, UiReject> {
        match action {
            "submit" => params.text("entry").map(Some),
            "cancel" => Ok(None),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

pub(super) fn close_window(world: &mut World, window: Entity) {
    if let Some(mut window) = world.get_mut::<UiWindow>(window) {
        window.cap_status(ss13_shared::ui::UiStatus::Close);
    }
}

pub(super) fn release_rune(world: &mut World, kind: &PromptKind) {
    if let PromptKind::Ritual { rune } | PromptKind::ConfirmFeast { rune } = kind
        && let Some(mut r) = world.get_mut::<TransmutationRune>(*rune)
    {
        r.in_use = false;
    }
}

pub(super) fn list_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<HereticListWindow>>>,
) {
    let pending: Vec<(Entity, Entity, Entity, Entity, Option<String>)> = acts
        .read(world.resource::<Messages<UiActed<HereticListWindow>>>())
        .map(|a| (a.window, a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (window, client, actor, source, pick) in pending {
        close_window(world, window);
        let Some(prompt) = world.entity_mut(source).take::<HereticPrompt>() else {
            continue;
        };
        release_rune(world, &prompt.kind);
        let Some(pick) = pick else {
            continue;
        };
        if actor != source || !is_heretic(world, actor) || !can_act(world, actor) {
            continue;
        }
        let Some(i) = prompt.items.iter().position(|it| *it == pick) else {
            continue;
        };
        match prompt.kind {
            PromptKind::Ritual { rune } => {
                let Some(&path) = prompt.paths.get(i) else {
                    continue;
                };
                // The rune must still be there and in reach (`require_near`).
                let near = match (world.get::<TilePos>(rune), world.get::<TilePos>(actor)) {
                    (Some(r), Some(a)) => {
                        r.z == a.z && (r.x - a.x).abs() <= 1 && (r.y - a.y).abs() <= 1
                    }
                    _ => false,
                };
                let invokable = world
                    .get::<Heretic>(actor)
                    .is_some_and(|h| h.get(path).is_some_and(|r| can_be_invoked(h, r)));
                if near && invokable {
                    do_ritual(world, client, actor, rune, path);
                }
            }
            PromptKind::Track => {
                if let Some(&target) = prompt.targets.get(i) {
                    track_pick(world, client, actor, target);
                }
            }
            PromptKind::ConfirmFinal { path, category } => {
                if pick == "Да" {
                    try_research(world, actor, path, category, true);
                }
            }
            PromptKind::ConfirmFeast { rune } => {
                let near = match (world.get::<TilePos>(rune), world.get::<TilePos>(actor)) {
                    (Some(r), Some(a)) => {
                        r.z == a.z && (r.x - a.x).abs() <= 2 && (r.y - a.y).abs() <= 2
                    }
                    _ => false,
                };
                if pick == "Да, я уверен" && near {
                    feast_of_owls(world, actor);
                }
            }
        }
    }
}

/// `feast_of_owls/on_finished_recipe` after the alert: five points, no ascension.
// shortcut: the five points come at once (tg: one a second with screams and jitter).
pub(super) fn feast_of_owls(world: &mut World, mob: Entity) {
    let Some(h) = world.get::<Heretic>(mob) else {
        return;
    };
    if h.feast_of_owls {
        return;
    }
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        h.feast_of_owls = true;
    }
    crate::status_effects::apply(world, mob, Incap::Paralyze, 5.0);
    for _ in 0..5 {
        adjust_points(world, mob, 1);
    }
    chat_to(
        world,
        mob,
        Span::Danger,
        "Вы чувствуете, как что-то невидимое разрушает саму вашу сущность!",
    );
    chat_to(
        world,
        mob,
        Span::Danger,
        "Ваши амбиции разрушены, но на их месте пробуждается нечто могущественное...",
    );
    let drain = pick_str(world, INFLUENCE_DRAIN_MESSAGE);
    chat_to(world, mob, Span::Hypnophrase, drain);
    update_ui(world, mob);
}

// ---------------------------------------------------------------------------------------------
// The research window

/// `act('research', { path, category })`.
#[derive(Clone, Debug, PartialEq)]
pub struct ResearchAct {
    pub path: String,
    pub category: String,
}

impl ServerUi for AntagInfoHereticUi {
    type Source = (&'static Heretic, &'static HereticPaths);
    type UserParam = Option<Res<'static, IconIndex>>;
    type Act = ResearchAct;

    fn static_data((_, paths): (&Heretic, &HereticPaths)) -> HereticStatic {
        paths.0.clone()
    }

    fn data(
        (heretic, _): (&Heretic, &HereticPaths),
        _: Entity,
        icons: &Option<Res<IconIndex>>,
    ) -> HereticInfo {
        info_of(heretic, icons.as_deref())
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ResearchAct, UiReject> {
        if action != RESEARCH {
            return Err(UiReject::UnknownAction);
        }
        let get = |k: &str| {
            params
                .get(k)
                .map(str::to_owned)
                .ok_or_else(|| UiReject::BadParams(format!("{k}: missing")))
        };
        Ok(ResearchAct {
            path: get("path")?,
            category: get("category")?,
        })
    }
}

pub(super) fn node(
    entry: &Entry,
    researched: Option<&Researched>,
    heretic: &Heretic,
    icons: Option<&IconIndex>,
) -> KnowledgeNode {
    let def = entry.def();
    let resolve = |dmi: &str, state: &str| icons.and_then(|i| i.get(dmi, state));
    let mut desc = def.desc.to_owned();
    if researched.is_some() && def.kind == Kind::Spell && def.max_charges != u32::MAX {
        desc.push_str(&format!(
            "<br>Количество зарядов: {}{}.",
            def.max_charges,
            if def.transmute_text.is_empty() {
                ""
            } else {
                ". После их расхода заклинание нужно перезарядить"
            }
        ));
    }
    let mut notice = def.notice.to_owned();
    let mut disabled = false;
    if def.kind == Kind::Ultimate
        && let Err(reason) = heretic.can_ascend()
    {
        disabled = true;
        notice.push_str("<br>");
        notice.push_str(reason);
    }
    KnowledgeNode {
        path: def.path.to_owned(),
        name: def.name.to_owned(),
        desc,
        gain_flavor: def.gain_text.to_owned(),
        cost: entry.cost,
        depth: entry.depth,
        bgr: entry.bgr.to_owned(),
        bgr_icon: resolve(KNOWLEDGE_DMI, entry.bgr),
        icon: resolve(def.icon.0, def.icon.1),
        category: entry.category.as_str().to_owned(),
        done: researched.is_some(),
        ascension: def.kind == Kind::Ultimate,
        disabled,
        notice,
        info: def.transmute_text.to_owned(),
    }
}

/// `ui_data`.
pub fn info_of(heretic: &Heretic, icons: Option<&IconIndex>) -> HereticInfo {
    let mut tiers: Vec<Vec<KnowledgeNode>> = Vec::new();
    let mut shop_nodes = Vec::new();
    let place = |tiers: &mut Vec<Vec<KnowledgeNode>>, n: KnowledgeNode| {
        let depth = n.depth.max(1) as usize;
        while tiers.len() < depth {
            tiers.push(Vec::new());
        }
        tiers[depth - 1].push(n);
    };
    for r in &heretic.researched {
        let n = node(&r.entry, Some(r), heretic, icons);
        if matches!(r.entry.category, Category::Shop | Category::Draft) {
            shop_nodes.push(n);
        } else {
            place(&mut tiers, n);
        }
    }
    let can = heretic.researchable();
    for e in &heretic.tree {
        if e.def().kind == Kind::Starting || !can.contains(&e.id) {
            continue;
        }
        place(&mut tiers, node(e, None, heretic, icons));
    }
    if heretic.route.is_some() {
        for e in &heretic.draft {
            if can.contains(&e.id) {
                place(&mut tiers, node(e, None, heretic, icons));
            }
        }
        for e in &heretic.shop {
            if can.contains(&e.id) {
                shop_nodes.push(node(e, None, heretic, icons));
            }
        }
    }
    HereticInfo {
        charges: heretic.points,
        total_sacrifices: heretic.total_sacrifices,
        ascended: heretic.ascended,
        objectives: heretic
            .objectives
            .iter()
            .enumerate()
            .map(|(i, o)| ObjectiveInfo {
                count: i as u32 + 1,
                explanation: o.explanation(),
                complete: o.complete(heretic),
            })
            .collect(),
        can_change_objective: false,
        passive_level: 1,
        points_to_aura: POINTS_TO_AURA,
        knowledge_tiers: tiers,
        knowledge_shop: shop_nodes,
    }
}

/// `path_info`: every path's preview from a fresh start shop.
pub fn static_info(icons: Option<&IconIndex>) -> HereticStatic {
    let shop = knowledge::start_shop();
    let blank = Heretic::new(String::new(), 0.0);
    let entry = |path: &str| shop.iter().find(|e| e.path == path);
    let paths = PATHS
        .iter()
        .filter_map(|p| {
            let start = entry(p.start)?;
            Some(HereticPath {
                route: p.route.to_owned(),
                complexity: p.complexity.to_owned(),
                complexity_color: p.complexity_color.to_owned(),
                description: p.description.iter().map(|s| (*s).to_owned()).collect(),
                pros: p.pros.iter().map(|s| (*s).to_owned()).collect(),
                cons: p.cons.iter().map(|s| (*s).to_owned()).collect(),
                tips: p.tips.iter().map(|s| (*s).to_owned()).collect(),
                starting_knowledge: node(start, None, &blank, icons),
                preview_abilities: [p.tier1, p.tier2, p.tier3, p.tier4]
                    .iter()
                    .filter_map(|t| entry(t))
                    .map(|e| node(e, None, &blank, icons))
                    .collect(),
                passive_name: p.passive.name.to_owned(),
                passive_recharge: p.passive.recharge.to_owned(),
                passive_description: p
                    .passive
                    .descriptions
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect(),
            })
        })
        .collect();
    HereticStatic {
        paths,
        node_finished: icons.and_then(|i| i.get(KNOWLEDGE_DMI, "node_finished")),
        node_locked: icons.and_then(|i| i.get(KNOWLEDGE_DMI, "node_locked")),
    }
}

/// `path_info` with the sprites resolved, on the heretic (static data does not see resources).
#[derive(Component, Clone, Debug)]
pub struct HereticPaths(pub HereticStatic);

pub(super) fn update_ui(world: &mut World, mob: Entity) {
    // The component changed: the window's data follows (`update_data_for_all_viewers`).
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        h.set_changed();
    }
}

pub(super) fn research_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<AntagInfoHereticUi>>>,
) {
    let pending: Vec<(Entity, Entity, Entity, ResearchAct)> = acts
        .read(world.resource::<Messages<UiActed<AntagInfoHereticUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, act) in pending {
        if actor != source || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client) {
            continue;
        }
        if world.get::<Stat>(actor) == Some(&Stat::Dead) {
            continue;
        }
        let (Some(def), Some(category)) =
            (knowledge::def(&act.path), Category::parse(&act.category))
        else {
            warn!("heretic {actor}: research of an unknown knowledge {act:?}");
            continue;
        };
        try_research(world, actor, def.path, category, false);
    }
}

/// `ui_act("research")`: researchable, ascension allowed, final knowledge confirmed, paid.
pub(super) fn try_research(
    world: &mut World,
    mob: Entity,
    path: &'static str,
    category: Category,
    confirmed: bool,
) {
    let Some(h) = world.get::<Heretic>(mob) else {
        return;
    };
    let Some(entry) = h.shop_of(category).iter().find(|e| e.path == path) else {
        warn!("heretic {mob} potentially attempted to href exploit: {path} not in its shop");
        return;
    };
    if !h.researchable().contains(&entry.id) {
        warn!("heretic {mob} potentially attempted to href exploit: {path} not researchable");
        return;
    }
    let def = entry.def();
    if def.kind == Kind::Ultimate && h.can_ascend().is_err() {
        warn!("heretic {mob} attempted a final knowledge without its objectives");
        return;
    }
    if entry.cost > h.points {
        return;
    }
    if def.is_final && !h.unlimited_blades && !confirmed {
        if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
            open_prompt(
                world,
                client,
                mob,
                HereticPrompt {
                    kind: PromptKind::ConfirmFinal { path, category },
                    title: "Изучить финальное заклинание?".into(),
                    message: "ЭТО ОТКЛЮЧИТ ВОЗМОЖНОСТЬ РАЗБИТЬ ВАШ КЛИНОК. Вы готовы изучить это? \
                              Ограничение на количества клинков будет убрано."
                        .into(),
                    items: vec!["Да".into(), "Нет".into()],
                    paths: Vec::new(),
                    targets: Vec::new(),
                },
            );
        }
        return;
    }
    purchase(world, mob, path, category);
}

/// `/datum/action/antag_info/Trigger` of a heretic.
pub(super) fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if is_heretic(world, source) {
            world.write_message(OpenUi::new::<AntagInfoHereticUi>(
                client,
                actor,
                source,
                HERETIC_NAME,
            ));
        }
    }
}

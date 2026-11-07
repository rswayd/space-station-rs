//! Gaining the heretic, knowledge and points, and the influences.

use super::*;

/// `add_antag_datum(/datum/antagonist/heretic)`. `false` if it already is one or cannot be.
pub fn add_heretic(world: &mut World, mob: Entity) -> bool {
    if is_heretic(world, mob) || world.get::<Health>(mob).is_none() {
        return false;
    }
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let hud_icon = icon(world, ANTAG_HUD_DMI, "heretic");
    let t = now(world);
    let paths = static_info(world.get_resource::<IconIndex>());
    world.entity_mut(mob).insert((
        Heretic::new(key, t),
        HereticPaths(paths),
        AntagHudData {
            hud_icon,
            ..default()
        },
    ));
    // `on_gain`: the start knowledge for free.
    let starts: Vec<&'static str> = knowledge::start_knowledge().map(|k| k.path).collect();
    for path in starts {
        gain_knowledge(world, mob, path, Category::Start);
    }
    // `apply_innate_effects`: `add_tracked_mind`, new influences for an on-station heretic.
    world.resource_mut::<RealityTracker>().tracked.push(mob);
    generate_influences(world);
    if let Some(client) = client {
        crate::hallucination::see_groups(world, client, HERETIC_SIGHT, true);
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
    }
    greet(world, mob);
    refresh(world, mob);
    info!("{mob} became a heretic");
    true
}

/// `greet()` and `announce_objectives`, then the window.
pub(super) fn greet(world: &mut World, mob: Entity) {
    chat_to(world, mob, Span::Bold, format!("Вы - {HERETIC_NAME}."));
    announce_objectives(world, mob);
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        world.write_message(OpenUi::new::<AntagInfoHereticUi>(
            client,
            mob,
            mob,
            HERETIC_NAME,
        ));
    }
}

pub(super) fn announce_objectives(world: &mut World, mob: Entity) {
    let Some(lines) = world.get::<Heretic>(mob).map(|h| {
        h.objectives
            .iter()
            .enumerate()
            .map(|(i, o)| format!("{OBJECTIVE_NAME} #{}: {}", i + 1, o.explanation()))
            .collect::<Vec<_>>()
    }) else {
        return;
    };
    chat_to(world, mob, Span::Notice, "Ваши текущие задачи:");
    for line in lines {
        chat_to(world, mob, Span::Plain, line);
    }
}

/// `gain_knowledge`: the entry moves from its shop to the researched knowledge, then
/// `on_research`.
pub(super) fn gain_knowledge(
    world: &mut World,
    mob: Entity,
    path: &'static str,
    category: Category,
) -> bool {
    let Some(mut heretic) = world.get_mut::<Heretic>(mob) else {
        return false;
    };
    if heretic.has(path) {
        return false;
    }
    let shop = heretic.shop_mut(category);
    let entry = match shop.iter().position(|e| e.path == path) {
        Some(i) => shop.remove(i),
        None => Entry::new(path, None, category, 1, None),
    };
    let mut entry = entry;
    if entry.purchased_depth != 0 {
        entry.depth = entry.purchased_depth;
    }
    let def = entry.def();
    heretic.researched.push(Researched {
        entry,
        charges: def.max_charges,
        created: Vec::new(),
    });
    on_research(world, mob, def);
    true
}

/// `purchase_knowledge`: the cost, then [`gain_knowledge`].
pub(super) fn purchase(
    world: &mut World,
    mob: Entity,
    path: &'static str,
    category: Category,
) -> bool {
    let Some(cost) = world.get::<Heretic>(mob).and_then(|h| {
        h.shop_of(category)
            .iter()
            .find(|e| e.path == path)
            .map(|e| e.cost)
    }) else {
        return false;
    };
    if world.get::<Heretic>(mob).is_none_or(|h| cost > h.points) {
        return false;
    }
    if !gain_knowledge(world, mob, path, category) {
        return false;
    }
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        h.adjust_points(-(cost as i32));
    }
    info!("heretic {mob} gained knowledge: {path}");
    true
}

/// `on_research` of each knowledge family.
pub(super) fn on_research(world: &mut World, mob: Entity, def: &'static KnowledgeDef) {
    if !def.gain_text.is_empty() {
        chat_to(world, mob, Span::Warning, def.gain_text);
    }
    match def.path {
        LIVING_HEART => {
            if let Some(mut h) = world.get_mut::<Heretic>(mob) {
                h.living_heart = true;
            }
        }
        HUNT_AND_SACRIFICE => {
            obtain_targets(world, mob, true);
        }
        _ => {}
    }
    if def.kind == Kind::Starting {
        pick_path(world, mob, def);
    }
    if def.is_final {
        disable_blade_breaking(world, mob);
    }
    refresh(world, mob);
}

/// `limited_amount/starting/on_research`: the path, its tree, drafts and shop, the objectives.
pub(super) fn pick_path(world: &mut World, mob: Entity, def: &'static KnowledgeDef) {
    let Some(column) = knowledge::path_of_start(def.path) else {
        chat_to(
            world,
            mob,
            Span::Warning,
            "Вот чёрт! Что-то сломалось, путь не найден!",
        );
        return;
    };
    let mut tree = knowledge::path_tree(column);
    let mut picks: Vec<u32> = Vec::new();
    for _ in 0..64 {
        let v = world
            .get_resource_mut::<CombatRng>()
            .map_or(0, |mut r| r.range(0, u32::MAX - 1));
        picks.push(v);
    }
    let mut i = 0;
    let (shop, draft) = knowledge::drafts(column, &mut tree, |n| {
        let v = picks[i % picks.len()] as usize;
        i += 1;
        if n == 0 { 0 } else { v % n }
    });
    let extra = world
        .get_resource_mut::<CombatRng>()
        .map_or(2, |mut r| r.range(2, 4));
    let heads = alive_heads(world);
    let Some(mut h) = world.get_mut::<Heretic>(mob) else {
        return;
    };
    h.route = Some(column.route);
    let target = knowledge::research_target(tree.len(), draft.len(), extra);
    h.tree = tree;
    h.shop = shop;
    h.draft = draft;
    // `forge_primary_objectives`.
    h.objectives = vec![
        HereticObjective::Research { target },
        HereticObjective::MinorSacrifice {
            target: if heads < 2 { 5 } else { 4 },
        },
    ];
    if heads >= 2 {
        h.objectives.push(HereticObjective::MajorSacrifice);
    }
    info!("heretic {mob} took the {}", column.route);
    announce_objectives(world, mob);
}

pub(super) fn alive_heads(world: &mut World) -> u32 {
    let mut q = world.query_filtered::<(Entity, &Stat), With<PlayerMob>>();
    let alive: Vec<Entity> = q
        .iter(world)
        .filter(|(_, s)| **s != Stat::Dead)
        .map(|(e, _)| e)
        .collect();
    alive.into_iter().filter(|&e| is_head(world, e)).count() as u32
}

/// `disable_blade_breaking`.
pub(super) fn disable_blade_breaking(world: &mut World, mob: Entity) {
    let Some(mut h) = world.get_mut::<Heretic>(mob) else {
        return;
    };
    if h.unlimited_blades {
        return;
    }
    h.unlimited_blades = true;
    chat_to(
        world,
        mob,
        Span::BoldWarning,
        "Вы обрели значительное могущество, Мансус больше не позволит вам ломать свои клинки, но \
         теперь вы можете сделать их столько, сколько пожелаете.",
    );
}

pub(super) fn adjust_points(world: &mut World, mob: Entity, amount: i32) {
    let crossed = world
        .get_mut::<Heretic>(mob)
        .is_some_and(|mut h| h.adjust_points(amount));
    if crossed {
        disable_blade_breaking(world, mob);
    }
}

/// `COMSIG_HERETIC_INFLUENCE_DRAINED` and `COMSIG_HERETIC_SACRIFICE` on the basic knowledge:
/// the grasp is fully recharged.
pub(super) fn recharge_grasp(world: &mut World, mob: Entity) {
    if let Some(mut h) = world.get_mut::<Heretic>(mob)
        && let Some(r) = h.get_mut(BASIC)
    {
        r.charges = r.def().max_charges;
    }
    refresh(world, mob);
}

// ---------------------------------------------------------------------------------------------
// Influences

/// `generate_new_influences`: `max(5 - n + 1, 1)` for the n-th tracked heretic, minus what
/// exists or was drained; at least a tile between two.
pub fn generate_influences(world: &mut World) {
    let tracker = world.resource::<RealityTracker>().clone();
    let wanted: u32 = (1..=tracker.tracked.len() as u32)
        .map(|n| (NUM_INFLUENCES_PER_HERETIC + 1).saturating_sub(n).max(1))
        .sum();
    let mut sanity = 0;
    while (world.resource::<RealityTracker>().smashes.len() as u32
        + world.resource::<RealityTracker>().num_drained)
        < wanted
        && sanity < 100
    {
        let Some(pos) = crate::events::electrical_storm::random_station_turf(world) else {
            break;
        };
        let near = {
            let index = world.resource::<ss13_world::SpatialIndex>();
            (-1..=1).any(|dx| {
                (-1..=1).any(|dy| {
                    index
                        .on_tile(TilePos::new(pos.x + dx, pos.y + dy, pos.z))
                        .iter()
                        .any(|&e| {
                            world.get::<Influence>(e).is_some()
                                || world.get::<VisibleInfluence>(e).is_some()
                        })
                })
            })
        };
        if near {
            sanity += 1;
            continue;
        }
        if spawn_influence(world, pos).is_none() {
            break;
        }
    }
}

/// A new `/obj/effect/heretic_influence`: its `real_icon_state` image for heretics and ghosts.
pub fn spawn_influence(world: &mut World, pos: TilePos) -> Option<Entity> {
    let e = crate::construction::spawn_obj(world, INFLUENCE, pos)?;
    let name = format!(
        "{} {}",
        pick_str(world, INFLUENCE_PREFIX),
        pick_str(world, INFLUENCE_POSTFIX)
    );
    let state = icon(world, ELDRITCH_DMI, "reality_smash");
    let mut entity = world.entity_mut(e);
    entity.insert((
        Influence::default(),
        ss13_net::server::Networked,
        Invisibility(crate::ghost::INVISIBILITY_OBSERVER),
        VisibleToGroups(HERETIC_SIGHT),
        Name::new(name.clone()),
    ));
    if let Some(mut n) = entity.get_mut::<DmName>() {
        n.name = name;
    }
    // tg draws it only as the heretics' image (`real_icon_state`); its own `icon_state` is null.
    if let Some(state) = state {
        match entity.get_mut::<ss13_shared::Appearance>() {
            Some(mut appearance) => appearance.icon_state = state,
            None => {
                entity.insert(ss13_shared::Appearance {
                    icon_state: state,
                    // `OBJ_LAYER` on `GAME_PLANE`.
                    layer: ss13_shared::Layer(3.0),
                    plane: -4,
                    color: None,
                    pixel_offset: [0, 0],
                    rotation: 0,
                    overlays: Vec::new(),
                    mouse_opacity: ss13_shared::MouseOpacity::Icon,
                });
            }
        }
    }
    world.resource_mut::<RealityTracker>().smashes.push(e);
    Some(e)
}

/// Starts draining an influence (`drain_influence`): `points` knowledge in `delay_ds`.
pub(super) fn start_drain(
    world: &mut World,
    client: Entity,
    mob: Entity,
    influence: Entity,
    points: u32,
    delay_ds: f32,
) {
    if world
        .get::<Influence>(influence)
        .is_some_and(|i| i.being_drained)
    {
        tell(world, client, Span::Warning, "уже добывается!");
        return;
    }
    if let Some(mut i) = world.get_mut::<Influence>(influence) {
        i.being_drained = true;
    }
    tell(world, client, Span::Notice, "добыча влияния...");
    let item = world.get::<Inventory>(mob).and_then(Inventory::active_item);
    let actor = Actor { client, mob, item };
    if !do_after(
        world,
        actor,
        influence,
        delay_ds,
        0,
        Job::Heretic(HereticJob::Drain { points }),
    ) && let Some(mut i) = world.get_mut::<Influence>(influence)
    {
        i.being_drained = false;
    }
}

pub(super) fn finish_drain(world: &mut World, mob: Entity, action: &DoAfter, points: u32) {
    let influence = action.target;
    if world.get::<Influence>(influence).is_none() || !is_heretic(world, mob) {
        return;
    }
    tell(world, action.client, Span::Notice, "влияние добыто");
    adjust_points(world, mob, points as i32);
    recharge_grasp(world, mob);
    let drain = pick_str(world, INFLUENCE_DRAIN_MESSAGE);
    chat_to(world, mob, Span::Hypnophrase, drain);
    let name = name_of(world, influence);
    chat_to(
        world,
        mob,
        Span::Warning,
        format!("{} начинает проясняться в реальность!", capitalize(&name)),
    );
    let pos = world.get::<TilePos>(influence).copied();
    {
        let mut tracker = world.resource_mut::<RealityTracker>();
        tracker.smashes.retain(|&e| e != influence);
        tracker.num_drained += 1;
    }
    despawn(world, influence);
    if let Some(pos) = pos
        && let Some(visible) = crate::construction::spawn_obj(world, VISIBLE_INFLUENCE, pos)
    {
        let drained = pick_str(world, INFLUENCE_DRAINED);
        let full = format!("{drained} {name}");
        let born = now(world);
        let mut e = world.entity_mut(visible);
        e.insert((VisibleInfluence { born }, Name::new(full.clone())));
        if let Some(mut n) = e.get_mut::<DmName>() {
            n.name = full;
        }
        if let Some(mut appearance) = e.get_mut::<ss13_shared::Appearance>() {
            appearance.color = Some([255, 255, 255, 0]);
        }
    }
    update_ui(world, mob);
}

/// `show_presence`: `alpha` 0 for 15 s, then up to 255 over 15 s.
pub(super) fn fade_in_influences(
    time: Res<Time>,
    mut q: Query<(&VisibleInfluence, &mut ss13_shared::Appearance)>,
) {
    let t = time.elapsed_secs_f64();
    for (v, mut appearance) in &mut q {
        // Steps of 1/15 so the appearance changes once a second, not every tick.
        let alpha = (((t - v.born - 15.0) / 15.0).clamp(0.0, 1.0) * 15.0).floor() / 15.0;
        let color = if alpha >= 1.0 {
            None
        } else {
            Some([255, 255, 255, (alpha * 255.0) as u8])
        };
        if appearance.color != color {
            appearance.color = color;
        }
    }
}

/// Clients whose body is a heretic see [`HERETIC_SIGHT`]; others lose it (ghosts see influences
/// anyway, by their `see_invisible`).
pub(super) fn sync_sight(world: &mut World) {
    let mut heretics = world.query_filtered::<&PlayerMob, With<Heretic>>();
    let clients: Vec<Entity> = heretics.iter(world).map(|p| p.client).collect();
    for client in clients {
        if world
            .get::<SeeGroups>(client)
            .is_none_or(|g| g.0 & HERETIC_SIGHT == 0)
        {
            crate::hallucination::see_groups(world, client, HERETIC_SIGHT, true);
        }
    }
    // Other bits are hallucination images (`crate::hallucination`): only this one is ours.
    let mut seeing = world.query::<(Entity, &SeeGroups)>();
    let seeing: Vec<Entity> = seeing
        .iter(world)
        .filter(|(_, g)| g.0 & HERETIC_SIGHT != 0)
        .map(|(e, _)| e)
        .collect();
    for client in seeing {
        let mob = world.get_resource::<Players>().and_then(|p| p.mob(client));
        let keep = mob.is_some_and(|m| is_heretic(world, m) || world.get::<Ghost>(m).is_some());
        if !keep {
            crate::hallucination::see_groups(world, client, HERETIC_SIGHT, false);
        }
    }
}

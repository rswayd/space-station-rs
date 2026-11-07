//! Helpers, and gaining the wizard: the outfit, the objectives and the name.

use super::*;

pub(super) fn line(world: &mut World, mob: Entity, segments: Vec<ChatSegment>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage {
                channel: ChatChannel::System,
                segments,
            },
        });
    }
}

pub(super) fn with_rng<R>(world: &mut World, f: impl FnOnce(&mut World, &mut Rng) -> R) -> R {
    let mut rng = world
        .remove_resource::<WizardRng>()
        .map_or_else(|| Rng::new(0), |r| r.0);
    let out = f(world, &mut rng);
    world.insert_resource(WizardRng(rng));
    out
}

/// `pick(GLOB.wizard_first) pick(GLOB.wizard_second)`.
pub fn random_name(world: &mut World) -> String {
    let (first, second) = world
        .get_resource::<WizardNames>()
        .map(|n| (n.first.clone(), n.second.clone()))
        .unwrap_or_default();
    with_rng(world, |_, rng| {
        let mut pick = |list: &[String], fallback: &str| {
            if list.is_empty() {
                fallback.to_owned()
            } else {
                list[rng.range(0, list.len() as i64 - 1) as usize].clone()
            }
        };
        let a = pick(&first, "Мерлин");
        let b = pick(&second, "Мудрый");
        format!("{a} {b}")
    })
}

pub(super) fn hud_icon(world: &World, state: &str) -> Option<ss13_shared::IconStateId> {
    world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(ANTAG_HUD_DMI, state))
}

pub(super) fn name_of(world: &World, e: Entity) -> String {
    crate::medical::name_of(world, e)
}

/// `path` spawned on `mob` (tg `equip_to_slot`): replicated and carried. `None` without its
/// prototype.
pub(super) fn carried(world: &mut World, mob: Entity, path: &str) -> Option<Entity> {
    let item = crate::construction::spawn_detached(world, path)?;
    world.entity_mut(item).insert((Networked, CarriedBy(mob)));
    Some(item)
}

/// `path` spawned into `pack` (`equip_to_storage`).
pub(super) fn into_pack(world: &mut World, pack: Entity, path: &str) -> Option<Entity> {
    let item = crate::construction::spawn_detached(world, path)?;
    world.entity_mut(item).insert(Networked);
    crate::storage::put_inside(world, item, pack);
    Some(item)
}

/// `put_in_hands`, else on the floor under `mob`; true when in a hand.
pub(crate) fn give(world: &mut World, mob: Entity, item: Entity) -> bool {
    let hand = world.get::<Inventory>(mob).and_then(|i| {
        [i.active, 1 - i.active]
            .into_iter()
            .find(|&h| i.hands[h].is_none())
    });
    match hand {
        Some(h) => {
            world.entity_mut(item).insert((Networked, CarriedBy(mob)));
            if let Some(mut inv) = world.get_mut::<Inventory>(mob) {
                inv.hands[h] = Some(item);
            }
            crate::inventory::refresh_look(world, mob);
            true
        }
        None => {
            let pos = world.get::<TilePos>(mob).copied().unwrap_or_default();
            world.entity_mut(item).insert((Networked, pos));
            crate::construction::tile_changed(world, pos);
            false
        }
    }
}

/// `delete_equipment`: everything the body carries goes.
pub(super) fn strip(world: &mut World, mob: Entity) {
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        let items: Vec<Entity> = inventory
            .equipped()
            .chain(inventory.hands.iter().flatten().copied())
            .collect();
        for item in items {
            let inside: Vec<Entity> = world
                .get::<Stored>(item)
                .map(|s| s.items().to_vec())
                .unwrap_or_default();
            for e in inside {
                if world.get_entity(e).is_ok() {
                    world.despawn(e);
                }
            }
            if world.get_entity(item).is_ok() {
                world.despawn(item);
            }
        }
        let active = inventory.active;
        world.entity_mut(mob).insert(Inventory {
            active,
            ..Inventory::default()
        });
    }
}

/// `/datum/outfit/wizard` (`apprentice`: `/datum/outfit/wizard/apprentice`) and `post_equip`.
pub(super) fn equip(world: &mut World, mob: Entity, apprentice: bool) {
    let mut inv = world.get::<Inventory>(mob).cloned().unwrap_or_default();
    inv.uniform = carried(world, mob, LIGHTPURPLE);
    inv.suit = carried(world, mob, WIZROBE);
    inv.ears = carried(world, mob, HEADSET);
    inv.head = carried(world, mob, WIZARD_HAT);
    inv.shoes = carried(world, mob, MAGIC_SANDALS);
    let scroll = carried(
        world,
        mob,
        if apprentice {
            APPRENTICE_SCROLL
        } else {
            SCROLL
        },
    );
    if let Some(scroll) = scroll {
        let uses = if apprentice {
            APPRENTICE_SCROLL_USES
        } else {
            SCROLL_USES
        };
        world.entity_mut(scroll).insert(TeleportScroll { uses });
    }
    inv.pockets[1] = scroll;
    if !apprentice {
        inv.hands[0] = carried(world, mob, STAFF);
    }
    world.entity_mut(mob).insert(inv.clone());
    inv.back = crate::storage::spawn_backpack(world, mob);
    world.entity_mut(mob).insert(inv.clone());
    if let Some(pack) = inv.back {
        into_pack(world, pack, SURVIVAL_BOX);
        if !apprentice && let Some(book) = into_pack(world, pack, SPELLBOOK) {
            // `post_equip`: `new_spellbook.owner = wizard.mind`.
            let owner_name = name_of(world, mob);
            let rituals = world
                .get_resource::<crate::antag::midround::Midround>()
                .is_some_and(|m| m.tier == DYNAMIC_TIER_HIGH);
            world.entity_mut(book).insert(Spellbook {
                owner: Some(mob),
                owner_name,
                rituals,
                ..default()
            });
        }
    }
    crate::inventory::refresh_look(world, mob);
}

// ---------------------------------------------------------------------------------------------
// Gaining the wizard

/// The next den start tile (`pick(GLOB.wizardstart)`), `None` without a den.
pub(super) fn den_spawn(world: &mut World) -> Option<TilePos> {
    let den = world.resource::<WizardDen>().clone();
    if den.spawns.is_empty() {
        return None;
    }
    let (i, pos) = crate::server::pick_free(world, &den.spawns, den.next);
    world.resource_mut::<WizardDen>().next = i + 1;
    Some(pos)
}

/// The rulesets' `assign_role` for the players they took: a body in the den each.
pub fn spawn_wizards(world: &mut World, clients: &[Entity]) {
    for &client in clients {
        let (pos, rushed) = match den_spawn(world) {
            Some(pos) => (pos, false),
            None => (crate::server::pick_latejoin(world), true),
        };
        let name = random_name(world);
        let mob = crate::server::spawn_mob(world, client, pos, name);
        world.entity_mut(client).remove::<crate::round::NewPlayer>();
        add_wizard(world, mob);
        if rushed {
            tell(
                world,
                client,
                Span::Plain,
                "ПРЯМО СЕЙЧАС, ПОШЕЛ ПОШЕЛ ПОШЕЛ",
            );
        }
    }
}

/// Dev (`SS13_DEV_TRAITOR=wizard`): a joined crew body becomes a wizard in the den (a dev spawn
/// on the den's level is kept).
pub fn dev_wizard(world: &mut World, client: Entity, mob: Entity) {
    let den_z = world.resource::<WizardDen>().z;
    let kept = crate::server::dev_spawn(world, client).is_some_and(|p| Some(p.z) == den_z);
    if !kept && let Some(pos) = den_spawn(world) {
        world.entity_mut(mob).insert(pos);
    }
    add_wizard(world, mob);
}

/// `add_antag_datum(/datum/antagonist/wizard)`: stripped and dressed, objectives, greeting, HUD,
/// info panel, the rename prompt. `false` if it already is one or is not a living mob.
pub fn add_wizard(world: &mut World, mob: Entity) -> bool {
    if world.get::<Wizard>(mob).is_some() || world.get::<Health>(mob).is_none() {
        return false;
    }
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    strip(world, mob);
    // `owner.set_assigned_role(space_wizard)`, `set_species(/datum/species/human)`.
    crate::noncrew_jobs::set_assigned_role(world, mob, crate::noncrew_jobs::SPACE_WIZARD);
    crate::species::set_species(world, mob, crate::species::SpeciesId::Human);
    equip(world, mob, false);
    let objectives = with_rng(world, |world, rng| forge_objectives(world, mob, rng));
    let hud_icon = hud_icon(world, "wizard");
    world.entity_mut(mob).insert((
        Wizard {
            key,
            master: None,
            school: None,
            objectives,
            purchases: Vec::new(),
        },
        AntagHudData {
            hud_icon,
            ..default()
        },
    ));
    greet(world, mob, None);
    if let Some(client) = client {
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
        world.write_message(OpenUi::new::<WizardInfoWindow>(
            client,
            mob,
            mob,
            WIZARD_NAME,
        ));
        rename_prompt(world, client, mob);
    }
    info!("{mob} is a wizard");
    true
}

/// `create_objectives`.
pub(super) fn forge_objectives(
    world: &mut World,
    owner: Entity,
    rng: &mut Rng,
) -> Vec<WizardObjective> {
    let targets: Vec<StealItemDef> = world
        .get_resource::<crate::antag::uplink::UplinkCatalog>()
        .map(|c| c.data.steal_items.clone())
        .unwrap_or_default();
    let common = |world: &World, kind: ObjectiveKind| {
        WizardObjective::Common(Objective {
            explanation: kind.explanation(world),
            kind,
        })
    };
    let mut out = Vec::new();
    match rng.range(1, 100) {
        1..=30 => {
            let target = find_target(world, &[owner], &[], assassinate, rng);
            out.push(common(&*world, assassinate(target)));
            out.push(common(&*world, ObjectiveKind::Escape));
        }
        31..=60 => {
            let item = find_steal_target(world, &[owner], &[], &targets, rng);
            out.push(common(&*world, ObjectiveKind::Steal { item }));
            out.push(common(&*world, ObjectiveKind::Escape));
        }
        61..=85 => {
            let target = find_target(world, &[owner], &[], assassinate, rng);
            out.push(common(&*world, assassinate(target)));
            let item = find_steal_target(world, &[owner], &[], &targets, rng);
            out.push(common(&*world, ObjectiveKind::Steal { item }));
            out.push(WizardObjective::Survive);
        }
        _ => out.push(common(&*world, ObjectiveKind::Hijack)),
    }
    out
}

pub(super) fn assassinate(target: Option<Entity>) -> ObjectiveKind {
    ObjectiveKind::Assassinate { target }
}

/// `greet` and `announce_objectives`.
pub(super) fn greet(world: &mut World, mob: Entity, lesson: Option<String>) {
    let Some(wizard) = world.get::<Wizard>(mob).cloned() else {
        return;
    };
    match wizard.master {
        Some(master) => {
            let master = name_of(world, master);
            chat_to(
                world,
                mob,
                Span::Bold,
                format!(
                    "Вы ученик {master}! Вы связаны магическим контрактом, чтобы следовать \
                     приказам и помогать в выполнении целей своего учителя."
                ),
            );
            if let Some(lesson) = lesson {
                chat_to(world, mob, Span::Bold, lesson);
            }
        }
        None => chat_to(
            world,
            mob,
            Span::Bold,
            format!("Вы - {}.", wizard.antag_name()),
        ),
    }
    chat_to(world, mob, Span::Notice, "Ваши текущие задачи:");
    for (i, o) in wizard.objectives.iter().enumerate() {
        line(
            world,
            mob,
            vec![
                ChatSegment::new(Span::Bold, format!("{OBJECTIVE_NAME} #{}", i + 1)),
                ChatSegment::new(Span::Plain, format!(": {}", o.explanation())),
            ],
        );
    }
}

/// `rename_wizard`.
pub(super) fn rename_prompt(world: &mut World, client: Entity, mob: Entity) {
    let default = name_of(world, mob);
    let ask = Ask::new(
        client,
        mob,
        "Смена имени",
        format!("Ваше имя - {WIZARD_NAME}. Вы хотите изменить его на другое?"),
        PromptKind::Text {
            default: default.clone(),
            max_length: MAX_NAME_LEN,
            multiline: false,
        },
    );
    let prompt = ask_world(world, ask);
    world
        .entity_mut(prompt)
        .insert((WizardPrompt::Rename, RenameDefault(default)));
}

/// `add_antag_datum(/datum/antagonist/wizard/apprentice)` on a fresh body for `master`.
pub fn add_apprentice(world: &mut World, mob: Entity, master: Entity, school: School) -> bool {
    if world.get::<Wizard>(mob).is_some() || world.get::<Health>(mob).is_none() {
        return false;
    }
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    strip(world, mob);
    // `set_assigned_role(wizard_apprentice)`.
    crate::noncrew_jobs::set_assigned_role(world, mob, crate::noncrew_jobs::WIZARD_APPRENTICE);
    crate::species::set_species(world, mob, crate::species::SpeciesId::Human);
    equip(world, mob, true);
    for &spell in school.spells() {
        crate::spells::learn(world, mob, spell);
    }
    if school == School::Healing
        && let Some(staff) =
            crate::construction::spawn_detached(world, ss13_mapload::antag::HEALING_STAFF)
    {
        crate::spells::magic_guns::adopt_full(world, staff);
        give(world, mob, staff);
    }
    let master_name = name_of(world, master);
    let hud_icon = hud_icon(world, "apprentice");
    world.entity_mut(mob).insert((
        Wizard {
            key,
            master: Some(master),
            school: Some(school),
            objectives: vec![WizardObjective::Protect {
                target: master,
                text: format!("Защищайте волшебника - {master_name}."),
            }],
            purchases: Vec::new(),
        },
        AntagHudData {
            hud_icon,
            ..default()
        },
    ));
    greet(world, mob, Some(school.lesson(&master_name)));
    if let Some(client) = client {
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
    }
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/effects/magic.ogg", 50.0, false),
        );
    }
    info!("{mob} is the {} apprentice of {master}", school.key());
    true
}

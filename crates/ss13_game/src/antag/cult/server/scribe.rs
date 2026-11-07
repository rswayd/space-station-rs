//! The ritual dagger: scribing and erasing runes.

use super::*;

/// `attack_self` of the dagger (the rune list) or of a blood magic aura (cast on the user).
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if let Some(aura) = world.get::<BloodAura>(item).copied() {
        cast_aura(world, client, mob, item, aura, mob);
        return true;
    }
    if path_of(world, item).as_deref() != Some(DAGGER) {
        return false;
    }
    if world.get::<Cultist>(mob).is_none() {
        let name = name_of(world, item);
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} is covered in unintelligible shapes and markings."),
        );
        return true;
    }
    if world
        .get::<Cultist>(mob)
        .is_some_and(|c| c.scribing.is_some())
    {
        tell(
            world,
            client,
            Span::Warning,
            "You are already drawing a rune.",
        );
        return true;
    }
    if !can_scribe(world, client, mob) {
        return true;
    }
    let items: Vec<String> = RuneKind::ALL
        .iter()
        .map(|k| k.def().cultist_name.to_owned())
        .collect();
    let values = vec![mob; items.len()];
    open_list(
        world,
        client,
        mob,
        CultPrompt {
            kind: PromptKind::Rune,
            title: "Sigils of Power".into(),
            message: "Choose a rite to scribe".into(),
            items,
            values,
            spell_ids: Vec::new(),
        },
    );
    true
}

/// `can_scribe_rune` + `check_rune_turf`.
pub(super) fn can_scribe(world: &mut World, client: Entity, mob: Entity) -> bool {
    if held_dagger(world, mob).is_none() && !held_any_dagger(world, mob) {
        return false;
    }
    if !can_act(world, mob) {
        tell(
            world,
            client,
            Span::Warning,
            "You can't draw a rune right now.",
        );
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return false;
    };
    let turf = crate::construction::turf_at(world, pos).and_then(|t| path_of(world, t));
    if turf
        .as_deref()
        .is_some_and(|p| p.starts_with("/turf/open/space"))
    {
        tell(
            world,
            client,
            Span::Warning,
            "You cannot scribe runes in space!",
        );
        return false;
    }
    if rune_at(world, pos).is_some() {
        tell(world, client, Span::Cult, "There is already a rune here.");
        return false;
    }
    let permitted = area_flags(world, pos).is_none_or(|f| f & CULT_PERMITTED != 0);
    if !permitted {
        tell(
            world,
            client,
            Span::Warning,
            "The veil is not weak enough here.",
        );
        return false;
    }
    true
}

pub(super) fn held_any_dagger(world: &World, mob: Entity) -> bool {
    world.get::<Inventory>(mob).is_some_and(|i| {
        i.hands
            .iter()
            .flatten()
            .any(|&h| path_of(world, h).as_deref() == Some(DAGGER))
    })
}

pub(super) fn area_of(world: &World, pos: TilePos) -> Option<Entity> {
    world.get_resource::<Areas>()?.area_at(pos)
}

pub(super) fn area_flags(world: &World, pos: TilePos) -> Option<u32> {
    let area = area_of(world, pos)?;
    world.get::<DmPower>(area).map(|p| p.area_flags as u32)
}

pub(super) fn area_name(world: &World, pos: TilePos) -> String {
    area_of(world, pos)
        .and_then(|a| world.get::<DmName>(a))
        .map(|n| {
            n.name
                .trim_start_matches("\\improper ")
                .trim_start_matches("\\proper ")
                .to_owned()
        })
        .unwrap_or_default()
}

pub fn rune_at(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<Rune>(e).is_some())
}

/// The picked rune: a keyword first for the teleport rune; the Nar'Sie rune's checks.
pub(super) fn chose_rune(world: &mut World, client: Entity, mob: Entity, kind: RuneKind) {
    if !can_scribe(world, client, mob) {
        return;
    }
    if kind.def().req_keyword {
        open_text(world, client, mob, TextKind::Keyword);
        return;
    }
    begin_scribe(world, client, mob, kind, None);
}

/// `do_scribe_rune` after the choices: the arm is cut and the `do_after` starts.
pub(super) fn begin_scribe(
    world: &mut World,
    client: Entity,
    mob: Entity,
    kind: RuneKind,
    keyword: Option<String>,
) {
    if !can_scribe(world, client, mob) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    if kind == RuneKind::Summon && area_of(world, pos).is_none() {
        tell(
            world,
            client,
            Span::CultItalic,
            "The veil is not weak enough here to summon a cultist, you must be on station!",
        );
        return;
    }
    if kind == RuneKind::NarSie && !scribe_narsie_rune(world, client, mob, pos) {
        return;
    }
    let def = kind.def();
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        "You slice open your arm and begin drawing a sigil of the Geometer.",
        &format!("{user} cuts open their arm and begins writing in their own blood!"),
        Span::Warning,
    );
    let zone = arm(world);
    hurt(world, mob, Some(zone), def.scribe_damage);
    let item = world.get::<Inventory>(mob).and_then(Inventory::active_item);
    if let Some(mut c) = world.get_mut::<Cultist>(mob) {
        c.scribing = Some((kind, keyword, pos));
    }
    let actor = Actor { client, mob, item };
    if !do_after(
        world,
        actor,
        mob,
        def.scribe_delay,
        0,
        Job::Cult(CultJob::Scribe),
    ) && let Some(mut c) = world.get_mut::<Cultist>(mob)
    {
        c.scribing = None;
    }
}

/// `scribe_narsie_rune`: in a ritual site, after the sacrifice, before the summoning; CentCom
/// is told.
pub(super) fn scribe_narsie_rune(
    world: &mut World,
    client: Entity,
    mob: Entity,
    pos: TilePos,
) -> bool {
    let team = world.resource::<CultTeam>().clone();
    let spots = team.summon_spots();
    let here = area_of(world, pos);
    if spots.is_empty() {
        tell(
            world,
            client,
            Span::Warning,
            "There are no ritual sites on this station to scribe this rune!",
        );
        return false;
    }
    if !spots.iter().any(|(a, _)| Some(*a) == here) {
        let names = english_list(spots.iter().map(|(_, n)| n.as_str()));
        tell(
            world,
            client,
            Span::Warning,
            format!("The ritual can only begin where the veil is weak - in {names}!"),
        );
        return false;
    }
    if !team.sacrifice_done() {
        tell(
            world,
            client,
            Span::Warning,
            "The sacrifice is not complete. The portal would lack the power to open if you tried!",
        );
        return false;
    }
    if team.narsie_summoned {
        tell(
            world,
            client,
            Span::CultLarge,
            "\"I am already here. There is no need to try to summon me now.\"",
        );
        return false;
    }
    let name = name_of(world, mob);
    let area = area_name(world, pos);
    crate::shuttle::priority_announce(
        world,
        &format!(
            "{}: Отдел паранормальных явлений",
            crate::events::COMMAND_NAME
        ),
        "",
        &format!(
            "Зафиксирован призыв древнего божества культистом {name} в {area}. Прервите ритуал \
             любой ценой!"
        ),
    );
    true
}

/// [`Job::Cult`] completions.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: CultJob) {
    match job {
        CultJob::Scribe => finish_scribe(world, mob, action),
        CultJob::Erase => finish_erase(world, mob, action),
        CultJob::Carve => finish_carving(world, mob),
        CultJob::Shackle => finish_shackle(world, mob, action),
    }
}

pub(super) fn finish_scribe(world: &mut World, mob: Entity, action: &DoAfter) {
    let Some((kind, keyword, pos)) = world
        .get_mut::<Cultist>(mob)
        .and_then(|mut c| c.scribing.take())
    else {
        return;
    };
    if !can_scribe(world, action.client, mob) || world.get::<TilePos>(mob) != Some(&pos) {
        return;
    }
    let Some(rune) = spawn_rune(world, kind, pos, keyword) else {
        return;
    };
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        "You finish drawing the arcane markings of the Geometer.",
        &format!("{user} creates a strange circle in their own blood."),
        Span::Warning,
    );
    let def = kind.def();
    chat_to(
        world,
        mob,
        Span::Cult,
        format!(
            "The {} rune {}",
            def.cultist_name.to_lowercase(),
            def.cultist_desc
        ),
    );
    info!(
        "{mob} scribed a {} rune ({rune}) at {pos:?}",
        def.cultist_name
    );
}

/// A new rune of `kind` at `pos` (`new rune_to_scribe(turf, keyword)`).
pub fn spawn_rune(
    world: &mut World,
    kind: RuneKind,
    pos: TilePos,
    keyword: Option<String>,
) -> Option<Entity> {
    let rune = crate::construction::spawn_obj(world, kind.def().path, pos)?;
    let area = area_name(world, pos);
    let listkey = match &keyword {
        Some(k) => format!("{k} {area}"),
        None => area,
    };
    world.entity_mut(rune).insert(Rune {
        kind,
        keyword,
        listkey,
        in_use: false,
        used: false,
        barrier: None,
    });
    Some(rune)
}

/// An admin's spawn: a rune gets its state like a scribed one (`new rune_type(loc)`).
// shortcut: runes placed on maps get no rune state (none of the station maps place any).
pub fn spawn_obj_or_rune(world: &mut World, path: &str, pos: TilePos) -> Option<Entity> {
    match RuneKind::from_path(path) {
        Some(kind) => spawn_rune(world, kind, pos, None),
        None => crate::construction::spawn_obj(world, path, pos),
    }
}

/// The dagger on a rune (scrape it away) — `item_interaction` before the inventory's own.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if let Some(aura) = world.get::<BloodAura>(held).copied() {
        if world.get::<Health>(target).is_none() {
            cast_aura(world, client, mob, held, aura, target);
        }
        return true;
    }
    if world.get::<Rune>(target).is_none() || path_of(world, held).as_deref() != Some(DAGGER) {
        return false;
    }
    if world.get::<Cultist>(mob).is_none() {
        return false;
    }
    let erase = world.get::<Rune>(target).map(|r| r.kind.def().erase_time);
    let item = Some(held);
    let actor = Actor { client, mob, item };
    do_after(
        world,
        actor,
        target,
        erase.unwrap_or(15.0),
        0,
        Job::Cult(CultJob::Erase),
    );
    true
}

pub(super) fn finish_erase(world: &mut World, mob: Entity, action: &DoAfter) {
    let rune = action.target;
    let Some(kind) = world.get::<Rune>(rune).map(|r| r.kind) else {
        return;
    };
    if world.get::<Cultist>(mob).is_none() || !can_act(world, mob) {
        return;
    }
    chat_to(
        world,
        mob,
        Span::Notice,
        format!(
            "You carefully erase the {} rune.",
            kind.def().cultist_name.to_lowercase()
        ),
    );
    delete_rune(world, rune);
}

pub(super) fn delete_rune(world: &mut World, rune: Entity) {
    if let Some(barrier) = world.get::<Rune>(rune).and_then(|r| r.barrier)
        && world.get_entity(barrier).is_ok()
    {
        world.despawn(barrier);
    }
    if world.get_entity(rune).is_ok() {
        world.despawn(rune);
    }
}

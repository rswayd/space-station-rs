//! Reinforcements (server) after tg `/obj/item/antag_spawner/nuke_ops` (+ `/clown`) and
//! `/datum/antagonist/nukeop/reinforcement` (+ `/clownop`).
//!
//! An operative uses the beacon in hand ([`use_beacon`]): only an operative may
//! (`check_usability`); the first ghost opted in to "Operative (Midround)" becomes a
//! reinforcement where the beacon is, in a random faction's reinforcement outfit
//! (`use_subtypes`: Gorlex, Cybersun, Donk, Waffle, Interdyne, MI13; the clown beacon: the clown
//! reinforcement's), with an empty uplink of the team's kind, the team's code and objective; the
//! beacon is used up. Nobody willing: the beacon stays.
//! - The cyborg beacons (`borg_tele`: assault, medical, saboteur) build a syndicate cyborg of
//!   their model instead of a human (`crate::silicon::cyborg::spawn_syndicate`), an operative
//!   of the team too.
// shortcut: no drop pod (the reinforcement appears at the beacon), no 15 s poll; the overwatch
// agent (no overwatch console, body cameras or base outpost) is not sold; the syndicate cyborg
// has no minimap, tacmap or PDA and keeps the "Cyborg" brain name (not the team's last name).

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_shared::chat::Span;

use super::server::{Nukeop, OUTFIT_CLOWN_REINFORCEMENT, OpKind, add_op, chat_to, willing_ghost};

/// `/obj/item/antag_spawner/nuke_ops`.
pub const BEACON: &str = "/obj/item/antag_spawner/nuke_ops";
/// `/obj/item/antag_spawner/nuke_ops/clown`.
pub const CLOWN_BEACON: &str = "/obj/item/antag_spawner/nuke_ops/clown";
/// `subtypesof(/datum/outfit/syndicate/reinforcement)` without the plasmaman's.
pub const REINFORCEMENT_OUTFITS: [&str; 6] = [
    "/datum/outfit/syndicate/reinforcement/gorlex",
    "/datum/outfit/syndicate/reinforcement/cybersun",
    "/datum/outfit/syndicate/reinforcement/donk",
    "/datum/outfit/syndicate/reinforcement/waffle",
    "/datum/outfit/syndicate/reinforcement/interdyne",
    "/datum/outfit/syndicate/reinforcement/mi13",
];

/// `/obj/item/antag_spawner/nuke_ops/borg_tele/*`: the model each builds.
const BORG_BEACONS: [(&str, &str); 3] = [
    (
        "/obj/item/antag_spawner/nuke_ops/borg_tele/assault",
        "/obj/item/robot_model/syndicate",
    ),
    (
        "/obj/item/antag_spawner/nuke_ops/borg_tele/medical",
        "/obj/item/robot_model/syndicate_medical",
    ),
    (
        "/obj/item/antag_spawner/nuke_ops/borg_tele/saboteur",
        "/obj/item/robot_model/saboteur",
    ),
];

fn borg_model(path: &str) -> Option<&'static str> {
    BORG_BEACONS
        .iter()
        .find(|(beacon, _)| *beacon == path)
        .map(|&(_, model)| model)
}

/// Whether `item` is a reinforcement beacon this port sells.
pub fn is_beacon(world: &World, item: Entity) -> bool {
    world.get::<DmItem>(item).is_some_and(|i| {
        i.path == BEACON || i.path == CLOWN_BEACON || borg_model(&i.path).is_some()
    })
}

/// `check_usability`: an operative of any kind.
pub fn usable_by(world: &World, user: Entity) -> bool {
    world.get::<Nukeop>(user).is_some()
}

/// `attack_self`: the poll and `spawn_antag`.
pub fn use_beacon(world: &mut World, client: Entity, user: Entity, beacon: Entity) {
    if !usable_by(world, user) {
        chat_to(
            world,
            Some(client),
            Span::Danger,
            "ОШИБКА АУТЕНТИФИКАЦИИ. ДОСТУП ЗАПРЕЩЁН.",
        );
        return;
    }
    chat_to(
        world,
        Some(client),
        Span::Notice,
        "Вы активируете маяк и ждёте подтверждения.",
    );
    let Some((_, recruit)) = willing_ghost(world, crate::antag::ROLE_OPERATIVE_MIDROUND) else {
        chat_to(
            world,
            Some(client),
            Span::Warning,
            "Не удаётся связаться с командованием Синдиката. Подождите и попробуйте позже или \
             верните маяк в аплинк, чтобы получить обратно телекристаллы.",
        );
        return;
    };
    let Some(pos) = crate::modpc::pda::turf_of(world, beacon) else {
        return;
    };
    let clown = world
        .get::<DmItem>(beacon)
        .is_some_and(|i| i.path == CLOWN_BEACON);
    let borg = world
        .get::<DmItem>(beacon)
        .and_then(|i| borg_model(&i.path));
    if let Some(model) = borg {
        call_cyborg(world, user, recruit, pos, model, beacon);
        return;
    }
    let mut rng = super::server::take_rng(world);
    let outfit = if clown {
        OUTFIT_CLOWN_REINFORCEMENT
    } else {
        REINFORCEMENT_OUTFITS[rng.range(0, REINFORCEMENT_OUTFITS.len() as i64 - 1) as usize]
    };
    // `creator_op.get_team()`: the lone operative's reinforcement joins its team.
    let lone_team = world.get::<Nukeop>(user).is_some_and(|n| n.lone_team);
    let mob = crate::server::spawn_mob(world, recruit, pos, String::new());
    crate::noncrew_jobs::set_assigned_role(world, mob, crate::noncrew_jobs::NUCLEAR_OPERATIVE);
    add_op(
        world,
        mob,
        OpKind::Reinforcement,
        lone_team,
        0,
        Some(outfit),
        &mut rng,
    );
    super::server::put_rng(world, rng);
    info!("{user} called a reinforcement operative {mob} ({outfit}) at {pos:?}");
    if world.get_entity(beacon).is_ok() {
        world.despawn(beacon);
    }
}

/// `borg_tele/spawn_antag`: the cyborg, then `add_antag_datum(/datum/antagonist/nukeop/
/// reinforcement/cyborg)` with the creator's team.
fn call_cyborg(
    world: &mut World,
    user: Entity,
    recruit: Entity,
    pos: ss13_shared::TilePos,
    model: &str,
    beacon: Entity,
) {
    let Some(mob) = crate::silicon::cyborg::spawn_syndicate(world, recruit, pos, model) else {
        return;
    };
    let name = world.get::<Name>(mob).cloned();
    let lone_team = world.get::<Nukeop>(user).is_some_and(|n| n.lone_team);
    let mut rng = super::server::take_rng(world);
    add_op(
        world,
        mob,
        OpKind::Reinforcement,
        lone_team,
        0,
        None,
        &mut rng,
    );
    super::server::put_rng(world, rng);
    if let Some(name) = name {
        world.entity_mut(mob).insert(name);
    }
    info!("{user} called a syndicate cyborg {mob} ({model}) at {pos:?}");
    if world.get_entity(beacon).is_ok() {
        world.despawn(beacon);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_operatives_can_call_reinforcements() {
        let mut world = World::new();
        let crew = world.spawn_empty().id();
        let op = world
            .spawn(Nukeop {
                key: "op".into(),
                leader: false,
                memory: String::new(),
                kind: OpKind::Operative,
                lone_team: false,
            })
            .id();
        assert!(!usable_by(&world, crew));
        assert!(usable_by(&world, op));
        let beacon = world
            .spawn(DmItem {
                path: BEACON.into(),
                ..default()
            })
            .id();
        let borg = world
            .spawn(DmItem {
                path: "/obj/item/antag_spawner/nuke_ops/borg_tele/assault".into(),
                ..default()
            })
            .id();
        assert!(is_beacon(&world, beacon));
        assert!(is_beacon(&world, borg));
        assert_eq!(
            borg_model("/obj/item/antag_spawner/nuke_ops/borg_tele/saboteur"),
            Some("/obj/item/robot_model/saboteur")
        );
    }

    #[test]
    fn reinforcement_kits_follow_the_outfits() {
        use super::super::server::{outfit_path, starting_tc, uplink_of};
        use ss13_mapload::antag::{UPLINK_CLOWN_OPS, UPLINK_LONE_OP, UPLINK_NUKE_OPS};
        // `tc = 0`, but the uplink is still given.
        assert_eq!(starting_tc(OpKind::Reinforcement, 40), 0);
        assert_eq!(starting_tc(OpKind::Operative, 26), 35);
        assert_eq!(starting_tc(OpKind::Lone, 60), 25);
        assert_eq!(uplink_of(OpKind::Reinforcement, false).1, UPLINK_NUKE_OPS);
        assert_eq!(uplink_of(OpKind::Reinforcement, true).1, UPLINK_CLOWN_OPS);
        assert_eq!(uplink_of(OpKind::Lone, true).1, UPLINK_LONE_OP);
        assert_eq!(
            outfit_path(OpKind::Reinforcement, true),
            OUTFIT_CLOWN_REINFORCEMENT
        );
        assert_eq!(
            outfit_path(OpKind::Lone, false),
            "/datum/outfit/syndicate/full/loneop"
        );
    }
}

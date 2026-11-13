//! Traitor objectives after tg `code/game/gamemodes/objective.dm` and the traitor's forging
//! (`forge_traitor_objectives`, `forge_single_generic_objective`, `forge_ending_objective`).
//!
//! A mind here is the body the player spawned with (its entity); a target is another crew body
//! (an entity with [`PlayerJob`]). Completion is checked at round end against the live world.
// Escape and maroon read the emergency shuttle (`crate::shuttle`): escaped is alive on CentCom
// once the shuttle reached `SHUTTLE_ENDGAME`, or any living mind when the round was ended by
// force (`SSticker.force_ending`).
// Hijack needs every owner alive aboard the hijacked shuttle at CentCom
// (`crate::shuttle_console_server`). shortcut: the shuttle brig is not told apart.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::antag::StealItemDef;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::spawner::Rng;
use ss13_tgui::interfaces::ru_jobs::job_ru;

use crate::health::Stat;
use crate::inventory::{ALL_SLOTS, Inventory};
use crate::jobs::{Jobs, PlayerJob};
use crate::storage::Stored;

/// tg `HIJACK_PROB`, `HIJACK_MIN_PLAYERS`, `MARTYR_PROB`, `KILL_PROB`, `MAROON_PROB`.
pub const HIJACK_PROB: f32 = 10.0;
pub const HIJACK_MIN_PLAYERS: usize = 30;
pub const MARTYR_PROB: f32 = 20.0;
pub const KILL_PROB: f32 = 50.0;
pub const MAROON_PROB: f32 = 30.0;
/// BandaStation `TRAITOR_OBJECTIVES_AMOUNT` and `BROTHER_OBJECTIVES_AMOUNT`
/// (`config/game_options.txt`).
pub const TRAITOR_OBJECTIVES_AMOUNT: usize = 2;
pub const BROTHER_OBJECTIVES_AMOUNT: usize = 2;

const FREE_OBJECTIVE: &str = "Свободная задача.";

#[derive(Clone, Debug, PartialEq)]
pub enum ObjectiveKind {
    /// `/datum/objective/assassinate`.
    Assassinate { target: Option<Entity> },
    /// `/datum/objective/maroon`.
    Maroon { target: Option<Entity> },
    /// `/datum/objective/steal`.
    Steal { item: Option<StealItemDef> },
    /// `/datum/objective/escape`.
    Escape,
    /// `/datum/objective/martyr`.
    Martyr,
    /// `/datum/objective/hijack`.
    Hijack,
    /// `/datum/objective/convert_brother`: the team has more than one member.
    ConvertBrother,
    /// `/datum/objective/protect` (and `protect/nonhuman`: the targets here are crew bodies).
    Protect { target: Option<Entity> },
    /// `/datum/objective/exile`.
    Exile,
    /// `/datum/objective/custom` a player wrote (`submit_player_objective`): `completed`, and
    /// `no_failure` (the report prints no verdict).
    Custom,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Objective {
    pub kind: ObjectiveKind,
    pub explanation: String,
}

/// How the round ended, for the escape checks.
#[derive(Clone, Copy, Debug, Default)]
pub struct RoundEnd {
    /// tg `SSticker.force_ending`.
    pub force_ending: bool,
}

impl Objective {
    /// `martyr_compatible`.
    pub fn martyr_compatible(&self) -> bool {
        matches!(
            self.kind,
            ObjectiveKind::Assassinate { .. }
                | ObjectiveKind::Maroon { .. }
                | ObjectiveKind::Protect { .. }
        )
    }

    fn same_target(&self, other: &ObjectiveKind) -> bool {
        match (&self.kind, other) {
            (
                ObjectiveKind::Assassinate { target: a },
                ObjectiveKind::Assassinate { target: b },
            )
            | (ObjectiveKind::Maroon { target: a }, ObjectiveKind::Maroon { target: b })
            | (ObjectiveKind::Protect { target: a }, ObjectiveKind::Protect { target: b }) => {
                a == b
            }
            (ObjectiveKind::Steal { item: a }, ObjectiveKind::Steal { item: b }) => {
                a.as_ref().map(|i| &i.targetitem) == b.as_ref().map(|i| &i.targetitem)
            }
            _ => false,
        }
    }

    /// `check_completion` for its `owners` (tg `get_owners`: the owner, or the team's members).
    pub fn check_completion(&self, world: &World, owners: &[Entity], end: RoundEnd) -> bool {
        match &self.kind {
            ObjectiveKind::Assassinate { target } => {
                target.is_none_or(|t| !considered_alive(world, t))
            }
            ObjectiveKind::Maroon { target } => target.is_none_or(|t| {
                !considered_alive(world, t) || !crate::shuttle::escape_status(world, t).0
            }),
            ObjectiveKind::Steal { item } => item.as_ref().is_none_or(|item| {
                owners
                    .iter()
                    .any(|&owner| has_steal_target(world, owner, item))
            }),
            // "Require all owners escape safely."
            ObjectiveKind::Escape => owners
                .iter()
                .all(|&owner| considered_escaped(world, owner, end)),
            // `TRAITOR_SUICIDED` is not tracked: any death counts.
            ObjectiveKind::Martyr => owners.iter().all(|&owner| !considered_alive(world, owner)),
            // "Requires all owners to escape."
            ObjectiveKind::Hijack => {
                crate::shuttle::endgame(world)
                    && owners.iter().all(|&owner| {
                        considered_alive(world, owner)
                            && crate::shuttle::escape_status(world, owner).1
                    })
                    && crate::shuttle::hijacked(world)
            }
            ObjectiveKind::ConvertBrother => owners.len() > 1,
            // shortcut: `TRAIT_SUICIDED` is not tracked, a suicide is a death here.
            ObjectiveKind::Protect { target } => target.is_some_and(|t| considered_alive(world, t)),
            // "went to centcom or ended round on station".
            ObjectiveKind::Exile => owners.iter().all(|&owner| {
                considered_alive(world, owner)
                    && crate::explosions::turf_of(world, owner).is_some_and(|pos| {
                        !crate::comms_server::on_station_level(world, pos)
                            && world
                                .get_resource::<crate::shuttle::EmergencyShuttle>()
                                .is_none_or(|s| s.centcom_z != pos.z)
                    })
            }),
            ObjectiveKind::Custom => true,
        }
    }
}

/// tg `considered_alive(mind, enforce_human = TRUE)`; a death coma is alive.
pub fn considered_alive(world: &World, mind: Entity) -> bool {
    world.get_entity(mind).ok().is_some_and(|e| {
        e.get::<Stat>().is_some_and(|s| *s != Stat::Dead)
            || e.contains::<crate::health::DeathComa>()
    })
}

/// tg `considered_escaped`.
pub fn considered_escaped(world: &World, mind: Entity, end: RoundEnd) -> bool {
    considered_alive(world, mind)
        && (end.force_ending
            || crate::shuttle::endgame(world) && crate::shuttle::escape_status(world, mind).0)
}

/// tg `get_all_contents` of a mob: what it holds and wears, and everything inside those.
pub fn all_contents(world: &World, mob: Entity) -> Vec<Entity> {
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return Vec::new();
    };
    let mut out: Vec<Entity> = ALL_SLOTS
        .iter()
        .filter_map(|&s| inventory.slot(s))
        .collect();
    let mut i = 0;
    while i < out.len() {
        if let Some(stored) = world.get::<Stored>(out[i]) {
            out.extend_from_slice(stored.items());
        }
        i += 1;
    }
    out
}

fn has_steal_target(world: &World, owner: Entity, item: &StealItemDef) -> bool {
    all_contents(world, owner).into_iter().any(|e| {
        world.get::<DmItem>(e).is_some_and(|dm| {
            is_under(&dm.path, &item.targetitem) || item.altitems.contains(&dm.path)
        })
    })
}

/// A crew body's name and job for objective texts: `"[target.name], [job_title_ru(title)]"`.
pub(crate) fn target_text(world: &World, target: Entity) -> String {
    let name = world
        .get::<Name>(target)
        .map(|n| n.to_string())
        .unwrap_or_default();
    let title = world
        .get::<PlayerJob>(target)
        .and_then(|j| {
            Some(
                world
                    .get_resource::<Jobs>()?
                    .data
                    .jobs
                    .get(j.0)?
                    .title
                    .clone(),
            )
        })
        .unwrap_or_default();
    let title = job_ru(&title).map_or(title.clone(), str::to_owned);
    format!("{name}, {title}")
}

impl ObjectiveKind {
    /// `update_explanation_text` / `set_target`.
    pub fn explanation(&self, world: &World) -> String {
        match self {
            Self::Assassinate { target: Some(t) } => {
                format!("Убейте {}.", target_text(world, *t))
            }
            Self::Maroon { target: Some(t) } => format!(
                "Не дайте {}, эвакуироватся живым со станции.",
                target_text(world, *t)
            ),
            Self::Steal { item: Some(item) } => format!("Украдите {}", item.name),
            Self::Protect { target: Some(t) } => {
                format!("Защитите {}.", target_text(world, *t))
            }
            Self::Exile => "Оставайтесь в живых за пределами станции. Не улетайте на ЦК.".to_owned(),
            Self::Assassinate { target: None }
            | Self::Maroon { target: None }
            | Self::Protect { target: None }
            | Self::Steal { item: None } => FREE_OBJECTIVE.to_owned(),
            Self::Escape => {
                "Сбегите живым на эвакуационном шаттле или челноке, при этом не являясь в заключении."
                    .to_owned()
            }
            Self::Martyr => "Умрите славной смертью!".to_owned(),
            Self::ConvertBrother => "Пробудите цель без защиты разума, используя вашу вспышку \
                 непосредственно на цели. Любая портативная вспышка сработает, если вы потеряете или \
                 сломаете стартовую."
                .to_owned(),
            // The player's own text is the explanation.
            Self::Custom => String::new(),
            Self::Hijack => "Угоните эвакуационный шаттл, взломав его навигационные протоколы через \
                 консоль управления (Alt-Клик по консоли эвакуационного шаттла)."
                .to_owned(),
        }
    }
}

pub(crate) fn objective(world: &World, kind: ObjectiveKind) -> Objective {
    Objective {
        explanation: kind.explanation(world),
        kind,
    }
}

/// `find_target` of a kill-type objective: a living crew body that is not an owner and not
/// already the target of the same kind of objective of the owners.
pub(crate) fn find_target(
    world: &mut World,
    owners: &[Entity],
    existing: &[Objective],
    kind: fn(Option<Entity>) -> ObjectiveKind,
    rng: &mut Rng,
) -> Option<Entity> {
    let mut crew = world.query_filtered::<(Entity, &Stat), With<PlayerJob>>();
    let mut possible: Vec<Entity> = crew
        .iter(world)
        .filter(|(e, stat)| !owners.contains(e) && **stat != Stat::Dead)
        .map(|(e, _)| e)
        .filter(|e| !existing.iter().any(|o| o.same_target(&kind(Some(*e)))))
        .collect();
    possible.sort();
    (!possible.is_empty()).then(|| possible[rng.range(0, possible.len() as i64 - 1) as usize])
}

/// `/datum/objective/steal/find_target`: a target item that exists, that no owner's job
/// excludes and that the owners do not have to steal already.
pub(crate) fn find_steal_target(
    world: &mut World,
    owners: &[Entity],
    existing: &[Objective],
    targets: &[StealItemDef],
    rng: &mut Rng,
) -> Option<StealItemDef> {
    let titles: Vec<String> = owners
        .iter()
        .filter_map(|&owner| {
            let job = world.get::<PlayerJob>(owner)?.0;
            Some(
                world
                    .get_resource::<Jobs>()?
                    .data
                    .jobs
                    .get(job)?
                    .title
                    .clone(),
            )
        })
        .collect();
    let mut items = world.query::<&DmItem>();
    let present: Vec<&str> = items.iter(world).map(|dm| dm.path.as_str()).collect();
    let approved: Vec<&StealItemDef> = targets
        .iter()
        // shortcut: targets with their own completion, existence or `New()` rules (AI cards, the
        // supermatter, extraction kits) are left out until those mechanics exist.
        .filter(|t| !t.special)
        .filter(|t| !titles.iter().any(|title| t.excludefromjob.contains(title)))
        .filter(|t| !t.exists_on_map || present.iter().any(|p| is_under(p, &t.targetitem)))
        .filter(|t| {
            let kind = ObjectiveKind::Steal {
                item: Some((*t).clone()),
            };
            !existing.iter().any(|o| o.same_target(&kind))
        })
        .collect();
    (!approved.is_empty())
        .then(|| approved[rng.range(0, approved.len() as i64 - 1) as usize].clone())
}

/// `forge_single_generic_objective` (no job objectives: only the HoP has one in tg; no AI to
/// destroy).
fn forge_single(
    world: &mut World,
    owners: &[Entity],
    existing: &[Objective],
    targets: &[StealItemDef],
    rng: &mut Rng,
) -> Objective {
    if rng.prob(KILL_PROB) {
        if rng.prob(MAROON_PROB) {
            let target = find_target(world, owners, existing, maroon, rng);
            return objective(world, maroon(target));
        }
        let target = find_target(world, owners, existing, assassinate, rng);
        return objective(world, assassinate(target));
    }
    let item = find_steal_target(world, owners, existing, targets, rng);
    objective(world, ObjectiveKind::Steal { item })
}

fn assassinate(target: Option<Entity>) -> ObjectiveKind {
    ObjectiveKind::Assassinate { target }
}

pub(crate) fn protect(target: Option<Entity>) -> ObjectiveKind {
    ObjectiveKind::Protect { target }
}

fn maroon(target: Option<Entity>) -> ObjectiveKind {
    ObjectiveKind::Maroon { target }
}

/// `forge_traitor_objectives` + `forge_ending_objective`: the objectives and the ending one
/// (escape, martyr or, for a hijacker, hijack, which tg keeps out of the list).
pub fn forge_traitor_objectives(
    world: &mut World,
    owner: Entity,
    targets: &[StealItemDef],
    rng: &mut Rng,
) -> (Vec<Objective>, ObjectiveKind) {
    let joined = world
        .query_filtered::<(), With<PlayerJob>>()
        .iter(world)
        .count();
    let hijacker = joined >= HIJACK_MIN_PLAYERS && rng.prob(HIJACK_PROB);
    let mut objectives = Vec::new();
    for _ in usize::from(hijacker)..TRAITOR_OBJECTIVES_AMOUNT {
        let generated = forge_single(world, &[owner], &objectives, targets, rng);
        objectives.push(generated);
    }
    if hijacker {
        return (objectives, ObjectiveKind::Hijack);
    }
    let martyr_ok = objectives.iter().all(Objective::martyr_compatible);
    let ending = if martyr_ok && rng.prob(MARTYR_PROB) {
        ObjectiveKind::Martyr
    } else {
        ObjectiveKind::Escape
    };
    objectives.push(objective(world, ending.clone()));
    (objectives, ending)
}

/// `/datum/team/brother_team/forge_brother_objectives` for the team's `owners` (its members when
/// the first one joins): convert a brother, `BROTHER_OBJECTIVES_AMOUNT` kill or steal objectives
/// (one more for a team that may convert two, one less for a hijacker, at least one), then
/// hijack (10%) or escape.
// shortcut: no `/datum/objective/destroy` (there are no AIs to destroy yet).
pub fn forge_brother_objectives(
    world: &mut World,
    owners: &[Entity],
    brothers_left: u32,
    targets: &[StealItemDef],
    rng: &mut Rng,
) -> Vec<Objective> {
    let mut objectives = vec![objective(world, ObjectiveKind::ConvertBrother)];
    let hijacker = rng.prob(HIJACK_PROB);
    let count = (BROTHER_OBJECTIVES_AMOUNT + usize::from(brothers_left > 2))
        .saturating_sub(usize::from(hijacker))
        .max(1);
    for _ in 0..count {
        let generated = forge_single(world, owners, &objectives, targets, rng);
        objectives.push(generated);
    }
    let ending = if hijacker {
        ObjectiveKind::Hijack
    } else {
        ObjectiveKind::Escape
    };
    objectives.push(objective(world, ending));
    objectives
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::CarriedBy;
    use crate::storage::StoredIn;

    fn steal_def(target: &str) -> StealItemDef {
        StealItemDef {
            path: "/datum/objective_item/steal/caplaser".into(),
            name: "капитанское антикварное лазерное оружие".into(),
            targetitem: target.into(),
            exists_on_map: true,
            ..default()
        }
    }

    fn item(world: &mut World, path: &str) -> Entity {
        world
            .spawn(DmItem {
                path: path.into(),
                ..default()
            })
            .id()
    }

    #[test]
    fn kill_objectives_complete_when_the_target_is_dead_or_gone() {
        let mut world = World::new();
        let owner = world.spawn((Stat::Conscious, PlayerJob(0))).id();
        let target = world
            .spawn((Stat::Conscious, PlayerJob(0), Name::new("Боб")))
            .id();
        let kill = Objective {
            kind: ObjectiveKind::Assassinate {
                target: Some(target),
            },
            explanation: String::new(),
        };
        let end = RoundEnd::default();
        assert!(!kill.check_completion(&world, &[owner], end));
        world.entity_mut(target).insert(Stat::SoftCrit);
        assert!(!kill.check_completion(&world, &[owner], end));
        world.entity_mut(target).insert(Stat::Dead);
        assert!(kill.check_completion(&world, &[owner], end));
        world.despawn(target);
        assert!(kill.check_completion(&world, &[owner], end));
        // A free objective is always complete.
        let free = objective(&world, assassinate(None));
        assert_eq!(free.explanation, FREE_OBJECTIVE);
        assert!(free.check_completion(&world, &[owner], end));
    }

    #[test]
    fn steal_needs_the_item_anywhere_on_the_owner() {
        let mut world = World::new();
        let owner = world.spawn((Stat::Conscious, Inventory::default())).id();
        let steal = Objective {
            kind: ObjectiveKind::Steal {
                item: Some(steal_def("/obj/item/gun/energy/laser/captain")),
            },
            explanation: String::new(),
        };
        let end = RoundEnd::default();
        assert!(!steal.check_completion(&world, &[owner], end));
        // In a box in the backpack: found (a subtype counts, as `istype`).
        let pack = item(&mut world, "/obj/item/storage/backpack");
        let boxed = item(&mut world, "/obj/item/storage/box");
        let laser = item(&mut world, "/obj/item/gun/energy/laser/captain/scattershot");
        world.entity_mut(pack).insert(CarriedBy(owner));
        world.entity_mut(boxed).insert(StoredIn(pack));
        world.entity_mut(laser).insert(StoredIn(boxed));
        world.get_mut::<Inventory>(owner).unwrap().back = Some(pack);
        assert!(steal.check_completion(&world, &[owner], end));
        // Dropped: not any more.
        world.entity_mut(laser).remove::<StoredIn>();
        assert!(!steal.check_completion(&world, &[owner], end));
    }

    #[test]
    fn escape_and_martyr_follow_the_owner() {
        let mut world = World::new();
        let owner = world.spawn(Stat::Conscious).id();
        let escape = objective(&world, ObjectiveKind::Escape);
        let martyr = objective(&world, ObjectiveKind::Martyr);
        let forced = RoundEnd { force_ending: true };
        assert!(escape.check_completion(&world, &[owner], forced));
        assert!(!escape.check_completion(&world, &[owner], RoundEnd::default()));
        assert!(!martyr.check_completion(&world, &[owner], forced));
        world.entity_mut(owner).insert(Stat::Dead);
        assert!(!escape.check_completion(&world, &[owner], forced));
        assert!(martyr.check_completion(&world, &[owner], forced));
        assert!(!objective(&world, ObjectiveKind::Hijack).check_completion(
            &world,
            &[owner],
            forced
        ));
    }

    #[test]
    fn forging_picks_real_targets_and_an_ending() {
        let mut world = World::new();
        world.insert_resource(Jobs::new(crate::jobs::fallback_jobs(), 0));
        let owner = world
            .spawn((Stat::Conscious, PlayerJob(0), Name::new("Агент")))
            .id();
        let crew = world
            .spawn((Stat::Conscious, PlayerJob(0), Name::new("Боб")))
            .id();
        world.spawn((Stat::Dead, PlayerJob(0), Name::new("Труп")));
        item(&mut world, "/obj/item/gun/energy/laser/captain");
        let targets = vec![
            steal_def("/obj/item/gun/energy/laser/captain"),
            // Not on the map: never picked.
            steal_def("/obj/item/hand_tele"),
        ];
        for seed in 0..40 {
            let mut rng = Rng::new(seed);
            let (objectives, ending) =
                forge_traitor_objectives(&mut world, owner, &targets, &mut rng);
            assert_eq!(objectives.len(), TRAITOR_OBJECTIVES_AMOUNT + 1);
            assert_eq!(objectives.last().unwrap().kind, ending);
            assert!(matches!(
                ending,
                ObjectiveKind::Escape | ObjectiveKind::Martyr
            ));
            for o in &objectives[..TRAITOR_OBJECTIVES_AMOUNT] {
                match &o.kind {
                    // The only living crew member, at most once per kind.
                    ObjectiveKind::Assassinate { target } | ObjectiveKind::Maroon { target } => {
                        assert!(target.is_none() || *target == Some(crew));
                    }
                    ObjectiveKind::Steal { item } => {
                        assert!(item.as_ref().is_none_or(|i| {
                            i.targetitem == "/obj/item/gun/energy/laser/captain"
                        }))
                    }
                    other => panic!("{other:?}"),
                }
            }
            let kills = objectives
                .iter()
                .filter(|o| o.kind == assassinate(Some(crew)))
                .count();
            assert!(kills <= 1);
            if ending == ObjectiveKind::Martyr {
                assert!(objectives[..2].iter().all(Objective::martyr_compatible));
            }
        }
        let kill = objective(&world, assassinate(Some(crew)));
        let assistant = job_ru("Assistant").unwrap_or("Assistant");
        assert_eq!(kill.explanation, format!("Убейте Боб, {assistant}."));
    }
}

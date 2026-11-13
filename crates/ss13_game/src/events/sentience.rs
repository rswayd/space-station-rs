//! Random / Station-wide Human-level Intelligence (`code/modules/events/ghost_role/sentience.dm`,
//! `_ghost_role.dm`): the ghosts are polled ("random animal"); each volunteer takes one mindless
//! living basic mob on the station levels (the starter pets and farm animals of
//! [`HIGH_PRIORITY`] first, the rest after, each group shuffled). The mob understands every
//! language, no longer vanishes on death (`DEL_ON_DEATH` off), gets at least 200 max health,
//! fully healed, and is told it gained human-level intelligence. The station-wide variant
//! (weight 0: admins only) does it for as many mobs as there are volunteers.
//!
//! The announcement ("...обрел(и) уровень интеллекта...") comes only after a successful spawn
//! (tg caches `announce_chance` until the poll ends); a false alarm announces it at once.
// shortcut: simple animals (megafauna) are not candidates; with no mob to take the event fails
// before polling (tg polls first, then retries three times, 30/60/120 s later); no job bans,
// deadchat broadcast or ghost notification of the new mobs.

use std::sync::Arc;

use bevy::prelude::*;
use ss13_chat::language::{LanguageId, LanguageSource, UNDERSTOOD, edit_holder};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::controller::{EventRng, EventState, RoundEvent, station_levels};
use crate::ghost::Ghost;
use crate::ghost_poll::{self, Candidate, DEFAULT_POLL_SECS, PollSpec};
use crate::health::{Health, Stat};
use crate::mobs::BasicMob;
use crate::server::PlayerMob;
use crate::tools::tell;

pub const NAME: &str = "Random Human-level Intelligence";
pub const NAME_ALL: &str = "Station-wide Human-level Intelligence";
const ROLE_NAME: &str = "random animal";
/// `selected.maxHealth = max(selected.maxHealth, 200)`.
const MIN_MAX_HEALTH: f32 = 200.0;

/// `GLOB.high_priority_sentience`.
pub const HIGH_PRIORITY: [&str; 21] = [
    "/mob/living/basic/bat",
    "/mob/living/basic/butterfly",
    "/mob/living/basic/carp/pet/cayenne",
    "/mob/living/basic/chicken",
    "/mob/living/basic/crab",
    "/mob/living/basic/cow",
    "/mob/living/basic/goat",
    "/mob/living/basic/goose/vomit",
    "/mob/living/basic/lizard",
    "/mob/living/basic/mouse/brown/tom",
    "/mob/living/basic/parrot",
    "/mob/living/basic/pet",
    "/mob/living/basic/pig",
    "/mob/living/basic/rabbit",
    "/mob/living/basic/sheep",
    "/mob/living/basic/sloth",
    "/mob/living/basic/snake",
    "/mob/living/basic/spider/giant/sgt_araneus",
    "/mob/living/basic/bot/secbot/beepsky",
    "/mob/living/basic/bear/snow/misha",
    "/mob/living/basic/mining/lobstrosity/juvenile",
];

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Sentience {
        animals: 1,
        one: "кто-то",
    })
}

pub fn make_all(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Sentience {
        animals: usize::MAX,
        one: "все",
    })
}

/// `/datum/round_event/ghost_role/sentience`.
struct Sentience {
    animals: usize,
    one: &'static str,
}

impl RoundEvent for Sentience {
    fn announce(&mut self, world: &mut World, _: &mut EventState, _fake: bool) {
        announce(world, self.one);
    }

    fn start(&mut self, world: &mut World, ev: &mut EventState) {
        // Announced by the poll's end when it worked (`cached_announcement_chance`).
        let chance = ev.announce_chance;
        ev.announce_chance = 0.0;
        if potential(world).is_empty() {
            info!("{ROLE_NAME}: no mindless animal on the station");
            return;
        }
        let (animals, one) = (self.animals, self.one);
        let spec = PollSpec {
            question: format!("Хотите стать ролью: {ROLE_NAME}?"),
            role_name: ROLE_NAME.to_owned(),
            secs: DEFAULT_POLL_SECS,
            ..default()
        };
        let group = ghost_poll::ghosts(world);
        ghost_poll::poll_candidates(world, spec, group, move |world, signed_up| {
            if spawn_role(world, signed_up, animals) == 0 {
                info!("{ROLE_NAME}: not enough candidates or animals");
                return;
            }
            if world.resource_mut::<EventRng>().0.prob(chance) {
                announce(world, one);
            }
        });
    }
}

fn announce(world: &mut World, one: &str) {
    let text = {
        let mut rng = world.resource_mut::<EventRng>();
        let data = *rng
            .pick(&[
                "данных со сканеров дальнего действия",
                "наших сложных вероятностных моделях",
                "коммуникационном трафике на вашей станции",
                "обнаруженом всплеске энергии",
                "[ДАННЫЕ УДАЛЕНЫ]",
            ])
            .expect("not empty");
        let pets = *rng
            .pick(&[
                "животных/ботов",
                "ботов/животных",
                "питомцев",
                "простых животных",
                "низших форм жизни",
                "[ДАННЫЕ УДАЛЕНЫ]",
            ])
            .expect("not empty");
        let strength = *rng
            .pick(&[
                "человека",
                "ящеров",
                "офицеров службы безопасности",
                "командования",
                "клоуна",
                "[ДАННЫЕ УДАЛЕНЫ]",
            ])
            .expect("not empty");
        format!(
            "Основаваясь на {data}, мы думаем что {one} из {pets} обрел(и) уровень интеллекта как у \
         {strength}, и возможность общаться."
        )
    };
    let title = format!("{}: Сообщение со средним приоритетом", super::COMMAND_NAME);
    super::priority_announce(world, &title, &text);
}

pub fn high_priority(path: &str) -> bool {
    HIGH_PRIORITY
        .iter()
        .any(|root| ss13_mapload::fidelity::is_under(path, root))
}

/// tg `set_mob_priority` over the living basic mobs: on a station level, no player or mind, not
/// a hologram; the high-priority ones first, each group shuffled.
fn potential(world: &mut World) -> Vec<Entity> {
    let levels = station_levels(world);
    let bodies: Vec<Entity> = world
        .query::<&Ghost>()
        .iter(world)
        .filter_map(|g| g.body)
        .collect();
    let mut query = world.query_filtered::<(Entity, &BasicMob, &TilePos, &Stat), (
        Without<PlayerMob>,
        Without<crate::holodeck::Hologram>,
    )>();
    let mut mobs: Vec<(Entity, bool)> = query
        .iter(world)
        .filter(|(e, _, pos, stat)| {
            **stat != Stat::Dead && levels.contains(&pos.z) && !bodies.contains(e)
        })
        .map(|(e, mob, _, _)| (e, high_priority(&mob.def.path)))
        .collect();
    mobs.sort_by_key(|(e, _)| *e);
    let (mut hi, mut low): (Vec<_>, Vec<_>) = mobs.into_iter().partition(|(_, hi)| *hi);
    super::wizard::shuffle(world, &mut hi);
    super::wizard::shuffle(world, &mut low);
    hi.into_iter().chain(low).map(|(e, _)| e).collect()
}

/// `spawn_role`: up to `animals` volunteers each take a mob; the number taken.
fn spawn_role(world: &mut World, mut candidates: Vec<Candidate>, animals: usize) -> usize {
    // Checked again now: the poll took its time.
    let mut potential = potential(world).into_iter();
    let mut spawned = 0;
    while spawned < animals {
        let Some(mob) = potential.next() else {
            break;
        };
        let Some(chosen) = ghost_poll::pick_candidate(world, &mut candidates) else {
            break;
        };
        if !crate::medical::brainmob::possess(world, chosen.client, chosen.mob, mob) {
            continue;
        }
        world.despawn(chosen.mob);
        make_sentient(world, mob);
        tell(world, chosen.client, Span::BoldWarning, "Hello world!");
        tell(
            world,
            chosen.client,
            Span::Warning,
            "Due to freak radiation and/or chemicals and/or lucky chance, you have gained human \
             level intelligence and the ability to speak and understand human language!",
        );
        info!("{ROLE_NAME}: client {} took {mob}", chosen.client);
        spawned += 1;
    }
    spawned
}

/// What the event gives the chosen mob.
pub fn make_sentient(world: &mut World, mob: Entity) {
    edit_holder(world, mob, |holder| {
        for language in LanguageId::ALL {
            holder.grant(language, UNDERSTOOD, LanguageSource::Atom);
        }
    });
    if let Some(mut basic) = world.get_mut::<BasicMob>(mob) {
        let mut def = (*basic.def).clone();
        def.basic_mob_flags &= !crate::mobs::DEL_ON_DEATH;
        basic.def = Arc::new(def);
    }
    if let Some(health) = world.get::<Health>(mob) {
        let mut health = health.clone();
        health.max = health.max.max(MIN_MAX_HEALTH);
        let health = health.healed();
        world.entity_mut(mob).insert(health);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_pets_come_first() {
        assert!(high_priority("/mob/living/basic/pet/dog/corgi/ian"));
        assert!(high_priority("/mob/living/basic/bot/secbot/beepsky"));
        assert!(!high_priority("/mob/living/basic/bot/secbot"));
        assert!(!high_priority("/mob/living/basic/carp"));
    }

    #[test]
    fn a_sentient_mob_survives_death_and_understands_all() {
        let mut world = World::new();
        let def = ss13_mapload::mobs::MobDef {
            path: "/mob/living/basic/mouse".into(),
            basic_mob_flags: crate::mobs::DEL_ON_DEATH,
            ..default()
        };
        let mob = world
            .spawn((
                BasicMob {
                    def: Arc::new(def),
                    icon_living: String::new(),
                    icon_dead: String::new(),
                    mouth: false,
                },
                Health::basic(5.0, [1.0; 5]),
            ))
            .id();
        make_sentient(&mut world, mob);
        let basic = world.get::<BasicMob>(mob).unwrap();
        assert_eq!(basic.def.basic_mob_flags & crate::mobs::DEL_ON_DEATH, 0);
        let health = world.get::<Health>(mob).unwrap();
        assert_eq!(health.max, 200.0);
        assert_eq!(health.health(), 200.0);
        let holder = world
            .get::<ss13_chat::language::LanguageHolder>(mob)
            .unwrap();
        assert!(holder.has(LanguageId::ALL[0], UNDERSTOOD));
    }
}

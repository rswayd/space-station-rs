//! Summon Events' wizard events (`code/modules/events/wizard/`) on ported systems. They roll only
//! in wizard mode ([`super::Events::wizardmode`]); admins can force them.
//!
//! - Robeless Casting (`aid.dm`): every known spell stops needing the robe and hat.
//! - Improved Casting (`aid.dm`): every spell gains a level, one past its cap at most.
//! - Change Places! (`shuffle.dm` `shuffleloc`): the living humans on the station levels swap
//!   places at random; a puff of smoke on every living human.
//! - Change Faces! (`shuffle.dm` `shufflenames`): the living humans' names are shuffled among
//!   them; smoke as above.
//! - Change Minds! (`shuffle.dm` `shuffleminds`): the conscious living humans with a mind, the
//!   wizards spared, swap minds in pairs; smoke as above.
//! - Tower of Babel (`tower_of_babel.dm`, `spells/spell_types/tower_of_babel.dm`, the
//!   `tower_of_babel/magical` status effect): every player's carbon body on the station levels
//!   speaks and understands one random language only ([`curse_of_babel`]), its tongue no limit;
//!   antimagic of the mind protects; latejoiners are cursed too ([`TowerOfBabel`]). Wizards
//!   learn every language instead.
//! - Summon Guns (`summons.dm`): `summon_guns` with a 10% survivalist chance
//!   (`crate::antag::survivalist`).
//!
//! Summon Events itself is the spellbook ritual (`crate::antag::wizard::server`), calling
//! [`super::controller::summon_events`].
// shortcut: the teleport ignores antimagic and teleport-blocking areas (tg `do_teleport` magic
// channel) and things in lockers; antagonist datums, mind languages and traits stay with the
// body when minds swap (ss13-rs keeps them on bodies), no `TRAIT_NO_MINDSWAP`; the Tower of
// Babel has no deadchat broadcast, admin undo or status alert, and its curse is kept by the body.

use bevy::prelude::*;
use ss13_chat::language::{self, LanguageId, LanguageSource, edit_holder};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::controller::{EventRng, EventState, RoundEvent, station_levels};
use crate::health::Stat;
use crate::medical::brainmob::{Mind, mind_of};
use crate::server::PlayerMob;
use crate::spells::{SPELL_REQUIRES_WIZARD_GARB, Spells};

pub const ROBELESS: &str = "Robeless Casting";
pub const IMPROVED: &str = "Improved Casting";
pub const SHUFFLE_PLACES: &str = "Change Places!";
pub const SHUFFLE_NAMES: &str = "Change Faces!";
pub const SHUFFLE_MINDS: &str = "Change Minds!";
pub const BABEL: &str = "Tower of Babel";
pub const SUMMON_GUNS: &str = "Summon Guns";

pub fn make_robeless(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Robeless)
}

pub fn make_improved(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Improved)
}

pub fn make_shuffle_places(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(ShufflePlaces)
}

pub fn make_shuffle_names(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(ShuffleNames)
}

pub fn make_shuffle_minds(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(ShuffleMinds)
}

pub fn make_babel(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Babel)
}

pub fn make_summon_guns(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(SummonGuns)
}

struct SummonGuns;

impl RoundEvent for SummonGuns {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        crate::antag::survivalist::summon_guns(world, None, 10.0);
    }
}

/// tg `IS_WIZARD` (apprentices too).
fn is_wizard(world: &World, mob: Entity) -> bool {
    world
        .get::<crate::antag::wizard::server::Wizard>(mob)
        .is_some()
}

fn casters(world: &mut World) -> Vec<Entity> {
    let mut query = world.query_filtered::<Entity, With<Spells>>();
    let mut all: Vec<Entity> = query.iter(world).collect();
    all.sort();
    all
}

fn tell(world: &mut World, mob: Entity, text: &str) {
    crate::medical::diseases::say_to(world, mob, Span::Notice, text);
}

struct Robeless;

impl RoundEvent for Robeless {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        for mob in casters(world) {
            let mut improved = false;
            if let Some(mut spells) = world.get_mut::<Spells>(mob) {
                for known in &mut spells.known {
                    if known.spell.def().requirements & SPELL_REQUIRES_WIZARD_GARB != 0
                        && !known.robeless
                    {
                        known.robeless = true;
                        improved = true;
                    }
                }
            }
            if improved {
                tell(
                    world,
                    mob,
                    "You suddenly feel like you never needed those garish robes in the first \
                     place...",
                );
            }
        }
    }
}

/// `level_spell(bypass_cap = TRUE)` once, unless Improved Casting already took it past its cap.
pub fn improve(known: &mut crate::spells::KnownSpell) -> bool {
    let max = known.spell.def().max_level;
    if max <= 1 || known.level > max {
        return false;
    }
    known.level += 1;
    true
}

struct Improved;

impl RoundEvent for Improved {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        for mob in casters(world) {
            // tg keeps only the last spell's result; any upgrade counts here.
            let mut upgraded = false;
            if let Some(mut spells) = world.get_mut::<Spells>(mob) {
                for known in &mut spells.known {
                    upgraded |= improve(known);
                }
            }
            if upgraded {
                crate::spells::refresh(world, mob);
                tell(
                    world,
                    mob,
                    "You suddenly feel more competent with your casting!",
                );
            }
        }
    }
}

/// The living humans (`GLOB.alive_mob_list` humans), with their tile when on one.
fn living_humans(world: &mut World) -> Vec<(Entity, Option<TilePos>)> {
    let mut query =
        world.query_filtered::<(Entity, &Stat, Option<&TilePos>), With<crate::human::HumanLook>>();
    let mut all: Vec<(Entity, Option<TilePos>)> = query
        .iter(world)
        .filter(|(_, stat, _)| **stat != Stat::Dead)
        .map(|(e, _, pos)| (e, pos.copied()))
        .collect();
    all.sort_by_key(|(e, _)| *e);
    all
}

fn smoke_everyone(world: &mut World) {
    for pos in living_humans(world).into_iter().filter_map(|(_, pos)| pos) {
        crate::grenades::do_smoke(world, pos, 0);
    }
}

/// DM `shuffle_inplace` (Fisher-Yates) with the event stream.
fn shuffle<T>(world: &mut World, items: &mut [T]) {
    let mut rng = world.resource_mut::<EventRng>();
    for i in (1..items.len()).rev() {
        let j = rng.0.range(0, i as i64) as usize;
        items.swap(i, j);
    }
}

struct ShufflePlaces;

impl RoundEvent for ShufflePlaces {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let levels = station_levels(world);
        let (mut mobs, mut locs): (Vec<Entity>, Vec<TilePos>) = living_humans(world)
            .into_iter()
            .filter_map(|(mob, pos)| pos.filter(|p| levels.contains(&p.z)).map(|p| (mob, p)))
            .unzip();
        if mobs.is_empty() {
            return;
        }
        shuffle(world, &mut locs);
        shuffle(world, &mut mobs);
        for mob in mobs {
            let Some(dest) = locs.pop() else {
                break;
            };
            world.entity_mut(mob).insert(dest);
        }
        smoke_everyone(world);
    }
}

struct ShuffleNames;

impl RoundEvent for ShuffleNames {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let mut mobs: Vec<Entity> = living_humans(world).into_iter().map(|(e, _)| e).collect();
        if mobs.is_empty() {
            return;
        }
        let mut names: Vec<String> = mobs
            .iter()
            .map(|&mob| crate::medical::name_of(world, mob))
            .collect();
        shuffle(world, &mut names);
        shuffle(world, &mut mobs);
        for mob in mobs {
            let Some(name) = names.pop() else {
                break;
            };
            world.entity_mut(mob).insert(Name::new(name));
        }
        smoke_everyone(world);
    }
}

struct ShuffleMinds;

impl RoundEvent for ShuffleMinds {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let mut mobs = Vec::new();
        for (mob, _) in living_humans(world) {
            // `IS_UNCONSCIOUS_OR_CRIT`, no mind, the wizards; and nowhere to be put.
            if world.get::<Stat>(mob) != Some(&Stat::Conscious)
                || is_wizard(world, mob)
                || crate::explosions::turf_of(world, mob).is_none()
                || mind_of(world, mob).is_none()
            {
                continue;
            }
            mobs.push(mob);
        }
        if mobs.is_empty() {
            return;
        }
        shuffle(world, &mut mobs);
        while mobs.len() > 1 {
            let (Some(a), Some(b)) = (mobs.pop(), mobs.pop()) else {
                break;
            };
            swap_minds(world, a, b);
        }
        smoke_everyone(world);
    }
}

/// `mind_transfer/swap_minds`: the mind of `a` goes to `b` and the other way round.
pub fn swap_minds(world: &mut World, a: Entity, b: Entity) {
    let (mind_a, mind_b) = (mind_of(world, a), mind_of(world, b));
    let set_ghost = |world: &mut World, ghost: Entity, body: Entity| {
        if let Some(mut g) = world.get_mut::<crate::ghost::Ghost>(ghost) {
            g.body = Some(body);
        }
    };
    match mind_a {
        Some(Mind::Client(client)) => {
            crate::medical::brainmob::possess(world, client, a, b);
        }
        Some(Mind::Ghost(ghost)) => set_ghost(world, ghost, b),
        None => {}
    }
    match mind_b {
        Some(Mind::Client(client)) => {
            // `b` already holds the client of `a`: leave from a stand-in instead.
            let stand_in =
                matches!(mind_a, Some(Mind::Client(_))).then(|| world.spawn_empty().id());
            crate::medical::brainmob::possess(world, client, stand_in.unwrap_or(b), a);
            if let Some(e) = stand_in {
                world.despawn(e);
            }
        }
        Some(Mind::Ghost(ghost)) => set_ghost(world, ghost, a),
        None => {}
    }
}

/// `GLOB.tower_of_babel`: while it stands, crew joining is cursed.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct TowerOfBabel;

/// `/datum/mood_event/tower_of_babel` (BandaStation's text).
const BABEL_MOOD: crate::mood::MoodEvent = crate::mood::MoodEvent {
    id: "tower_of_babel",
    description: "Мой язык - это новый Вавилон, где никто ничего не понимает...",
    mood_change: -1.0,
    timeout: 15.0,
    hidden: false,
};

struct Babel;

impl RoundEvent for Babel {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        world.insert_resource(TowerOfBabel);
        let levels = station_levels(world);
        let mut players: Vec<Entity> = world
            .query_filtered::<Entity, (With<PlayerMob>, With<crate::human::HumanLook>)>()
            .iter(world)
            .collect();
        players.sort();
        for mob in players {
            if is_wizard(world, mob) {
                // Immune, and they speak every language to taunt their victims over the radio.
                edit_holder(world, mob, |h| {
                    h.grant_all(LanguageSource::Babel);
                    h.babel = true;
                });
                tell_babel(
                    world,
                    mob,
                    "You feel a magical force improving your speech patterns!",
                );
                continue;
            }
            if world.get::<Stat>(mob) == Some(&Stat::Dead) {
                continue;
            }
            if crate::explosions::turf_of(world, mob).is_some_and(|p| !levels.contains(&p.z)) {
                continue;
            }
            curse_of_babel(world, mob);
        }
    }
}

fn tell_babel(world: &mut World, mob: Entity, text: &str) {
    crate::medical::diseases::say_to(world, mob, Span::Hypnophrase, text);
}

/// tg `curse_of_babel(mob)` and the `tower_of_babel/magical` status effect's `on_apply`: one
/// random language (`pick(GLOB.all_languages)`), every other one blocked, the tongue no limit
/// (`TRAIT_TOWER_OF_BABEL`). True when cursed.
pub fn curse_of_babel(world: &mut World, mob: Entity) -> bool {
    // Silicons are immune (`iscarbon`).
    if world.get::<crate::human::HumanLook>(mob).is_none() {
        return false;
    }
    let immune = world
        .get::<ss13_chat::language::LanguageHolder>(mob)
        .is_some_and(|h| h.babel);
    if immune
        || crate::spells::can_block_magic(
            world,
            mob,
            crate::spells::MAGIC_RESISTANCE | crate::spells::MAGIC_RESISTANCE_MIND,
        )
    {
        crate::medical::diseases::say_to(
            world,
            mob,
            Span::Notice,
            "You have a strange feeling for a moment, but then it passes.",
        );
        return false;
    }
    let Some(&picked) = world.resource_mut::<EventRng>().pick(&LanguageId::ALL) else {
        return false;
    };
    edit_holder(world, mob, |h| {
        h.grant(picked, language::ALL, LanguageSource::Babel);
        for other in LanguageId::ALL.into_iter().filter(|&l| l != picked) {
            h.block(other, language::ALL, LanguageSource::Babel);
        }
        h.babel = true;
    });
    crate::mood::add_event(world, mob, "tower_of_babel_magic", &BABEL_MOOD);
    crate::medical::diseases::emote(world, mob, "mumble");
    let pos = crate::explosions::turf_of(world, mob);
    crate::hallucination::play_local(
        world,
        mob,
        pos,
        "sound/effects/magic/magic_block_mind.ogg",
        75.0,
        true,
    );
    tell_babel(
        world,
        mob,
        "You feel a magical force affecting your speech patterns!",
    );
    info!("{mob} is cursed by the Tower of Babel: {picked:?}");
    true
}

/// `COMSIG_GLOB_CREWMEMBER_JOINED` while the tower stands: the new crew is cursed.
pub(super) fn curse_joiners(
    tower: Option<Res<TowerOfBabel>>,
    joined: Query<Entity, Added<crate::jobs::PlayerJob>>,
    mut commands: Commands,
) {
    if tower.is_none() {
        return;
    }
    for mob in &joined {
        commands.queue(move |world: &mut World| {
            curse_of_babel(world, mob);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spells::{KnownSpell, Spell};

    #[test]
    fn swap_minds_trades_clients_and_ghosts() {
        let mut world = World::new();
        let (ca, cb) = (world.spawn_empty().id(), world.spawn_empty().id());
        let a = world
            .spawn((TilePos::new(1, 1, 1), PlayerMob { client: ca }))
            .id();
        let b = world
            .spawn((TilePos::new(2, 1, 1), PlayerMob { client: cb }))
            .id();
        swap_minds(&mut world, a, b);
        assert_eq!(world.get::<PlayerMob>(a).map(|p| p.client), Some(cb));
        assert_eq!(world.get::<PlayerMob>(b).map(|p| p.client), Some(ca));
        assert_eq!(
            world
                .get::<ss13_net::server::ControlledEntity>(ca)
                .map(|c| c.0),
            Some(b)
        );
        // A body whose player ghosted: the ghost now belongs to the other body.
        world.entity_mut(b).remove::<PlayerMob>();
        let ghost = world.spawn(crate::ghost::Ghost { body: Some(b) }).id();
        swap_minds(&mut world, a, b);
        assert_eq!(
            world.get::<crate::ghost::Ghost>(ghost).and_then(|g| g.body),
            Some(a)
        );
        assert_eq!(world.get::<PlayerMob>(b).map(|p| p.client), Some(cb));
        assert!(world.get::<PlayerMob>(a).is_none());
    }

    #[test]
    fn improved_casting_goes_one_past_the_cap() {
        let spell = Spell::SummonItem;
        let max = spell.def().max_level;
        let mut known = KnownSpell {
            spell,
            level: 1,
            ready_at: 0.0,
            marked: None,
            hand: None,
            robeless: false,
        };
        // Instant Summons has one level only: never levelled.
        assert_eq!(max, 1);
        assert!(!improve(&mut known));
        let leveled = crate::spells::SPELLS
            .iter()
            .find(|d| d.max_level >= 2)
            .expect("a levelled spell");
        known.spell = leveled.spell;
        known.level = leveled.max_level;
        assert!(improve(&mut known), "past the cap once");
        assert_eq!(known.level, leveled.max_level + 1);
        assert!(!improve(&mut known), "but no further");
    }
}

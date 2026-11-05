//! Summon Events' wizard events (`code/modules/events/wizard/`) on ported systems. They roll only
//! in wizard mode ([`super::Events::wizardmode`]); admins can force them.
//!
//! - Robeless Casting (`aid.dm`): every known spell stops needing the robe and hat.
//! - Improved Casting (`aid.dm`): every spell gains a level, one past its cap at most.
//! - Change Places! (`shuffle.dm` `shuffleloc`): the living humans on the station levels swap
//!   places at random; a puff of smoke on every living human.
//! - Change Faces! (`shuffle.dm` `shufflenames`): the living humans' names are shuffled among
//!   them; smoke as above.
// shortcut: Summon Events itself (the wizard's spellbook entry that turns wizard mode on) is not
// ported; the teleport ignores antimagic and teleport-blocking areas (tg `do_teleport` magic
// channel) and things in lockers.

use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::controller::{EventRng, EventState, RoundEvent, station_levels};
use crate::health::Stat;
use crate::spells::{SPELL_REQUIRES_WIZARD_GARB, Spells};

pub const ROBELESS: &str = "Robeless Casting";
pub const IMPROVED: &str = "Improved Casting";
pub const SHUFFLE_PLACES: &str = "Change Places!";
pub const SHUFFLE_NAMES: &str = "Change Faces!";

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spells::{KnownSpell, Spell};

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

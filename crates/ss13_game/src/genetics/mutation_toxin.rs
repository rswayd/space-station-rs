//! Mutation toxins (`/datum/reagent/mutationtoxin`, `code/modules/reagents/chemistry/reagents/
//! other_reagents.dm`, and BandaStation's `modular_bandastation/species/code/reagents/
//! mutation.dm`): in the blood of a human they warn now and then and after 20 cycles turn it
//! into their species and are gone. The imperfect one (`jelly`) picks a jellyperson subspecies,
//! at once on a slime body. The slime extract recipes that make them come with the reaction
//! data (`ss13_reagents`).

use bevy::prelude::*;
use ss13_reagents::Chemistry;
use ss13_reagents::metabolism::Metabolized;
use ss13_shared::chat::Span;

use crate::combat::CombatRng;
use crate::species::{SpeciesId, biotype, def_of, set_species};

/// `CYCLES_TO_TURN`, `CYCLES_MSG_IMMEDIATE`, `CYCLES_MSG_EXTENDED`.
const CYCLES_TO_TURN: u32 = 20;
const CYCLES_MSG_IMMEDIATE: u32 = 6;
const CYCLES_MSG_EXTENDED: u32 = 16;
/// `affected_biotype`.
const AFFECTED: u16 = biotype::ORGANIC | biotype::SKELETAL | biotype::UNDEAD;
const JELLY: &str = "/datum/reagent/mutationtoxin/jelly";

/// `mutationtexts` by `MUT_MSG_IMMEDIATE`, `MUT_MSG_EXTENDED`, `MUT_MSG_ABOUT2TURN`.
const IMMEDIATE: [&str; 2] = [
    "You don't feel very well.",
    "Your skin feels a bit abnormal.",
];
const EXTENDED: [&str; 2] = [
    "Your limbs begin to take on a different shape.",
    "Your appendages begin morphing.",
];
const ABOUT2TURN: [&str; 1] = ["You feel as though you're about to change at any moment!"];

/// Each toxin's `race`.
const RACES: [(&str, SpeciesId); 22] = [
    ("/datum/reagent/mutationtoxin", SpeciesId::Human),
    ("/datum/reagent/mutationtoxin/classic", SpeciesId::Slime),
    ("/datum/reagent/mutationtoxin/felinid", SpeciesId::Felinid),
    ("/datum/reagent/mutationtoxin/lizard", SpeciesId::Lizard),
    ("/datum/reagent/mutationtoxin/fly", SpeciesId::Fly),
    ("/datum/reagent/mutationtoxin/moth", SpeciesId::Moth),
    ("/datum/reagent/mutationtoxin/ethereal", SpeciesId::Ethereal),
    ("/datum/reagent/mutationtoxin/pod", SpeciesId::Pod),
    ("/datum/reagent/mutationtoxin/cerulean", SpeciesId::Cerulean),
    (JELLY, SpeciesId::Jelly),
    ("/datum/reagent/mutationtoxin/golem", SpeciesId::Golem),
    ("/datum/reagent/mutationtoxin/abductor", SpeciesId::Abductor),
    ("/datum/reagent/mutationtoxin/android", SpeciesId::Android),
    ("/datum/reagent/mutationtoxin/skeleton", SpeciesId::Skeleton),
    ("/datum/reagent/mutationtoxin/zombie", SpeciesId::Zombie),
    ("/datum/reagent/mutationtoxin/ash", SpeciesId::Ashwalker),
    ("/datum/reagent/mutationtoxin/ghost", SpeciesId::Spirit),
    ("/datum/reagent/mutationtoxin/shadow", SpeciesId::Shadow),
    ("/datum/reagent/mutationtoxin/plasma", SpeciesId::Plasmaman),
    // BandaStation.
    (
        "/datum/reagent/mutationtoxin/vulpkanin",
        SpeciesId::Vulpkanin,
    ),
    ("/datum/reagent/mutationtoxin/tajaran", SpeciesId::Tajaran),
    ("/datum/reagent/mutationtoxin/skrell", SpeciesId::Skrell),
];

/// `subtypesof(/datum/species/jelly)`.
const JELLY_SUBSPECIES: [SpeciesId; 3] = [
    SpeciesId::Slime,
    SpeciesId::Luminescent,
    SpeciesId::Stargazer,
];

/// The species a mutation toxin turns into.
pub fn race_of(path: &str) -> Option<SpeciesId> {
    RACES.iter().find(|(p, _)| *p == path).map(|(_, s)| *s)
}

/// The `MUT_MSG_*` lines for the toxin's `current_cycle`.
fn lines_for(cycle: u32) -> &'static [&'static str] {
    if cycle <= CYCLES_MSG_IMMEDIATE {
        &IMMEDIATE
    } else if cycle <= CYCLES_MSG_EXTENDED {
        &EXTENDED
    } else {
        &ABOUT2TURN
    }
}

/// The `on_mob_life` of the mutation toxins metabolized this tick (`crate::chem`).
// Cost: every 2 s per mob with reagents, one table lookup per metabolized reagent.
pub fn metabolized(
    world: &mut World,
    chem: &Chemistry,
    mob: Entity,
    list: &[Metabolized],
    secs: f32,
) {
    for m in list {
        let path = chem.def(m.id).path.as_str();
        let Some(race) = race_of(path) else {
            continue;
        };
        // `ishuman`.
        if world.get::<crate::human::HumanLook>(mob).is_none() {
            return;
        }
        let biotypes = def_of(world, mob).biotypes;
        if path == JELLY && biotypes & biotype::SLIME != 0 {
            let species = pick_jelly(world);
            let text = "Your jelly shifts and morphs, turning you into another subspecies!";
            turn(world, mob, m, species, text.to_owned());
            continue;
        }
        // The jelly toxin's own `on_mob_life` turns without the biotype check.
        if path == JELLY && m.current_cycle >= CYCLES_TO_TURN {
            let species = pick_jelly(world);
            let text = format!("You've become a {}!", species.def().name);
            turn(world, mob, m, species, text);
            continue;
        }
        if biotypes & AFFECTED == 0 {
            continue;
        }
        let line = {
            let mut rng = world.get_resource_or_init::<CombatRng>();
            let lines = lines_for(m.current_cycle);
            crate::medical::wounds::spt_prob(&mut rng, 5.0, secs)
                .then(|| lines[rng.range(0, lines.len() as u32 - 1) as usize])
        };
        if let Some(line) = line {
            crate::tools::tell_mob(world, mob, Span::Warning, line);
        }
        if m.current_cycle >= CYCLES_TO_TURN {
            let text = format!("You've become a {}!", race.def().name.to_lowercase());
            turn(world, mob, m, race, text);
        }
    }
}

fn pick_jelly(world: &mut World) -> SpeciesId {
    let roll = world.resource_mut::<CombatRng>().range(0, 2) as usize;
    JELLY_SUBSPECIES[roll]
}

/// `set_species(race)`, `holder.del_reagent(type)` and the line.
fn turn(world: &mut World, mob: Entity, m: &Metabolized, species: SpeciesId, text: String) {
    set_species(world, mob, species);
    if let Some(mut reagents) = world.get_mut::<crate::chem::MobReagents>(mob) {
        let volume = reagents.blood.amount(m.id);
        reagents.blood.remove_reagent(m.id, volume);
    }
    crate::tools::tell_mob(world, mob, Span::Warning, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fork_toxins_turn_into_fork_species() {
        assert_eq!(
            race_of("/datum/reagent/mutationtoxin/vulpkanin"),
            Some(SpeciesId::Vulpkanin)
        );
        assert_eq!(
            race_of("/datum/reagent/mutationtoxin/tajaran"),
            Some(SpeciesId::Tajaran)
        );
        assert_eq!(
            race_of("/datum/reagent/mutationtoxin/skrell"),
            Some(SpeciesId::Skrell)
        );
        assert_eq!(race_of("/datum/reagent/mulligan"), None);
    }

    #[test]
    fn messages_follow_the_cycle() {
        assert_eq!(lines_for(1), &IMMEDIATE);
        assert_eq!(lines_for(6), &IMMEDIATE);
        assert_eq!(lines_for(7), &EXTENDED);
        assert_eq!(lines_for(17), &ABOUT2TURN);
    }
}

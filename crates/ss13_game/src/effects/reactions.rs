//! The chemical reactions that make smoke and foam in a container (their `on_reaction`, tg
//! `recipes/pyrotechnics.dm` and `recipes/others.dm`), run by `crate::chem::refresh_container`
//! for the instant reactions a holder ran:
//!
//! - smoke powder (not with stabilizing agent in the holder) and heated smoke powder: chemical
//!   smoke of `3 × created` (`created`) nodes carrying the rest of the holder, which is emptied;
//! - foam (fluorosurfactant and water; hollow and holy water 24 s, salt water and soda 1 s,
//!   salt, ice and holy not slippery): `2 × created` nodes of foam carrying the holder;
//! - metal, smart and iron foam: `5 × created` nodes leaving foamed metal (iron).
// shortcut: a mob's own holder (smoke powder drunk) makes no smoke and hurts no lungs; the
// firefighting foam reagent's own `expose_turf` foam is not ported.

use bevy::prelude::*;
use ss13_reagents::Chemistry;

use super::foam::{FOAMED_IRON, FOAMED_METAL, FoamKind, FoamOptions, create_foam};
use super::smoke::{SmokeKind, do_chem_smoke};
use crate::chem::Reagents;

const SMOKE_POWDER: &str = "/datum/chemical_reaction/smoke_powder";
const SMOKE_POWDER_SMOKE: &str = "/datum/chemical_reaction/smoke_powder_smoke";
const FOAM: &str = "/datum/chemical_reaction/foam";
const METAL_FOAM: &str = "/datum/chemical_reaction/metalfoam";
const SMART_FOAM: &str = "/datum/chemical_reaction/smart_foam";
const IRON_FOAM: &str = "/datum/chemical_reaction/ironfoam";
const SMOKE_POWDER_REAGENT: &str = "/datum/reagent/smoke_powder";
const STABILIZING_AGENT: &str = "/datum/reagent/stabilizing_agent";
const SMOKE_SOUND: &str = "sound/effects/smoke.ogg";

/// What a reaction path does here.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Effect {
    SmokePowder,
    HeatedSmokePowder,
    /// `lifetime` (s) and `slippery` of the `/datum/chemical_reaction/foam` subtype.
    Foam(f64, bool),
    Metal,
    Smart,
    Iron,
}

fn effect(path: &str) -> Option<Effect> {
    Some(match path {
        SMOKE_POWDER => Effect::SmokePowder,
        SMOKE_POWDER_SMOKE => Effect::HeatedSmokePowder,
        METAL_FOAM => Effect::Metal,
        SMART_FOAM => Effect::Smart,
        IRON_FOAM => Effect::Iron,
        _ => {
            let sub = path.strip_prefix(FOAM)?;
            let (lifetime, slippery) = match sub {
                "" => (8.0, true),
                "/hollow" => (24.0, true),
                "/salt" => (1.0, false),
                "/ice" => (8.0, false),
                "/soda" => (1.0, true),
                "/holy" => (24.0, false),
                _ => return None,
            };
            Effect::Foam(lifetime, slippery)
        }
    })
}

/// Any of `reacted` makes smoke or foam.
pub(crate) fn any(chem: &Chemistry, reacted: &[(u32, f32)]) -> bool {
    reacted
        .iter()
        .any(|&(i, _)| effect(&chem.reactions[i as usize].def.path).is_some())
}

/// The `on_reaction` of each smoke or foam reaction `container` ran, in order (the first one
/// empties the holder for the rest).
pub(crate) fn on_reactions(
    world: &mut World,
    chem: &Chemistry,
    container: Entity,
    reacted: &[(u32, f32)],
) {
    for &(index, created) in reacted {
        let Some(effect) = effect(&chem.reactions[index as usize].def.path) else {
            continue;
        };
        if world.get_entity(container).is_err() {
            return;
        }
        let nodes = |n: f32| (n * created).ceil().max(0.0) as u32;
        match effect {
            Effect::SmokePowder | Effect::HeatedSmokePowder => {
                let Some(pos) = crate::explosions::turf_of(world, container) else {
                    continue;
                };
                let Some(mut r) = world.get_mut::<Reagents>(container) else {
                    continue;
                };
                let amount = if effect == Effect::SmokePowder {
                    if chem
                        .id(STABILIZING_AGENT)
                        .is_some_and(|id| r.0.has(id).is_some())
                    {
                        continue;
                    }
                    if let Some(id) = chem.id(SMOKE_POWDER_REAGENT) {
                        r.0.remove_reagent(id, created * 3.0);
                    }
                    nodes(3.0)
                } else {
                    nodes(1.0)
                };
                let carry = r.0.clone();
                r.0.clear();
                do_chem_smoke(world, 0, pos, Some(amount), &carry, SmokeKind::Chem);
                crate::sound::playsound(
                    world,
                    crate::sound::PlaySound::new(pos, SMOKE_SOUND, 50.0, true).extrarange(-3.0),
                );
            }
            Effect::Foam(lifetime, slippery) => create_foam(
                world,
                container,
                FoamKind::Plain,
                nodes(2.0),
                FoamOptions {
                    lifetime: Some(lifetime),
                    slippery: Some(slippery),
                    result: None,
                },
                Some("The solution spews out foam!"),
            ),
            Effect::Metal | Effect::Smart | Effect::Iron => {
                let (kind, result, text) = match effect {
                    Effect::Metal => (
                        FoamKind::Metal,
                        FOAMED_METAL,
                        "The solution spews out a metallic foam!",
                    ),
                    Effect::Smart => (
                        FoamKind::Smart,
                        FOAMED_METAL,
                        "The solution spews out metallic foam!",
                    ),
                    _ => (
                        FoamKind::Iron,
                        FOAMED_IRON,
                        "The solution spews out a metallic foam!",
                    ),
                };
                create_foam(
                    world,
                    container,
                    kind,
                    nodes(5.0),
                    FoamOptions {
                        result: Some(result),
                        ..default()
                    },
                    Some(text),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaction_paths_map_to_their_effects() {
        assert_eq!(effect(SMOKE_POWDER), Some(Effect::SmokePowder));
        assert_eq!(
            effect("/datum/chemical_reaction/foam"),
            Some(Effect::Foam(8.0, true))
        );
        assert_eq!(
            effect("/datum/chemical_reaction/foam/holy"),
            Some(Effect::Foam(24.0, false))
        );
        assert_eq!(effect("/datum/chemical_reaction/foaming_agent"), None);
        assert_eq!(effect("/datum/chemical_reaction/foam/unknown"), None);
        assert_eq!(effect(IRON_FOAM), Some(Effect::Iron));
    }
}

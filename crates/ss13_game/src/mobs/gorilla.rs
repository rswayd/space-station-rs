//! Gorillas (server), after tg `/mob/living/basic/gorilla` (`farm_animals/gorilla/gorilla.dm`).
//! Their tree is `gorilla.bt.json` (`crate::mobs::ai::trees`).
//!
//! - A landed punch knocks the target down for 2 s (`paralyze_chance`: 20%, lab and feral
//!   gorillas 0%), else throws it 1-2 tiles the way the gorilla faces; a player's gorilla
//!   ooga-s every 2-6 punches.
//! - They eat bananas, banana bread and treats, fruit and jungle salads and sundaes (10 health).
//! - The cargorilla is a pacifist (`TRAIT_PACIFISM`): it never attacks.
//! - Genetics turns monkeys into lab gorillas (`crate::transform::gorillize`).
// shortcut: no hands (`dextrous`: holding, the standing sprite and its 1.2 slowdown, crafting,
// stripping), no crate carrying (`crate_carrier`), no limb punching (`amputating_limbs`), no
// wall tearing, no gorilla brain on gib, no fitness examine; Pun Pun's bar gorilla runs the
// monkey tree it has in tg only through `crate::mobs::ai::monkey` when that is attached.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mobs::MobDef;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
use ss13_shared::{Dir, Facing, TilePos};

use crate::combat::CombatRng;
use crate::health::DEFAULT_MESSAGE_RANGE;
use crate::server::PlayerMob;
use crate::status_effects::Incap;

pub const GORILLA: &str = "/mob/living/basic/gorilla";
const CARGORILLA: &str = "/mob/living/basic/gorilla/cargorilla";
const NO_PARALYZE: [&str; 2] = [
    "/mob/living/basic/gorilla/genetics",
    "/mob/living/basic/gorilla/hostile",
];
/// `paralyze_chance`, `Paralyze(2 SECONDS)`, `throw_at(range = rand(1, 2), speed = 7)`.
const PARALYZE_CHANCE: f32 = 20.0;
const PARALYZE_SECS: f32 = 2.0;
const THROW_SPEED: f32 = 7.0;

/// A gorilla's `paralyze_chance` and `oogas` counter; `pacifist`: the cargorilla.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Gorilla {
    paralyze_chance: f32,
    oogas: i32,
    pacifist: bool,
}

pub(super) fn attach(world: &mut World, mob: Entity, def: &MobDef) {
    if !is_under(&def.path, GORILLA) {
        return;
    }
    let paralyze_chance = if NO_PARALYZE.iter().any(|p| is_under(&def.path, p)) {
        0.0
    } else {
        PARALYZE_CHANCE
    };
    world.entity_mut(mob).insert(Gorilla {
        paralyze_chance,
        oogas: 0,
        pacifist: is_under(&def.path, CARGORILLA),
    });
}

/// `TRAIT_PACIFISM` of the cargorilla.
pub fn pacifist(world: &World, mob: Entity) -> bool {
    world.get::<Gorilla>(mob).is_some_and(|g| g.pacifist)
}

/// `/mob/living/basic/gorilla/melee_attack` after a landed punch.
pub fn after_melee(world: &mut World, gorilla: Entity, target: Entity) {
    let Some(mut g) = world.get::<Gorilla>(gorilla).copied() else {
        return;
    };
    // `ooga_ooga`: "Sorry NPCs".
    if world.get::<PlayerMob>(gorilla).is_some() {
        g.oogas -= 1;
        if g.oogas <= 0 {
            g.oogas = world.resource_mut::<CombatRng>().range(2, 6) as i32;
            super::space_fauna::run_emote(world, gorilla, "ooga");
        }
        world.entity_mut(gorilla).insert(g);
    }
    let Some(&to) = world.get::<TilePos>(target) else {
        return;
    };
    let paralyze =
        g.paralyze_chance > 0.0 && world.resource_mut::<CombatRng>().chance(g.paralyze_chance);
    if paralyze {
        crate::status_effects::apply(world, target, Incap::Paralyze, PARALYZE_SECS);
        let (who, whom) = (
            super::defense::name_of(world, gorilla),
            super::defense::name_of(world, target),
        );
        let clients = crate::obj_damage::clients_in_range(world, to, DEFAULT_MESSAGE_RANGE, &[]);
        world.write_message(ss13_net::chat::server::SendChat {
            clients,
            message: ChatMessage::simple(
                ChatChannel::Combat,
                Span::Danger,
                format!("{who} knocks {whom} down!"),
            ),
        });
        return;
    }
    // `get_edge_target_turf(target, dir)`: as far as the throw reaches.
    let range = world.resource_mut::<CombatRng>().range(1, 2);
    let dir = world.get::<Facing>(gorilla).map_or(Dir::South, |f| f.0);
    let (dx, dy) = dir.offset();
    let edge = TilePos::new(to.x + dx * range as i32, to.y + dy * range as i32, to.z);
    crate::throw::throw_at(world, target, edge, range, THROW_SPEED);
}

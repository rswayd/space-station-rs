//! Unique plant traits (tg `unique_plant_genes.dm`, `/datum/element/plant_backfire`): genes
//! that only certain plants carry and nobody can graft.
//!
//! - On attack (`trait/attack`): the plant's `force` is `round(5 + potency * multiplier)`; a hit
//!   on a mob ([`on_item_attack`], from `crate::combat`) runs the plant's effect (the
//!   novaflower sets the target on fire, the sunflower shouts FLOWER POWER) and degrading plants
//!   lose `rand(1, force / 3 + 1)` force per hit until they fall apart.
//! - Backfire (`trait/backfire`, dangerous mob transformation): picking the plant up or swinging
//!   it at a mob bare-handed (no thick gloves) pricks (rose), burns (novaflower, nettles),
//!   stuns (death nettle), squashes (bluespace tomato) or wakes it (killer tomato); some cancel
//!   the attack.
//! - Explosive contents (`trait/bomb_plant`): in hand the stem is plucked and the gunpowder
//!   reacts, blowing up 5 to 10 s later with `5 + round(gunpowder / 10)` power; the potency
//!   based kind (fire lemons) is primed and explodes 1 to 6 s later (light 2, flames
//!   `clamp(round(potency / 20), 1, 5)`).
//! - Dormant ferocity (`trait/mob_transformation`): shaken in hand the plant wakes after 3 s
//!   into its mob (killer tomato, walking mushroom) with the seed's stats on top.
// shortcut: no anti-magic (holymelons: no magic resistance checks), no chili heat (no
// `adjust_bodytemperature` of the holder), no backfire on throws (`COMSIG_MOVABLE_PRE_THROW`)
// or picking up from storage, no `TRAIT_PLANT_SAFE`/`TRAIT_PIERCEIMMUNE`; several backfire
// genes on one plant run once together (tg runs every gene's effect per element); no gas
// production (corpse flowers), opium, one-bite or modified volume genes; no feeding block of
// an awakening plant; the bomb has no sparks.

use bevy::prelude::*;
use ss13_mapload::{DmClothing, DmItem, IconIndex};
use ss13_reagents::is_under;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, TilePos};

use super::Botany;
use super::produce::{SQUASH, STICKY, is_grown, squash};
use super::seeds::{Dice, Seed, dm_round};
use crate::chem::{Chem, Reagents, map_pos, name_of, tell_mob, visible};
use crate::combat::CombatRng;
use crate::health::{DamageType, Health, Stat};
use crate::inventory::{CarriedBy, Inventory};
use crate::zone::BodyZone;

pub const ATTACK: &str = "/datum/plant_gene/trait/attack";
const NOVAFLOWER_ATTACK: &str = "/datum/plant_gene/trait/attack/novaflower_attack";
const SUNFLOWER_ATTACK: &str = "/datum/plant_gene/trait/attack/sunflower_attack";
const NETTLE_ATTACK: &str = "/datum/plant_gene/trait/attack/nettle_attack";
const DEATH_NETTLE_ATTACK: &str = "/datum/plant_gene/trait/attack/nettle_attack/death";
pub const BACKFIRE: &str = "/datum/plant_gene/trait/backfire";
const ROSE_THORNS: &str = "/datum/plant_gene/trait/backfire/rose_thorns";
const NOVAFLOWER_HEAT: &str = "/datum/plant_gene/trait/backfire/novaflower_heat";
const NETTLE_BURN: &str = "/datum/plant_gene/trait/backfire/nettle_burn";
const DEATH_NETTLE_BURN: &str = "/datum/plant_gene/trait/backfire/nettle_burn/death";
const BLUESPACE_BACKFIRE: &str = "/datum/plant_gene/trait/backfire/bluespace";
const CHILI_HEAT: &str = "/datum/plant_gene/trait/backfire/chili_heat";
const CHEM_HEATING: &str = "/datum/plant_gene/trait/chem_heating";
pub const BOMB_PLANT: &str = "/datum/plant_gene/trait/bomb_plant";
const BOMB_POTENCY: &str = "/datum/plant_gene/trait/bomb_plant/potency_based";
pub const MOB_TRANSFORMATION: &str = "/datum/plant_gene/trait/mob_transformation";
const TOMATO_TRANSFORMATION: &str = "/datum/plant_gene/trait/mob_transformation/tomato";
const SHROOM_TRANSFORMATION: &str = "/datum/plant_gene/trait/mob_transformation/shroom";
pub const KILLER_TOMATO: &str = "/mob/living/basic/killer_tomato";
pub const WALKING_MUSHROOM: &str = "/mob/living/basic/mushroom";
const GUNPOWDER: &str = "/datum/reagent/gunpowder";
const HARVEST_DMI: &str = "icons/obj/service/hydroponics/harvest.dmi";
const CHERRY_BOMB: &str = "/obj/item/food/grown/cherry_bomb";
const FUSE_SOUND: &str = "sound/effects/fuse.ogg";
const ARMBOMB_SOUND: &str = "sound/items/weapons/armbomb.ogg";
/// `HANDS` (`HAND_LEFT | HAND_RIGHT`).
const HANDS: u32 = (1 << 9) | (1 << 10);
/// `gunpowder_explosion`: `modifier`, `strengthdiv`.
const GUNPOWDER_MODIFIER: f32 = 5.0;
const GUNPOWDER_STRENGTHDIV: f32 = 10.0;

/// A plant counting down to its explosion or awakening.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct PlantTimer {
    pub at: f64,
    pub kind: TimerKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TimerKind {
    /// `reagent_explode` of the gunpowder that reacted.
    Gunpowder(f32),
    /// `bomb_plant/potency_based/detonate`.
    Potency,
    /// `mob_transformation/awaken`.
    Awaken,
}

/// What a `trait/attack` gene does: `force_multiplier`, `degrades_after_hit`,
/// `degradation_noun`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttackGene {
    pub force_multiplier: f32,
    pub degrades: bool,
    pub noun: &'static str,
}

pub fn attack_gene(path: &str) -> Option<AttackGene> {
    let gene = |force_multiplier, degrades, noun| {
        Some(AttackGene {
            force_multiplier,
            degrades,
            noun,
        })
    };
    if is_under(path, NOVAFLOWER_ATTACK) {
        gene(0.2, true, "petals")
    } else if is_under(path, DEATH_NETTLE_ATTACK) {
        gene(0.4, true, "leaves")
    } else if is_under(path, NETTLE_ATTACK) {
        gene(0.2, true, "leaves")
    } else if is_under(path, ATTACK) {
        gene(0.0, false, "leaves")
    } else {
        None
    }
}

/// `trait/attack/on_new_plant`: `round(5 + potency * force_multiplier, 1)`.
pub fn attack_force(potency: f32, multiplier: f32) -> f32 {
    (5.0 + potency * multiplier + 0.5).floor()
}

/// `after_attack_effect` degradation: the force left after a hit with `roll` =
/// `rand(1, force / 3 + 1)`, `None` when the plant falls apart (it hit with no force left).
pub fn degrade(force: f32, roll: f32) -> Option<f32> {
    (force > 0.0).then(|| force - roll)
}

/// `bomb_plant/potency_based/detonate`: `clamp(round(potency / 20), 1, 5)`.
pub fn flame_reach(potency: f32) -> i32 {
    (dm_round(potency / 20.0) as i32).clamp(1, 5)
}

/// `reagent_explode` of the gunpowder: `modifier + round(volume / strengthdiv, 1)`.
pub fn gunpowder_power(volume: f32) -> f32 {
    GUNPOWDER_MODIFIER + (volume / GUNPOWDER_STRENGTHDIV + 0.5).floor()
}

/// `mob_transformation` multipliers: health, melee, speed.
fn transformation(path: &str) -> Option<(&'static str, f32, f32, f32, bool)> {
    if is_under(path, TOMATO_TRANSFORMATION) {
        Some((KILLER_TOMATO, 0.33, 0.1, 0.02, true))
    } else if is_under(path, SHROOM_TRANSFORMATION) {
        Some((WALKING_MUSHROOM, 0.25, 0.05, 0.02, false))
    } else {
        None
    }
}

fn trait_paths(seed: &Seed) -> impl Iterator<Item = &str> {
    seed.genes.iter().filter_map(|g| match g {
        super::genes::Gene::Trait { path, .. } => Some(path.as_str()),
        super::genes::Gene::Reagent { .. } => None,
    })
}

pub(super) fn build(app: &mut App) {
    app.add_observer(set_force).add_systems(
        Update,
        (
            backfire_on_attack
                .after(crate::security::security_clicks)
                .before(crate::combat::MeleeSystems),
            run_timers.run_if(any_with_component::<PlantTimer>),
        ),
    );
}

/// `trait/attack/on_new_plant` on every seed put on a grown item.
fn set_force(insert: On<Insert<Seed>>, mut items: Query<(&mut DmItem, &Seed)>) {
    let Ok((mut item, seed)) = items.get_mut(insert.entity) else {
        return;
    };
    if !is_grown(&item.path) {
        return;
    }
    let multiplier = trait_paths(seed)
        .filter_map(attack_gene)
        .map(|g| g.force_multiplier)
        .find(|m| *m != 0.0);
    if let Some(m) = multiplier {
        let force = attack_force(seed.potency, m);
        if item.force != force {
            item.force = force;
        }
    }
}

fn grown_seed(world: &World, item: Entity) -> Option<Seed> {
    let grown = world.get::<DmItem>(item).is_some_and(|i| is_grown(&i.path));
    world.get::<Seed>(item).filter(|_| grown).cloned()
}

/// `COMSIG_ITEM_ATTACK` and `COMSIG_ITEM_AFTERATTACK` of a plant `attacker` swung at the living
/// `target` (`crate::combat`).
pub fn on_item_attack(world: &mut World, attacker: Entity, target: Entity, plant: Entity) {
    let Some(seed) = grown_seed(world, plant) else {
        return;
    };
    let genes: Vec<String> = trait_paths(&seed)
        .filter(|p| is_under(p, ATTACK))
        .map(str::to_owned)
        .collect();
    if genes.is_empty() {
        return;
    }
    let name = name_of(world, plant);
    for gene in &genes {
        if is_under(gene, NOVAFLOWER_ATTACK) && world.get::<Health>(target).is_some() {
            tell_mob(
                world,
                target,
                Span::Danger,
                &format!("You are lit on fire from the intense heat of {name}!"),
            );
            crate::fire::adjust_fire_stacks(world, target, dm_round(seed.potency / 20.0));
            crate::fire::ignite_mob(world, target);
        }
    }
    for gene in &genes {
        if is_under(gene, SUNFLOWER_ATTACK) {
            let (user, victim) = (name_of(world, attacker), name_of(world, target));
            let mut personal = vec![(
                attacker,
                Span::Notice,
                format!("Your {name}'s FLOWER POWER strikes {victim}!"),
            )];
            if target != attacker {
                personal.push((
                    target,
                    Span::Notice,
                    format!("{user} smacks you with {name}! FLOWER POWER!"),
                ));
            }
            if let Some(pos) = map_pos(world, attacker) {
                crate::security::show(
                    world,
                    pos,
                    crate::health::DEFAULT_MESSAGE_RANGE,
                    ChatChannel::Info,
                    (
                        Span::Notice,
                        format!("{user} smacks {victim} with their {name}! FLOWER POWER!"),
                    ),
                    &personal,
                );
            }
        }
        let Some(kind) = attack_gene(gene).filter(|k| k.degrades) else {
            continue;
        };
        let Some(force) = world.get::<DmItem>(plant).map(|i| i.force) else {
            return;
        };
        let high = (force / 3.0 + 1.0).floor() as i32;
        let roll = world.resource_mut::<CombatRng>().rand(1, high.max(1)) as f32;
        match degrade(force, roll) {
            Some(left) => {
                if let Some(mut item) = world.get_mut::<DmItem>(plant) {
                    item.force = left;
                }
            }
            None => {
                tell_mob(
                    world,
                    attacker,
                    Span::Warning,
                    &format!(
                        "All the {} have fallen off {name} from violent whacking!",
                        kind.noun
                    ),
                );
                world.despawn(plant);
                return;
            }
        }
    }
}

/// `plant_safety_check`: not a carbon, a plant without the genes that make it dangerous, or
/// thick gloves.
fn safe_with(world: &World, user: Entity, gene: &str, seed: &Seed) -> bool {
    let Some(inventory) = world.get::<Inventory>(user) else {
        return true;
    };
    let needs = if is_under(gene, BLUESPACE_BACKFIRE) {
        Some(SQUASH)
    } else if is_under(gene, CHILI_HEAT) {
        Some(CHEM_HEATING)
    } else {
        None
    };
    if needs.is_some_and(|g| !seed.has_gene(g)) {
        return true;
    }
    inventory.equipped().any(|item| {
        world.get::<DmClothing>(item).is_some_and(|c| {
            c.body_parts_covered & HANDS != 0 && c.clothing_flags & crate::chem::THICKMATERIAL != 0
        })
    })
}

/// The backfire genes of a seed (and a dangerous transformation), with `cancel_action`.
fn backfire_genes(seed: &Seed) -> Vec<(String, bool)> {
    trait_paths(seed)
        .filter_map(|p| {
            let cancel = if is_under(p, NOVAFLOWER_HEAT)
                || is_under(p, DEATH_NETTLE_BURN)
                || is_under(p, BLUESPACE_BACKFIRE)
            {
                true
            } else if is_under(p, BACKFIRE) {
                false
            } else if transformation(p).is_some_and(|t| t.4) {
                true
            } else {
                return None;
            };
            Some((p.to_owned(), cancel))
        })
        .collect()
}

/// `plant_backfire/backfire` of `plant` handled by `user`: the effects when unsafe. `Some(cancel)`
/// when it backfired.
pub fn backfire(world: &mut World, plant: Entity, user: Entity) -> Option<bool> {
    let seed = grown_seed(world, plant)?;
    let unsafe_genes: Vec<(String, bool)> = backfire_genes(&seed)
        .into_iter()
        .filter(|(g, _)| !safe_with(world, user, g, &seed))
        .collect();
    if unsafe_genes.is_empty() {
        return None;
    }
    let cancel = unsafe_genes.iter().any(|(_, c)| *c);
    for (gene, _) in &unsafe_genes {
        if world.get_entity(plant).is_err() {
            break;
        }
        backfire_effect(world, plant, user, gene, &seed);
    }
    Some(cancel)
}

fn hurt_hand(world: &mut World, mob: Entity, amount: f32, kind: DamageType) {
    let hand = world.get::<Inventory>(mob).map_or(0, |i| i.active);
    let zone = if hand == 0 {
        BodyZone::LArm
    } else {
        BodyZone::RArm
    };
    if let Ok(mut e) = world.get_entity_mut(mob)
        && let Ok((mut health, mut stat)) = e.get_components_mut::<(&mut Health, &mut Stat)>()
    {
        crate::health::apply_damage_zone(&mut health, &mut stat, zone, kind, amount);
    }
}

fn backfire_effect(world: &mut World, plant: Entity, user: Entity, gene: &str, seed: &Seed) {
    let name = name_of(world, plant);
    let (force, kind) = world
        .get::<DmItem>(plant)
        .map_or((0.0, DamageType::Brute), |i| {
            (
                i.force,
                DamageType::from_dm(&i.damtype).unwrap_or(DamageType::Brute),
            )
        });
    if is_under(gene, ROSE_THORNS) {
        if !seed.has_gene(STICKY) && world.resource_mut::<CombatRng>().prob(66.0) {
            tell_mob(
                world,
                user,
                Span::Danger,
                &format!("{name}'s thorns nearly prick your hand. Best be careful."),
            );
            return;
        }
        tell_mob(
            world,
            user,
            Span::Danger,
            &format!("{name}'s thorns prick your hand. Ouch."),
        );
        hurt_hand(world, user, 2.0, DamageType::Brute);
    } else if is_under(gene, NOVAFLOWER_HEAT) {
        tell_mob(
            world,
            user,
            Span::Danger,
            &format!("{name} singes your bare hand!"),
        );
        hurt_hand(world, user, force, kind);
    } else if is_under(gene, NETTLE_BURN) {
        tell_mob(
            world,
            user,
            Span::Danger,
            &format!("{name} burns your bare hand!"),
        );
        hurt_hand(world, user, force, kind);
        // tg's `if(!. || prob(50))` reads the parent's null return and never stuns; the
        // intended 50% is ported.
        if is_under(gene, DEATH_NETTLE_BURN) && !world.resource_mut::<CombatRng>().prob(50.0) {
            crate::status_effects::apply(world, user, crate::status_effects::Incap::Paralyze, 10.0);
            tell_mob(
                world,
                user,
                Span::Danger,
                &format!("You are stunned by the powerful acids of {name}!"),
            );
        }
    } else if is_under(gene, BLUESPACE_BACKFIRE) {
        if world.resource_mut::<CombatRng>().prob(50.0) {
            return;
        }
        tell_mob(
            world,
            user,
            Span::Danger,
            &format!("{name} slips out of your hand!"),
        );
        squash(world, plant, Some(user));
    } else if transformation(gene).is_some() {
        // `early_awakening`.
        let space =
            map_pos(world, user).is_some_and(|p| crate::mobs::ai::behaviors::is_space(world, p));
        if world.get::<PlantTimer>(plant).is_none()
            && !space
            && world.resource_mut::<CombatRng>().prob(25.0)
        {
            if let Some(pos) = map_pos(world, plant) {
                crate::security::show(
                    world,
                    pos,
                    crate::health::DEFAULT_MESSAGE_RANGE,
                    ChatChannel::Warning,
                    (Span::Danger, format!("{name} begins to growl and shake!")),
                    &[],
                );
            }
            start_timer(world, plant, 1.0, TimerKind::Awaken);
        }
    }
}

/// `COMSIG_ITEM_PICKUP` of a plant picked up off the floor by an empty hand (the pickup goes on).
pub fn pickup_backfire(world: &mut World, mob: Entity, target: Entity) -> bool {
    let on_floor =
        world.get::<TilePos>(target).is_some() && world.get::<CarriedBy>(target).is_none();
    if on_floor && world.contains_resource::<Botany>() {
        backfire(world, target, mob);
    }
    false
}

/// `COMSIG_ITEM_PRE_ATTACK` of a held plant clicked on a living mob: a backfire that cancels
/// stops the swing.
fn backfire_on_attack(
    world: &mut World,
    mut clicks: Local<bevy::ecs::message::MessageCursor<crate::click::ClickResolved>>,
) {
    let clicks: Vec<crate::click::ClickResolved> = clicks
        .read(world.resource::<bevy::ecs::message::Messages<crate::click::ClickResolved>>())
        .copied()
        .collect();
    if !world.contains_resource::<Botany>() {
        return;
    }
    for click in clicks {
        let mob = click.mob;
        if world
            .resource::<crate::security::MeleeHandled>()
            .0
            .contains(&mob)
            || world.get::<Health>(click.target).is_none()
        {
            continue;
        }
        let Some(held) = world.get::<Inventory>(mob).and_then(Inventory::active_item) else {
            continue;
        };
        if world.get::<CarriedBy>(held).map(|c| c.0) != Some(mob) {
            continue;
        }
        if backfire(world, held, mob) == Some(true) {
            world
                .resource_mut::<crate::security::MeleeHandled>()
                .0
                .insert(mob);
        }
    }
}

fn start_timer(world: &mut World, plant: Entity, secs: f64, kind: TimerKind) {
    let at = world.resource::<Time>().elapsed_secs_f64() + secs;
    world.entity_mut(plant).insert(PlantTimer { at, kind });
}

/// `attack_self` of a plant with explosive contents or dormant ferocity. True when it was one.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, plant: Entity) -> bool {
    let Some(seed) = grown_seed(world, plant) else {
        return false;
    };
    let name = name_of(world, plant);
    if let Some(gene) = trait_paths(&seed)
        .find(|p| is_under(p, BOMB_PLANT))
        .map(str::to_owned)
    {
        if world.get::<PlantTimer>(plant).is_some() {
            return true;
        }
        light_fuse(world, plant);
        let user = name_of(world, mob);
        let Some(pos) = map_pos(world, mob) else {
            return true;
        };
        if is_under(&gene, BOMB_POTENCY) {
            visible(
                world,
                mob,
                &format!("You prime {name}!"),
                &format!("{user} primes {name}!"),
                Span::Warning,
            );
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(pos, ARMBOMB_SOUND, 75.0, true).extrarange(-3.0),
            );
            let secs = world.resource_mut::<CombatRng>().rand(10, 60) as f64 / 10.0;
            start_timer(world, plant, secs, TimerKind::Potency);
        } else {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(pos, FUSE_SOUND, seed.potency, false),
            );
            visible(
                world,
                mob,
                &format!("You pluck the stem from {name}, which begins to hiss loudly!"),
                &format!("{user} plucks the stem from {name}!"),
                Span::Warning,
            );
            detonate_reagents(world, plant);
        }
        return true;
    }
    let Some((_, _, _, _, dangerous)) = trait_paths(&seed).find_map(transformation) else {
        return false;
    };
    // `manual_awakening`.
    let space = map_pos(world, mob).is_some_and(|p| crate::mobs::ai::behaviors::is_space(world, p));
    if world.get::<PlantTimer>(plant).is_some() || space {
        return true;
    }
    if dangerous
        && world
            .get::<crate::medical::traumas::Pacifist>(mob)
            .is_some()
    {
        crate::tools::tell(
            world,
            client,
            Span::Notice,
            format!("You decide not to awaken {name}. It may be very dangerous!"),
        );
        return true;
    }
    crate::tools::tell(
        world,
        client,
        Span::Notice,
        format!("You begin to awaken {name}..."),
    );
    start_timer(world, plant, 3.0, TimerKind::Awaken);
    true
}

/// `alt_icon` when the plant has one (cherry bombs), else `color = COLOR_RED`.
fn light_fuse(world: &mut World, plant: Entity) {
    let lit = world
        .get::<DmItem>(plant)
        .is_some_and(|i| is_under(&i.path, CHERRY_BOMB))
        .then(|| {
            world
                .get_resource::<IconIndex>()
                .and_then(|icons| icons.get(HARVEST_DMI, "cherry_bomb_lit"))
        })
        .flatten();
    if let Some(mut appearance) = world.get_mut::<Appearance>(plant) {
        match lit {
            Some(id) => appearance.icon_state = id,
            None => appearance.color = Some([0xff, 0, 0, 0xff]),
        }
    }
}

/// `bomb_plant/detonate`: `chem_temp = 1000` and the reactions; the gunpowder reacts
/// (`gunpowder_explosion`, 474 K) and blows up 5 to 10 s later.
fn detonate_reagents(world: &mut World, plant: Entity) {
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let Some(mut holder) = world.get::<Reagents>(plant).map(|r| r.0.clone()) else {
        return;
    };
    holder.chem_temp = 1000.0;
    let gunpowder = chem.id(GUNPOWDER).map_or(0.0, |id| {
        let volume = holder.amount(id);
        holder.remove_reagent(id, volume);
        volume
    });
    holder.handle_reactions(&chem);
    if let Some(mut r) = world.get_mut::<Reagents>(plant) {
        r.0 = holder;
    }
    if gunpowder > 0.0 {
        let secs = world.resource_mut::<CombatRng>().rand(50, 100) as f64 / 10.0;
        start_timer(world, plant, secs, TimerKind::Gunpowder(gunpowder));
    }
}

fn run_timers(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let due: Vec<(Entity, TimerKind)> = world
        .query::<(Entity, &PlantTimer)>()
        .iter(world)
        .filter(|(_, t)| now >= t.at)
        .map(|(e, t)| (e, t.kind))
        .collect();
    for (plant, kind) in due {
        world.entity_mut(plant).remove::<PlantTimer>();
        let Some(pos) = map_pos(world, plant) else {
            continue;
        };
        let seed = world.get::<Seed>(plant).cloned();
        match kind {
            TimerKind::Gunpowder(volume) => {
                let clients = crate::chem::clients_near(world, pos, None);
                if !clients.is_empty() {
                    world.write_message(ss13_net::chat::server::SendChat {
                        clients,
                        message: crate::chem::line(
                            Span::Danger,
                            "The solution violently explodes!",
                        ),
                    });
                }
                world.despawn(plant);
                crate::explosions::dyn_explosion(
                    world,
                    pos,
                    gunpowder_power(volume),
                    None,
                    None,
                    true,
                );
            }
            TimerKind::Potency => {
                let flame = flame_reach(seed.map_or(0.0, |s| s.potency));
                world.despawn(plant);
                crate::explosions::explosion(
                    world,
                    pos,
                    crate::explosions::Explosion::new(-1, -1, 2).flame(flame),
                );
            }
            TimerKind::Awaken => {
                if let Some(seed) = seed {
                    awaken(world, plant, pos, &seed);
                }
            }
        }
    }
}

/// `mob_transformation/awaken`: the mob with the seed's endurance, potency and production on
/// top of its own stats.
fn awaken(world: &mut World, plant: Entity, pos: TilePos, seed: &Seed) {
    let Some((path, health_mult, melee_mult, speed_mult, _)) =
        trait_paths(seed).find_map(transformation)
    else {
        return;
    };
    let name = name_of(world, plant);
    world.despawn(plant);
    let Some(mob) = crate::mobs::spawn::spawn_mob(world, path, pos) else {
        return;
    };
    if let Some(basic) = world.get::<crate::mobs::BasicMob>(mob).cloned() {
        let mut def = (*basic.def).clone();
        def.max_health += dm_round(seed.endurance * health_mult);
        def.health = def.max_health;
        let melee = dm_round(seed.potency * melee_mult);
        def.melee_damage_lower += melee;
        def.melee_damage_upper += melee;
        // `round(production * multiplier, 0.01)`.
        def.speed -= (seed.production * speed_mult * 100.0 + 0.5).floor() / 100.0;
        let health = Health::basic(def.max_health, def.physiology);
        let speed = def.speed.max(0.0) / 10.0;
        world.entity_mut(mob).insert((
            crate::mobs::BasicMob {
                def: std::sync::Arc::new(def),
                ..basic
            },
            health,
            ss13_world::Mover::new(speed),
        ));
    }
    if let Some(pos) = map_pos(world, mob) {
        crate::security::show(
            world,
            pos,
            crate::health::DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (
                Span::Notice,
                format!("{name} growls as it suddenly awakens!"),
            ),
            &[],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attack_genes_set_force_from_potency() {
        let nova = attack_gene(NOVAFLOWER_ATTACK).unwrap();
        assert_eq!(attack_force(50.0, nova.force_multiplier), 15.0);
        let death = attack_gene(DEATH_NETTLE_ATTACK).unwrap();
        assert_eq!(death.force_multiplier, 0.4);
        assert_eq!(attack_force(100.0, death.force_multiplier), 45.0);
        // round(x, 1) halves up.
        assert_eq!(attack_force(12.5, 0.2), 8.0);
        assert!(!attack_gene(SUNFLOWER_ATTACK).unwrap().degrades);
        assert_eq!(attack_gene("/datum/plant_gene/trait/squash"), None);
    }

    #[test]
    fn degrading_plants_fall_apart_only_at_no_force() {
        assert_eq!(degrade(15.0, 6.0), Some(9.0));
        assert_eq!(degrade(2.0, 2.0), Some(0.0));
        assert_eq!(degrade(0.0, 1.0), None);
    }

    #[test]
    fn bombs_scale_with_potency_and_gunpowder() {
        assert_eq!(flame_reach(10.0), 1);
        assert_eq!(flame_reach(59.0), 2);
        assert_eq!(flame_reach(100.0), 5);
        // 70 u of gunpowder (potency 100 cherry bomb): 5 + 7.
        assert_eq!(gunpowder_power(70.0), 12.0);
        assert_eq!(gunpowder_power(4.0), 5.0);
        assert_eq!(gunpowder_power(5.0), 6.0);
    }

    #[test]
    fn only_dangerous_transformations_backfire() {
        let gene = |path: &str| super::super::genes::Gene::Trait {
            path: path.into(),
            rate: 0.05,
            mutability: 0,
            ids: 0,
            flags: 0,
        };
        let plants = super::super::test_plants();
        let def = plants.seed_def("/obj/item/seeds/tomato").unwrap();
        let mut seed = Seed::new(def, &plants);
        seed.genes = vec![gene(TOMATO_TRANSFORMATION), gene(ROSE_THORNS)];
        assert_eq!(
            backfire_genes(&seed),
            vec![
                (TOMATO_TRANSFORMATION.to_owned(), true),
                (ROSE_THORNS.to_owned(), false)
            ]
        );
        seed.genes = vec![gene(SHROOM_TRANSFORMATION), gene(DEATH_NETTLE_BURN)];
        assert_eq!(
            backfire_genes(&seed),
            vec![(DEATH_NETTLE_BURN.to_owned(), true)]
        );
    }
}

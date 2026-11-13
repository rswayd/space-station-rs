//! The voidwalker and the sunwalker after tg `code/modules/antagonists/voidwalker/`: the mob
//! (`/mob/living/basic/voidwalker`, its `/sunwalker` subtype), the antagonists
//! (`/datum/antagonist/voidwalker`, `/datum/antagonist/sunwalker`), the curse
//! (`/datum/brain_trauma/voided`, `/stable`), the cosmic skull and the "Voidwalker" light
//! midround (`/datum/dynamic_ruleset/midround/from_ghosts/voidwalker`).
//!
//! - **Selection** ([`can_spawn`], [`spawn_voidwalkers`], from `super::midround`): a polled
//!   ghost becomes a voidwalker on a space carp spawn (`find_space_spawn`), the antagonist, its
//!   objective (one of three texts; 5% "Чувак, я чертовски люблю стекло."; done while alive),
//!   the greeting with the objective, tg's default panel, the round-end entry.
//! - **The mob** ([`Walker`], any `/mob/living/basic/voidwalker` that appears): passes windows and
//!   grilles (`TRAIT_PASSWINDOW`); a voidwalker gets a name from `strings/names/voidwalker.txt`
//!   and Telepathy; in space it is invisible (`space_camo`, back for 5 s after it attacks) and,
//!   from 6 s after the last damage, regenerates 2 brute and 2 burn a second (twice in crit).
//!   On death: 4-6 glass shards, the cosmic skull, the shatter sound.
//! - **Clicks** ([`walker_clicks`], `early_melee_attack`): a human is chomped
//!   (`void_chomped`: cannot die for 15 s); one already voided is put to sleep for 30 s; one
//!   unconscious or in crit on space (or nebula vomit) is taken in 6 s while paralyzed
//!   ([`VoidJob::Kidnap`]). The hit follows: oxygen damage on humans, brute on the rest and on a
//!   right click (the sunwalker: burn on humans). A right click on a wall soft enough
//!   (`hardness >= 40`) opens it in 8 s ([`VoidJob::Wall`]): it and the soft walls around it
//!   become void-smeared windows on grilles; 2 such openings, +1 per kidnapping, 60 s apart.
//! - **Kidnapping** ([`take_them`]): the victim gets the voided curse and comes back on a random
//!   station tile, 80 brute and 20 burn healed (tg's fallback without the void).
//! - **The curse** ([`voided_life`], `crate::medical::traumas::Kind::Voided`): pacifism, a 2%
//!   chance per `Life()` to vomit nebula and fall; a step into space sends the cursed back to
//!   one of its last tiles (`banned_from_space`, [`banned_from_space`]). The stable curse has none
//!   of this.
//! - **The cosmic skull** ([`VoidJob::Skull`]): used in hand by a human for 10 s (once): the
//!   curse becomes stable, or a stable one gets the void eater in hand.
//! - **The sunwalker**: no kidnapping (it burns with its own claws), no name or camouflage.
// shortcut: no void (lazy template with the wisp ride and its 60 s heal), no Unsettle stare, no
// Cosmic Dash, no space dive, no captain-key radio, no gravity slowdown or planet allergy, no
// glass-debris bleeding, space texture or 1.1 brute modifier of the cursed, no window
// deforming for others; the sunwalker's ignition, hotspots, vision hurt, water damage and Stellar
// Charge are missing (it is admin-spawned only). Add each with its mechanic.

use std::path::PathBuf;
use std::sync::Arc;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::MapLoadPlugin;
use ss13_mapload::fidelity::is_under;
use ss13_net::server::ClientToken;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::UiInterface;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiSystems};
use ss13_world::{Moved, PassFlags};

use super::objectives::RoundEnd;
use super::{AntagHudData, AntagHudUi, BrotherInfo, ObjectiveInfo};
use crate::click::ClickResolved;
use crate::click_handlers::{AddClickHandler, ClickPhase};
use crate::combat::{CombatRng, NextMove};
use crate::construction::Job;
use crate::health::{DamageType, Health, Stat};
use crate::medical::traumas::Kind;
use crate::mobs::BasicMob;
use crate::server::{PlayerMob, SpawnPoints};
use crate::status_effects::Incap;
use crate::tools::{Actor, DoAfter, do_after, now, tell, tell_mob};

pub use super::ROLE_VOIDWALKER;
pub const VOIDWALKER_MOB: &str = "/mob/living/basic/voidwalker";
pub const SUNWALKER_MOB: &str = "/mob/living/basic/voidwalker/sunwalker";
pub const COSMIC_SKULL: &str = "/obj/item/clothing/head/helmet/skull/cosmic";
pub const VOID_EATER: &str = "/obj/item/void_eater";
pub const VOID_WINDOW: &str = "/obj/structure/window/fulltile/voidwalker";
pub const GRILLE: &str = "/obj/structure/grille";
pub const NEBULA_VOMIT: &str = "/obj/effect/decal/cleanable/vomit/nebula";
/// `/obj/effect/spawner/random/glass_shards` `loot` (weights 2, 1, 1, 1).
pub const SHARDS: [(&str, u32); 4] = [
    ("/obj/item/shard", 2),
    ("/obj/item/shard/plasma", 1),
    ("/obj/item/shard/titanium", 1),
    ("/obj/item/shard/plastitanium", 1),
];
/// `PASSWINDOW` (windows' and grilles' `pass_flags_self`).
pub const PASSWINDOW: u32 = 1 << 14;
const WALL: &str = "/turf/closed/wall";
/// `WALL_CONVERT_STRENGTH`: walls with less `hardness` are too strong.
const WALL_CONVERT_STRENGTH: u32 = 40;
/// `kidnap_time`, the wall's and the skull's `do_after`s; deciseconds.
const KIDNAP_DS: f32 = 60.0;
const WALL_DS: f32 = 80.0;
const SKULL_DS: f32 = 100.0;
/// `COOLDOWN_START(src, wall_conversion, 60 SECONDS)`.
const WALL_COOLDOWN: f64 = 60.0;
/// `conversions_remaining`.
const START_CONVERSIONS: u32 = 2;
/// The regenerator: `regeneration_delay`, `brute_per_second`, `burn_per_second`.
const REGEN_DELAY: f64 = 6.0;
const REGEN_PER_SECOND: f32 = 2.0;
/// `space_camo` `reveal_after_combat`.
const REVEAL_SECS: f64 = 5.0;
/// `/datum/status_effect/void_chomped` `duration`.
const CHOMP_SECS: f64 = 15.0;
/// `voided` `vomit_frequency` (percent per `Life()`).
const VOMIT_CHANCE: f32 = 2.0;
/// `banned_from_space` `max_tile_list_size`.
const BANNED_TILES: usize = 4;

/// `/datum/job/voidwalker`.
const JOB: &str = "/datum/job/voidwalker";
pub const ANTAG_NAME: &str = "Войдволкер";
pub const SUN_ANTAG_NAME: &str = "Санволкер";
const ROUNDEND_CATEGORY: &str = "Войдволкеры";
const OBJECTIVE_NAME: &str = "Задача";
const VOID_OBJECTIVES: [&str; 3] = [
    "Покажи им красоту пустоты. Увлеки их в космическую бездну, а затем передай им истину о пустоте. Стремись просвещать, а не разрушать.",
    "Они должны увидеть то, что видели вы. Они должны пройти там, где прошли вы. Отведите их в пустоту и покажите им правду. Мёртвые не могут знать того, что знаете вы.",
    "Верните себе утраченное. Приведите своих детей чернильную тьму и верните их в свою паству.",
];
const SUN_OBJECTIVE: &str = "Научите их бояться пустоты. Покройте их своим великолепием, а затем расскажите об истине жертвоприношения. Стремитесь разрушать, а не просвещать.";
const GLASS_OBJECTIVE: &str = "Чувак, я чертовски люблю стекло.";

/// A `/mob/living/basic/voidwalker` and its own state.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Walker {
    /// The `/sunwalker` subtype: no kidnapping, no camouflage.
    pub sunwalker: bool,
    /// `conversions_remaining`.
    pub conversions: u32,
    /// The end of `wall_conversion`'s cooldown (game time).
    pub convert_ready: f64,
    /// `space_camo`'s `next_camo`.
    pub reveal_until: f64,
    /// The mob's colour before the camouflage (its alpha 255).
    look: Option<[u8; 4]>,
    camouflaged: bool,
    /// The regenerator: the last damage seen, and whether it is healing.
    last_damage: f64,
    last_total: f32,
    regenerating: bool,
}

/// `/datum/antagonist/voidwalker` or `/sunwalker` on its body.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct VoidwalkerAntag {
    pub key: String,
    pub objective: String,
    pub sunwalker: bool,
}

/// `/datum/status_effect/void_chomped`: `TRAIT_NODEATH` until `until`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct VoidChomped {
    until: f64,
    /// The body had no `TRAIT_NODEATH` of its own.
    ours: bool,
}

/// `banned_from_space`'s `tiles`: the last non-space tiles of a cursed mob.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct SpaceBan {
    tiles: Vec<TilePos>,
}

/// The cosmic skull's `uses`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CosmicSkull {
    pub uses: u32,
}

/// The voidwalker's `do_after`s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoidJob {
    /// `kidnap`: the target is the victim.
    Kidnap,
    /// `try_convert_wall`: the target is the wall.
    Wall,
    /// The cosmic skull's stare: the skull is the held item.
    Skull,
}

/// `GLOB.voidwalker_names`.
#[derive(Resource, Clone, Debug, Default)]
struct VoidwalkerNames(Vec<String>);

/// The voidwalker's info panel (tg's `AntagInfoVoidwalker`, here the default one).
pub struct VoidwalkerInfoWindow;

impl UiInterface for VoidwalkerInfoWindow {
    const NAME: &'static str = crate::antag::cult::AntagInfoGenericUi::NAME;
    type Data = ();
    type StaticData = BrotherInfo;
}

impl ServerUi for VoidwalkerInfoWindow {
    type Source = &'static VoidwalkerAntag;
    type UserParam = ();
    type Act = ();

    fn static_data(antag: &VoidwalkerAntag) -> BrotherInfo {
        info(antag, false)
    }

    fn data(_: &VoidwalkerAntag, _: Entity, _: &()) {}
}

fn info(antag: &VoidwalkerAntag, complete: bool) -> BrotherInfo {
    BrotherInfo {
        antag_name: if antag.sunwalker {
            SUN_ANTAG_NAME
        } else {
            ANTAG_NAME
        }
        .to_owned(),
        objectives: vec![ObjectiveInfo {
            count: 1,
            explanation: antag.objective.clone(),
            complete,
        }],
    }
}

pub struct VoidwalkerPlugin;

impl Plugin for VoidwalkerPlugin {
    fn build(&self, app: &mut App) {
        app.add_click_handler(ClickPhase::AttackSelf, 3389, |w, c| {
            use_skull(
                w,
                Actor {
                    client: c.client,
                    mob: c.mob,
                    item: Some(c.target),
                },
                c.target,
            )
        });
        app.register_ui::<VoidwalkerInfoWindow>()
            .add_observer(walker_added)
            .add_systems(Startup, load_names)
            .add_systems(
                Update,
                (
                    open_info.in_set(UiSystems::Handle),
                    walker_clicks,
                    walker_tick.run_if(any_with_component::<Walker>),
                    chomped_tick.run_if(any_with_component::<VoidChomped>),
                    banned_from_space,
                ),
            );
    }
}

fn load_names(world: &mut World) {
    let source: Option<PathBuf> = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.source.clone());
    let names = source
        .and_then(|s| ss13_data::read(&s.join("strings/names/voidwalker.txt")).ok())
        .map(|bytes| {
            String::from_utf8_lossy(&bytes)
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    world.insert_resource(VoidwalkerNames(names));
}

fn rng_index(world: &mut World, len: usize) -> usize {
    world.resource_mut::<CombatRng>().range(0, len as u32 - 1) as usize
}

/// `Initialize` + `unique_setup` of a voidwalker or sunwalker mob.
fn walker_added(add: On<Add<BasicMob>>, mobs: Query<&BasicMob>, mut commands: Commands) {
    let e = add.entity;
    let Ok(basic) = mobs.get(e) else {
        return;
    };
    if !is_under(&basic.def.path, VOIDWALKER_MOB) {
        return;
    }
    let sunwalker = is_under(&basic.def.path, SUNWALKER_MOB);
    commands.queue(move |world: &mut World| {
        if world.get_entity(e).is_err() || world.get::<Walker>(e).is_some() {
            return;
        }
        let pass = world.get::<PassFlags>(e).map_or(0, |p| p.0) | PASSWINDOW;
        let look = world
            .get::<ss13_shared::Appearance>(e)
            .and_then(|a| a.color);
        world.entity_mut(e).insert((
            Walker {
                sunwalker,
                conversions: START_CONVERSIONS,
                convert_ready: 0.0,
                reveal_until: 0.0,
                look,
                camouflaged: false,
                last_damage: 0.0,
                last_total: 0.0,
                regenerating: false,
            },
            PassFlags(pass),
        ));
        crate::spells::learn(world, e, crate::spells::Spell::Telepathy);
        crate::spells::refresh(world, e);
        if !sunwalker {
            let names = world
                .get_resource::<VoidwalkerNames>()
                .map(|n| n.0.clone())
                .unwrap_or_default();
            if !names.is_empty() {
                let i = rng_index(world, names.len());
                world.entity_mut(e).insert(Name::new(names[i].clone()));
            }
        }
    });
}

/// `find_space_spawn`'s candidates: the carp spawns.
fn space_spawns(world: &World) -> Vec<TilePos> {
    world
        .get_resource::<SpawnPoints>()
        .map(|s| s.carp.clone())
        .unwrap_or_default()
}

/// `can_be_selected`: a space spawn.
pub fn can_spawn(world: &mut World) -> bool {
    !space_spawns(world).is_empty()
}

/// `create_ruleset_body` + `assign_role` for each `(client, ghost)`: the bodies.
pub fn spawn_voidwalkers(world: &mut World, ghosts: &[(Entity, Entity)]) -> Vec<Entity> {
    let mut out = Vec::new();
    for &(client, ghost) in ghosts {
        let spawns = space_spawns(world);
        if spawns.is_empty() {
            break;
        }
        let pos = spawns[rng_index(world, spawns.len())];
        let Some(mob) = crate::mobs::spawn::spawn_mob(world, VOIDWALKER_MOB, pos) else {
            break;
        };
        if !crate::medical::brainmob::possess(world, client, ghost, mob) {
            world.despawn(mob);
            continue;
        }
        world.try_despawn(ghost).ok();
        add_antag(world, mob, false);
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/effects/magic/ethereal_exit.ogg", 50.0, true)
                .extrarange(-1.0),
        );
        info!("{mob} ({client}) is a voidwalker at {pos:?}");
        out.push(mob);
    }
    out
}

/// The objective's text (`voidwalker_objective/New`, `sunwalker/New`).
fn roll_objective(world: &mut World, sunwalker: bool) -> String {
    let mut rng = world.resource_mut::<CombatRng>();
    let text = if sunwalker {
        SUN_OBJECTIVE
    } else {
        VOID_OBJECTIVES[rng.range(0, VOID_OBJECTIVES.len() as u32 - 1) as usize]
    };
    if rng.chance(5.0) {
        GLASS_OBJECTIVE.to_owned()
    } else {
        text.to_owned()
    }
}

/// `add_antag_datum(/datum/antagonist/voidwalker)` (or `/sunwalker`): the objective, the
/// greeting, the panel. `false` if `mob` already is one.
pub fn add_antag(world: &mut World, mob: Entity, sunwalker: bool) -> bool {
    if world.get::<VoidwalkerAntag>(mob).is_some() || world.get_entity(mob).is_err() {
        return false;
    }
    crate::noncrew_jobs::set_assigned_role(world, mob, JOB);
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let objective = roll_objective(world, sunwalker);
    let antag = VoidwalkerAntag {
        key,
        objective,
        sunwalker,
    };
    world
        .entity_mut(mob)
        .insert((antag.clone(), AntagHudData::default()));
    let Some(client) = client else {
        return true;
    };
    let name = info(&antag, false).antag_name;
    tell(world, client, Span::Bold, format!("Вы - {name}."));
    // `announce_objectives`.
    tell(world, client, Span::Notice, "Ваши текущие задачи:");
    world.write_message(ss13_net::chat::server::SendChat {
        clients: vec![client],
        message: ChatMessage {
            channel: ChatChannel::System,
            segments: vec![
                ChatSegment::new(Span::Bold, format!("{OBJECTIVE_NAME} #1")),
                ChatSegment::new(Span::Plain, format!(": {}", antag.objective)),
            ],
        },
    });
    world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
    world.write_message(OpenUi::new::<VoidwalkerInfoWindow>(client, mob, mob, &name));
    true
}

/// `/datum/action/antag_info/Trigger` of a voidwalker.
fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if let Some(antag) = world.get::<VoidwalkerAntag>(source).cloned() {
            let name = info(&antag, false).antag_name;
            world.write_message(OpenUi::new::<VoidwalkerInfoWindow>(
                client, actor, source, &name,
            ));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Space: camouflage and regeneration

/// `space_camo` and the regenerator of each walker, frame-independent.
// Cost: per tick, one turf lookup per walker (one or two a round).
fn walker_tick(world: &mut World) {
    let now = now(world);
    let dt = world.resource::<Time>().delta_secs();
    let mut q = world.query::<(Entity, &Walker, &TilePos)>();
    let walkers: Vec<(Entity, TilePos)> = q.iter(world).map(|(e, _, &p)| (e, p)).collect();
    for (mob, pos) in walkers {
        let in_space = crate::construction::is_space(world, pos);
        let Some(mut w) = world.get::<Walker>(mob).cloned() else {
            continue;
        };
        let alive = world.get::<Stat>(mob).is_none_or(|s| *s != Stat::Dead);
        // The camouflage (`space_alpha = 0`, `non_space_alpha = 255`); the sunwalker has none.
        let camo = !w.sunwalker && alive && in_space && now >= w.reveal_until;
        if camo != w.camouflaged {
            w.camouflaged = camo;
            if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(mob) {
                look.color = if camo {
                    let [r, g, b, _] = w.look.unwrap_or([255, 255, 255, 255]);
                    Some([r, g, b, 0])
                } else {
                    w.look
                };
            }
        }
        // The regenerator: damage restarts the 6 s delay; it heals while in space and hurt.
        if let Some(health) = world.get::<Health>(mob).cloned() {
            let total = health.brute + health.burn + health.tox + health.oxy;
            if total > w.last_total + 0.01 {
                w.last_damage = now;
                w.regenerating = false;
            } else if !w.regenerating && now - w.last_damage >= REGEN_DELAY && in_space {
                w.regenerating = true;
            }
            let hurt = health.brute + health.burn > 0.0;
            if w.regenerating && (!in_space || !hurt || !alive) {
                w.regenerating = false;
            }
            if w.regenerating {
                let crit = world
                    .get::<Stat>(mob)
                    .is_some_and(|s| *s != Stat::Conscious);
                let heal = REGEN_PER_SECOND * if crit { 2.0 } else { 1.0 } * dt;
                let mut health = health;
                health.adjust(DamageType::Brute, -heal);
                health.adjust(DamageType::Burn, -heal);
                w.last_total = health.brute + health.burn + health.tox + health.oxy;
                world.entity_mut(mob).insert(health);
            } else {
                w.last_total = total;
            }
        }
        world.entity_mut(mob).insert(w);
    }
}

/// `COMSIG_ATOM_REVEAL` / an attack: out of the camouflage for 5 s.
fn reveal(world: &mut World, mob: Entity) {
    let until = now(world) + REVEAL_SECS;
    if let Some(mut w) = world.get_mut::<Walker>(mob) {
        w.reveal_until = until;
    }
}

// ---------------------------------------------------------------------------------------------
// Clicks: kidnapping, hits and walls

fn set_damage_type(world: &mut World, mob: Entity, kind: &str) {
    if let Some(mut basic) = world.get_mut::<BasicMob>(mob)
        && basic.def.melee_damage_type != kind
    {
        Arc::make_mut(&mut basic.def).melee_damage_type = kind.to_owned();
    }
}

/// `early_melee_attack` of a player's walker on what it clicked next to it.
fn walker_clicks(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    let t = now(world);
    for click in clicks {
        let Some(w) = world.get::<Walker>(click.mob).cloned() else {
            continue;
        };
        let (mob, target) = (click.mob, click.target);
        if target == mob
            || world.get::<Stat>(mob) != Some(&Stat::Conscious)
            || world.get::<NextMove>(mob).is_some_and(|n| t < n.0)
            || world.get::<DoAfter>(mob).is_some()
            || (click.mob_pos.x - click.target_pos.x).abs() > 1
            || (click.mob_pos.y - click.target_pos.y).abs() > 1
            || click.mob_pos.z != click.target_pos.z
        {
            continue;
        }
        let actor = Actor {
            client: click.client,
            mob,
            item: None,
        };
        let human = world.get::<crate::human::HumanLook>(target).is_some();
        // `try_kidnap`, then the chomp; the hit follows either way (tg's chain goes on).
        if human && !w.sunwalker {
            let kidnap = kidnap_check(world, mob, target);
            chomp(world, target);
            if kidnap {
                start_kidnap(world, actor, target);
            }
        }
        if world.get::<Health>(target).is_some() {
            // Left click: `initial(melee_damage_type)` on humans, brute on the rest; right
            // click: brute.
            let initial = world
                .get::<BasicMob>(mob)
                .map(|b| b.def.melee_damage_type.clone())
                .unwrap_or_default();
            let kind = if human && !click.modifiers.right {
                initial.clone()
            } else {
                "brute".to_owned()
            };
            set_damage_type(world, mob, &kind);
            if crate::mobs::defense::melee_attack(world, mob, target) {
                reveal(world, mob);
            }
            set_damage_type(world, mob, &initial);
            continue;
        }
        if click.modifiers.right
            && !w.sunwalker
            && crate::construction::path_of(world, target).is_some_and(|p| is_under(&p, WALL))
        {
            try_convert_wall(world, actor, &w, target);
        }
    }
}

fn balloon(world: &mut World, mob: Entity, text: &str) {
    tell_mob(world, mob, Span::Warning, text);
}

/// `apply_status_effect(/datum/status_effect/void_chomped)`: no death for 15 s (refreshed).
pub fn chomp(world: &mut World, victim: Entity) {
    let until = now(world) + CHOMP_SECS;
    let ours = match world.get::<VoidChomped>(victim) {
        Some(c) => c.ours,
        None => world
            .get::<Health>(victim)
            .is_some_and(|h| !h.mods.no_death),
    };
    if ours && let Some(mut health) = world.get_mut::<Health>(victim) {
        health.mods.no_death = true;
    }
    world.entity_mut(victim).insert(VoidChomped { until, ours });
}

fn chomped_tick(world: &mut World) {
    let now = now(world);
    let mut q = world.query::<(Entity, &VoidChomped)>();
    let over: Vec<(Entity, bool)> = q
        .iter(world)
        .filter(|(_, c)| c.until <= now)
        .map(|(e, c)| (e, c.ours))
        .collect();
    for (mob, ours) in over {
        world.entity_mut(mob).remove::<VoidChomped>();
        if ours && let Some(mut health) = world.get_mut::<Health>(mob) {
            health.mods.no_death = false;
        }
    }
}

/// `IS_UNCONSCIOUS_OR_CRIT`.
fn out_or_crit(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) != Some(&Stat::Conscious)
        || crate::status_effects::knocked_out(world, mob)
}

/// `try_kidnap`: whether to start taking the victim (an unconscious or critical one on space
/// or nebula vomit); the others get their balloon or sleep.
fn kidnap_check(world: &mut World, mob: Entity, victim: Entity) -> bool {
    if crate::medical::traumas::has(world, victim, Kind::Voided { stable: false }) {
        balloon(world, mob, "уже опустошен!");
        crate::status_effects::set(world, victim, Incap::Sleeping, 30.0);
        return false;
    }
    if world.get::<Stat>(victim) == Some(&Stat::Dead) {
        balloon(world, mob, "мёртв!");
        return false;
    }
    if !out_or_crit(world, victim) {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(victim) else {
        return false;
    };
    if !crate::construction::is_space(world, pos) && !nebula_on(world, pos) {
        balloon(world, mob, "не в космосе!");
        return false;
    }
    // `kidnapping`: the walker's own `do_after` already stops a second one.
    true
}

fn nebula_on(world: &mut World, pos: TilePos) -> bool {
    let mut q = world.query::<(&ss13_mapload::DmObj, &TilePos)>();
    q.iter(world)
        .any(|(o, p)| *p == pos && is_under(&o.path, NEBULA_VOMIT))
}

/// `kidnap`: the victim paralyzed for the 6 s.
fn start_kidnap(world: &mut World, actor: Actor, victim: Entity) {
    crate::status_effects::apply(world, victim, Incap::Paralyze, KIDNAP_DS / 10.0);
    do_after(
        world,
        actor,
        victim,
        KIDNAP_DS,
        0,
        Job::Voidwalker(VoidJob::Kidnap),
    );
}

/// The end of a voidwalker `do_after` (`construction`'s job dispatch).
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: VoidJob) {
    match job {
        VoidJob::Kidnap => {
            // `extra_checks = check_incapacitated`.
            if world.get::<Walker>(mob).is_some() && out_or_crit(world, action.target) {
                take_them(world, mob, action.target);
            }
        }
        VoidJob::Wall => convert_wall(world, mob, action.target),
        VoidJob::Skull => {
            if let Some(skull) = action.item {
                stare_into_skull(world, mob, skull);
            }
        }
    }
}

/// `take_them`: the curse; without the void, tg's fallback (a random station tile, healed).
pub fn take_them(world: &mut World, walker: Entity, victim: Entity) {
    if world.get::<crate::human::HumanLook>(victim).is_some()
        && let Some(brain) =
            crate::medical::organs::organ(world, victim, crate::medical::organs::Slot::Brain)
        && crate::medical::traumas::gain(world, brain, Kind::Voided { stable: false }, None)
    {
        world.entity_mut(victim).insert(SpaceBan::default());
    }
    if let Some(mut w) = world.get_mut::<Walker>(walker) {
        w.conversions += 1;
    }
    let Some(pos) = crate::events::electrical_storm::random_station_turf(world) else {
        return;
    };
    world.entity_mut(victim).insert(pos);
    if let Some(mut health) = world.get_mut::<Health>(victim) {
        health.adjust(DamageType::Brute, -80.0);
        health.adjust(DamageType::Burn, -20.0);
    }
    info!("{victim} was voidwalker kidnapped by {walker} and dumped at {pos:?}");
}

/// `try_convert_wall`: the checks, the 8 s.
fn try_convert_wall(world: &mut World, actor: Actor, w: &Walker, wall: Entity) {
    let mob = actor.mob;
    if w.conversions == 0 {
        balloon(world, mob, "нужны ещё похищения!");
        return;
    }
    let t = now(world);
    if t < w.convert_ready {
        let left = (w.convert_ready - t).ceil() as u32;
        balloon(world, mob, &format!("нужно подождать {left} сек.!"));
        return;
    }
    if !wall_soft_enough(world, wall) {
        balloon(world, mob, "слишком крепкая!");
        return;
    }
    if let Some(&pos) = world.get::<TilePos>(wall) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/effects/magic/blind.ogg", 100.0, true),
        );
    }
    balloon(world, mob, "открывает окно...");
    do_after(
        world,
        actor,
        wall,
        WALL_DS,
        0,
        Job::Voidwalker(VoidJob::Wall),
    );
}

/// `check_wall_validity`: `hardness >= WALL_CONVERT_STRENGTH`.
fn wall_soft_enough(world: &World, wall: Entity) -> bool {
    crate::construction::path_of(world, wall)
        .and_then(|p| crate::events::meteors::wall_hardness(&p))
        .is_some_and(|h| h >= WALL_CONVERT_STRENGTH)
}

/// The end of the 8 s: the wall and the soft walls around it become void windows on grilles.
fn convert_wall(world: &mut World, mob: Entity, wall: Entity) {
    let Some(w) = world.get::<Walker>(mob).cloned() else {
        return;
    };
    if w.conversions == 0 {
        return;
    }
    let Some(&center) = world.get::<TilePos>(wall) else {
        return;
    };
    let mut targets = vec![center];
    for dx in -1..=1 {
        for dy in -1..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let pos = TilePos::new(center.x + dx, center.y + dy, center.z);
            if let Some(turf) = crate::construction::turf_at(world, pos)
                && crate::construction::path_of(world, turf).is_some_and(|p| is_under(&p, WALL))
                && wall_soft_enough(world, turf)
            {
                targets.push(pos);
            }
        }
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(center, "sound/effects/magic/blind.ogg", 100.0, true),
    );
    for pos in targets {
        crate::construction::scrape_away(world, pos, false);
        crate::construction::spawn_obj(world, VOID_WINDOW, pos);
        crate::construction::spawn_obj(world, GRILLE, pos);
    }
    let ready = now(world) + WALL_COOLDOWN;
    if let Some(mut w) = world.get_mut::<Walker>(mob) {
        w.conversions -= 1;
        w.convert_ready = ready;
    }
}

// ---------------------------------------------------------------------------------------------
// Death and the cosmic skull

/// `voidwalker/death` (the mob is `DEL_ON_DEATH`): glass shards, the cosmic skull, the shatter.
pub fn on_death(world: &mut World, mob: Entity, pos: TilePos) {
    if world.get::<Walker>(mob).is_none() {
        return;
    }
    let count = world.resource_mut::<CombatRng>().range(4, 6);
    let total: u32 = SHARDS.iter().map(|(_, w)| w).sum();
    for _ in 0..count {
        let mut roll = world.resource_mut::<CombatRng>().range(1, total);
        let mut pick = SHARDS[0].0;
        for (path, weight) in SHARDS {
            if roll <= weight {
                pick = path;
                break;
            }
            roll -= weight;
        }
        crate::construction::spawn_obj(world, pick, pos);
    }
    if let Some(skull) = crate::construction::spawn_obj(world, COSMIC_SKULL, pos) {
        world.entity_mut(skull).insert(CosmicSkull { uses: 1 });
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, "shatter", 100.0, true),
    );
}

/// The cosmic skull's `attack_self` (`true`: the use was the skull's).
pub fn use_skull(world: &mut World, actor: Actor, skull: Entity) -> bool {
    let Some(item) = world.get::<ss13_mapload::DmItem>(skull) else {
        return false;
    };
    if !is_under(&item.path, COSMIC_SKULL) {
        return false;
    }
    if world.get::<Walker>(actor.mob).is_some() {
        tell(
            world,
            actor.client,
            Span::Danger,
            "О БОЖЕ НЕТ!!!! ПОООЧЕМУ??!!! КТО МОГ ЭТО СДЕЛАТЬ?!!",
        );
        return true;
    }
    let uses = world.get::<CosmicSkull>(skull).map_or(1, |s| s.uses);
    if uses == 0 || world.get::<crate::human::HumanLook>(actor.mob).is_none() {
        return true;
    }
    let name = crate::medical::name_of(world, skull);
    tell(
        world,
        actor.client,
        Span::Notice,
        format!("Вы начинаете всматриваться в {name}..."),
    );
    do_after(
        world,
        Actor {
            item: Some(skull),
            ..actor
        },
        actor.mob,
        SKULL_DS,
        0,
        Job::Voidwalker(VoidJob::Skull),
    );
    true
}

/// The end of the skull's 10 s: the curse becomes stable, or a stable one gets the void eater.
fn stare_into_skull(world: &mut World, mob: Entity, skull: Entity) {
    let uses = world.get::<CosmicSkull>(skull).map_or(1, |s| s.uses);
    if uses == 0 {
        return;
    }
    world
        .entity_mut(skull)
        .insert(CosmicSkull { uses: uses - 1 });
    if uses == 1 {
        let drained = world
            .get_resource::<ss13_mapload::IconIndex>()
            .and_then(|i| {
                i.get(
                    "icons/obj/weapons/voidwalker_items.dmi",
                    "cosmic_skull_drained",
                )
            });
        if let (Some(id), Some(mut look)) =
            (drained, world.get_mut::<ss13_shared::Appearance>(skull))
        {
            look.icon_state = id;
        }
    }
    let Some(brain) =
        crate::medical::organs::organ(world, mob, crate::medical::organs::Slot::Brain)
    else {
        return;
    };
    let pos = world.get::<TilePos>(mob).copied();
    if crate::medical::traumas::of_mob(world, mob)
        .iter()
        .any(|t| t.kind == Kind::Voided { stable: true })
    {
        if let Some(pos) = pos
            && let Some(eater) = crate::construction::spawn_obj(world, VOID_EATER, pos)
        {
            world.entity_mut(eater).insert(crate::inventory::NoDrop);
            if let Some(inventory) = world.get::<crate::inventory::Inventory>(mob).cloned() {
                crate::storage::put_in_hands(world, mob, &inventory, eater);
            }
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(pos, "sound/effects/blob/blobattack.ogg", 60.0, true),
            );
        }
    } else {
        crate::medical::traumas::remove_kind(world, brain, Kind::Voided { stable: false });
        crate::medical::traumas::gain(world, brain, Kind::Voided { stable: true }, None);
    }
    tell_mob(
        world,
        mob,
        Span::Notice,
        "И перед вами открывается целый мир.",
    );
    if let Some(pos) = pos {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/effects/curse/curse5.ogg", 60.0, true),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The curse

/// `/datum/brain_trauma/voided/on_life`: `prob(2)` per `Life()` (2 s) to vomit nebula and
/// fall, as a per-second chance over `secs`.
pub fn voided_life(world: &mut World, mob: Entity, stable: bool, rng: &mut CombatRng, secs: f32) {
    if stable || !crate::medical::wounds::spt_prob(rng, VOMIT_CHANCE / 2.0, secs) {
        return;
    }
    crate::medical::diseases::vomit(world, mob, 10.0, true, false, 0);
    crate::status_effects::knockdown(world, mob, 2.0, 0.0);
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::construction::spawn_obj(world, NEBULA_VOMIT, pos);
    }
}

/// `banned_from_space`: a cursed mob's step into space sends it back to the oldest of its
/// last 4 other tiles (or a random station tile). The component goes with the curse.
// Cost: per tick, a lookup per move; the trauma is read only for the cursed (a handful).
fn banned_from_space(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    for m in moved {
        let Some(mut ban) = world.get::<SpaceBan>(m.entity).cloned() else {
            continue;
        };
        if !crate::medical::traumas::of_mob(world, m.entity)
            .iter()
            .any(|t| t.kind == Kind::Voided { stable: false })
        {
            world.entity_mut(m.entity).remove::<SpaceBan>();
            continue;
        }
        let Some(&pos) = world.get::<TilePos>(m.entity) else {
            continue;
        };
        if crate::construction::is_space(world, pos) {
            let back = match ban.tiles.first() {
                Some(&t) => Some(t),
                None => crate::events::electrical_storm::random_station_turf(world),
            };
            if let Some(back) = back {
                world.entity_mut(m.entity).insert(back);
            }
            continue;
        }
        ban.tiles.push(pos);
        if ban.tiles.len() > BANNED_TILES {
            ban.tiles.remove(0);
        }
        world.entity_mut(m.entity).insert(ban);
    }
}

// ---------------------------------------------------------------------------------------------
// Round end

/// `voidwalker_objective/check_completion`: alive.
fn survived(world: &World, mob: Entity) -> bool {
    world.get_entity(mob).is_ok() && world.get::<Stat>(mob).is_some_and(|s| *s != Stat::Dead)
}

/// The "Войдволкеры" section: each voidwalker or sunwalker with its objective.
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let mut q = world.query::<(Entity, &VoidwalkerAntag)>();
    let mut walkers: Vec<(Entity, VoidwalkerAntag)> =
        q.iter(world).map(|(e, a)| (e, a.clone())).collect();
    if walkers.is_empty() {
        return Vec::new();
    }
    walkers.sort_by_key(|(e, _)| *e);
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = vec![line(vec![ChatSegment::new(
        Span::Bold,
        format!("{ROUNDEND_CATEGORY} были:"),
    )])];
    for (mob, antag) in walkers {
        out.push(super::traitor::print_player(world, mob, &antag.key));
        let won = survived(world, mob);
        out.push(line(vec![
            ChatSegment::new(Span::Bold, format!("{OBJECTIVE_NAME} #1")),
            ChatSegment::new(Span::Plain, format!(": {} ", antag.objective)),
            if won {
                ChatSegment::new(Span::GreenText, "Успех!")
            } else {
                ChatSegment::new(Span::RedText, "Неудача.")
            },
        ]));
        let name = info(&antag, won).antag_name;
        out.push(line(vec![if won {
            ChatSegment::new(Span::GreenText, format!("{name} был успешен!"))
        } else {
            ChatSegment::new(Span::RedText, format!("{name} провалился!"))
        }]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_glass_objective_replaces_the_text_one_time_in_twenty() {
        let mut world = World::new();
        world.insert_resource(CombatRng(3));
        let mut glass = 0;
        for _ in 0..2000 {
            let text = roll_objective(&mut world, false);
            if text == GLASS_OBJECTIVE {
                glass += 1;
            } else {
                assert!(VOID_OBJECTIVES.contains(&text.as_str()));
            }
        }
        assert!((50..=150).contains(&glass), "{glass}");
        let sun = roll_objective(&mut world, true);
        assert!(sun == SUN_OBJECTIVE || sun == GLASS_OBJECTIVE);
    }

    #[test]
    fn chomping_holds_off_death_and_gives_it_back() {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        let victim = world.spawn(Health::default()).id();
        chomp(&mut world, victim);
        assert!(world.get::<Health>(victim).unwrap().mods.no_death);
        // Refreshing keeps who owned the trait.
        chomp(&mut world, victim);
        assert!(world.get::<VoidChomped>(victim).unwrap().ours);
        world.get_mut::<VoidChomped>(victim).unwrap().until = -1.0;
        chomped_tick(&mut world);
        assert!(!world.get::<Health>(victim).unwrap().mods.no_death);
        assert!(world.get::<VoidChomped>(victim).is_none());
        // A body that cannot die anyway keeps its trait.
        world.get_mut::<Health>(victim).unwrap().mods.no_death = true;
        chomp(&mut world, victim);
        world.get_mut::<VoidChomped>(victim).unwrap().until = -1.0;
        chomped_tick(&mut world);
        assert!(world.get::<Health>(victim).unwrap().mods.no_death);
    }
}

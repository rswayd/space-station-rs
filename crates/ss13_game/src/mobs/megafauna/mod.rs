//! Lavaland megafauna (server), after tg `/mob/living/simple_animal/hostile/megafauna`
//! (`_megafauna.dm`), the hostile AI that drives them (`hostile.dm`) and their abilities
//! (`code/datums/actions/mobs/`).
//!
//! A megafauna is a basic mob ([`super::BasicMob`], its vars read by the converter from the
//! simple animal: health, melee, `del_on_death`, flying) with a [`Megafauna`] brain:
//!
//! - Every `SSnpcpool` tick (2 s, tg `handle_automated_action` -> `MoveToTarget`): it keeps its
//!   target while it is within `vision_range` (5, 18 once aggroed; through walls: megafauna
//!   smash their way), else takes the nearest living mob in sight that is not of its faction
//!   (dead ones too: `stat_attack = DEAD`). Not next to it, it opens fire (its abilities); it
//!   walks at its target (`Goto`, the AI movement of [`super::ai`]); next to it, it strikes
//!   after the `basic_mob_attack_telegraph` delay (0.25 s) and opens fire again. A dead target
//!   is devoured (`devour`: 500 brute, `gutted`, half the victim's max health healed off the
//!   station).
//! - Abilities share one cooldown (`MOB_SHARED_COOLDOWN_1`) and block melee for their cooldown
//!   (`next_melee_use_time`); their sleeps and timers are timed steps of the mob's queue,
//!   dropped when it dies (`SLEEP_CHECK_DEATH`). [`bubblegum`], [`colossus`], [`drake`] and
//!   [`legion`] hold them; the icemoon clockwork defender has none (melee twice a tick, its
//!   alloy on death).
//! - Explosions deal 250/100/50 brute and never gib (`ex_act`); death drops the necropolis
//!   chest with its loot (`loot`, `necropolis_chests.dm` `PopulateContents`).
//! - [`placement`]: the bosses on the mining level (tg biome population, `SPAWN_MEGAFAUNA`).
// shortcut: no wall smashing (`environment_smash`), GPS signals, achievements, crusher loot or
// trophies, sentient (player) control or ability buttons; no organ removal on `devour`.

pub mod bubblegum;
pub mod colossus;
pub mod drake;
pub mod legion;
pub mod placement;

use bevy::ecs::message::Messages;
use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mobs::MobDef;
use ss13_net::camera::server::SendCameraEffect;
use ss13_net::chat::server::SendChat;
use ss13_net::server::Networked;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::CameraEffect;
use ss13_shared::{Appearance, Layer, MouseOpacity, TilePos};
use ss13_world::SpatialIndex;

use super::BasicMob;
use super::ai::movement::Target;
use super::ai::{ALWAYS_HIGH_PRIORITY, AiController, RUN_WHILE_UNWATCHED};
use crate::combat::CombatRng;
use crate::explosions::Severity;
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType, Health, HealthSystems, Stat};
use crate::server::PlayerMob;

pub const BUBBLEGUM: &str = "/mob/living/simple_animal/hostile/megafauna/bubblegum";
/// Its copies are images of the hallucination charges, never spawned as mobs.
const BUBBLEGUM_HALLUCINATION: &str =
    "/mob/living/simple_animal/hostile/megafauna/bubblegum/hallucination";
pub const COLOSSUS: &str = "/mob/living/simple_animal/hostile/megafauna/colossus";
pub const DRAKE: &str = "/mob/living/simple_animal/hostile/megafauna/dragon";
/// The admin-spawned lesser drake (`dragon/lesser`): a [`Kind::Drake`] with fewer abilities.
pub const LESSER_DRAKE: &str = "/mob/living/simple_animal/hostile/megafauna/dragon/lesser";
/// The icemoon clockwork ruin's guard (`clockwork_knight.dm`): melee only.
pub const CLOCKWORK_DEFENDER: &str =
    "/mob/living/simple_animal/hostile/megafauna/clockwork_defender";
/// `clockwork_defender` `loot`.
const CLOCKWORK_ALLOY: &str = "/obj/item/clockwork_alloy";

/// `SSnpcpool` `wait`.
pub const NPC_TICK_SECS: f64 = 2.0;
/// `vision_range`, `aggro_vision_range`.
pub const VISION_RANGE: u32 = 5;
pub const AGGRO_VISION_RANGE: u32 = 18;
/// `attack_delay` (`basic_mob_attack_telegraph`'s `telegraph_duration`).
const ATTACK_DELAY_SECS: f64 = 0.25;
/// `devour`: brute to the victim.
const DEVOUR_DAMAGE: f32 = 500.0;
/// Steps one mob may run in one tick (catching up after a long frame).
const MAX_STEPS_PER_TICK: usize = 512;

/// `FLY_LAYER`, `ABOVE_MOB_LAYER`, `BELOW_MOB_LAYER`; `GAME_PLANE`, `ABOVE_GAME_PLANE`.
pub(super) const FLY_LAYER: f32 = 5.0;
pub(super) const ABOVE_MOB_LAYER: f32 = 4.1;
pub(super) const BELOW_MOB_LAYER: f32 = 3.7;
pub(super) const GAME_PLANE: i16 = -4;
pub(super) const ABOVE_GAME_PLANE: i16 = -3;

/// The ported bosses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Bubblegum,
    Colossus,
    Drake,
    Legion,
    ClockworkDefender,
}

impl Kind {
    pub fn of(path: &str) -> Option<Self> {
        if is_under(path, BUBBLEGUM) && !is_under(path, BUBBLEGUM_HALLUCINATION) {
            Some(Self::Bubblegum)
        } else if is_under(path, COLOSSUS) {
            Some(Self::Colossus)
        } else if is_under(path, DRAKE) {
            Some(Self::Drake)
        } else if is_under(path, legion::LEGION) {
            Some(Self::Legion)
        } else if is_under(path, CLOCKWORK_DEFENDER) {
            Some(Self::ClockworkDefender)
        } else {
            None
        }
    }
}

/// The hostile AI and ability state of a megafauna.
#[derive(Component, Debug)]
pub struct Megafauna {
    pub kind: Kind,
    /// tg `target`.
    pub target: Option<Entity>,
    /// `Aggro()`: `vision_range` is `aggro_vision_range` until `LoseAggro()`.
    pub aggro: bool,
    next_think: f64,
    /// An ability runs until then (tg `disable_cooldown_actions` during `Activate`).
    pub busy_until: f64,
    /// The shared cooldown of its abilities.
    pub ability_ready: f64,
    /// The last ability's `next_melee_use_time`.
    pub melee_ready: f64,
    /// `ranged_cooldown`.
    pub ranged_ready: f64,
    /// Health at the last tick: a loss looks around (tg `toggle_ai_on_damage` -> `FindTarget`).
    last_health: f32,
    /// Timed steps of the running abilities.
    queue: Vec<(f64, Step)>,
    pub bubblegum: bubblegum::State,
    pub colossus: colossus::State,
    pub drake: drake::State,
    pub legion: legion::State,
}

impl Megafauna {
    fn new(kind: Kind, think_at: f64, health: f32) -> Self {
        Self {
            kind,
            target: None,
            aggro: false,
            next_think: think_at,
            busy_until: 0.0,
            ability_ready: 0.0,
            melee_ready: 0.0,
            ranged_ready: 0.0,
            last_health: health,
            queue: Vec::new(),
            bubblegum: bubblegum::State::default(),
            colossus: colossus::State::default(),
            drake: drake::State::default(),
            legion: legion::State::default(),
        }
    }

    /// Runs `step` at game time `at`.
    pub(super) fn at(&mut self, at: f64, step: Step) {
        self.queue.push((at, step));
    }

    /// An ability may start (`IsAvailable` of the shared cooldown group).
    pub fn ability_available(&self, now: f64) -> bool {
        now >= self.busy_until && now >= self.ability_ready
    }

    /// `StartCooldown(cooldown)` of an ability ending at `end`.
    pub(super) fn cooldown(&mut self, end: f64, cooldown: f64) {
        self.busy_until = end;
        self.ability_ready = end + cooldown;
        self.melee_ready = self.melee_ready.max(end + cooldown);
    }

    pub fn pending_steps(&self) -> usize {
        self.queue.len()
    }
}

/// One timed step of a running ability.
#[derive(Debug)]
pub enum Step {
    /// The telegraphed strike lands (`basic_mob_attack_telegraph` -> `AttackingTarget`).
    Melee(Entity),
    Bubblegum(bubblegum::Step),
    Colossus(colossus::Step),
    Drake(drake::Step),
    Legion(legion::Step),
}

/// tg `/datum/status_effect/gutted`: devoured; while dead, never a target again.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Gutted;

/// A `/obj/effect/temp_visual` of the abilities: gone at `until`.
#[derive(Component, Clone, Copy, Debug)]
pub struct TempVisual {
    pub until: f64,
}

/// What a temp visual looks like.
#[derive(Clone, Copy, Debug)]
pub(super) struct Visual {
    pub icon: &'static str,
    pub state: &'static str,
    pub secs: f64,
    pub layer: f32,
    pub plane: i16,
    pub offset: [i16; 2],
    pub color: Option<[u8; 4]>,
}

impl Visual {
    pub const fn new(icon: &'static str, state: &'static str, secs: f64) -> Self {
        Self {
            icon,
            state,
            secs,
            layer: ABOVE_MOB_LAYER,
            plane: GAME_PLANE,
            offset: [0, 0],
            color: None,
        }
    }
}

pub(super) fn build(app: &mut App) {
    app.add_systems(PostStartup, placement::place).add_systems(
        Update,
        (
            think,
            run_queues,
            drake::land_strikes,
            bubblegum::bleed,
            legion::sentinels,
            expire_visuals,
        )
            .chain()
            .after(crate::click::ResolveClicks)
            .before(HealthSystems),
    );
}

/// tg `Initialize` of a megafauna: its brain, and an AI controller for its steps (its own
/// ticker plans; the controller only walks, always on: megafauna hunt off-screen too).
pub(super) fn attach(world: &mut World, mob: Entity, def: &MobDef) {
    let Some(kind) = Kind::of(&def.path) else {
        return;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    // Spread the ticks of many bosses over the subsystem's wait.
    let offset = f64::from(mob.index_u32() % 20) * NPC_TICK_SECS / 20.0;
    let mut controller = AiController::default();
    controller.kind = def.path.clone();
    controller.traits = RUN_WHILE_UNWATCHED | ALWAYS_HIGH_PRIORITY;
    let mut brain = Megafauna::new(kind, now + offset, def.max_health);
    brain.drake.lesser = is_under(&def.path, LESSER_DRAKE);
    brain.legion.size = legion::size_of(&def.path);
    world.entity_mut(mob).insert((brain, controller));
}

/// The ported megafauna kind of `mob`.
pub fn kind_of(world: &World, mob: Entity) -> Option<Kind> {
    world.get::<Megafauna>(mob).map(|m| m.kind)
}

/// Queues `step` of `mob` at `at`.
pub(super) fn schedule(world: &mut World, mob: Entity, at: f64, step: Step) {
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.at(at, step);
    }
}

use crate::tools::now;

/// `SSnpcpool` -> `handle_automated_action` of every megafauna whose tick is due.
// Cost: every 2 s per boss (a handful on the mining level) one scan of the tiles within its
// vision range (11x11, 37x37 aggroed) when it has no target.
fn think(world: &mut World) {
    let now = now(world);
    let due: Vec<Entity> = world
        .query::<(Entity, &Megafauna)>()
        .iter(world)
        .filter(|(_, m)| now >= m.next_think)
        .map(|(e, _)| e)
        .collect();
    for mob in due {
        if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
            m.next_think = now + NPC_TICK_SECS;
        }
        think_one(world, mob, now);
    }
}

fn think_one(world: &mut World, mob: Entity, now: f64) {
    let (Some(&pos), Some(&stat), Some(health)) = (
        world.get::<TilePos>(mob),
        world.get::<Stat>(mob),
        world.get::<Health>(mob).map(Health::health),
    ) else {
        return;
    };
    if stat != Stat::Conscious {
        stop_moving(world, mob);
        return;
    }
    let Some(m) = world.get::<Megafauna>(mob) else {
        return;
    };
    let (current, aggro, hurt) = (m.target, m.aggro, health < m.last_health);
    let (vision_range, aggro_range) = vision(m.kind);
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.last_health = health;
    }
    if moving_itself(world, mob) {
        return;
    }
    let vision = if aggro { aggro_range } else { vision_range };
    // `MoveToTarget`: a target out of sight is still chased within the vision range
    // (`environment_smash`), shot at too (`ranged_ignores_vision`).
    let kept = current.filter(|&t| {
        valid_target(world, mob, t)
            && world
                .get::<TilePos>(t)
                .and_then(|&p| p.chebyshev(pos))
                .is_some_and(|d| d <= vision)
    });
    let range = if hurt { aggro_range } else { vision };
    let Some(target) = kept.or_else(|| find_target(world, mob, pos, range)) else {
        lose_target(world, mob);
        return;
    };
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.target = Some(target);
        m.aggro = true;
    }
    let Some(&to) = world.get::<TilePos>(target) else {
        return;
    };
    let adjacent = world
        .resource::<SpatialIndex>()
        .adjacent(pos, to, |e| e == target);
    if !adjacent {
        open_fire(world, mob, target);
    }
    if !moving_itself(world, mob) {
        // `Goto(target, move_to_delay, minimum_distance)`.
        let min_dist = match world.get::<Megafauna>(mob).map(|m| m.kind) {
            Some(Kind::Bubblegum) => bubblegum::minimum_distance(world, mob, target, now),
            Some(Kind::Legion) => legion::minimum_distance(world, mob, now),
            _ => 1,
        };
        if let Some(mut c) = world.get_mut::<AiController>(mob) {
            c.movement.start(Target::Entity(target), min_dist);
        }
        if adjacent {
            melee_action(world, mob, target, now);
        }
    }
}

/// `vision_range` and `aggro_vision_range` of `kind`.
fn vision(kind: Kind) -> (u32, u32) {
    match kind {
        Kind::Legion => (13, AGGRO_VISION_RANGE),
        Kind::ClockworkDefender => (9, 9),
        _ => (VISION_RANGE, AGGRO_VISION_RANGE),
    }
}

/// An ability moves the boss (the drake's swoop, Bubblegum's charges): its AI neither walks
/// nor strikes.
fn moving_itself(world: &World, mob: Entity) -> bool {
    drake::swooping(world, mob) || bubblegum::charging(world, mob) || legion::flying(world, mob)
}

/// `CanAttack` of a megafauna: a living mob (dead too) not of its faction, not gutted.
pub fn valid_target(world: &World, mob: Entity, target: Entity) -> bool {
    let Ok(e) = world.get_entity(target) else {
        return false;
    };
    target != mob
        && e.contains::<Health>()
        && e.contains::<TilePos>()
        && !e.contains::<crate::ghost::Ghost>()
        && !e.contains::<crate::closets::InCloset>()
        && !(e.contains::<Gutted>() && e.get::<Stat>() == Some(&Stat::Dead))
        && !super::same_faction(world, mob, target)
}

/// `FindTarget` -> `PickTarget`: the nearest valid mob in sight within `range`.
fn find_target(world: &World, mob: Entity, pos: TilePos, range: u32) -> Option<Entity> {
    let index = world.resource::<SpatialIndex>();
    index
        .in_radius(pos, range)
        .filter(|&e| valid_target(world, mob, e))
        .filter_map(|e| {
            let p = *world.get::<TilePos>(e)?;
            super::ai::behaviors::can_see(world, pos, p, range)
                .then(|| (pos.chebyshev(p).unwrap_or(u32::MAX), e))
        })
        .min_by_key(|&(d, e)| (d, e.to_bits()))
        .map(|(_, e)| e)
}

/// `LoseTarget` -> `LoseAggro`.
fn lose_target(world: &mut World, mob: Entity) {
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.target = None;
        m.aggro = false;
    }
    stop_moving(world, mob);
}

pub(super) fn stop_moving(world: &mut World, mob: Entity) {
    if let Some(mut c) = world.get_mut::<AiController>(mob) {
        c.movement.stop();
    }
}

/// `OpenFire` of the boss, behind `ranged_cooldown`.
fn open_fire(world: &mut World, mob: Entity, target: Entity) {
    let now = now(world);
    let Some((kind, ready)) = world
        .get::<Megafauna>(mob)
        .map(|m| (m.kind, m.ranged_ready))
    else {
        return;
    };
    if now < ready {
        return;
    }
    match kind {
        Kind::Bubblegum => bubblegum::open_fire(world, mob, target, now),
        Kind::Colossus => colossus::open_fire(world, mob, target, now),
        Kind::Drake => drake::open_fire(world, mob, target, now),
        Kind::Legion => legion::open_fire(world, mob, target, now),
        // `clockwork_defender/OpenFire`: nothing.
        Kind::ClockworkDefender => {}
    }
}

/// `MeleeAction` through the telegraph: the strike lands after `attack_delay` if the target is
/// still next to it.
fn melee_action(world: &mut World, mob: Entity, target: Entity, now: f64) {
    let Some(m) = world.get::<Megafauna>(mob) else {
        return;
    };
    let pending = m.queue.iter().any(|(_, s)| matches!(s, Step::Melee(_)));
    if now < m.melee_ready || pending {
        return;
    }
    schedule(world, mob, now + ATTACK_DELAY_SECS, Step::Melee(target));
    // `rapid_melee = 2`: two strikes per AI tick, a second apart.
    if kind_of(world, mob) == Some(Kind::ClockworkDefender) {
        schedule(
            world,
            mob,
            now + ATTACK_DELAY_SECS + NPC_TICK_SECS / 2.0,
            Step::Melee(target),
        );
    }
}

/// `/mob/living/simple_animal/hostile/megafauna/AttackingTarget`.
fn attacking_target(world: &mut World, mob: Entity, target: Entity) {
    let now = now(world);
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(mob), world.get::<TilePos>(target)) else {
        return;
    };
    let near = world
        .resource::<SpatialIndex>()
        .adjacent(from, to, |e| e == target);
    let ready = world
        .get::<Megafauna>(mob)
        .is_some_and(|m| now >= m.melee_ready);
    if !near || !ready || !valid_target(world, mob, target) || moving_itself(world, mob) {
        return;
    }
    if world.get::<Stat>(target) == Some(&Stat::Dead) {
        devour(world, mob, target);
        return;
    }
    super::defense::melee_attack(world, mob, target);
    if kind_of(world, mob) == Some(Kind::Legion) {
        legion::after_attack(world, mob, target);
    }
    open_fire(world, mob, target);
}

/// `devour` -> `celebrate_kill`.
fn devour(world: &mut World, mob: Entity, victim: Entity) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let name = crate::security::name_of(world, mob);
    let victim_name = crate::security::name_of(world, victim);
    show(
        world,
        pos,
        Span::Danger,
        format!("{name} disembowels {victim_name}!"),
    );
    // NPC monsters do not heal on the station.
    let station = crate::events::controller::station_levels(world).contains(&pos.z);
    let heal = world.get::<Health>(victim).map_or(0.0, |h| h.max) * 0.5;
    if !station && let Some(mut health) = world.get_mut::<Health>(mob) {
        health.adjust(DamageType::Brute, -heal);
    }
    super::defense::hurt(world, victim, DamageType::Brute, DEVOUR_DAMAGE, None);
    world.entity_mut(victim).insert((Stat::Dead, Gutted));
    lose_target(world, mob);
    // `bubblegum/devour`: bubblegum in a dish.
    if kind_of(world, mob) == Some(Kind::Bubblegum) {
        crate::construction::spawn_obj(world, bubblegum::GUM, pos);
    }
}

/// Runs the due steps of every megafauna in time order; a dead one's are dropped.
fn run_queues(world: &mut World) {
    let now = now(world);
    let mobs: Vec<Entity> = world
        .query::<(Entity, &Megafauna)>()
        .iter(world)
        .filter(|(_, m)| m.queue.iter().any(|(t, _)| *t <= now))
        .map(|(e, _)| e)
        .collect();
    for mob in mobs {
        for _ in 0..MAX_STEPS_PER_TICK {
            let alive = world.get::<Stat>(mob).is_some_and(|s| *s != Stat::Dead);
            let Some(mut m) = world.get_mut::<Megafauna>(mob) else {
                break;
            };
            if !alive {
                m.queue.clear();
                break;
            }
            let Some(i) = m
                .queue
                .iter()
                .enumerate()
                .filter(|(_, (t, _))| *t <= now)
                .min_by(|a, b| a.1.0.total_cmp(&b.1.0))
                .map(|(i, _)| i)
            else {
                break;
            };
            let (at, step) = m.queue.swap_remove(i);
            match step {
                Step::Melee(target) => attacking_target(world, mob, target),
                Step::Bubblegum(step) => bubblegum::run(world, mob, at, step),
                Step::Colossus(step) => colossus::run(world, mob, at, step),
                Step::Drake(step) => drake::run(world, mob, at, step),
                Step::Legion(step) => legion::run(world, mob, at, step),
            }
        }
    }
}

fn expire_visuals(time: Res<Time>, visuals: Query<(Entity, &TempVisual)>, mut commands: Commands) {
    let now = time.elapsed_secs_f64();
    for (e, v) in &visuals {
        if now >= v.until {
            commands.entity(e).despawn();
        }
    }
}

/// A temp visual on `pos`; `None` without its icon.
pub(super) fn spawn_visual(world: &mut World, pos: TilePos, v: Visual) -> Option<Entity> {
    let icon = world.get_resource::<IconIndex>()?.get(v.icon, v.state)?;
    let until = now(world) + v.secs;
    Some(
        world
            .spawn((
                Name::new(v.state),
                pos,
                Appearance {
                    icon_state: icon,
                    layer: Layer(v.layer),
                    plane: v.plane,
                    color: v.color,
                    pixel_offset: v.offset,
                    rotation: 0,
                    overlays: vec![],
                    mouse_opacity: MouseOpacity::Transparent,
                },
                Networked,
                TempVisual { until },
            ))
            .id(),
    )
}

/// `visible_message` from `pos`.
pub(super) fn show(world: &mut World, pos: TilePos, span: Span, text: String) {
    let clients = crate::obj_damage::clients_in_range(world, pos, DEFAULT_MESSAGE_RANGE, &[]);
    world.write_message(SendChat {
        clients,
        message: ChatMessage::simple(ChatChannel::Combat, span, text),
    });
}

/// `to_chat(mob, span_userdanger(text))`.
pub(super) fn tell(world: &mut World, mob: Entity, text: String) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage::simple(ChatChannel::Combat, Span::BoldWarning, text),
        });
    }
}

/// `say(text)` with the mob's `speak_emote`.
pub(super) fn say(world: &mut World, mob: Entity, text: &str) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let name = crate::security::name_of(world, mob);
    let verb = world
        .get::<BasicMob>(mob)
        .and_then(|b| b.def.speak_emote.first().cloned())
        .unwrap_or_else(|| "says".to_owned());
    let message = ChatMessage {
        channel: ChatChannel::Local,
        segments: vec![
            ChatSegment::new(Span::Name, name),
            ChatSegment::new(Span::Say, format!(" {verb}, \"{text}\"")),
        ],
    };
    let clients = crate::obj_damage::clients_in_range(world, pos, DEFAULT_MESSAGE_RANGE, &[]);
    world.write_message(SendChat { clients, message });
}

/// `shake_camera(viewer, duration, strength)` for the players within `range` of `pos`.
pub(super) fn shake(world: &mut World, pos: TilePos, range: u32, duration_ds: f32, strength: f32) {
    if !world.contains_resource::<Messages<SendCameraEffect>>() {
        return;
    }
    let clients: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, range)
        .filter_map(|e| world.get::<PlayerMob>(e).map(|p| p.client))
        .collect();
    for client in clients {
        world.write_message(SendCameraEffect {
            client,
            effect: CameraEffect::Shake {
                duration_ds,
                strength,
            },
        });
    }
}

pub(super) fn sound(world: &mut World, pos: TilePos, file: &str, vol: f32, extrarange: f32) {
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, file, vol, true).extrarange(extrarange),
    );
}

/// The living mobs on `pos`.
pub(super) fn living_on(world: &World, pos: TilePos) -> Vec<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| world.get::<Health>(e).is_some() && world.get::<Stat>(e).is_some())
        .collect()
}

/// A combat roll of the shared stream.
pub(super) fn roll<T>(world: &mut World, f: impl FnOnce(&mut CombatRng) -> T) -> T {
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let out = f(&mut rng);
    world.resource_mut::<CombatRng>().0 = rng.0;
    out
}

/// `/mob/living/simple_animal/hostile/megafauna/ex_act` (`dragon/ex_act` ignores light ones,
/// `bubblegum/ex_act` too and takes harder ones as light): fixed brute, never gibbed. False for
/// other mobs.
pub fn ex_act(world: &mut World, mob: Entity, severity: Severity) -> bool {
    let Some(kind) = kind_of(world, mob) else {
        return false;
    };
    let amount = match severity {
        Severity::Light if matches!(kind, Kind::Drake | Kind::Bubblegum) => return true,
        _ if kind == Kind::Bubblegum => 50.0,
        Severity::Devastate => 250.0,
        Severity::Heavy => 100.0,
        Severity::Light => 50.0,
    };
    super::defense::hurt(world, mob, DamageType::Brute, amount, None);
    true
}

/// `projectile_hit` of a boss that blocks the projectile (`BULLET_ACT_BLOCK`): an enraged
/// Bubblegum. The projectile is spent without effect.
pub fn deflects_projectile(world: &mut World, target: Entity) -> bool {
    bubblegum::deflects(world, target)
}

/// A projectile's hit (`on_hit`, `COMSIG_ATOM_BULLET_ACT`): the colossus' bolts and shield.
pub fn on_projectile_hit(world: &mut World, p: &crate::projectile::Projectile, target: Entity) {
    if kind_of(world, target) == Some(Kind::Colossus) {
        colossus::shield(world, target);
    }
    if p.data.path == colossus::BOLT {
        colossus::bolt_hit(world, target);
    }
}

/// `PopulateContents` of a boss chest by its type, for a chest mapped or spawned without its
/// boss (`crate::mining::necropolis::populate`).
// shortcut: the crusher chests' trophies are left out (no crusher trophies yet).
pub fn chest_loot(world: &mut World, path: &str) -> Option<Vec<&'static str>> {
    if is_under(path, bubblegum::CHEST) {
        Some(bubblegum::loot(world))
    } else if is_under(path, colossus::CHEST) {
        Some(colossus::loot(world))
    } else if is_under(path, drake::CHEST) {
        Some(drake::loot(world))
    } else {
        None
    }
}

/// tg `drop_loot` of a dying megafauna, before `del_on_death`: its necropolis chest with what
/// `PopulateContents` puts in it. Its abilities stop.
pub(super) fn on_death(world: &mut World, mob: Entity, pos: TilePos) {
    let Some(kind) = kind_of(world, mob) else {
        return;
    };
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.queue.clear();
        m.target = None;
    }
    stop_moving(world, mob);
    drake::end_swoop_state(world, mob);
    if kind == Kind::Bubblegum {
        bubblegum::on_death(world, mob);
    }
    match kind {
        Kind::Legion => return legion::on_death(world, mob, pos),
        Kind::ClockworkDefender => {
            crate::construction::spawn_obj(world, CLOCKWORK_ALLOY, pos);
            return;
        }
        _ => {}
    }
    // `dragon/lesser`: `loot = list()`.
    if world.get::<Megafauna>(mob).is_some_and(|m| m.drake.lesser) {
        return;
    }
    let (chest, loot) = match kind {
        Kind::Bubblegum => (bubblegum::CHEST, bubblegum::loot(world)),
        Kind::Colossus => (colossus::CHEST, colossus::loot(world)),
        Kind::Drake => (drake::CHEST, drake::loot(world)),
        Kind::Legion | Kind::ClockworkDefender => return,
    };
    let Some(chest) = crate::construction::spawn_obj(world, chest, pos) else {
        return;
    };
    for path in loot {
        if let Some(item) = crate::construction::spawn_obj(world, path, pos) {
            crate::storage::put_inside(world, item, chest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_by_path() {
        assert_eq!(Kind::of(COLOSSUS), Some(Kind::Colossus));
        assert_eq!(Kind::of(DRAKE), Some(Kind::Drake));
        assert_eq!(Kind::of(LESSER_DRAKE), Some(Kind::Drake));
        assert_eq!(Kind::of(BUBBLEGUM), Some(Kind::Bubblegum));
        assert_eq!(Kind::of(BUBBLEGUM_HALLUCINATION), None);
        assert_eq!(Kind::of(legion::LEGION), Some(Kind::Legion));
        assert_eq!(Kind::of(legion::SMALL), Some(Kind::Legion));
        assert_eq!(Kind::of(CLOCKWORK_DEFENDER), Some(Kind::ClockworkDefender));
        assert_eq!(
            Kind::of("/mob/living/simple_animal/hostile/megafauna/hierophant"),
            None
        );
    }

    #[test]
    fn abilities_share_one_cooldown_and_block_melee() {
        let mut m = Megafauna::new(Kind::Colossus, 0.0, 2500.0);
        assert!(m.ability_available(0.0));
        m.cooldown(4.5, 2.5);
        assert!(!m.ability_available(4.0), "running");
        assert!(!m.ability_available(6.0), "cooling down");
        assert!(m.ability_available(7.0));
        assert_eq!(m.melee_ready, 7.0);
        // A shorter later cooldown never shortens the melee block.
        m.cooldown(5.0, 0.5);
        assert_eq!(m.melee_ready, 7.0);
        assert_eq!(m.ability_ready, 5.5);
    }
}

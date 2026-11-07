//! Vent crawling (server), after tg `code/modules/mob/living/ventcrawling.dm` and
//! `/obj/machinery/atmospherics/relaymove` (`atmosmachinery.dm`).
//!
//! - Ventcrawlers ([`ventcrawler`]): the basic mobs tg gives `TRAIT_VENTCRAWLER_ALWAYS` in
//!   `Initialize` ([`ALWAYS_ROOTS`]), monkeys `TRAIT_VENTCRAWLER_NUDE` (their chest), or anything
//!   with a [`Ventcrawler`] component.
//! - An alt-click on an adjacent vent or scrubber (`crate::loot` `click_alt`) checks
//!   `can_enter_vent` (conscious, not immobilized, hands free, not buckled, nothing worn or held
//!   for a nude ventcrawler, not welded), then crawls in after a 2.5 s `do_after` if the vent is
//!   connected to a pipe: the mob is inside the vent ([`crate::closets::InCloset`], hidden,
//!   unclickable) with [`Ventcrawling`] (`TRAIT_MOVE_VENTCRAWLING`).
//! - Inside, a step (`ClosetRelayMove`) goes to the pipe or machine connected that way (tg
//!   `find_connecting` over the device's nodes; a diagonal tries both cardinals, the one not
//!   used last first), if it lets crawlers through (`VENTCRAWL_ALLOWED`: not pumps, filters,
//!   mixers, thermomachines, crystallizers, circulators or closed valves), with
//!   `ventcrawl.ogg` at most every 3 s. Reaching a vent or scrubber, or stepping nowhere while
//!   in one, starts climbing out (1 s `do_after`, refused when welded).
//! - A pipe deconstructed under the crawler lets it out where the pipe was.
// shortcut: no pipe vision (the pipe images plane and darkened lighting need client planes), the
// crawler breathes the turf's air rather than the pipe's, steps inside follow the mob's own
// walk delay (the client's prediction) instead of tg's 0.5 ds, no multi-z pipes, no passive vents
// or airlock pumps as entrances, no vent arrow indicator overlay, no login hint, no AI behaviours.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_atmos::pipenet::{Custom, Pipenet};
use ss13_mapload::DmAtmosDevice;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::Span;
use ss13_shared::{Dir, TilePos};
use ss13_world::Density;

use crate::atmos_machinery::AtmosDevice;
use crate::atmos_machinery::devices::{VentPump, VentScrubber};
use crate::closets::{ClosetRelayMove, InCloset};
use crate::construction::Job;
use crate::health::Stat;
use crate::medical::{name_of, visible};
use crate::sound::{PlaySound, playsound};
use crate::tools::Actor;

/// Basic mobs with `TRAIT_VENTCRAWLER_ALWAYS` from their `Initialize`.
pub const ALWAYS_ROOTS: &[&str] = &[
    "/mob/living/basic/ant",
    "/mob/living/basic/axolotl",
    "/mob/living/basic/bat",
    "/mob/living/basic/bee",
    "/mob/living/basic/blob_minion/spore",
    "/mob/living/basic/butterfly",
    "/mob/living/basic/chicken",
    "/mob/living/basic/chick",
    "/mob/living/basic/clown/fleshclown",
    "/mob/living/basic/cockroach",
    "/mob/living/basic/crab",
    "/mob/living/basic/drone",
    "/mob/living/basic/frog",
    "/mob/living/basic/garden_gnome",
    "/mob/living/basic/headslug",
    "/mob/living/basic/heretic_summon/fire_shark",
    "/mob/living/basic/killer_tomato",
    "/mob/living/basic/lightgeist",
    "/mob/living/basic/lizard",
    "/mob/living/basic/mining/legion/lesser",
    "/mob/living/basic/morph",
    "/mob/living/basic/mothroach",
    "/mob/living/basic/mouse",
    "/mob/living/basic/mushroom",
    "/mob/living/basic/pet/cat",
    "/mob/living/basic/possum",
    "/mob/living/basic/regal_rat",
    "/mob/living/basic/shade",
    "/mob/living/basic/slime",
    "/mob/living/basic/snake",
    "/mob/living/basic/spider/giant/scout",
    "/mob/living/basic/spider/giant/sgt_araneus",
    "/mob/living/basic/spider/giant/viper/wizard",
    "/mob/living/basic/spider/growing/spiderling",
    "/mob/living/basic/spider/growing/young/scout",
    "/mob/living/basic/spider/maintenance",
    "/mob/living/basic/stoat",
];

/// Machines with `vent_movement = NONE` or without `VENTCRAWL_ALLOWED` (circulators).
const BLOCKING_ROOTS: &[&str] = &[
    "/obj/machinery/atmospherics/components/binary/pump",
    "/obj/machinery/atmospherics/components/binary/volume_pump",
    "/obj/machinery/atmospherics/components/binary/temperature_pump",
    "/obj/machinery/atmospherics/components/binary/circulator",
    "/obj/machinery/atmospherics/components/binary/crystallizer",
    "/obj/machinery/atmospherics/components/trinary",
    "/obj/machinery/atmospherics/components/unary/thermomachine",
];

const VENT_SOUND: &str = "sound/machines/ventcrawl.ogg";
/// `VENT_SOUND_DELAY` (30 ds).
const VENT_SOUND_SECS: f64 = 3.0;
/// The `do_after`s of `handle_ventcrawl`, deciseconds.
const ENTER_DS: f32 = 25.0;
const EXIT_DS: f32 = 10.0;

/// `TRAIT_VENTCRAWLER_ALWAYS` (`nude: false`) or `TRAIT_VENTCRAWLER_NUDE` given at runtime.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ventcrawler {
    pub nude: bool,
}

/// `TRAIT_MOVE_VENTCRAWLING`: the mob is inside the pipe its [`InCloset`] names. It is not
/// dense while inside, so it does not block the tiles its pipes run under.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Ventcrawling {
    /// It was dense before crawling in.
    pub dense: bool,
    /// `last_vent_dir`.
    pub last_dir: u8,
    /// `last_played_vent`.
    pub last_sound: f64,
}

/// The `do_after`s of this module (`crate::construction::Job::Ventcrawl`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VentJob {
    Enter,
    Exit,
}

pub struct VentcrawlPlugin;

impl Plugin for VentcrawlPlugin {
    fn build(&self, app: &mut App) {
        // `ClosetRelayMove` is the closets' message (`crate::closets`).
        app.add_systems(Update, relay_moves)
            .add_observer(release_on_pipe_removed);
    }
}

/// `TRAIT_VENTCRAWLER_ALWAYS` / `_NUDE`: `Some(nude_only)` for a ventcrawler.
pub fn ventcrawler(world: &World, mob: Entity) -> Option<bool> {
    if let Some(v) = world.get::<Ventcrawler>(mob) {
        return Some(v.nude);
    }
    if let Some(basic) = world.get::<crate::mobs::BasicMob>(mob) {
        return ALWAYS_ROOTS
            .iter()
            .any(|root| is_under(&basic.def.path, root))
            .then_some(false);
    }
    (world.get::<crate::species::Species>(mob).map(|s| s.0)
        == Some(crate::species::SpeciesId::Monkey))
    .then_some(true)
}

/// A vent or scrubber (`VENTCRAWL_ENTRANCE_ALLOWED`) and whether it is welded.
fn entrance(world: &World, device: Entity) -> Option<bool> {
    world
        .get::<VentPump>(device)
        .map(|v| v.welded)
        .or_else(|| world.get::<VentScrubber>(device).map(|s| s.welded))
}

/// `can_enter_vent`, with its feedback.
fn can_enter_vent(world: &mut World, mob: Entity, vent: Entity, feedback: bool) -> bool {
    let Some(nude) = ventcrawler(world, mob) else {
        return false;
    };
    let Some(welded) = entrance(world, vent) else {
        return false;
    };
    let (Some(&at), Some(&vent_pos)) = (world.get::<TilePos>(mob), world.get::<TilePos>(vent))
    else {
        return false;
    };
    if at.z != vent_pos.z || at.chebyshev(vent_pos).is_none_or(|d| d > 1) {
        return false;
    }
    let traits = crate::status_effects::incapacitation(world, mob);
    let dressed = nude
        && world
            .get::<crate::inventory::Inventory>(mob)
            .is_some_and(|inv| inv.equipped().next().is_some());
    let refusal = if world.get::<Stat>(mob) != Some(&Stat::Conscious) {
        Some("Вы должны быть в сознании!")
    } else if traits.immobilized {
        Some("Вы сейчас не можете переместиться в вентиляцию!")
    } else if traits.hands_blocked {
        Some("У вас должны быть свободны руки!")
    } else if world.get::<crate::medical::surgery::Buckled>(mob).is_some() {
        Some("Вы должны отстегнуться!")
    } else if dressed {
        Some("Вы должны снять предметы!")
    } else if welded {
        Some("Вентиляция заварена!")
    } else {
        None
    };
    match refusal {
        Some(text) => {
            if feedback {
                crate::chem::tell_mob(world, mob, Span::Warning, text);
            }
            false
        }
        None => true,
    }
}

/// tg `atmos/click_alt` -> `handle_ventcrawl`: an alt-click on a vent. True when it was a
/// ventcrawler's click on a vent.
pub fn click_alt(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if entrance(world, target).is_none() || ventcrawler(world, mob).is_none() {
        return false;
    }
    handle_ventcrawl(world, client, mob, target);
    true
}

/// tg `handle_ventcrawl`: starts climbing in or out of `vent`.
fn handle_ventcrawl(world: &mut World, client: Entity, mob: Entity, vent: Entity) {
    if !can_enter_vent(world, mob, vent, true) {
        return;
    }
    let item = world
        .get::<crate::inventory::Inventory>(mob)
        .and_then(crate::inventory::Inventory::active_item);
    let actor = Actor { client, mob, item };
    if world.get::<Ventcrawling>(mob).is_some() {
        crate::chem::tell_mob(
            world,
            mob,
            Span::Notice,
            "Вы начинаете вылазить из вентиляционной сети..",
        );
        crate::tools::do_after(
            world,
            actor,
            vent,
            EXIT_DS,
            0,
            Job::Ventcrawl(VentJob::Exit),
        );
        return;
    }
    if !connected(world, vent) {
        crate::chem::tell_mob(
            world,
            mob,
            Span::Warning,
            "Эта вентиляция не подключена к чему-либо!",
        );
        return;
    }
    let name = capitalize(&name_of(world, mob));
    visible(
        world,
        mob,
        "Вы начинаете залазить в вентиляцию...",
        &format!("{name} начинает залазить в вентиляцию..."),
        Span::Notice,
    );
    crate::tools::do_after(
        world,
        actor,
        vent,
        ENTER_DS,
        0,
        Job::Ventcrawl(VentJob::Enter),
    );
}

/// `vent_parent.members.len || vent_parent.other_atmos_machines`: something is on its pipe.
fn connected(world: &World, vent: Entity) -> bool {
    let (Some(device), Some(net)) = (
        world.get::<AtmosDevice>(vent),
        world.get_resource::<Pipenet>(),
    ) else {
        return false;
    };
    net.device(device.id)
        .is_some_and(|d| d.nodes.iter().any(Option::is_some))
}

/// The end of a vent `do_after`, revalidated (the vent may have been welded meanwhile).
pub fn complete(world: &mut World, mob: Entity, action: &crate::tools::DoAfter, job: VentJob) {
    let vent = action.target;
    if !can_enter_vent(world, mob, vent, true) {
        return;
    }
    let name = capitalize(&name_of(world, mob));
    match job {
        VentJob::Enter => {
            if world.get::<Ventcrawling>(mob).is_some() || !connected(world, vent) {
                return;
            }
            visible(
                world,
                mob,
                "Вы залезаете в вентиляцию.",
                &format!("{name} залезает в вентиляцию!"),
                Span::Notice,
            );
            move_into_vent(world, mob, vent);
        }
        VentJob::Exit => {
            let inside = world.get::<InCloset>(mob).is_some_and(|c| c.closet == vent);
            if world.get::<Ventcrawling>(mob).is_none() || !inside {
                return;
            }
            leave(world, mob);
            visible(
                world,
                mob,
                "Вы вылезаете из вентиляции.",
                &format!("{name} вылезает из вентиляции!"),
                Span::Notice,
            );
        }
    }
}

/// tg `move_into_vent`.
pub fn move_into_vent(world: &mut World, mob: Entity, vent: Entity) {
    let Some(&pos) = world.get::<TilePos>(vent) else {
        return;
    };
    crate::closets::put_mob_inside(world, mob, vent);
    let dense = world.entity(mob).contains::<Density>();
    world
        .entity_mut(mob)
        .remove::<Density>()
        .insert((pos, Ventcrawling { dense, ..default() }));
}

/// Out onto the tile of the pipe the mob is in (`forceMove(ventcrawl_target.loc)`).
fn leave(world: &mut World, mob: Entity) {
    let Ok(mut entity) = world.get_entity_mut(mob) else {
        return;
    };
    if entity.take::<Ventcrawling>().is_some_and(|c| c.dense) {
        entity.insert(Density);
    }
    crate::closets::let_out(world, mob);
}

use ss13_chat::capitalize;

/// Whether crawlers pass through `device` (`vent_movement & VENTCRAWL_ALLOWED`).
fn passable(world: &World, net: &Pipenet, device: Entity) -> bool {
    if world
        .get::<DmAtmosDevice>(device)
        .is_some_and(|dm| BLOCKING_ROOTS.iter().any(|r| is_under(&dm.path, r)))
    {
        return false;
    }
    let closed_valve = world
        .get::<AtmosDevice>(device)
        .and_then(|d| net.device(d.id))
        .is_some_and(|d| matches!(d.custom, Custom::Valve { open: false }));
    !closed_valve
}

/// tg `find_connecting(direction)` over the device's nodes: the connected device one step
/// `dir` away.
fn find_connecting(world: &World, device: Entity, dir: u8) -> Option<Entity> {
    let net = world.get_resource::<Pipenet>()?;
    let from = net.device(world.get::<AtmosDevice>(device)?.id)?;
    let step = Dir::from_bits(dir)?;
    let wanted = from.pos.step(step);
    from.nodes
        .iter()
        .flatten()
        .filter_map(|&n| net.device(n))
        .find(|n| n.pos == wanted)
        .map(|n| n.entity)
}

/// tg `/obj/machinery/atmospherics/relaymove`.
fn relay_moves(world: &mut World, mut moves: Local<MessageCursor<ClosetRelayMove>>) {
    let steps: Vec<(Entity, Dir)> = moves
        .read(world.resource::<Messages<ClosetRelayMove>>())
        .map(|m| (m.mob, m.dir))
        .collect();
    for (mob, dir) in steps {
        crawl_step(world, mob, dir);
    }
}

/// One step of a crawler inside its pipe.
fn crawl_step(world: &mut World, mob: Entity, dir: Dir) {
    let Some(crawl) = world.get::<Ventcrawling>(mob).copied() else {
        return;
    };
    let Some(here) = world.get::<InCloset>(mob).map(|c| c.closet) else {
        return;
    };
    if world.get::<Stat>(mob) != Some(&Stat::Conscious)
        || world
            .get::<crate::tools::DoAfter>(mob)
            .is_some_and(|d| !matches!(d.job, Job::Ventcrawl(VentJob::Exit)) || d.target != here)
    {
        return;
    }
    let client = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client);
    let mut target = None;
    let mut last_dir = crawl.last_dir;
    for cardinal in [1u8, 2, 4, 8] {
        if dir as u8 & cardinal == 0 {
            continue;
        }
        let Some(next) = find_connecting(world, here, cardinal) else {
            continue;
        };
        target = Some(next);
        // At a fork with two directions held, the one not used last wins.
        if crawl.last_dir == cardinal {
            continue;
        }
        last_dir = cardinal;
        break;
    }
    let Some(next) = target else {
        // Nowhere to go: climb out if this is a vent.
        if entrance(world, here).is_some()
            && let Some(client) = client
            && world.get::<crate::tools::DoAfter>(mob).is_none()
        {
            handle_ventcrawl(world, client, mob, here);
        }
        return;
    };
    let passes = world
        .get_resource::<Pipenet>()
        .is_some_and(|net| passable(world, net, next));
    if !passes {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(next) else {
        return;
    };
    // Moving cancels a climb out (`do_after` checks the user's turf).
    world.entity_mut(mob).remove::<crate::tools::DoAfter>();
    if let Some(mut inside) = world.get_mut::<InCloset>(mob) {
        inside.closet = next;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let sound = now - crawl.last_sound > VENT_SOUND_SECS;
    world.entity_mut(mob).insert((
        pos,
        Ventcrawling {
            last_dir,
            last_sound: if sound { now } else { crawl.last_sound },
            ..crawl
        },
    ));
    if sound {
        playsound(
            world,
            PlaySound::new(pos, VENT_SOUND, 50.0, true).extrarange(-3.0),
        );
    }
    if entrance(world, next).is_some()
        && let Some(client) = client
    {
        handle_ventcrawl(world, client, mob, next);
    }
}

/// A pipe deconstructed with a crawler inside lets it out where the pipe was.
fn release_on_pipe_removed(
    remove: On<Remove<AtmosDevice>>,
    inside: Query<(Entity, &InCloset), With<Ventcrawling>>,
    mut commands: Commands,
) {
    for (mob, c) in &inside {
        if c.closet == remove.entity {
            commands.queue(move |world: &mut World| leave(world, mob));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atmos_machinery::{VENT_PUMP, device_spec};

    /// A device on (x, 1, 1) in `net`, as `init_atmos_machinery` adds it.
    fn device(
        world: &mut World,
        net: &mut Pipenet,
        path: &str,
        x: i32,
        dir: u8,
        kind: f64,
    ) -> Entity {
        let pos = TilePos::new(x, 1, 1);
        let dm = DmAtmosDevice {
            path: path.into(),
            piping_layer: 3.0,
            pipe_color: "#EEEEEE".into(),
            device_type: kind,
            initialize_directions: 15.0,
            ..default()
        };
        let e = world.spawn((pos, dm.clone())).id();
        let spec = device_spec(e, pos, &dm, dir).unwrap().0;
        let id = net.add(spec);
        world
            .entity_mut(e)
            .insert(AtmosDevice { id, layer: 3, dir });
        e
    }

    fn vent() -> VentPump {
        VentPump {
            on: true,
            releasing: true,
            pressure_checks: 1,
            external_pressure_bound: 101.325,
            internal_pressure_bound: 0.0,
            welded: false,
            area: None,
            overclocked: false,
        }
    }

    #[test]
    fn a_crawler_follows_the_pipe_to_the_next_vent_and_climbs_out() {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.init_resource::<Messages<PlaySound>>();
        // Moving the crawler out updates the spatial index.
        world.init_resource::<ss13_world::SpatialIndex>();
        world
            .resource_mut::<ss13_world::SpatialIndex>()
            .create_level(1, 16, 16)
            .unwrap();
        let mut net = Pipenet::default();
        // Vent (1) -> smart pipe (2) -> vent (3).
        let a = device(&mut world, &mut net, VENT_PUMP, 1, 4, 1.0);
        let pipe = device(
            &mut world,
            &mut net,
            "/obj/machinery/atmospherics/pipe/smart/simple",
            2,
            2,
            4.0,
        );
        let b = device(&mut world, &mut net, VENT_PUMP, 3, 8, 1.0);
        net.build();
        world.insert_resource(net);
        world.entity_mut(a).insert(vent());
        world.entity_mut(b).insert(vent());
        let mob = world
            .spawn((
                TilePos::new(1, 1, 1),
                Stat::Conscious,
                Ventcrawler { nude: false },
                Density,
            ))
            .id();
        assert!(connected(&world, a));
        assert!(can_enter_vent(&mut world, mob, a, false));
        move_into_vent(&mut world, mob, a);
        assert!(!world.entity(mob).contains::<Density>());
        // North leads nowhere from the vent; east is the pipe, then the other vent.
        crawl_step(&mut world, mob, Dir::North);
        assert_eq!(world.get::<InCloset>(mob).unwrap().closet, a);
        crawl_step(&mut world, mob, Dir::East);
        assert_eq!(world.get::<InCloset>(mob).unwrap().closet, pipe);
        assert_eq!(world.get::<TilePos>(mob), Some(&TilePos::new(2, 1, 1)));
        crawl_step(&mut world, mob, Dir::East);
        assert_eq!(world.get::<InCloset>(mob).unwrap().closet, b);
        // Welded shut: no way out.
        world.get_mut::<VentPump>(b).unwrap().welded = true;
        assert!(!can_enter_vent(&mut world, mob, b, false));
        world.get_mut::<VentPump>(b).unwrap().welded = false;
        let client = world.spawn_empty().id();
        handle_ventcrawl(&mut world, client, mob, b);
        let action = world.get::<crate::tools::DoAfter>(mob).cloned().unwrap();
        assert_eq!(action.job, Job::Ventcrawl(VentJob::Exit));
        assert_eq!(action.ends_at, 1.0);
        complete(&mut world, mob, &action, VentJob::Exit);
        assert!(world.get::<InCloset>(mob).is_none());
        assert!(world.get::<Ventcrawling>(mob).is_none());
        assert!(world.entity(mob).contains::<Density>());
        assert_eq!(world.get::<TilePos>(mob), Some(&TilePos::new(3, 1, 1)));
    }

    #[test]
    fn ventcrawler_roots_are_subtype_checks() {
        assert!(
            ALWAYS_ROOTS
                .iter()
                .any(|r| is_under("/mob/living/basic/mouse/gray", r))
        );
        assert!(
            !ALWAYS_ROOTS
                .iter()
                .any(|r| is_under("/mob/living/basic/mousetrap", r))
        );
        assert!(
            BLOCKING_ROOTS
                .iter()
                .any(|r| is_under("/obj/machinery/atmospherics/components/trinary/filter", r))
        );
    }
}

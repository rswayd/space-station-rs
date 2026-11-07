//! Table clocks (server), after tg `/obj/item/table_clock`
//! (`code/game/objects/items/devices/table_clock.dm`).
//!
//! - A working clock ticks (`/datum/looping_sound/clock`: `ticking_clock.ogg` every 4 s at 50,
//!   not through walls) and tells the time on examine.
//! - Hit with an item of force 5 or more, or thrown, it breaks (`table_clock_broken`, silent).
//! - Used in hand, a broken clock is fixed in 10 s, unless it broke more than
//!   `MAX_CLOCK_REPAIRS` (2) times; a working one refuses to be touched.
// shortcut: the clock tells the round time as NST (tg adds the random `gametime_offset`, which
// the port's PDAs do not have either); no beauty element, no literacy check.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::Span;

use super::DeviceJob;
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, do_after, tell};

pub const TABLE_CLOCK: &str = "/obj/item/table_clock";
/// `MAX_CLOCK_REPAIRS`.
const MAX_CLOCK_REPAIRS: u8 = 2;
/// `/datum/looping_sound/clock`.
const TICK_SOUND: &str = "sound/ambience/misc/ticking_clock.ogg";
const TICK_SECS: f64 = 4.0;
const SMASH_SOUND: &str = "sound/effects/magic/clockwork/ark_activation.ogg";
const THROW_SOUND: &str = "sound/effects/footstep/glass_step.ogg";
/// `do_after(user, 10 SECONDS, src)`, deciseconds.
const REPAIR_DS: f32 = 100.0;

/// `/obj/item/table_clock`: `broken`, `times_broken`, and when the loop ticks next.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct TableClock {
    pub broken: bool,
    pub times_broken: u8,
    next_tick: f64,
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // Used in hand: the repair (`interact`).
    app.add_click_handler(ClickPhase::AttackSelf, 1753, |w, c| {
        crate::devices::table_clock::interact(w, c.client, c.mob, c.target)
    });
    // `attackby` on a clock lying around or in the other hand.
    app.add_click_handler(ClickPhase::HeldOnTarget, 4313, |w, c| {
        c.held
            .is_some_and(|h| crate::devices::table_clock::attackby(w, c.mob, h, c.target))
    });
    app.add_click_handler(ClickPhase::OtherHand, 1493, |w, c| {
        c.held
            .is_some_and(|h| crate::devices::table_clock::attackby(w, c.mob, h, c.target))
    });
    app.add_observer(add_clock)
        .add_systems(Update, (break_thrown, tick));
}

fn add_clock(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if items
        .get(add.entity)
        .is_ok_and(|i| is_under(&i.path, TABLE_CLOCK))
    {
        commands
            .entity(add.entity)
            .try_insert_if_new(TableClock::default());
    }
}

fn refresh_icon(world: &mut World, clock: Entity, broken: bool) {
    let state = if broken {
        "table_clock_broken"
    } else {
        "table_clock"
    };
    crate::obj_damage::set_icon_state(world, clock, state);
}

/// `break_clock`: true when it was working.
pub fn break_clock(world: &mut World, clock: Entity, sound: &str) -> bool {
    let Some(mut c) = world.get_mut::<TableClock>(clock) else {
        return false;
    };
    if c.broken {
        return false;
    }
    c.broken = true;
    c.times_broken = c.times_broken.saturating_add(1);
    if let Some(pos) = crate::explosions::turf_of(world, clock) {
        playsound(world, PlaySound::new(pos, sound, 40.0, false));
    }
    refresh_icon(world, clock, true);
    true
}

/// `attackby`: an item of force 5 or more smashes a working clock. True when it broke.
pub fn attackby(world: &mut World, mob: Entity, held: Entity, clock: Entity) -> bool {
    if world.get::<TableClock>(clock).is_none_or(|c| c.broken) {
        return false;
    }
    if world.get::<DmItem>(held).is_none_or(|d| d.force < 5.0) {
        return false;
    }
    if !break_clock(world, clock, SMASH_SOUND) {
        return false;
    }
    let user = crate::security::name_of(world, mob);
    let name = crate::security::name_of(world, clock);
    crate::medical::visible(
        world,
        mob,
        "Терпеть эту дурацкую машину больше не могу! Да заткнись ты уже!",
        &format!("{user} разбивает {name} так сильно, что они перестают ломаться!"),
        Span::Warning,
    );
    true
}

/// `throw_at` of a clock: it breaks as it flies.
fn break_thrown(
    thrown: Query<Entity, (Added<crate::throw::Thrown>, With<TableClock>)>,
    mut commands: Commands,
) {
    for clock in &thrown {
        commands.queue(move |world: &mut World| {
            break_clock(world, clock, THROW_SOUND);
        });
    }
}

/// `interact` (used in hand): the repair of a broken clock.
pub fn interact(world: &mut World, client: Entity, mob: Entity, clock: Entity) -> bool {
    let Some(c) = world.get::<TableClock>(clock).copied() else {
        return false;
    };
    if !c.broken {
        tell(
            world,
            client,
            Span::Warning,
            "Трогать часы? И рисковать их сломать? Вы с ума сошли??",
        );
        return true;
    }
    if c.times_broken > MAX_CLOCK_REPAIRS {
        crate::balloon::balloon_alert(world, mob, mob, "часы не починить!");
        return true;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(clock),
    };
    // The clock is in a hand (no tile of its own): the do_after is on the user.
    if do_after(
        world,
        actor,
        mob,
        REPAIR_DS,
        0,
        crate::construction::Job::Device(DeviceJob::RepairClock(clock)),
    ) {
        crate::balloon::balloon_alert(world, mob, mob, "чиним часы...");
    }
    true
}

/// The end of the repair: the clock (still in the active hand, `do_after`) works again.
pub fn complete_repair(world: &mut World, mob: Entity, clock: Entity) {
    let Some(mut c) = world.get_mut::<TableClock>(clock) else {
        return;
    };
    if !c.broken {
        return;
    }
    c.broken = false;
    c.next_tick = 0.0;
    refresh_icon(world, clock, false);
    crate::balloon::balloon_alert(world, mob, mob, "часы починены!");
}

/// The soundloop of the working clocks. Cost: one pass over the table clocks per tick (a
/// handful per map), a sound each 4 s.
fn tick(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &TableClock)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c)| !c.broken && c.next_tick <= now)
        .map(|(e, _)| e)
        .collect();
    for clock in due {
        if let Some(mut c) = world.get_mut::<TableClock>(clock) {
            c.next_tick = now + TICK_SECS;
        }
        if let Some(pos) = crate::explosions::turf_of(world, clock) {
            playsound(
                world,
                PlaySound::new(pos, TICK_SOUND, 50.0, false).ignore_walls(false),
            );
        }
    }
}

/// The time on a working clock, the hint on a broken one.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(c) = world.get::<TableClock>(target) else {
        return;
    };
    if c.broken {
        lines.push((
            Span::Info,
            "Похоже, часы сломаны. Их можно починить, используя в руке.".to_owned(),
        ));
        return;
    }
    let shift = crate::records::manifest::round_timestamp(world);
    let nst = shift.get(..5).unwrap_or(&shift).to_owned();
    lines.push((Span::Info, format!("Текущее время NST: {nst}.")));
    lines.push((Span::Info, format!("Значит, смена идёт уже {shift}.")));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A light item does nothing, a heavy one breaks it once; the repair makes it tick again.
    #[test]
    fn heavy_hits_break_and_repairs_mend() {
        let mut world = World::new();
        world.init_resource::<bevy::ecs::message::Messages<ss13_net::chat::server::SendChat>>();
        let clock = world.spawn(TableClock::default()).id();
        let light = world
            .spawn(DmItem {
                force: 4.0,
                ..default()
            })
            .id();
        let heavy = world
            .spawn(DmItem {
                force: 10.0,
                ..default()
            })
            .id();
        let mob = world.spawn_empty().id();
        assert!(!attackby(&mut world, mob, light, clock));
        assert!(attackby(&mut world, mob, heavy, clock));
        assert!(!attackby(&mut world, mob, heavy, clock));
        let c = *world.get::<TableClock>(clock).unwrap();
        assert!(c.broken && c.times_broken == 1);
        complete_repair(&mut world, mob, clock);
        assert!(!world.get::<TableClock>(clock).unwrap().broken);
    }
}

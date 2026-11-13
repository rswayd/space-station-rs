//! Radio jammers (server), after tg `/obj/item/jammer` (`code/game/objects/items/devices/
//! traitordevices.dm`) with BandaStation's whitelisted frequencies and its Nanotrasen jammer
//! (`modular_bandastation/objects/code/items/devices/nanotrasen_radio_jammer.dm`).
//!
//! - Right click on the jammer in the active hand toggles it (`attack_self_secondary`). An
//!   active jammer stops every radio within its `range` on its level from transmitting
//!   (`is_within_radio_jammer_range` in `talk_into`, [`JammerZones::jams`]), except on its
//!   whitelisted frequencies: the syndicate one for the syndicate and makeshift jammers, the
//!   CentCom one for the Nanotrasen jammer.
//! - Used in hand (`attack_self`) the syndicate and makeshift jammers release a disruptor wave:
//!   the microphones of every radio in view within `disruptor_range` go off (syndicate radios are
//!   spared), then 15 s of cooldown. Clicked at something (`interact_with_atom`) they switch off
//!   the radios on it. The Nanotrasen jammer has neither.
// shortcut: no PDA messenger jamming (`messenger_program`), machines' internal radios are not
// jammed (`MachineRadio` carries no tile); add both with the PDA messenger's network checks.
// Cost: one `turf_of` per active jammer per tick (a handful at most), one comparison per active
// jammer per transmission.

use bevy::prelude::*;
use ss13_chat::radio::{FREQ_CENTCOM, FREQ_SYNDICATE, RADIO_SPECIAL_SYNDIE};
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::SpatialIndex;

use crate::balloon::balloon_alert;
use crate::radio::Radio;
use crate::tools::{now, tell_mob};

pub const JAMMER: &str = "/obj/item/jammer";
pub const NT_JAMMER: &str = "/obj/item/jammer/nt";
pub const MAKESHIFT_JAMMER: &str = "/obj/item/jammer/makeshift";
/// `jam_cooldown_duration`.
const WAVE_COOLDOWN_SECS: f64 = 15.0;

/// `/obj/item/jammer`: `active`, `range`, `disruptor_range`, `jam_cooldown`,
/// `whitelisted_frequencies`; `waves` is false for the Nanotrasen jammer (its `attack_self` and
/// `interact_with_atom` do nothing).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Jammer {
    pub active: bool,
    pub range: u32,
    pub disruptor_range: u32,
    pub wave_ready_at: f64,
    pub whitelist: &'static [u16],
    pub waves: bool,
}

impl Jammer {
    pub fn of_path(path: &str) -> Option<Self> {
        if !is_under(path, JAMMER) {
            return None;
        }
        let base = Self {
            active: false,
            range: 12,
            disruptor_range: 7,
            wave_ready_at: 0.0,
            whitelist: &[FREQ_SYNDICATE],
            waves: true,
        };
        Some(if is_under(path, NT_JAMMER) {
            Self {
                range: 18,
                whitelist: &[FREQ_CENTCOM],
                waves: false,
                ..base
            }
        } else if is_under(path, MAKESHIFT_JAMMER) {
            Self {
                range: 5,
                disruptor_range: 3,
                ..base
            }
        } else {
            base
        })
    }
}

/// tg `GLOB.active_jammers`, with each one's tile this tick.
#[derive(Resource, Default, Debug)]
pub struct JammerZones(pub Vec<(TilePos, u32, &'static [u16])>);

impl JammerZones {
    /// tg `is_within_radio_jammer_range(source, freq)`: `IN_GIVEN_RANGE` of an active jammer
    /// whose whitelist does not hold `freq`.
    pub fn jams(&self, pos: TilePos, freq: u16) -> bool {
        self.0.iter().any(|&(at, range, whitelist)| {
            pos.chebyshev(at).is_some_and(|d| d <= range) && !whitelist.contains(&freq)
        })
    }
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // Used in hand: the disruptor wave.
    app.add_click_handler(ClickPhase::AttackSelf, 1777, |w, c| {
        crate::devices::jammer::attack_self(w, c.mob, c.target)
    });
    // Clicked at something: the directed wave.
    app.add_click_handler(ClickPhase::HeldOnTarget, 1537, |w, c| {
        !c.modifiers.right
            && c.held.is_some_and(|h| {
                crate::devices::jammer::interact_with_atom(w, c.mob, c.mob_pos, h, c.target, c.pos)
            })
    });
    app.init_resource::<JammerZones>()
        .add_observer(add_jammer)
        .add_systems(Update, jammer_zones.before(ss13_chat::server::SaySystems));
}

fn add_jammer(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if let Some(jammer) = items
        .get(add.entity)
        .ok()
        .and_then(|i| Jammer::of_path(&i.path))
    {
        commands.entity(add.entity).try_insert_if_new(jammer);
    }
}

fn jammer_zones(world: &mut World) {
    let mut active = world.query::<(Entity, &Jammer)>();
    let jammers: Vec<(Entity, u32, &'static [u16])> = active
        .iter(world)
        .filter(|(_, j)| j.active)
        .map(|(e, j)| (e, j.range, j.whitelist))
        .collect();
    let zones = jammers
        .into_iter()
        .filter_map(|(e, range, whitelist)| {
            crate::explosions::turf_of(world, e).map(|pos| (pos, range, whitelist))
        })
        .collect();
    world.resource_mut::<JammerZones>().0 = zones;
}

/// `attack_self_secondary`: the jammer in the active hand turns on or off. True when `item` is
/// a jammer.
pub fn toggle(world: &mut World, mob: Entity, item: Entity) -> bool {
    let Some(mut jammer) = world.get_mut::<Jammer>(item) else {
        return false;
    };
    let was = jammer.active;
    jammer.active = !was;
    let name = crate::security::name_of(world, item);
    let (verb, balloon) = if was {
        ("выключаете", "глушилка выключена")
    } else {
        ("включаете", "глушилка включена")
    };
    tell_mob(world, mob, Span::Notice, format!("Вы {verb} {name}."));
    balloon_alert(world, mob, mob, balloon);
    true
}

/// `attack_self`: the disruptor wave over everything in view.
pub fn attack_self(world: &mut World, mob: Entity, item: Entity) -> bool {
    let Some(jammer) = world.get::<Jammer>(item).cloned() else {
        return false;
    };
    if !jammer.waves {
        return true;
    }
    let t = now(world);
    if t < jammer.wave_ready_at {
        balloon_alert(world, mob, mob, "перезарядка!");
        return true;
    }
    balloon_alert(world, mob, mob, "волна помех выпущена!");
    tell_mob(
        world,
        mob,
        Span::Notice,
        "Вы выпускаете волну помех, отключая все радиоустройства поблизости.",
    );
    if let Some(at) = crate::explosions::turf_of(world, mob) {
        let range = jammer.disruptor_range;
        let seen: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .in_radius(at, range)
            .collect();
        for e in seen {
            let visible = world.get::<TilePos>(e).is_some_and(|&p| {
                p == at || crate::mobs::ai::behaviors::can_see(world, at, p, range)
            });
            if visible {
                disable_radios_on(world, e, true);
            }
        }
    }
    if let Some(mut j) = world.get_mut::<Jammer>(item) {
        j.wave_ready_at = t + WAVE_COOLDOWN_SECS;
    }
    true
}

/// `interact_with_atom`: the directed wave on `target`.
pub fn interact_with_atom(
    world: &mut World,
    mob: Entity,
    mob_pos: TilePos,
    item: Entity,
    target: Entity,
    pos: TilePos,
) -> bool {
    let Some(jammer) = world.get::<Jammer>(item).cloned() else {
        return false;
    };
    if !jammer.waves || mob_pos.chebyshev(pos).is_none_or(|d| d > 1) {
        return false;
    }
    if pos != mob_pos
        && !crate::mobs::ai::behaviors::can_see(world, mob_pos, pos, jammer.disruptor_range)
    {
        balloon_alert(world, mob, mob, "слишком далеко!");
        return true;
    }
    balloon_alert(world, mob, target, "радио подавлено!");
    let name = crate::security::name_of(world, target);
    tell_mob(
        world,
        mob,
        Span::Notice,
        format!("Вы выпускаете направленную волну помех, отключая все радиоустройства на {name}."),
    );
    disable_radios_on(world, target, false);
    true
}

/// `disable_radios_on`: every radio on `target` or inside it stops broadcasting
/// (`set_broadcasting(FALSE, actual_setting = FALSE)`).
fn disable_radios_on(world: &mut World, target: Entity, ignore_syndie: bool) {
    let mut things = vec![target];
    things.extend(crate::antag::objectives::all_contents(world, target));
    things.extend(crate::explosions::ex_act::contents_of(world, target));
    for e in things {
        if let Some(mut radio) = world.get_mut::<Radio>(e) {
            if ignore_syndie && radio.special_channels & RADIO_SPECIAL_SYNDIE != 0 {
                continue;
            }
            radio.broadcasting = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jammer_kinds_and_whitelists() {
        let base = Jammer::of_path(JAMMER).unwrap();
        assert_eq!(
            (base.range, base.disruptor_range, base.waves),
            (12, 7, true)
        );
        let nt = Jammer::of_path(NT_JAMMER).unwrap();
        assert_eq!((nt.range, nt.waves), (18, false));
        let makeshift = Jammer::of_path(MAKESHIFT_JAMMER).unwrap();
        assert_eq!((makeshift.range, makeshift.disruptor_range), (5, 3));
        assert!(Jammer::of_path("/obj/item/radio").is_none());

        let at = TilePos::new(10, 10, 2);
        let zones = JammerZones(vec![(at, nt.range, nt.whitelist)]);
        let near = TilePos::new(28, 10, 2);
        assert!(zones.jams(near, FREQ_SYNDICATE));
        assert!(
            !zones.jams(near, FREQ_CENTCOM),
            "the CentCom frequency is whitelisted"
        );
        assert!(
            !zones.jams(TilePos::new(29, 10, 2), FREQ_SYNDICATE),
            "out of range"
        );
        assert!(
            !zones.jams(TilePos::new(10, 10, 3), FREQ_SYNDICATE),
            "another level"
        );
        let syndie = JammerZones(vec![(at, base.range, base.whitelist)]);
        assert!(!syndie.jams(at, FREQ_SYNDICATE));
        assert!(syndie.jams(at, FREQ_CENTCOM));
    }
}

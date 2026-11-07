//! Aurora Caelus (`code/modules/events/aurora_caelus.dm`): for a few minutes starlight turns to
//! shifting greens and blues and the kitchen's floors glow with it.
//!
//! Starlight is the light of the starlit space turfs (`Optics::light` on space next to lit
//! tiles, see `ss13_mapload::light`): [`set_starlight`] recolors those sources, which the server
//! replicates like any light change. The fades (5 steps 8 s apart, tg `fade_space` and
//! `fade_kitchen`) run on [`AuroraFade`] from [`process_fades`].
//!
//! Cost: a starlight change walks the turfs with optics once (every 16 s while it runs).
// shortcut: the ambient of space tiles themselves (`TileLighting::Space`, tg
// `starlight_objects`) keeps its color: recoloring it means replicating every space tile;
// upgrade with a replicated starlight color resource (a protocol change).
// shortcut: no `SSmapping.empty_space` check (every ported station map has space), no
// `sound_midi` volume preference (the sound plays at 20).
// shortcut: the ruined roast fills the oven's tile with hot plasma but the cook neither says
// nor screams anything.

use bevy::prelude::*;
use ss13_mapload::Turf;
use ss13_shared::{LightSource, Optics, TilePos};

use super::controller::{EventRng, EventState, RoundEvent, station_levels};
use crate::obj_damage::is_under;

pub const NAME: &str = "Aurora Caelus";
const SENDER: &str = "Отдел Метеорологии Нанотрейзен";
const NOTICE: &str = "sound/announcer/notice/notice2.ogg";
pub const AURORA_SOUND: &str = "sound/ambience/aurora_caelus/aurora_caelus.ogg";
const KITCHEN: &str = "/area/station/service/kitchen";
const OVEN: &str = "/obj/machinery/oven";
/// The gradient's ends.
pub const FROM: [u8; 3] = [0xA2, 0xFF, 0x80];
pub const TO: [u8; 3] = [0xA2, 0xFF, 0xEE];
const BLACK: [u8; 3] = [0, 0, 0];
/// `sleep(8 SECONDS)` between fade steps.
const FADE_STEP_SECS: f64 = 8.0;

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(AuroraCaelus)
}

/// `/datum/round_event/aurora_caelus`.
struct AuroraCaelus;

impl RoundEvent for AuroraCaelus {
    fn setup(&mut self, _: &mut World, ev: &mut EventState) {
        ev.announce_when = 1;
        ev.start_when = 21;
        ev.end_when = 80;
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, fake: bool) {
        let text = format!(
            "{}: безобидное облако ионов приближается к вашей станции и истощает свою энергию, \
             ударяя по корпусу. Компания Нанотрейзен одобрила небольшой перерыв для всех \
             сотрудников, чтобы они могли расслабиться и понаблюдать за этим очень редким \
             событием. В это время звездный свет будет ярким, но нежным, меняющимся между \
             спокойными зелеными и синими цветами. Любой персонал, желающий увидеть эти огни \
             самостоятельно, может пройти в ближайшую к ним зону со смотровыми иллюминаторами \
             на космос. Мы надеемся, что вам понравится это явление.",
            super::station_name(world)
        );
        announce(world, text);
        if fake {
            return;
        }
        let levels = station_levels(world);
        let mut q = world.query::<(Entity, &TilePos, &crate::server::PlayerMob)>();
        let mobs: Vec<(Entity, TilePos)> = q
            .iter(world)
            .filter(|(_, p, _)| levels.contains(&p.z))
            .map(|(e, &p, _)| (e, p))
            .collect();
        for (mob, pos) in mobs {
            crate::hallucination::play_local(world, mob, Some(pos), AURORA_SOUND, 20.0, false);
        }
        start_fade(world, true);
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let april = super::holidays::check_holidays(world, super::holidays::APRIL_FOOLS);
        if !april && !world.resource_mut::<EventRng>().0.prob(1.0) {
            return;
        }
        ruin_roasts(world);
    }

    fn tick(&mut self, world: &mut World, ev: &mut EventState) {
        if !ev.active_for.is_multiple_of(8) {
            return;
        }
        let t = f64::from(ev.active_for - ev.start_when) / f64::from(ev.end_when - ev.start_when);
        let color = hsl_gradient(t, FROM, TO);
        set_starlight(world, color, None, None);
        for turf in kitchen_floors(world) {
            if let Some(mut optics) = world.get_mut::<Optics>(turf)
                && let Some(light) = optics.light.as_mut()
            {
                light.color = color;
            }
        }
    }

    fn end(&mut self, world: &mut World, _: &mut EventState) {
        start_fade(world, false);
        let text = format!(
            "Событие aurora caelus подходит к концу. Условия звездного света постепенно вернутся \
             в норму. Когда все закончится, пожалуйста, вернитесь на свое рабочее место и \
             продолжайте работать в обычном режиме. Приятной смены, {} и спасибо, что смотрите \
             с нами.",
            super::station_name(world)
        );
        announce(world, text);
    }
}

fn announce(world: &mut World, text: String) {
    crate::announce::Priority::new(text, "")
        .sound(NOTICE)
        .sender(SENDER)
        .send(world);
}

/// tg `GLOB.base_starlight_color`, `starlight_range` and `starlight_power`: read from the
/// first starlit turf when the aurora begins.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Starlight {
    pub color: [u8; 3],
    pub range: f32,
    pub power: f32,
}

/// A running `fade_space` + `fade_kitchen`.
#[derive(Resource, Debug)]
pub struct AuroraFade {
    pub base: Starlight,
    pub fade_in: bool,
    /// When step 1 applies.
    pub started: f64,
    /// The next step, 1..=6 (6: the end values).
    pub next: u32,
    /// The kitchen's open turfs.
    pub kitchen: Vec<Entity>,
}

/// Whether a turf's optics are starlight: a space-lit tile (no lighting object) with a light.
fn is_starlit(optics: &Optics) -> bool {
    optics.light.is_some() && optics.tile.is_some_and(|t| !t.dynamic())
}

/// The starlight before any aurora: the running fade's base, else the first starlit turf.
pub fn base_starlight(world: &mut World) -> Option<Starlight> {
    if let Some(fade) = world.get_resource::<AuroraFade>() {
        return Some(fade.base);
    }
    let mut q = world.query_filtered::<&Optics, With<Turf>>();
    q.iter(world).find(|o| is_starlit(o)).and_then(|o| {
        o.light.map(|l| Starlight {
            color: l.color,
            range: l.range,
            power: l.power,
        })
    })
}

/// tg `set_starlight(star_color, range, power)`: `None` keeps the current range or power.
pub fn set_starlight(world: &mut World, color: [u8; 3], range: Option<f32>, power: Option<f32>) {
    let mut q = world.query_filtered::<&mut Optics, With<Turf>>();
    for mut optics in q.iter_mut(world) {
        if !is_starlit(&optics) {
            continue;
        }
        let light = optics.light.as_mut().expect("starlit");
        light.color = color;
        if let Some(range) = range {
            light.range = range;
        }
        if let Some(power) = power {
            light.power = power;
        }
    }
}

/// The open turfs of `/area/station/service/kitchen` areas.
fn kitchen_floors(world: &mut World) -> Vec<Entity> {
    if let Some(fade) = world.get_resource::<AuroraFade>() {
        return fade.kitchen.clone();
    }
    find_kitchen_floors(world)
}

fn find_kitchen_floors(world: &World) -> Vec<Entity> {
    super::station_tiles(world)
        .into_iter()
        .filter(|&(area, _)| super::area_type(world, area).is_some_and(|p| is_under(p, KITCHEN)))
        .filter_map(|(_, pos)| crate::construction::turf_at(world, pos))
        .filter(|&t| {
            crate::construction::path_of(world, t).is_some_and(|p| is_under(&p, "/turf/open"))
        })
        .collect()
}

fn start_fade(world: &mut World, fade_in: bool) {
    let Some(base) = base_starlight(world) else {
        return;
    };
    let kitchen = match world.remove_resource::<AuroraFade>() {
        Some(fade) => fade.kitchen,
        None => find_kitchen_floors(world),
    };
    let started = world.resource::<Time>().elapsed_secs_f64();
    world.insert_resource(AuroraFade {
        base,
        fade_in,
        started,
        next: 1,
        kitchen,
    });
    process_fades(world);
}

/// A light's `(color, range, power)`.
pub type Lit = ([u8; 3], f32, f32);

/// Starlight's and the kitchen floors' light at fade step `i` (1..=5; 6 = the end values).
pub fn fade_step(base: Starlight, fade_in: bool, i: u32) -> (Lit, Lit) {
    let aurora = (FROM, base.range * 1.75, base.power * 0.6);
    let plain = (base.color, base.range, base.power);
    let kitchen_lit = (FROM, 1.0, 0.75);
    let kitchen_dark = (BLACK, 0.5, 0.0);
    let ((s0, s1), (k0, k1)) = if fade_in {
        ((plain, aurora), (kitchen_dark, kitchen_lit))
    } else {
        (
            ((TO, aurora.1, aurora.2), plain),
            ((TO, 1.0, 0.75), kitchen_dark),
        )
    };
    let t = (f64::from(i.min(5)) / 5.0) as f32;
    let walk = |a: Lit, b: Lit| {
        if i > 5 {
            return b;
        }
        (
            hsl_gradient(f64::from(t), a.0, b.0),
            a.1 + (b.1 - a.1) * t,
            a.2 + (b.2 - a.2) * t,
        )
    };
    (walk(s0, s1), walk(k0, k1))
}

/// Applies the fade's due steps; done after step 6.
pub fn process_fades(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    loop {
        let Some(fade) = world.get_resource::<AuroraFade>() else {
            return;
        };
        if now < fade.started + f64::from(fade.next - 1) * FADE_STEP_SECS {
            return;
        }
        let (base, fade_in, i) = (fade.base, fade.fade_in, fade.next);
        let kitchen = fade.kitchen.clone();
        let ((sc, sr, sp), (kc, kr, kp)) = fade_step(base, fade_in, i);
        set_starlight(world, sc, Some(sr), Some(sp));
        for turf in kitchen {
            set_turf_light(world, turf, kr, kp, kc);
        }
        if i >= 6 {
            world.remove_resource::<AuroraFade>();
            return;
        }
        world.resource_mut::<AuroraFade>().next += 1;
    }
}

/// tg `set_light(range, power, color)` on a turf: no power is no light.
fn set_turf_light(world: &mut World, turf: Entity, range: f32, power: f32, color: [u8; 3]) {
    let light = (power != 0.0).then_some(LightSource {
        range,
        power,
        color,
        height: 1.0,
        angle: 360,
        dir: ss13_shared::Dir::North,
        offset: [0, 0],
    });
    match world.get_mut::<Optics>(turf) {
        Some(mut optics) => optics.light = light,
        None if light.is_some() => {
            world.entity_mut(turf).insert(Optics {
                opaque: false,
                tile: None,
                light,
            });
        }
        None => {}
    }
}

/// The `1%` (always on April Fools) ruined roast: every kitchen oven's tile gets 100 mol of
/// plasma at 1000 K.
fn ruin_roasts(world: &mut World) {
    let kitchen = find_kitchen_floors(world);
    let ovens: Vec<TilePos> = kitchen
        .iter()
        .filter_map(|&t| world.get::<TilePos>(t).copied())
        .filter(|&pos| {
            world
                .resource::<ss13_world::SpatialIndex>()
                .on_tile(pos)
                .iter()
                .any(|&e| {
                    crate::construction::path_of(world, e).is_some_and(|p| is_under(&p, OVEN))
                })
        })
        .collect();
    for pos in ovens {
        if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
            let _ = atmos.spawn_air(pos, "plasma=100;TEMP=1000");
        }
        info!("Aurora Caelus event caused an oven to ignite at {pos:?}");
    }
}

/// BYOND `gradient(0, a, 1, b, space = COLORSPACE_HSL, t)`: hue, saturation and lightness
/// interpolated linearly.
pub fn hsl_gradient(t: f64, a: [u8; 3], b: [u8; 3]) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    let (h0, s0, l0) = rgb_to_hsl(a);
    let (h1, s1, l1) = rgb_to_hsl(b);
    hsl_to_rgb(h0 + (h1 - h0) * t, s0 + (s1 - s0) * t, l0 + (l1 - l0) * t)
}

/// Hue in degrees, saturation and lightness in 0..=1.
fn rgb_to_hsl([r, g, b]: [u8; 3]) -> (f64, f64, f64) {
    let (r, g, b) = (
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
    );
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    if max == min {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0, s, l)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    [to(r), to(g), to(b)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsl_gradient_keeps_its_ends() {
        assert_eq!(hsl_gradient(0.0, FROM, TO), FROM);
        assert_eq!(hsl_gradient(1.0, FROM, TO), TO);
        let mid = hsl_gradient(0.5, FROM, TO);
        assert_eq!(mid[1], 0xFF);
        assert!(mid[2] > FROM[2] && mid[2] < TO[2], "{mid:?}");
        assert_eq!(hsl_gradient(1.0, BLACK, FROM), FROM);
    }

    #[test]
    fn fades_walk_from_starlight_to_the_aurora_and_back() {
        let base = Starlight {
            color: [0x85, 0x89, 0xfa],
            range: 2.0,
            power: 1.0,
        };
        let ((c, r, p), (kc, kr, kp)) = fade_step(base, true, 5);
        assert_eq!(c, FROM);
        assert!((r - 3.5).abs() < 1e-5 && (p - 0.6).abs() < 1e-5);
        assert_eq!((kc, kr, kp), (FROM, 1.0, 0.75));
        let ((_, r1, _), (_, _, kp1)) = fade_step(base, true, 1);
        assert!((r1 - 2.3).abs() < 1e-5 && (kp1 - 0.15).abs() < 1e-5);
        let ((c, r, p), (kc, _, kp)) = fade_step(base, false, 6);
        assert_eq!((c, r, p), (base.color, 2.0, 1.0));
        assert_eq!((kc, kp), (BLACK, 0.0));
        let ((c, ..), _) = fade_step(base, false, 1);
        assert_ne!(c, TO, "walking away from the aurora");
    }
}

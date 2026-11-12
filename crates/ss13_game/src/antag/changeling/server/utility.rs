//! Utility powers: augmented eyesight (`augmented_eyesight.dm`), the shrieks (`shriek.dm`) and
//! digital camouflage (`digitalcamo.dm`).
//!
//! - Augmented eyesight toggles x-ray vision (`TRAIT_XRAY_VISION`, `crate::vision`) with a flash
//!   weakness; while off, the eyes are protected like a welding mask ([`ling_flash_protect`]).
//! - Resonant shriek: hearers within 4 that are not changelings and not fully ear-protected get
//!   a massive soundbang (1 min of deafness); silicons are paralysed 10–20 s, the rest confused
//!   25 s and jittering 100 s; lights within 4 break.
//! - Technophagic shriek: an EMP (heavy 2, light 5) and lights within 5 break.
//! - Digital camouflage: the AI cannot track the changeling (`crate::silicon::ai_track`).
// shortcut: the purchase-time flash protection applies even without the "Мы настраиваем глаза"
// line; digital camouflage does not hide the changeling from the AI eye's sight or add the
// strange examine line; pheromone receptors, chameleon skin, spiders, MMI talk and the hive exit
// are not ported.

use super::*;

/// `SOUNDBANG_MASSIVE`.
const SOUNDBANG_MASSIVE: f32 = 3.0;
/// `FLASH_PROTECTION_WELDER`, `FLASH_PROTECTION_HYPER_SENSITIVE`.
const FLASH_PROTECTION_WELDER: i32 = 2;
const FLASH_PROTECTION_HYPER_SENSITIVE: i32 = -1;

/// The changeling's eyes' `flash_protect` from augmented eyesight (0 without it).
pub fn ling_flash_protect(world: &World, mob: Entity) -> i32 {
    match world.get::<Changeling>(mob) {
        Some(ling) if ling.has(Power::AugmentedEyesight) && !ling.fallen => {
            if ling.xray {
                FLASH_PROTECTION_HYPER_SENSITIVE
            } else {
                FLASH_PROTECTION_WELDER
            }
        }
        _ => 0,
    }
}

/// `TRAIT_XRAY_VISION` from augmented eyesight.
pub fn ling_xray(world: &World, mob: Entity) -> bool {
    world
        .get::<Changeling>(mob)
        .is_some_and(|l| l.xray && !l.fallen && l.has(Power::AugmentedEyesight))
}

/// `/datum/element/digitalcamo`.
pub fn ling_digital_camo(world: &World, mob: Entity) -> bool {
    world
        .get::<Changeling>(mob)
        .is_some_and(|l| l.digital_camo && l.has(Power::DigitalCamo))
}

/// `augmented_eyesight/sting_action`.
pub(super) fn augmented_eyesight(world: &mut World, client: Entity, mob: Entity) -> bool {
    if crate::medical::organs::organ(world, mob, crate::medical::organs::Slot::Eyes).is_none() {
        tell(world, client, Span::Warning, "нет глаз!");
        return false;
    }
    let Some(xray) = world.get_mut::<Changeling>(mob).map(|mut l| {
        l.xray = !l.xray;
        l.xray
    }) else {
        return false;
    };
    let text = if xray {
        "Мы настраиваем глаза, чтобы чувствовать добычу сквозь стены."
    } else {
        "Мы настраиваем глаза, чтобы защитить их от яркого света."
    };
    chat_to(world, mob, Span::Changeling, text);
    true
}

/// `digitalcamo/sting_action`.
pub(super) fn digital_camo(world: &mut World, mob: Entity) -> bool {
    let Some(camo) = world.get_mut::<Changeling>(mob).map(|mut l| {
        l.digital_camo = !l.digital_camo;
        l.digital_camo
    }) else {
        return false;
    };
    let text = if camo {
        "Мы искажаем свою форму, чтобы скрыться от ИИ."
    } else {
        "Мы возвращаемся в нормальное состояние."
    };
    chat_to(world, mob, Span::Notice, text);
    true
}

/// The lights within `range` of `pos` (`range(range, user)`), their tubes broken.
fn break_lights(world: &mut World, pos: TilePos, range: u32) {
    let lights: Vec<Entity> = world
        .query_filtered::<(Entity, &TilePos), With<crate::power::LightFixture>>()
        .iter(world)
        .filter(|(_, p)| p.chebyshev(pos).is_some_and(|d| d <= range))
        .map(|(e, _)| e)
        .collect();
    for light in lights {
        crate::power::break_light_tube(world, light);
    }
}

fn ventcrawling(world: &mut World, client: Entity, mob: Entity) -> bool {
    if world.get::<crate::ventcrawl::Ventcrawling>(mob).is_some() {
        tell(world, client, Span::Warning, "не можем кричать в трубах!");
        return true;
    }
    false
}

/// `resonant_shriek/sting_action`.
pub(super) fn resonant_shriek(world: &mut World, client: Entity, mob: Entity) -> bool {
    if ventcrawling(world, client, mob) {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return false;
    };
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, "sound/effects/screech.ogg", 100.0, false),
    );
    for living in crate::grenades::hearers(world, pos, 4) {
        if world.get::<Changeling>(living).is_some() {
            continue;
        }
        let effect = crate::explosions::ex_act::soundbang_act(
            world,
            living,
            SOUNDBANG_MASSIVE,
            0.0,
            0.0,
            60.0,
        );
        if effect <= 0.0 {
            continue;
        }
        if world.get::<crate::silicon::Silicon>(living).is_some() {
            let secs = world
                .get_resource_mut::<crate::combat::CombatRng>()
                .map_or(10, |mut r| r.range(10, 20));
            crate::status_effects::apply(world, living, Incap::Paralyze, secs as f32);
            continue;
        }
        use crate::status_effects::{Timed, adjust_timed, set_timed};
        adjust_timed(world, living, Timed::Confusion, 25.0, None);
        set_timed(world, living, Timed::Jitter, 100.0, true);
    }
    break_lights(world, pos, 4);
    true
}

/// `dissonant_shriek/sting_action`.
pub(super) fn dissonant_shriek(world: &mut World, client: Entity, mob: Entity) -> bool {
    if ventcrawling(world, client, mob) {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return false;
    };
    crate::emp::empulse(
        world,
        crate::emp::Empulse {
            epicenter: pos,
            heavy: 2,
            light: 5,
        },
    );
    break_lights(world, pos, 5);
    true
}

//! The supermatter eating what touches it (server): tg `/datum/component/supermatter_crystal`
//! (`code/datums/components/supermatter_crystal.dm`: `hand_hit`, `unarmed_hit`, `attackby_hit`,
//! `bumped_hit`, `dust_mob`, `consume`) and the crystal's `consume_callback`.
//!
//! - An empty hand (a cyborg's too) on the crystal dusts the mob; aiming at the mouth gives the
//!   bite, lick and forehead lines.
//! - An item on it is dropped and turns to dust; an item that cannot leave the hand takes the
//!   holder with it. The roasting stick and the spray can are spared; a cigarette is lit
//!   (outside combat mode) or ashed.
//! - A living mob or an object walking or pushed into it is consumed.
//!
//! Consuming adds `matter_increase` to the crystal's `external_power_trickle` and
//! `damage_increase` to its `external_damage_immediate` (a mob: `200 * (mob_size + 2)`, a
//! clown's random ±60 damage; an item `70 * w_class`; another object half its integrity, at most
//! 1000; the nuke core 10000 and 110 damage) and sends a radiation pulse.
// shortcut: no blob, paw, animal, hulk, z-fall and tram hits (those mobs and movers do not
// reach the crystal here), no clumsy cigarette dismemberment, no `TRAIT_GODMODE` or incorporeal
// checks (neither exists yet), no nuclear bomb with its core, no admin and investigate logs;
// thrown items do not hit it.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmObj, DmWeight};
use ss13_net::chat::server::SendChat;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_world::Bumped;

use super::supermatter::Supermatter;
use crate::combat::{CombatMode, CombatRng};
use crate::health::{Health, ZoneSelected};
use crate::obj_damage::is_under;
use crate::server::PlayerMob;
use crate::sound::{PlaySound, playsound};
use crate::zone::TargetZone;

const SOUND: &str = "sound/effects/supermatter.ogg";
/// `sm_item_whitelist`.
const WHITELIST: [&str; 2] = [
    "/obj/item/melee/roastingstick",
    "/obj/item/toy/crayon/spraycan",
];
const CIGARETTE: &str = "/obj/item/cigarette";
/// `consumed_object.type == /obj/item/nuke_core`: no subtypes (not the sliver).
const NUKE_CORE: &str = "/obj/item/nuke_core";
const EFFECT: &str = "/obj/effect";
/// `range(10)` of the radiation burn lines.
const BURN_LINES_RANGE: u32 = 10;

pub struct SupermatterConsumePlugin;

impl Plugin for SupermatterConsumePlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // `hand_hit`, `unarmed_hit` (cyborgs): before a cyborg's `attack_ai` (100).
        app.add_click_handler(ClickPhase::EmptyHand, 11, |w, c| {
            hand_hit(w, c.mob, c.target)
        });
        // `attackby_hit` (and `tool_hit` of anything but a movable crystal's wrench).
        app.add_click_handler(ClickPhase::HeldOnTarget, 21, |w, c| {
            c.held.is_some_and(|h| attackby_hit(w, c.mob, h, c.target))
        });
        app.add_systems(
            Update,
            bumped_hits.after(ss13_world::WorldSystems::Movement),
        );
    }
}

fn sm_pos(world: &World, crystal: Entity) -> Option<TilePos> {
    world.get::<Supermatter>(crystal)?;
    world.get::<TilePos>(crystal).copied()
}

fn name(world: &World, e: Entity) -> String {
    crate::security::name_of(world, e)
}

fn say(world: &mut World, at: TilePos, others: String, mob: Option<(Entity, String)>) {
    let personal: Vec<(Entity, Span, String)> = mob
        .into_iter()
        .map(|(m, text)| (m, Span::BoldWarning, text))
        .collect();
    let lines: Vec<SendChat> = crate::pull::visible_message(
        world,
        at,
        ChatChannel::Info,
        (Span::Danger, others),
        &personal,
    );
    for line in lines {
        world.write_message(line);
    }
}

/// `hand_hit` (and a cyborg's `unarmed_hit`): every way ends in `dust_mob`.
pub fn hand_hit(world: &mut World, mob: Entity, crystal: Entity) -> bool {
    let Some(pos) = sm_pos(world, crystal) else {
        return false;
    };
    if world.get::<Health>(mob).is_none() {
        return false;
    }
    let (who, sm) = (name(world, mob), name(world, crystal));
    let mouth = world
        .get::<ZoneSelected>(mob)
        .is_some_and(|z| z.0 == TargetZone::Mouth);
    let lines = if !mouth {
        None
    } else if crate::clothing::cover::mouth_covered(world, mob).is_none() {
        if world.get::<CombatMode>(mob).is_some_and(|c| c.0) {
            Some((
                format!(
                    "{who} пытается откусить кусок {sm}, всё вокруг затихает, а затем тело \
                     начинает светиться, вспыхивает и обращается в пепел."
                ),
                format!(
                    "Вы пытаетесь откусить кусок {sm}, но он слишком твёрдый — всё вокруг \
                     начинает гореть, а в ушах нарастает звон!"
                ),
            ))
        } else {
            Some((
                format!(
                    "{who} нерешительно наклоняется и лижет {sm}, всё вокруг затихает, а затем \
                     тело вспыхивает и обращается в пепел!"
                ),
                format!(
                    "Вы осторожно лижете {sm}, но не успеваете понять вкус — всё вокруг \
                     начинает гореть, а в ушах нарастает звон!"
                ),
            ))
        }
    } else {
        Some((
            format!(
                "Лоб {who} касается {sm}, вызывая резонанс... Всё затихает, прежде чем голова \
                 обращается в пепел!"
            ),
            "Вы касаетесь лбом кристалла, и всё внезапно затихает. Голову наполняет звон, и \
             вы понимаете, что это было не самое мудрое решение."
                .to_owned(),
        ))
    };
    dust_mob(world, crystal, pos, mob, lines);
    true
}

/// `dust_mob`: the lines (tg's defaults without), the sound, then `consume`.
fn dust_mob(
    world: &mut World,
    crystal: Entity,
    pos: TilePos,
    mob: Entity,
    lines: Option<(String, String)>,
) {
    let (who, sm) = (name(world, mob), name(world, crystal));
    let (others, own) = lines.unwrap_or_else(|| {
        (
            format!(
                "{who} касается {sm}, вызывая резонанс... Тело начинает светиться, вспыхивает \
                 и обращается в пыль!"
            ),
            format!(
                "Вы касаетесь {sm}. Всё вокруг начинает гореть, и слышен лишь звон. Последняя \
                 мысль: «Это было не самое мудрое решение»."
            ),
        )
    });
    if let Some(&at) = world.get::<TilePos>(mob) {
        say(world, at, others, Some((mob, own)));
    }
    playsound(world, PlaySound::new(pos, SOUND, 50.0, true));
    consume(world, crystal, mob);
}

/// `attackby_hit`. True when the click is taken.
pub fn attackby_hit(world: &mut World, mob: Entity, item: Entity, crystal: Entity) -> bool {
    let Some(pos) = sm_pos(world, crystal) else {
        return false;
    };
    let Some(path) = world.get::<DmItem>(item).map(|i| i.path.clone()) else {
        return false;
    };
    if world.get::<Health>(mob).is_none() || WHITELIST.iter().any(|w| is_under(&path, w)) {
        return false;
    }
    let (who, what, sm) = (name(world, mob), name(world, item), name(world, crystal));
    if is_under(&path, CIGARETTE) {
        let combat = world.get::<CombatMode>(mob).is_some_and(|c| c.0);
        let can_light = world
            .get::<crate::smoking::Smokable>(item)
            .is_some_and(|s| !s.lit && s.smoketime > 0.0);
        if combat || !can_light {
            say(
                world,
                pos,
                format!(
                    "{what} с ужасным звуком обращается в пепел при касании {sm}. Кажется, это была плохая идея."
                ),
                None,
            );
            playsound(world, PlaySound::new(pos, SOUND, 150.0, true));
            consume(world, crystal, item);
            crate::radiation::radiation_pulse(world, crystal, 3.0, 0.1, 50.0, 0.0);
        } else {
            say(
                world,
                pos,
                format!("{who} прикуривает {what} от {sm}, и комнату наполняет тишина..."),
                Some((
                    mob,
                    format!(
                        "Время словно замедляется, пока вы касаетесь {sm} сигаретой. {what} \
                         вспыхивает жутким светом, и вы невозмутимо убираете руку. Чёрт."
                    ),
                )),
            );
            crate::smoking::light(world, item, None);
            playsound(world, PlaySound::new(pos, SOUND, 50.0, true));
            crate::radiation::radiation_pulse(world, crystal, 1.0, 0.0, 100.0, 0.0);
        }
        return true;
    }
    if !crate::inventory::no_drop(world, item) {
        if let Some(&at) = world.get::<TilePos>(mob) {
            crate::inventory::put_down(world, item, at);
        }
        say(
            world,
            pos,
            format!("{who} касается {sm} предметом {what}, и комнату наполняет тишина..."),
            Some((
                mob,
                format!(
                    "Вы касаетесь {sm} предметом {what}, и всё внезапно затихает. {what} \
                     обращается в пыль, а вы отдёргиваете руку."
                ),
            )),
        );
        consume(world, crystal, item);
        playsound(world, PlaySound::new(pos, SOUND, 50.0, true));
        crate::radiation::radiation_pulse(world, crystal, 3.0, 0.1, 50.0, 0.0);
        return true;
    }
    // Stuck to the hand: the holder goes too.
    let lines = (
        format!(
            "{who} касается {sm} предметом {what}, вызывая резонанс... {what} ненадолго \
             светится, затем свет переходит на тело — оно вспыхивает и обращается в пыль!"
        ),
        format!(
            "Вы касаетесь {sm} предметом {what}. Всё вокруг начинает гореть, и слышен лишь \
             звон. Последняя мысль: «Это было не самое мудрое решение»."
        ),
    );
    dust_mob(world, crystal, pos, mob, Some(lines));
    true
}

/// `bumped_hit`: a living mob or an object (not an effect) that walks or is pushed into it.
fn bumped_hits(world: &mut World, mut cursor: Local<MessageCursor<Bumped>>) {
    let bumps: Vec<Bumped> = cursor
        .read(world.resource::<Messages<Bumped>>())
        .filter(|b| world.get::<Supermatter>(b.obstacle).is_some())
        .copied()
        .collect();
    for bump in bumps {
        let (crystal, thing) = (bump.obstacle, bump.entity);
        let (Some(pos), Ok(_)) = (sm_pos(world, crystal), world.get_entity(thing)) else {
            continue;
        };
        let (what, sm) = (name(world, thing), name(world, crystal));
        if world.get::<Health>(thing).is_some() {
            say(
                world,
                pos,
                format!(
                    "{what} врезается в {sm}, вызывая резонанс... Тело начинает светиться, \
                     вспыхивает и обращается в пыль!"
                ),
                Some((
                    thing,
                    format!(
                        "Вы врезаетесь в {sm}, и уши наполняет неземной звон. Последняя мысль: \
                         «Вот блин»."
                    ),
                )),
            );
        } else if world
            .get::<DmObj>(thing)
            .map(|o| o.path.as_str())
            .or_else(|| world.get::<DmItem>(thing).map(|i| i.path.as_str()))
            .is_some_and(|p| !is_under(p, EFFECT))
        {
            say(
                world,
                pos,
                format!("{what} ударяется о {sm} и мгновенно обращается в пепел."),
                None,
            );
        } else {
            continue;
        }
        playsound(world, PlaySound::new(pos, SOUND, 50.0, true));
        consume(world, crystal, thing);
    }
}

/// What the crystal eats, as `consume` sorts it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Eaten {
    /// `mob_size`, and the clown's `rand(-30, 30)`.
    Mob {
        mob_size: u8,
        clown_roll: Option<f64>,
    },
    Item {
        w_class: u8,
        nuke_core: bool,
    },
    Obj {
        max_integrity: f32,
    },
    Effect,
}

/// `consume`'s `object_size`, `matter_increase`, `damage_increase` and `radiation_range`.
fn gains(eaten: Eaten) -> (f64, f64, f64, f32) {
    match eaten {
        Eaten::Mob {
            mob_size,
            clown_roll,
        } => {
            let size = f64::from(mob_size) + 2.0;
            (
                size,
                100.0 * size * 2.0,
                clown_roll.unwrap_or(0.0) * 2.0,
                6.0,
            )
        }
        Eaten::Item {
            nuke_core: true, ..
        } => (10.0, 10_000.0, 110.0, 12.0),
        Eaten::Item { w_class, .. } => {
            let size = f64::from(w_class);
            (size, 70.0 * size, 0.0, 6.0)
        }
        Eaten::Obj { max_integrity } => {
            (0.0, (0.5 * f64::from(max_integrity)).min(1000.0), 0.0, 6.0)
        }
        Eaten::Effect => (0.0, 0.0, 0.0, 6.0),
    }
}

fn clown_roll(world: &mut World, mob: Entity) -> Option<f64> {
    let clown = world
        .get::<crate::jobs::PlayerJob>(mob)
        .and_then(|j| {
            world
                .get_resource::<crate::jobs::Jobs>()?
                .data
                .jobs
                .get(j.0)
        })
        .is_some_and(|j| j.title == "Clown");
    clown.then(|| f64::from(world.resource_mut::<CombatRng>().range(0, 60)) - 30.0)
}

/// `consume(atom_source, consumed_object)` and `consume_callback`.
pub fn consume(world: &mut World, crystal: Entity, thing: Entity) {
    if world.get_entity(thing).is_err() || world.get::<Supermatter>(crystal).is_none() {
        return;
    }
    let eaten = if world.get::<Health>(thing).is_some() {
        Eaten::Mob {
            mob_size: crate::mobs::pets::mob_size(world, thing),
            clown_roll: clown_roll(world, thing),
        }
    } else if let Some(item) = world.get::<DmItem>(thing) {
        Eaten::Item {
            w_class: world
                .get::<DmWeight>(thing)
                .copied()
                .unwrap_or_default()
                .w_class,
            nuke_core: item.path == NUKE_CORE,
        }
    } else {
        match world.get::<DmObj>(thing) {
            Some(o) if !is_under(&o.path, EFFECT) => Eaten::Obj {
                max_integrity: o.max_integrity,
            },
            _ => Eaten::Effect,
        }
    };
    if matches!(eaten, Eaten::Mob { .. }) {
        crate::health::dust(world, thing);
    } else {
        super::singularity::qdel(world, thing);
    }
    let (size, matter_increase, damage_increase, radiation_range) = gains(eaten);
    let size = size as f32;
    crate::radiation::radiation_pulse(
        world,
        crystal,
        radiation_range,
        1.2 / size.max(1.0),
        10.0 * size,
        0.0,
    );
    burn_lines(world, crystal);
    if let Some(mut sm) = world.get_mut::<Supermatter>(crystal) {
        sm.crystal.external_power_trickle += matter_increase;
        sm.crystal.external_damage_immediate += damage_increase;
    }
}

/// The radiation burn lines to the living within 10 tiles (players: only they read them).
fn burn_lines(world: &mut World, crystal: Entity) {
    let Some(pos) = sm_pos(world, crystal) else {
        return;
    };
    let sm = name(world, crystal);
    let mut q = world.query::<(Entity, &PlayerMob, &TilePos)>();
    let near: Vec<(Entity, TilePos)> = q
        .iter(world)
        .filter(|(_, _, p)| p.chebyshev(pos).is_some_and(|d| d <= BURN_LINES_RANGE))
        .map(|(e, _, &p)| (e, p))
        .collect();
    for (mob, at) in near {
        if world.get::<Health>(mob).is_none()
            || world.get::<crate::health::Stat>(mob) == Some(&crate::health::Stat::Dead)
            || world.get::<crate::silicon::Silicon>(mob).is_some()
            || crate::radiation::rad_immune(world, mob)
            || crate::radiation::wearing_rad_protected_clothing(world, mob)
        {
            continue;
        }
        let seen = at
            .chebyshev(pos)
            .is_some_and(|d| d <= crate::health::DEFAULT_MESSAGE_RANGE);
        let text = if seen {
            format!(
                "Когда {sm} медленно перестаёт резонировать, вы обнаруживаете на коже свежие радиационные ожоги."
            )
        } else {
            "Уши наполняет неземной звон, и вы обнаруживаете на коже свежие радиационные ожоги."
                .to_owned()
        };
        crate::tools::tell_mob(world, mob, Span::Danger, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_each_meal_gives() {
        // A human (`MOB_SIZE_HUMAN` 2): size 4, 800 matter.
        assert_eq!(
            gains(Eaten::Mob {
                mob_size: 2,
                clown_roll: None
            }),
            (4.0, 800.0, 0.0, 6.0)
        );
        assert_eq!(
            gains(Eaten::Mob {
                mob_size: 2,
                clown_roll: Some(-12.0)
            })
            .2,
            -24.0
        );
        assert_eq!(
            gains(Eaten::Item {
                w_class: 3,
                nuke_core: false
            }),
            (3.0, 210.0, 0.0, 6.0)
        );
        assert_eq!(
            gains(Eaten::Item {
                w_class: 1,
                nuke_core: true
            }),
            (10.0, 10_000.0, 110.0, 12.0)
        );
        assert_eq!(
            gains(Eaten::Obj {
                max_integrity: 3000.0
            })
            .1,
            1000.0
        );
        assert_eq!(gains(Eaten::Effect).1, 0.0);
    }
}

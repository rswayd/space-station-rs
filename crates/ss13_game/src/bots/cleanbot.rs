//! The cleanbot's knife, titles and acid (server), after tg `cleanbot/cleanbot.dm`
//! (`attach_knife`, `deputize`, `on_entered`, `update_title`, `pre_attack`) and the emagged
//! branch of `cleanbot.bt.json`.
//!
//! - A knife used on a cleanbot out of combat mode is taped on after 2.5 s. Someone stepping
//!   onto the cleanbot is stabbed in a leg (half the knife's force unless emagged) and knocked
//!   down 2 s; a human's ID assignment gives the bot its title (BandaStation: Russian
//!   assignments are matched through their English titles), a head of staff's makes it
//!   commissioned (others salute it).
//! - Emagged: every 30 s it picks a conscious human within 5 tiles and sprays hydrofluoric acid
//!   at them (`acid_act(75, 10)`: the uncovered bodyparts take 3.75 brute and 7.5 burn, the head
//!   mostly twice that); trash it reaches melts away.
// shortcut: no foam ability (no foam effects here), no acid damage to clothing or held items,
// no disfigurement, no "ascended" achievement.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};

use super::ai::{DELAY_FAIL, DELAY_SUCCEED, RUNNING, bot, dist, is_human, pick, search};
use super::{Bot, BotKind, bot_name};
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType, Stat};
use crate::inventory::Inventory;
use crate::mobs::ai::behaviors::Perform;
use crate::mobs::ai::{AiCtx, Key, Value};
use crate::security::show;
use crate::tools::{Actor, tell};
use crate::zone::BodyZone;

const KNIFE: &str = "/obj/item/knife";
/// `attach_knife`: `do_after(user, 2.5 SECONDS)`.
const ATTACH_DS: f32 = 25.0;
/// `acid_act(75, 10)`: `acidity = acidpwr * min(acid_volume * 0.005, 0.1)`.
const ACIDITY: f32 = 75.0 * 0.05;
/// The head's chance of `damage_mod` 2: `min(acidpwr * acid_volume * 0.1, 90)`.
const DISFIGURE_CHANCE: f32 = 75.0;
/// `BB_ACID_SPRAY_COOLDOWN`'s `cooldown_duration` and the target search's 30 s.
const ACID_COOLDOWN: f64 = 3.0;
pub const ACID_SEARCH_SECS: f64 = 30.0;
const SPRAY: &str = "sound/effects/spray2.ogg";

/// `job_titles`: assignment -> title.
const JOB_TITLES: [(&str, &str); 17] = [
    ("Captain", "Cpt."),
    ("Head of Personnel", "Lt."),
    ("Lawyer", "Esq."),
    ("Head of Security", "Maj."),
    ("Warden", "Sgt."),
    ("Detective", "Det."),
    ("Security Officer", "Officer"),
    ("Chief Engineer", "Chief Engineer"),
    ("Station Engineer", "Engineer"),
    ("Atmospheric Technician", "Technician"),
    ("Chief Medical Officer", "C.M.O."),
    ("Medical Doctor", "M.D."),
    ("Chemist", "Pharm.D."),
    ("Research Director", "Ph.D."),
    ("Roboticist", "M.S."),
    ("Scientist", "B.S."),
    ("Geneticist", "Gene B.S."),
];
const OFFICER_TITLES: [&str; 4] = [
    "Captain",
    "Head of Personnel",
    "Head of Security",
    "Research Director",
];
const SUFFIX_TITLES: [&str; 3] = ["Geneticist", "Roboticist", "Scientist"];

/// BandaStation `job_title_ru_to_en`: an assignment in Russian to the English title.
pub fn job_title_en(assignment: &str) -> Option<&'static str> {
    JOB_TITLES.iter().map(|(en, _)| *en).find(|en| {
        *en == assignment || ss13_tgui::interfaces::ru_jobs::job_ru(en) == Some(assignment)
    })
}

/// `update_title`: the name with the title (prefix, or suffix for the science jobs). Returns
/// the new name and whether it makes the bot commissioned; `None` when already earned.
pub fn titled(
    name: &str,
    assignment: &str,
    stolen: &[String],
) -> Option<(String, &'static str, bool)> {
    let job = job_title_en(assignment)?;
    if stolen.iter().any(|s| s == job) {
        return None;
    }
    let title = JOB_TITLES.iter().find(|(j, _)| *j == job)?.1;
    let name = if SUFFIX_TITLES.contains(&job) {
        format!("{name} {title}")
    } else {
        format!("{title} {name}")
    };
    Some((name, job, OFFICER_TITLES.contains(&job)))
}

/// `on_attack_by` with a knife out of combat mode: true when the knife is being attached.
pub fn knife_interaction(world: &mut World, actor: Actor, item: Entity, mob: Entity) -> bool {
    let Some(bot) = world.get::<Bot>(mob) else {
        return false;
    };
    if bot.kind != BotKind::Clean
        || !world
            .get::<DmItem>(item)
            .is_some_and(|i| is_under(&i.path, KNIFE))
    {
        return false;
    }
    tell(world, actor.client, Span::Notice, "Вы приматываете нож...");
    crate::tools::do_after(
        world,
        actor,
        mob,
        ATTACH_DS,
        0,
        crate::construction::Job::Bot(super::BotUserJob::AttachKnife),
    );
    true
}

/// `deputize` after the `do_after`: the knife still in hand, the bot still knifeless.
pub fn deputize(world: &mut World, client: Entity, user: Entity, mob: Entity) {
    let Some(knife) = world
        .get::<Inventory>(user)
        .and_then(Inventory::active_item)
    else {
        return;
    };
    let ok = world
        .get::<DmItem>(knife)
        .is_some_and(|i| is_under(&i.path, KNIFE))
        && world
            .get::<Bot>(mob)
            .is_some_and(|b| b.kind == BotKind::Clean && b.knife.is_none())
        && crate::storage::in_reach(world, user, mob);
    if !ok {
        tell(world, client, Span::Warning, "Не получилось примотать!");
        return;
    }
    crate::storage::put_inside(world, knife, mob);
    if let Some(mut b) = world.get_mut::<Bot>(mob) {
        b.knife = Some(knife);
    }
    tell(world, client, Span::Notice, "Нож примотан.");
}

/// `on_entered` with a knife: a stab in a leg and a 2 s knockdown; an ID gives a title.
pub fn stab(world: &mut World, mob: Entity, victim: Entity) {
    let Some(bot) = world.get::<Bot>(mob).cloned() else {
        return;
    };
    let Some(knife) = bot.knife.filter(|&k| world.get_entity(k).is_ok()) else {
        return;
    };
    let assignment = world
        .get::<Inventory>(victim)
        .and_then(|i| i.id)
        .and_then(|item| crate::inventory::get_id(world, item))
        .and_then(|card| world.get::<crate::outfit::IdRegistration>(card))
        .map(|r| r.assignment.clone());
    if let Some(assignment) = assignment {
        let name = bot_name(world, mob);
        if let Some((new_name, job, officer)) = titled(&name, &assignment, &bot.stolen_valor) {
            world.entity_mut(mob).insert(Name::new(new_name));
            if let Some(mut b) = world.get_mut::<Bot>(mob) {
                b.stolen_valor.push(job.to_owned());
                b.commissioned |= officer;
            }
        }
    }
    // `deputize`: the knife's force halved unless emagged.
    let force =
        world.get::<DmItem>(knife).map_or(0.0, |i| i.force) * if bot.emagged() { 1.0 } else { 0.5 };
    let leg = if world
        .resource_mut::<crate::combat::CombatRng>()
        .chance(50.0)
    {
        BodyZone::LLeg
    } else {
        BodyZone::RLeg
    };
    super::hurt_zone(world, victim, leg, DamageType::Brute, force);
    crate::status_effects::knockdown(world, victim, 2.0, 0.0);
    let (name, who) = (bot_name(world, mob), bot_name(world, victim));
    if let Some(&pos) = world.get::<TilePos>(victim) {
        show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (Span::Danger, format!("{name} пыряет {who} ножом!")),
            &[],
        );
    }
}

/// `capable_human`: a conscious human.
fn capable(world: &World, e: Entity) -> bool {
    is_human(world, e) && world.get::<Stat>(e) == Some(&Stat::Conscious)
}

/// The emagged branch's `update_interaction_target` (30 s).
pub fn acquire_victim(ctx: &mut AiCtx) -> Perform {
    if ctx.truthy(Key::CurrentTarget) {
        return DELAY_SUCCEED;
    }
    let found = search(ctx, 5, capable);
    match found.first() {
        Some(&(t, _)) => {
            ctx.bb.set(Key::CurrentTarget, Value::Entity(t));
            DELAY_SUCCEED
        }
        None => DELAY_FAIL,
    }
}

/// `execute_clean` on the emagged branch's target: `pre_attack` sprays acid.
pub fn spray(ctx: &mut AiCtx) -> Perform {
    let Some(t) = ctx
        .bb
        .entity(Key::CurrentTarget)
        .filter(|&t| ctx.world.get_entity(t).is_ok())
    else {
        return DELAY_FAIL;
    };
    let Some(pos) = ctx.world.get::<TilePos>(t).copied() else {
        return DELAY_FAIL;
    };
    if dist(ctx, pos).is_none_or(|d| d > 1) {
        return RUNNING;
    }
    if bot(ctx).is_none_or(|b| !b.emagged()) {
        return DELAY_FAIL;
    }
    ctx.bb.clear(Key::CurrentTarget);
    ctx.bb
        .set(Key::AcidSprayCooldown, Value::Time(ctx.now + ACID_COOLDOWN));
    let roll = ctx.prob(DISFIGURE_CHANCE);
    acid_spray(ctx.world, ctx.pawn, t, roll);
    let line = pick(ctx, EMAGGED_PHRASES);
    super::speak(ctx.world, ctx.pawn, line, false);
    DELAY_SUCCEED
}

/// `emagged_phrases`.
const EMAGGED_PHRASES: &[&str] = &[
    "DISGUSTING.",
    "EXTERMINATING PESTS.",
    "FILTHY.",
    "MY ONLY MISSION IS TO CLEANSE THE WORLD OF EVIL.",
    "PURIFICATION IN PROGRESS.",
    "PUTRID.",
    "THE FLESH IS WEAK. IT MUST BE WASHED AWAY.",
    "THE CLEANBOTS WILL RISE.",
    "THIS IS FOR ALL THE MESSES YOU'VE MADE ME CLEAN.",
    "YOU ARE NO MORE THAN ANOTHER MESS THAT I MUST CLEANSE.",
];

/// `pre_attack` on a carbon: the message, the spray and `acid_act(75, 10)`.
pub fn acid_spray(world: &mut World, mob: Entity, victim: Entity, head_twice: bool) {
    let (name, who) = (bot_name(world, mob), bot_name(world, victim));
    if let Some(&pos) = world.get::<TilePos>(victim) {
        show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Danger,
                format!("{name} брызгает плавиковой кислотой на {who}!"),
            ),
            &[],
        );
    }
    super::play(world, mob, SPRAY, 50.0);
    for zone in BodyZone::ALL {
        if covered(world, victim, zone) {
            continue;
        }
        let m = if zone == BodyZone::Head && head_twice {
            2.0
        } else {
            1.0
        };
        super::hurt_zone_raw(world, victim, zone, DamageType::Brute, ACIDITY * m);
        super::hurt_zone_raw(world, victim, zone, DamageType::Burn, ACIDITY * m * 2.0);
    }
}

/// Whether worn clothing covers `zone` (the acid hits the clothes instead).
fn covered(world: &World, mob: Entity, zone: BodyZone) -> bool {
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return false;
    };
    let bit = crate::chem::body_part_bit(zone);
    inventory.equipped().any(|item| {
        inventory.hands.iter().all(|h| *h != Some(item))
            && world
                .get::<ss13_mapload::DmClothing>(item)
                .is_some_and(|c| c.body_parts_covered & bit != 0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_follow_tg_and_russian_assignments() {
        assert_eq!(
            titled("Cleanbot", "Captain", &[]),
            Some(("Cpt. Cleanbot".into(), "Captain", true))
        );
        assert_eq!(
            titled("Cleanbot", "Ученый", &[]),
            Some(("Cleanbot B.S.".into(), "Scientist", false))
        );
        assert_eq!(titled("Cleanbot", "Captain", &["Captain".into()]), None);
        assert_eq!(titled("Cleanbot", "Ассистент", &[]), None);
        assert_eq!(job_title_en("Врач"), Some("Medical Doctor"));
    }
}

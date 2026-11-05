//! `emp_act` of mobs and what they carry: silicons (`/mob/living/silicon`, the cyborg, the
//! AI), station bots, synthetic organs, radios, energy guns, stun batons.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::DmGun;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::security::DmBaton;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::power::cell_drain;
use super::{
    EMP_HEAVY, EMP_LIGHT, EmpAct, EmpRng, EmpSystems, Emped, STANDARD_CELL_CHARGE, emp_acts,
    emped_expired, path_of, set_emped, sparks,
};
use crate::bots::{Bot, MODE_ON};
use crate::guns::Gun;
use crate::health::{DamageType, Health, Stat};
use crate::inventory::CarriedBy;
use crate::medical::organs::{Organ, OrganOf, Slot};
use crate::radio::Radio;
use crate::security::Baton;
use crate::server::PlayerMob;
use crate::silicon::Silicon;
use crate::silicon::ai::{AiCore, AiEye};
use crate::silicon::cyborg::Cyborg;
use crate::status_effects::Incap;

/// `/obj/item/radio/emp_act`: `end_emp_effect` after 20 s.
const RADIO_EMP_SECS: f64 = 20.0;
/// `/mob/living/silicon/robot/emp_act`: `Unconscious(16 / 6 SECONDS)`.
const BORG_KNOCKOUT_SECS: [f32; 2] = [16.0, 6.0];
/// `/mob/living/silicon/emp_act`: `take_bodypart_damage(burn = 20 / 10)`.
const SILICON_BURN: [f32; 2] = [20.0, 10.0];
/// The AI's evacuation call on a surge.
const AI_EVAC_REASON: &str = "ALERT: Energy surge detected in AI core! Station integrity may be \
                              compromised! Initiati--%m091#ar-BZZT";
/// `emp_vulnerability` of synthetic organs by tier.
const ORGAN_VULNERABILITY: [(&str, f64); 3] =
    [("/surplus", 100.0), ("/tier3", 20.0), ("/tier2", 40.0)];
const BASE_ORGAN_VULNERABILITY: f64 = 80.0;
/// `severe_cooldown` of cybernetic hearts, lungs, livers and stomachs.
const HEART_COOLDOWN: f64 = 15.0;
const LUNGS_COOLDOWN: f64 = 30.0;
const LIVER_COOLDOWN: f64 = 10.0;
const STOMACH_COOLDOWN: f64 = 10.0;
/// `Restart` after `10 SECONDS / severity`.
const HEART_RESTART_SECS: f64 = 10.0;
/// The assault laser's `emp_resistance` (the rest have 1).
const EMP_RESISTANT_GUNS: [(&str, f64); 1] = [("/obj/item/gun/energy/laser/assault", 2.0)];

pub(super) struct EmpMobsPlugin;

impl Plugin for EmpMobsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                (silicons, bots, organs, radios, guns, batons).in_set(EmpSystems::Effects),
                (restore_radios, restore_bots).in_set(EmpSystems::Restore),
                restart_hearts.run_if(any_with_component::<HeartRestart>),
            ),
        );
    }
}

fn severity_index(severity: u8) -> Option<usize> {
    match severity {
        EMP_HEAVY => Some(0),
        EMP_LIGHT => Some(1),
        _ => None,
    }
}

fn tell_mob(world: &mut World, mob: Entity, span: Span, text: &str) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        crate::tools::tell(world, client, span, text);
    }
}

/// `emp_damage_modifier` of a cell by its capacity, for devices that only know the
/// `maxcharge` of their `preload_cell_type`: high cells 3, super and better 5.
pub fn modifier_by_capacity(maxcharge: f64) -> f64 {
    if maxcharge >= 20.0 * STANDARD_CELL_CHARGE {
        5.0
    } else if maxcharge >= 10.0 * STANDARD_CELL_CHARGE {
        3.0
    } else {
        1.0
    }
}

/// `/mob/living/silicon/emp_act` (burn, `*БЗЗЗТ*`, the static flash), then the cyborg's
/// knockout and cell drain or the AI's surge (`view_core` or an evacuation call, 30%).
// shortcut: buckled riders do not fall off (no riding cyborgs); no AI shells to disconnect.
fn silicons(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        let mob = ev.target;
        if world.get::<Silicon>(mob).is_none() {
            continue;
        }
        tell_mob(
            world,
            mob,
            Span::Danger,
            "Предупреждение: обнаружен электромагнитный импульс.",
        );
        if !ev.hits_self() {
            continue;
        }
        if let Some(i) = severity_index(ev.severity)
            && let Some(mut health) = world.get_mut::<Health>(mob)
        {
            health.adjust(DamageType::Burn, SILICON_BURN[i]);
        }
        tell_mob(world, mob, Span::BoldWarning, "*БЗЗЗТ*");
        crate::vision::flash_act(world, mob, 1, false, false, crate::vision::FLASH_LENGTH);
        if let Some(cyborg) = world.get::<Cyborg>(mob).cloned() {
            if let Some(i) = severity_index(ev.severity) {
                crate::status_effects::apply(world, mob, Incap::Unconscious, BORG_KNOCKOUT_SECS[i]);
            }
            // The cell is in the cyborg's contents (`/mob/living/emp_act`).
            if ev.hits_contents() {
                let drain = cell_drain(ev.severity, modifier_by_capacity(cyborg.max_charge));
                if let Some(mut c) = world.get_mut::<Cyborg>(mob) {
                    c.charge = (c.charge - drain).max(0.0);
                }
            }
        }
        if let Some(eye) = world.get::<AiCore>(mob).map(|c| c.eye) {
            let (surge, pick) = {
                let mut rng = world.resource_mut::<EmpRng>();
                (rng.prob(30.0), rng.range(1, 2))
            };
            if !surge {
                continue;
            }
            if pick == 1 {
                // `view_core`: the eye back on the core.
                if let Some(&pos) = world.get::<TilePos>(mob)
                    && world.get::<AiEye>(eye).is_some()
                {
                    world.entity_mut(eye).insert(pos);
                }
            } else if matches!(crate::comms_server::can_evac(world), Some(Ok(()))) {
                crate::shuttle::call_evac_shuttle(world, AI_EVAC_REASON, None);
            }
        }
    }
}

/// A bot an EMP switched off: it comes back on at the time if it was on.
#[derive(Component, Clone, Copy, Debug)]
pub struct BotEmped {
    pub was_on: bool,
}

/// `/mob/living/basic/bot/emp_act`: sparks; a robotic basic mob's `emp_reaction` (60% of its
/// health on a light pulse, all of it on a heavy one); the bot goes off and, if it was on,
/// back on after `severity * 30 s`.
// tg's basic bot turns an off bot on and leaves an on one off for good (`else addtimer(turn_on)`);
// the simple bot it replaced did what is ported here. No pAI card (no pAIs) or language.
fn bots(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        let mob = ev.target;
        let Some(on) = world.get::<Bot>(mob).map(|b| b.mode_flags & MODE_ON != 0) else {
            continue;
        };
        let max = world.get::<Health>(mob).map_or(0.0, |h| h.max);
        let damage = match ev.severity {
            EMP_LIGHT => Some((max * 0.6, "сильно трясется, а детали разбалтываются!")),
            EMP_HEAVY => Some((max, "внезапно разрывается на части!")),
            _ => None,
        };
        if let Some((amount, text)) = damage {
            let name = crate::security::name_of(world, mob);
            if let Some(&pos) = world.get::<TilePos>(mob) {
                crate::security::show(
                    world,
                    pos,
                    crate::health::DEFAULT_MESSAGE_RANGE,
                    ss13_shared::chat::ChatChannel::Local,
                    (Span::Danger, format!("{name} {text}")),
                    &[],
                );
            }
            if let Ok(mut entity) = world.get_entity_mut(mob)
                && let Ok((mut health, mut stat)) =
                    entity.get_components_mut::<(&mut Health, &mut Stat)>()
            {
                let mut new = *stat;
                crate::health::apply_damage(&mut health, &mut new, DamageType::Brute, amount);
                stat.set_if_neq(new);
            }
        }
        if !ev.hits_self() {
            continue;
        }
        sparks(world, mob);
        if world.get::<Stat>(mob) == Some(&Stat::Dead) {
            continue;
        }
        crate::bots::turn_off(world, mob);
        let was_on = on || world.get::<BotEmped>(mob).is_some_and(|b| b.was_on);
        world.entity_mut(mob).insert(BotEmped { was_on });
        set_emped(world, mob, 30.0 * f64::from(ev.severity));
    }
}

fn restore_bots(world: &mut World) {
    for mob in emped_expired::<With<BotEmped>>(world) {
        let was_on = world.get::<BotEmped>(mob).is_some_and(|b| b.was_on);
        world.entity_mut(mob).remove::<(Emped, BotEmped)>();
        if was_on {
            crate::bots::turn_on(world, mob);
        }
    }
}

/// When a synthetic organ's `severe_cooldown` ends.
#[derive(Component, Clone, Copy, Debug)]
pub struct OrganEmpCooldown(pub f64);

/// A cybernetic heart an EMP stopped, beating again at the time.
#[derive(Component, Clone, Copy, Debug)]
pub struct HeartRestart(pub f64);

fn vulnerability(path: &str) -> f64 {
    ORGAN_VULNERABILITY
        .iter()
        .find(|(t, _)| path.contains(t))
        .map_or(BASE_ORGAN_VULNERABILITY, |&(_, v)| v)
}

/// The synthetic organs' `emp_act`: cybernetic hearts, lungs, livers and stomachs (a severe
/// effect at most once per cooldown, `ORGAN_EMP` failure at `emp_vulnerability / severity`
/// percent, a heart stopping for `10 s / severity`), cybernetic ears (organ damage), robotic eyes
/// (static) and cybernetic brains (damage, stagger, paralysis).
// tg starts the severe effects only while the cooldown runs (`!COOLDOWN_FINISHED`), so they never
// happen; the intent ("so we cant just spam emp") is once per cooldown, ported so. No dizziness
// or vomiting (not ported).
fn organs(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    for ev in emp_acts(world, &mut cursor) {
        let organ = ev.target;
        let (Some(state), Some(owner)) = (
            world.get::<Organ>(organ).cloned(),
            world.get::<OrganOf>(organ).map(|o| o.0),
        ) else {
            continue;
        };
        let Some(path) = path_of(world, organ) else {
            continue;
        };
        if !ev.hits_self() || !state.robotic && !is_under(&path, "/obj/item/organ/eyes/robotic") {
            continue;
        }
        let sev = f64::from(ev.severity);
        let cooled = world
            .get::<OrganEmpCooldown>(organ)
            .is_none_or(|c| now >= c.0);
        let start_cooldown = |world: &mut World, secs: f64| {
            world.entity_mut(organ).insert(OrganEmpCooldown(now + secs));
        };
        let permanent = |world: &mut World| {
            let chance = vulnerability(&path) / sev;
            world.resource_mut::<EmpRng>().prob(chance)
        };
        match state.slot {
            Slot::Heart => {
                if cooled {
                    if let Some(mut r) = world.get_mut::<crate::breath::Respiration>(owner) {
                        r.losebreath += 10.0;
                    }
                    start_cooldown(world, HEART_COOLDOWN);
                }
                if permanent(world) {
                    if let Some(mut o) = world.get_mut::<Organ>(organ) {
                        o.emped = true;
                        o.beating = false;
                    }
                    world
                        .entity_mut(organ)
                        .insert(HeartRestart(now + HEART_RESTART_SECS / sev));
                    let name = crate::security::name_of(world, owner);
                    if let Some(&pos) = world.get::<TilePos>(owner) {
                        crate::security::show(
                            world,
                            pos,
                            crate::health::DEFAULT_MESSAGE_RANGE,
                            ss13_shared::chat::ChatChannel::Local,
                            (
                                Span::Danger,
                                format!("{name} хватается за грудь, словно у него останавливается сердце!"),
                            ),
                            &[(
                                owner,
                                Span::BoldWarning,
                                "Вы чувствуете ужасную боль в груди, словно ваше сердце остановилось!"
                                    .to_owned(),
                            )],
                        );
                    }
                }
            }
            Slot::Lungs | Slot::Liver | Slot::Stomach => {
                if cooled {
                    match state.slot {
                        Slot::Lungs => {
                            if let Some(mut r) = world.get_mut::<crate::breath::Respiration>(owner)
                            {
                                r.losebreath += (20.0 / sev) as f32;
                            }
                            start_cooldown(world, LUNGS_COOLDOWN);
                        }
                        Slot::Liver => {
                            crate::medical::hurt_limb(
                                world,
                                owner,
                                crate::zone::BodyZone::Chest,
                                DamageType::Tox,
                                (10.0 / sev) as f32,
                            );
                            start_cooldown(world, LIVER_COOLDOWN);
                        }
                        _ => start_cooldown(world, STOMACH_COOLDOWN),
                    }
                }
                if permanent(world)
                    && let Some(mut o) = world.get_mut::<Organ>(organ)
                {
                    o.emped = true;
                }
            }
            Slot::Ears => {
                crate::medical::organs::damage_organ(
                    world,
                    organ,
                    (20.0 / sev) as f32,
                    state.spec().max_health,
                );
            }
            Slot::Eyes => {
                if world.resource_mut::<EmpRng>().prob(100.0 / sev) {
                    continue;
                }
                tell_mob(
                    world,
                    owner,
                    Span::Warning,
                    "Static obfuscates your vision!",
                );
                crate::vision::flash_act(world, owner, 1, false, true, crate::vision::FLASH_LENGTH);
            }
            Slot::Brain => {
                let (amount, cap, stagger) = match ev.severity {
                    EMP_HEAVY => (20.0, crate::medical::organs::BRAIN_DAMAGE_SEVERE, 0.5),
                    EMP_LIGHT => (10.0, crate::medical::organs::BRAIN_DAMAGE_MILD, 0.2),
                    _ => continue,
                };
                crate::medical::organs::damage_organ(world, organ, amount, cap);
                // BandaStation's robobrain rebalance: paralysis and stagger by the damage.
                let damage = world.get::<Organ>(organ).map_or(0.0, |o| o.damage);
                if ev.severity == EMP_HEAVY {
                    crate::status_effects::apply(world, owner, Incap::Paralyze, 0.25 * damage);
                }
                let dead = world.get::<Stat>(owner) == Some(&Stat::Dead);
                if let Some(mut fx) = crate::status_effects::effects_mut(world, owner) {
                    fx.adjust_staggered_up_to(stagger * damage, 20.0, dead);
                }
            }
            _ => {}
        }
    }
}

/// `Restart()` of a cybernetic heart an EMP stopped.
fn restart_hearts(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &HeartRestart)>();
    let due: Vec<Entity> = query
        .iter(world)
        .filter(|(_, r)| now >= r.0)
        .map(|(e, _)| e)
        .collect();
    for heart in due {
        world.entity_mut(heart).remove::<HeartRestart>();
        if let Some(mut o) = world.get_mut::<Organ>(heart) {
            o.beating = true;
        }
    }
}

/// On a radio an EMP switched off.
#[derive(Component, Clone, Copy, Debug)]
pub struct RadioEmped;

/// `/obj/item/radio/emp_act`: every channel off and the radio off ("overloads" if carried and
/// listening), on again 20 s after the last pulse.
fn radios(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        if !ev.hits_self() {
            continue;
        }
        let Some(listening) = world.get::<Radio>(ev.target).map(|r| r.listening && r.on) else {
            continue;
        };
        if listening && let Some(holder) = world.get::<CarriedBy>(ev.target).map(|c| c.0) {
            let name = crate::security::name_of(world, ev.target);
            tell_mob(
                world,
                holder,
                Span::Warning,
                &format!("{name} перегружается."),
            );
        }
        if let Some(mut radio) = world.get_mut::<Radio>(ev.target) {
            for channel in &mut radio.channels {
                channel.1 = false;
            }
            radio.on = false;
        }
        // `curremp`: only the last pulse's timer turns it back on.
        let until = world.resource::<Time>().elapsed_secs_f64() + RADIO_EMP_SECS;
        world
            .entity_mut(ev.target)
            .insert((RadioEmped, Emped { until }));
    }
}

fn restore_radios(world: &mut World) {
    for radio in emped_expired::<With<RadioEmped>>(world) {
        world.entity_mut(radio).remove::<(Emped, RadioEmped)>();
        if let Some(mut r) = world.get_mut::<Radio>(radio) {
            r.on = true;
        }
    }
}

/// `/obj/item/gun/emp_act` (the cell in its contents drains) and
/// `/obj/item/gun/energy/emp_act`: then `charge / emp_resistance / severity` more goes.
// shortcut: the drain treats every gun's cell as a plain cell (`emp_damage_modifier` 1).
fn guns(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        if !ev.hits_contents() {
            continue;
        }
        let Some(dm) = world.get::<DmGun>(ev.target).cloned() else {
            continue;
        };
        if dm.cell_maxcharge <= 0.0 {
            continue;
        }
        let resistance = EMP_RESISTANT_GUNS
            .iter()
            .find(|(t, _)| is_under(&dm.path, t))
            .map_or(1.0, |&(_, r)| r);
        let Some(mut gun) = world.get_mut::<Gun>(ev.target) else {
            continue;
        };
        gun.charge = (gun.charge - cell_drain(ev.severity, 1.0)).max(0.0);
        let more = (gun.charge / resistance / f64::from(ev.severity)).round();
        gun.charge = (gun.charge - more).max(0.0);
    }
}

/// `/obj/item/melee/baton/security/emp_act`: its cell drains; still charged enough for a hit,
/// it flips on and off `rand(6, 12) + 1` times.
// shortcut: the flips happen at once (tg spreads them over a few seconds with sparks).
fn batons(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        let (Some(dm), Some(charge)) = (
            world.get::<DmBaton>(ev.target).cloned(),
            world.get::<Baton>(ev.target).and_then(|b| b.charge),
        ) else {
            continue;
        };
        let mut charge = charge;
        if ev.hits_self() {
            charge = (charge - cell_drain(ev.severity, modifier_by_capacity(dm.cell_maxcharge)))
                .max(0.0);
        }
        let flips = if charge >= dm.cell_hit_cost {
            1 + world.resource_mut::<EmpRng>().range(6, 12)
        } else {
            0
        };
        if let Some(mut b) = world.get_mut::<Baton>(ev.target) {
            b.charge = Some(charge);
            if flips % 2 == 1 {
                b.active = !b.active;
            }
        }
        crate::security::refresh_baton(world, ev.target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn organ_vulnerability_by_tier() {
        assert_eq!(vulnerability("/obj/item/organ/heart/cybernetic"), 80.0);
        assert_eq!(
            vulnerability("/obj/item/organ/heart/cybernetic/tier2"),
            40.0
        );
        assert_eq!(
            vulnerability("/obj/item/organ/lungs/cybernetic/tier3"),
            20.0
        );
        assert_eq!(
            vulnerability("/obj/item/organ/liver/cybernetic/surplus"),
            100.0
        );
    }

    #[test]
    fn cell_modifier_by_capacity() {
        assert_eq!(modifier_by_capacity(STANDARD_CELL_CHARGE), 1.0);
        assert_eq!(modifier_by_capacity(10.0 * STANDARD_CELL_CHARGE), 3.0);
        assert_eq!(modifier_by_capacity(40.0 * STANDARD_CELL_CHARGE), 5.0);
    }
}

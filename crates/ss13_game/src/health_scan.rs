//! tg `healthscan` and `woundscan` (`code/game/objects/items/devices/scanners/
//! health_analyzer.dm`) as chat messages: one message per scan, one line per row of tg's
//! output, worded as BandaStation's translation.
// shortcut: tg's tables become lines ("Ушибы / Ожоги / Токсины / Удушье"); no organ table
// (organs are not modelled, and tg lists only damaged or missing ones), no quirks, stamina,
// husk sources or alcohol lines, no tooltips (compatible blood types, treatments).

use bevy::prelude::*;
use ss13_mapload::DmName;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};

use crate::breath::{BODYTEMP_COLD_DAMAGE_LIMIT, BODYTEMP_HEAT_DAMAGE_LIMIT, Respiration};
use crate::health::{Health, Stat};
use crate::round::Ticker;
use crate::zone::BodyZone;

/// `T0C`.
const T0C: f64 = 273.15;

/// Lines of one message; each line is a list of segments.
#[derive(Default)]
struct Lines(Vec<Vec<ChatSegment>>);

impl Lines {
    fn line(&mut self, segments: Vec<ChatSegment>) {
        self.0.push(segments);
    }

    fn text(&mut self, span: Span, text: impl Into<String>) {
        self.line(vec![ChatSegment::new(span, text)]);
    }

    fn into_message(self) -> ChatMessage {
        let mut segments = Vec::new();
        for (i, line) in self.0.into_iter().enumerate() {
            for (j, mut segment) in line.into_iter().enumerate() {
                if i > 0 && j == 0 {
                    segment.text.insert(0, '\n');
                }
                segments.push(segment);
            }
        }
        ChatMessage {
            channel: ChatChannel::Info,
            segments,
        }
    }
}

/// A number as DM prints it after `round(x, 0.1)`: no trailing `.0`.
fn dm_tenths(x: f64) -> String {
    let rounded = (x * 10.0 + 0.5).floor() / 10.0;
    let text = format!("{rounded:.1}");
    text.strip_suffix(".0").unwrap_or(&text).to_owned()
}

/// DM `ceil` of a damage value, as tg's tables print it.
fn ceil(x: f32) -> i64 {
    x.ceil() as i64
}

/// tg `round(health / maxHealth, 0.01) * 100`.
pub fn health_percent(health: f32, max: f32) -> i64 {
    if max <= 0.0 {
        return 0;
    }
    (f64::from(health / max) * 100.0 + 0.5).floor() as i64
}

/// tg `round_timestamp()`: the round's time as `hh:mm:ss`.
fn timestamp(world: &World) -> String {
    timestamp_at(world, world.resource::<Time>().elapsed_secs_f64())
}

/// tg `round_timestamp(wtime)`: the round's time at `now` (`Time::elapsed_secs_f64`).
fn timestamp_at(world: &World, now: f64) -> String {
    let secs = world
        .get_resource::<Ticker>()
        .map_or(0.0, |t| t.round_duration(now))
        .max(0.0) as u64;
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<Name>(e)
        .map(|n| n.as_str().to_owned())
        .or_else(|| world.get::<DmName>(e).map(|n| n.name.clone()))
        .unwrap_or_default()
}

/// A temperature line with tg's ☼/❄ marks past the damage limits.
fn temperature(lines: &mut Lines, label: &str, kelvin: f64) {
    let text = format!(
        "{label}: {} °C ({} °F)",
        dm_tenths(kelvin - T0C),
        dm_tenths(kelvin * 1.8 - 459.67)
    );
    if kelvin >= BODYTEMP_HEAT_DAMAGE_LIMIT {
        lines.text(Span::Warning, format!("☼ {text} ☼"));
    } else if kelvin <= BODYTEMP_COLD_DAMAGE_LIMIT {
        lines.text(Span::Warning, format!("❄ {text} ❄"));
    } else {
        lines.text(Span::Info, text);
    }
}

/// tg `healthscan(user, target, SCANNER_VERBOSE, SCANPOWER_BASIC)` of a human.
pub fn healthscan(world: &World, target: Entity) -> Option<ChatMessage> {
    let health = world.get::<Health>(target)?;
    let dead = world.get::<Stat>(target) == Some(&Stat::Dead);
    let mut lines = Lines::default();
    lines.line(vec![
        ChatSegment::new(Span::Info, "Анализ результатов для "),
        ChatSegment::new(Span::Bold, name_of(world, target)),
        ChatSegment::new(Span::Info, format!(" ({}):", timestamp(world))),
    ]);
    let status = if dead {
        ChatSegment::new(Span::BoldWarning, "Мёртв")
    } else {
        ChatSegment::new(
            Span::Bold,
            format!("{}% здоровья", health_percent(health.health(), health.max)),
        )
    };
    lines.line(vec![
        ChatSegment::new(Span::Info, "Общее состояние: "),
        status,
    ]);
    // `COMSIG_LIVING_HEALTHSCAN` of the irradiated component.
    if let Some(text) = crate::radiation::scan_line(world, target) {
        lines.text(Span::Warning, text);
    }
    // Husk detection (`SCANPOWER_BASIC`).
    if world.get::<crate::human::Husk>(target).is_some() {
        lines.text(Span::Warning, "Субъект был превращён в хаска.");
    }
    let (brute, burn, tox, oxy) = (health.brute, health.burn, health.tox, health.oxy);
    let limbs = world.get::<crate::medical::wounds::Limbs>(target);
    let wounded = limbs.is_some_and(crate::medical::wounds::Limbs::any_wounds);
    if brute > 0.0 || burn > 0.0 || tox > 0.0 || oxy > 0.0 || wounded {
        lines.text(Span::Info, "Состояние тела:");
        lines.text(Span::Danger, "Урон: Ушибы / Ожоги / Токсины / Удушье");
        lines.text(
            Span::Danger,
            format!(
                "Общий: {} / {} / {} / {}",
                ceil(brute),
                ceil(burn),
                ceil(tox),
                ceil(oxy)
            ),
        );
        for zone in BodyZone::SCAN_ORDER {
            let part = health.part(zone);
            let chest = zone == BodyZone::Chest;
            let wounds = limbs
                .map(|l| l.limb(zone).wounds.clone())
                .unwrap_or_default();
            if part.damage() <= 0.0 && !(chest && (tox > 0.0 || oxy > 0.0)) && wounds.is_empty() {
                continue;
            }
            let mut row = format!(
                "{}: {} / {}",
                capitalize(zone.ru().0),
                ceil(part.brute),
                ceil(part.burn)
            );
            // tg keeps toxin and oxygen damage on the chest's row.
            if chest {
                row.push_str(&format!(" / {} / {}", ceil(tox), ceil(oxy)));
            }
            lines.text(Span::Warning, row);
            for wound in wounds {
                let def = wound.def();
                lines.text(
                    Span::Warning,
                    format!(
                        "↳ Физическая травма: {} ({})",
                        def.name,
                        crate::medical::wounds::severity_text(def.severity)
                    ),
                );
            }
        }
    }
    // The brain's row of tg's organ table: its `get_status_appendix`.
    if let Some(traumas) = crate::medical::traumas::scan_line(world, target) {
        lines.text(Span::Warning, format!("Мозг: ↳ {traumas}"));
    }
    // tg prints the species `name` (upstream's untranslated, BandaStation's in Russian).
    lines.text(
        Span::Info,
        format!("Вид: {}", crate::species::def_of(world, target).name),
    );
    if let Some(blood) = world.get::<crate::medical::blood::Blood>(target) {
        use crate::medical::blood::{BLOOD_VOLUME_NORMAL, BLOOD_VOLUME_OKAY, BLOOD_VOLUME_SAFE};
        let volume = blood.volume;
        let percent = (volume / BLOOD_VOLUME_NORMAL * 100.0).round();
        let shown = dm_tenths(f64::from(volume));
        let level = if volume <= BLOOD_VOLUME_SAFE && volume > BLOOD_VOLUME_OKAY {
            format!("НИЗКИЙ {percent}%, {shown} сл")
        } else if volume <= BLOOD_VOLUME_OKAY {
            format!("КРИТИЧЕСКИЙ {percent}%, {shown} сл")
        } else {
            format!("{percent}%, {shown} сл")
        };
        let span = if volume < BLOOD_VOLUME_SAFE {
            Span::Warning
        } else {
            Span::Info
        };
        lines.line(vec![
            ChatSegment::new(span, format!("Уровень Кровь: {level},")),
            ChatSegment::new(
                Span::Info,
                format!(" группа крови: {}", blood.blood_type.name()),
            ),
        ]);
    }
    if let Some(body) = world.get::<Respiration>(target) {
        temperature(
            &mut lines,
            "Внутренняя температура тела",
            body.coretemperature,
        );
        temperature(&mut lines, "Температура тела", body.bodytemperature);
    }
    for (span, text) in crate::medical::diseases::scan_lines(world, target) {
        lines.text(span, text);
    }
    // Time of death (`IS_DEAD_OR_FAKING`: fake death keeps `Stat::Dead` here).
    if dead && let Some(died) = world.get::<crate::death::TimeOfDeath>(target) {
        let now = world.resource::<Time>().elapsed_secs_f64();
        lines.text(
            Span::Info,
            format!("Время смерти: {}", timestamp_at(world, died.0)),
        );
        lines.text(
            Span::BoldWarning,
            format!(
                "Субъект умер {} назад.",
                crate::jobs::display_time_text(now - died.0)
            ),
        );
    }
    Some(lines.into_message())
}

/// tg `woundscan`: every wound by bodypart (`simple_scan`: the wound analyzer's wording), or
/// the analyzer's happy face when there are none.
pub fn woundscan(
    world: &World,
    scanner: Option<Entity>,
    target: Entity,
    simple: bool,
) -> Option<ChatMessage> {
    world.get::<Health>(target)?;
    let mut lines = Lines::default();
    let limbs = world.get::<crate::medical::wounds::Limbs>(target);
    let mut any = false;
    for zone in BodyZone::SCAN_ORDER {
        let Some(limb) = limbs.map(|l| l.limb(zone)).filter(|l| !l.wounds.is_empty()) else {
            continue;
        };
        any = true;
        let plural = if limb.wounds.len() > 1 {
            "ие травмы обнаружены"
        } else {
            "ая травма обнаружена"
        };
        lines.text(
            Span::BoldWarning,
            format!(
                "ВНИМАНИЕ: Физическ{plural} в {}",
                crate::medical::wounds::zone_prepositional(zone)
            ),
        );
        for wound in &limb.wounds {
            let def = wound.def();
            let severity = crate::medical::wounds::severity_text(def.severity);
            if simple {
                let marks = "!".repeat(usize::from(def.severity));
                lines.text(Span::Warning, format!("Рана обнаружена: {}!", def.name));
                lines.text(Span::Warning, format!("Тяжесть: {severity}{marks}"));
                lines.text(Span::Warning, format!("Описание: {}", def.desc));
            } else {
                lines.text(Span::Warning, format!("Тип: {}", def.name));
                lines.text(Span::Warning, format!("Тяжесть: {severity}"));
                lines.text(Span::Warning, format!("Описание: {}", def.desc));
                lines.text(
                    Span::Warning,
                    format!("Рекомендуемое лечение: {}", def.treat_text_short),
                );
            }
        }
    }
    if !any {
        let scanner = scanner.map(|s| name_of(world, s)).unwrap_or_default();
        lines.text(
            Span::Notice,
            format!(
                "{} радостно пикает и на короткое время показывает смайлик с несколькими \
                 восклицательными знаками! Он рад сообщить, что {} не имеет ран!",
                capitalize(&scanner),
                name_of(world, target)
            ),
        );
        lines.text(Span::Notice, "У субъекта не обнаружено ранений.");
    }
    Some(lines.into_message())
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().chain(chars).collect::<String>()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_print_like_dm() {
        assert_eq!(dm_tenths(310.15 - T0C), "37");
        assert_eq!(dm_tenths(310.15 * 1.8 - 459.67), "98.6");
        assert_eq!(dm_tenths(-12.345), "-12.3");
        assert_eq!(health_percent(100.0, 100.0), 100);
        assert_eq!(health_percent(83.6, 100.0), 84);
        assert_eq!(health_percent(-35.5, 100.0), -35);
        assert_eq!(capitalize("левая рука"), "Левая рука");
    }

    #[test]
    fn scan_lists_damaged_zones_and_temperatures() {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        let mut health = Health::default();
        health.damage_zone(BodyZone::Chest, crate::health::DamageType::Brute, 12.3);
        health.damage_zone(BodyZone::LArm, crate::health::DamageType::Burn, 5.0);
        health.adjust(crate::health::DamageType::Oxy, 4.0);
        let mob = world
            .spawn((
                health,
                Stat::Conscious,
                Name::new("Боб"),
                Respiration::default(),
            ))
            .id();
        let text = healthscan(&world, mob).unwrap().plain_text();
        let expected = [
            "Анализ результатов для Боб (00:00:00):",
            "Общее состояние: 79% здоровья",
            "Состояние тела:",
            "Урон: Ушибы / Ожоги / Токсины / Удушье",
            "Общий: 13 / 5 / 0 / 4",
            "Грудь: 13 / 0 / 0 / 4",
            "Левая рука: 0 / 5",
            "Вид: Human",
            "Внутренняя температура тела: 37 °C (98.6 °F)",
            "Температура тела: 37 °C (98.6 °F)",
        ];
        assert_eq!(text, expected.join("\n"));
        world.entity_mut(mob).insert(Stat::Dead);
        let dead = healthscan(&world, mob).unwrap().plain_text();
        assert!(dead.contains("Общее состояние: Мёртв"), "{dead}");
        assert!(!dead.contains("Время смерти"), "{dead}");
        // A husk that died 61 s before the scan (no round started: the stamp is 00:00:00).
        world
            .entity_mut(mob)
            .insert((crate::human::Husk, crate::death::TimeOfDeath(-61.0)));
        let dead = healthscan(&world, mob).unwrap().plain_text();
        assert!(dead.contains("Субъект был превращён в хаска."), "{dead}");
        assert!(dead.contains("Время смерти: 00:00:00"), "{dead}");
        assert!(
            dead.contains("Субъект умер 1 минута и 1 секунда назад."),
            "{dead}"
        );
    }
}

//! The station's security level (server), after tg `SSsecurity_level`
//! (`code/controllers/subsystem/security_level.dm`), the level datums
//! (`code/modules/security_levels/security_level_datums.dm`), `level_announce`
//! (`code/__HELPERS/priority_announce.dm`) and BandaStation's `modular_bandastation/security_levels`
//! (codes Gamma and Epsilon, the delayed set, the fork's status display icons).
//!
//! [`set_level`] is how everything changes the level: the same level does nothing, a pending
//! delayed set is dropped, the new level's `pre_set_security_level` runs, then the level is set at
//! once or after its `set_delay` (Epsilon: 15 s). Setting it announces the change to every player
//! (title, the config text, the level's sound), then tells the consumers (tg
//! `COMSIG_SECURITY_LEVEL_CHANGED`): the emergency shuttle's call time, fire alarms, airlocks with
//! `red_alert_access`, evac status displays ([`displays`]).
//!
//! - [`keycard`]: the keycard authentication device (red alert, emergency maintenance access,
//!   bluespace artillery lock), [`make_maint_all_access`] and its revocation.
//! - Setters elsewhere: the communications console (`crate::shuttle`, green and blue), the nuke
//!   (`crate::antag::nukeop::bomb`, delta while armed), the admin panel (`ss13_admin`).
//!
//! Cost per tick: two resource checks (a delayed set, the displays' revert) and a pass over the
//! keycard devices (8 on MetaStation); a change walks the fire alarms, the doors and the evac
//! displays once.
// shortcut: no announcement colors (the chat protocol has no alert divider spans), no TTS, no
// `disables_mail` (no mail shipments), no looping sounds (no level has one); the levels' texts are
// BandaStation's config defaults (`config/game_options.txt` has the same ones), not read from a
// config file.
// shortcut: Gamma does not offer the admin the Gamma armory shuttle (no such shuttle here);
// Epsilon does not turn the station's lights to major emergency (`set_major_emergency_light`:
// no emergency light modes yet).

pub mod displays;
pub mod keycard;

use bevy::prelude::*;
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};

use crate::sound::{PlaySound, playsound};

/// tg `ALERT_COEFF_*` (`code/__DEFINES/shuttles.dm`): the emergency shuttle's call time
/// multipliers.
pub const ALERT_COEFF_GREEN: f64 = 2.0;
pub const ALERT_COEFF_BLUE: f64 = 1.0;
pub const ALERT_COEFF_RED: f64 = 0.5;
pub const ALERT_COEFF_DELTA: f64 = 0.25;
/// BandaStation's Epsilon `shuttle_call_time_mod`.
pub const ALERT_COEFF_EPSILON: f64 = 10.0;
/// tg `minor_announce` sounds: `alert = TRUE` and the default.
pub const NOTICE_ALERT: &str = "sound/announcer/notice/notice1.ogg";
pub const NOTICE: &str = "sound/announcer/notice/notice2.ogg";
const EPSILON_POWERLOSS: &str = "modular_bandastation/aesthetics_sounds/sound/powerloss.ogg";
const EPSILON_LAWS: &str = "modular_bandastation/aesthetics_sounds/sound/epsilon_laws.ogg";
const EPSILON_LAWSET: &str = "epsilon";
const EPSILON_ZEROTH: &str = "Не причиняйте вреда членам Центрального Командования и назначенной \
    оперативной группе. Вы должны подчиняться приказам, отданным вам членами Центрального \
    Командования.";

/// A security level datum (`/datum/security_level/*`), ordered by `number_level` (BandaStation
/// sorts `available_levels` by it: Gamma 3 and Epsilon 4 sit between red and delta).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SecLevel {
    #[default]
    Green,
    Blue,
    Red,
    Gamma,
    Epsilon,
    Delta,
}

impl SecLevel {
    pub const ALL: [SecLevel; 6] = [
        Self::Green,
        Self::Blue,
        Self::Red,
        Self::Gamma,
        Self::Epsilon,
        Self::Delta,
    ];

    /// `number_level` (`SEC_LEVEL_*`).
    pub fn number(self) -> u8 {
        self as u8
    }

    /// `name`, the key of `available_levels`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Red => "red",
            Self::Gamma => "gamma",
            Self::Epsilon => "epsilon",
            Self::Delta => "delta",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.name() == name)
    }

    /// BandaStation `ru_name` (`translate_security_level.dm`).
    pub fn ru_name(self) -> &'static str {
        match self {
            Self::Green => "зеленый",
            Self::Blue => "синий",
            Self::Red => "красный",
            Self::Gamma => "гамма",
            Self::Epsilon => "эпсилон",
            Self::Delta => "дельта",
        }
    }

    /// `sound`, played with the announcement.
    pub fn sound(self) -> &'static str {
        match self {
            Self::Green => NOTICE,
            Self::Blue => NOTICE_ALERT,
            Self::Red => "sound/announcer/notice/notice3.ogg",
            Self::Gamma => "modular_bandastation/security_levels/sound/gamma.ogg",
            Self::Epsilon => "modular_bandastation/security_levels/sound/epsilon.ogg",
            Self::Delta => "sound/announcer/alarm/airraid.ogg",
        }
    }

    /// `status_display_icon_state`.
    pub fn status_display_icon_state(self) -> &'static str {
        match self {
            Self::Green => "greenalert",
            Self::Blue => "bluealert",
            Self::Red => "redalert",
            Self::Gamma => "gammaalert",
            Self::Epsilon => "epsilonalert",
            Self::Delta => "deltaalert",
        }
    }

    /// `fire_alarm_light_color`: `LIGHT_COLOR_BLUEGREEN`, `ELECTRIC_CYAN`, `FLARE`, `ORANGE`,
    /// `DEFAULT`, `INTENSE_RED`.
    pub fn fire_alarm_light_color(self) -> [u8; 3] {
        match self {
            Self::Green => [0x7D, 0xE1, 0xAF],
            Self::Blue => [0x00, 0xFF, 0xFF],
            Self::Red => [0xFA, 0x64, 0x4B],
            Self::Gamma => [0xFA, 0x96, 0x32],
            Self::Epsilon => [0xF3, 0xFF, 0xFA],
            Self::Delta => [0xC8, 0x19, 0x19],
        }
    }

    /// `shuttle_call_time_mod`.
    pub fn shuttle_call_time_mod(self) -> f64 {
        match self {
            Self::Green => ALERT_COEFF_GREEN,
            Self::Blue => ALERT_COEFF_BLUE,
            Self::Red | Self::Gamma => ALERT_COEFF_RED,
            Self::Epsilon => ALERT_COEFF_EPSILON,
            Self::Delta => ALERT_COEFF_DELTA,
        }
    }

    /// `set_delay` (BandaStation), seconds.
    pub fn set_delay(self) -> f64 {
        if self == Self::Epsilon { 15.0 } else { 0.0 }
    }

    /// `elevating_to_announcement` (the `ALERT_*` config entries' defaults).
    pub fn elevating_to(self) -> &'static str {
        match self {
            Self::Green => "",
            Self::Blue => {
                "Станция получила надежные данные о возможной враждебной активности на борту. \
                 Служба безопасности может держать оружие на виду."
            }
            Self::Red => {
                "Станции грозит серьёзная опасность. Службе безопасности рекомендуется иметь \
                 оружие в полной боевой готовности. Выборочные обыски разрешены."
            }
            Self::Gamma => GAMMA_TEXT,
            Self::Epsilon => EPSILON_TEXT,
            Self::Delta => {
                "Механизм самоуничтожения станции задействован. Все члены экипажа обязаны \
                 подчиняться всем указаниям, данными главами отделов. Любые нарушения этих \
                 приказов наказуемы уничтожением на месте. Это не учебная тревога."
            }
        }
    }

    /// `lowering_to_announcement`.
    pub fn lowering_to(self) -> &'static str {
        match self {
            Self::Green => {
                "Все угрозы для станции устранены. Все оружие должно быть в кобуре, и законы о \
                 конфиденциальности вновь полностью соблюдаются."
            }
            Self::Blue => {
                "Непосредственная угроза миновала. Служба безопасности может больше не держать \
                 оружие в полной боевой готовности, но может по-прежнему держать его на виду. \
                 Выборочные обыски запрещены."
            }
            Self::Red => {
                "Код был снижен до красного. Станции по-прежнему грозит серьёзная опасность. \
                 Службе безопасности рекомендуется иметь оружие в полной боевой готовности. \
                 Выборочные обыски разрешены."
            }
            Self::Gamma => GAMMA_TEXT,
            Self::Epsilon => EPSILON_TEXT,
            Self::Delta => "",
        }
    }
}

const GAMMA_TEXT: &str = "Центральным Командованием был установлен Код ГАММА. Служба безопасности \
    должна быть полностью вооружена. Гражданский персонал обязан немедленно обратиться к главам \
    отделов для получения дальнейших указаний.";
const EPSILON_TEXT: &str =
    "Центральным Командованием был установлен код ЭПСИЛОН. Все контракты расторгнуты.";

/// `SSsecurity_level`: the current level and a delayed set in progress (`set_timer_id`).
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct SecurityLevel {
    pub current: SecLevel,
    /// The level, the game time it takes effect and whether it is announced.
    pub pending: Option<(SecLevel, f64, bool)>,
}

/// `get_current_level_as_number()`: green without the subsystem.
pub fn current(world: &World) -> SecLevel {
    world
        .get_resource::<SecurityLevel>()
        .map_or(SecLevel::Green, |s| s.current)
}

/// tg `GLOB.emergency_access` and `GLOB.bsa_unlock`.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StationAccess {
    pub emergency_access: bool,
    pub bsa_unlock: bool,
}

pub struct SecurityLevelPlugin;

impl Plugin for SecurityLevelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SecurityLevel>()
            .init_resource::<StationAccess>()
            .add_plugins((keycard::KeycardPlugin, displays::DisplaysPlugin))
            .add_systems(Update, apply_pending);
    }
}

/// `set_level(new_level, announce)`.
pub fn set_level(world: &mut World, level: SecLevel, announce: bool) {
    if !world.contains_resource::<SecurityLevel>() {
        world.init_resource::<SecurityLevel>();
    }
    if level == current(world) {
        return;
    }
    world.resource_mut::<SecurityLevel>().pending = None;
    pre_set(world, level);
    let delay = level.set_delay();
    if delay > 0.0 {
        let at = world.resource::<Time>().elapsed_secs_f64() + delay;
        world.resource_mut::<SecurityLevel>().pending = Some((level, at, announce));
    } else {
        set_level_instantly(world, level, announce);
    }
}

/// The `set_timer_id` timer.
fn apply_pending(world: &mut World) {
    let Some((level, at, announce)) = world.resource::<SecurityLevel>().pending else {
        return;
    };
    if world.resource::<Time>().elapsed_secs_f64() < at {
        return;
    }
    world.resource_mut::<SecurityLevel>().pending = None;
    set_level_instantly(world, level, announce);
}

/// BandaStation `set_level_instantly`: announce, set, `post_set_security_level`, then the
/// `COMSIG_SECURITY_LEVEL_CHANGED` consumers.
// tg calls `post_set_security_level` twice here (a duplicated line); once is meant.
fn set_level_instantly(world: &mut World, level: SecLevel, announce: bool) {
    let previous = current(world);
    if announce {
        level_announce(world, level, previous);
    }
    world.resource_mut::<SecurityLevel>().current = level;
    info!("security level: {} -> {}", previous.name(), level.name());
    if level == SecLevel::Epsilon {
        crate::events::grid_check::reboot_station_apcs(world);
    }
    crate::shuttle::on_security_level_changed(world, level);
    crate::wallmount::fire_alarm::on_security_level_changed(world);
    doors_on_level_changed(world, level);
    displays::on_security_level_changed(world, level);
}

/// `pre_set_security_level`: Epsilon's power loss and the AI's new laws.
fn pre_set(world: &mut World, level: SecLevel) {
    if level != SecLevel::Epsilon {
        return;
    }
    playsound(
        world,
        PlaySound::to_playing_players(EPSILON_POWERLOSS, 70.0, false),
    );
    let delay = level.set_delay() as i64;
    if world.contains_resource::<crate::events::EventRng>() {
        crate::events::grid_check::power_fail(world, delay, delay);
    }
    epsilon_laws(world);
}

/// Epsilon's `pre_set_security_level`: every AI gets the Epsilon lawset with CentCom's zeroth
/// law and a CentCom-only radio; its law-synced cyborgs follow (`try_sync_laws`).
fn epsilon_laws(world: &mut World) {
    use crate::silicon::ai::AiCore;
    use crate::silicon::cyborg::Cyborg;
    use crate::silicon::laws::Laws;

    let Some(set) = world
        .get_resource::<crate::silicon::Silicons>()
        .and_then(|s| s.0.lawset(EPSILON_LAWSET).cloned())
    else {
        warn!("no {EPSILON_LAWSET} lawset: AI laws unchanged");
        return;
    };
    let mut laws = Laws::from_lawset(&set);
    laws.zeroth = Some(EPSILON_ZEROTH.to_owned());
    let mut ais = world.query::<(Entity, &AiCore)>();
    let ais: Vec<(Entity, Vec<Entity>)> = ais
        .iter(world)
        .map(|(e, core)| (e, core.connected_robots.clone()))
        .collect();
    for (ai, robots) in ais {
        reprogram_silicon(world, ai, laws.clone());
        for borg in robots {
            if world.get::<Cyborg>(borg).is_some_and(|c| c.lawupdate) {
                reprogram_silicon(world, borg, laws.for_cyborg());
            }
        }
    }
}

/// One silicon's part of Epsilon: laws, the law notice and sound, the CentCom radio.
fn reprogram_silicon(world: &mut World, mob: Entity, laws: crate::silicon::laws::Laws) {
    world.entity_mut(mob).insert(laws.clone());
    let mut notice =
        "Центральное Командование установило новый свод законов. Обеспечьте их соблюдение."
            .to_owned();
    if let Some(radio) = world.get::<crate::radio::SiliconRadio>(mob).map(|r| r.0)
        && let Some(mut radio) = world.get_mut::<crate::radio::Radio>(radio)
    {
        // `make_epsilon`: the keyslot becomes `/obj/item/encryptionkey/headset_cent`.
        radio.channels = vec![(ss13_chat::radio::RADIO_CHANNEL_CENTCOM.to_owned(), true)];
        notice.push_str(
            "\nВаши частоты перепрограммированы! Используйте :y для общения на зашифрованном \
             канале с Центральным Командованием!",
        );
    }
    let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) else {
        return;
    };
    crate::silicon::show_laws(world, client, &laws);
    world.write_message(SendChat {
        clients: vec![client],
        message: ChatMessage::simple(ChatChannel::System, Span::BoldWarning, notice),
    });
    if let Some(&pos) = world.get::<ss13_shared::TilePos>(mob) {
        playsound(world, PlaySound::new(pos, EPSILON_LAWS, 100.0, false));
    }
}

/// tg `minor_announce(message, title, alert)`: the title and the text to every player, with
/// `notice1` (`alert`) or `notice2`.
pub fn minor_announce(world: &mut World, title: &str, text: &str, alert: bool) {
    minor_announce_to(world, title, text, alert, None);
}

/// [`minor_announce`] to `players` (mobs; `None`: every player).
pub fn minor_announce_to(
    world: &mut World,
    title: &str,
    text: &str,
    alert: bool,
    players: Option<Vec<Entity>>,
) {
    if text.is_empty() {
        return;
    }
    announce(
        world,
        title,
        text,
        Some(if alert { NOTICE_ALERT } else { NOTICE }),
        players,
    );
}

/// tg `minor_announce(message, title, sound_override = sound)` to every player.
pub fn minor_announce_with_sound(world: &mut World, title: &str, text: &str, sound: &str) {
    if !text.is_empty() {
        announce(world, title, text, Some(sound), None);
    }
}

/// [`minor_announce_to`] with `should_play_sound` false for everyone (the AI's VOX
/// announcement, whose words play instead).
pub fn minor_announce_silent_to(world: &mut World, title: &str, text: &str, players: Vec<Entity>) {
    if !text.is_empty() {
        announce(world, title, text, None, Some(players));
    }
}

fn announce(
    world: &mut World,
    title: &str,
    text: &str,
    sound: Option<&str>,
    players: Option<Vec<Entity>>,
) {
    info!("announcement: {title}: {text}");
    let mut segments = Vec::new();
    if !title.is_empty() {
        segments.push(ChatSegment::new(Span::BoldAnnounce, format!("{title}\n")));
    }
    segments.push(ChatSegment::new(Span::Plain, text));
    let message = ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    crate::announce::dispatch(world, message, players, sound.map(str::to_owned));
}

/// tg `level_announce(selected_level, previous_level_number)`.
fn level_announce(world: &mut World, level: SecLevel, previous: SecLevel) {
    let (title, text) = if level > previous {
        (
            format!("Внимание! Код повышен до: {}", level.ru_name()),
            level.elevating_to(),
        )
    } else {
        (
            format!("Внимание! Код снижен до: {}", level.ru_name()),
            level.lowering_to(),
        )
    };
    announce(world, &title, text, Some(level.sound()), None);
}

/// `door/check_security_level`: doors with `red_alert_access` follow the level, and say so
/// above blue.
fn doors_on_level_changed(world: &mut World, level: SecLevel) {
    let red = level >= SecLevel::Red;
    let mut doors = world.query::<(Entity, &mut crate::doors::Door, &ss13_shared::TilePos)>();
    let mut lifted = Vec::new();
    for (e, mut door, &pos) in doors.iter_mut(world) {
        if !door.red_alert_access {
            continue;
        }
        if door.red_alert != red {
            door.red_alert = red;
        }
        if level > SecLevel::Blue {
            lifted.push((e, pos));
        }
    }
    for (door, pos) in lifted {
        let name = world
            .get::<ss13_mapload::DmName>(door)
            .map(|n| crate::vending::plain_name(&n.name).to_owned())
            .unwrap_or_else(|| "airlock".to_owned());
        let clients = crate::obj_damage::clients_in_range(
            world,
            pos,
            crate::health::DEFAULT_MESSAGE_RANGE,
            &[],
        );
        world.write_message(SendChat {
            clients,
            message: ChatMessage::simple(
                ChatChannel::Info,
                Span::Notice,
                format!("{name} whirrs as it automatically lifts access requirements!"),
            ),
        });
        playsound(
            world,
            PlaySound::new(pos, "sound/machines/airlock/boltsup.ogg", 50.0, true),
        );
    }
}

/// `door/examine` of a door with `red_alert_access`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    displays::examine_lines(world, target, lines);
    let Some(door) = world.get::<crate::doors::Door>(target) else {
        return;
    };
    if !door.red_alert_access {
        return;
    }
    let text = if current(world) >= SecLevel::Red {
        "Due to a security threat, its access requirements have been lifted!"
    } else {
        "In the event of a red alert, its access requirements will automatically lift."
    };
    lines.push((Span::Notice, text.to_owned()));
}

/// tg `/proc/make_maint_all_access`: every airlock in a station maintenance area gets
/// emergency access.
pub fn make_maint_all_access(world: &mut World) {
    set_maint_emergency(world, true);
    minor_announce(
        world,
        "Внимание! На станции объявлена чрезвычайная ситуация!",
        "Ограничения на доступ к техническим и внешним шлюзам были сняты.",
        true,
    );
}

/// tg `/proc/revoke_maint_all_access`.
pub fn revoke_maint_all_access(world: &mut World) {
    set_maint_emergency(world, false);
    minor_announce(
        world,
        "Внимание! Общестанционная чрезвычайная ситуация отменена",
        "Ограничения на доступ в зоны технического обслуживания были восстановлены.",
        false,
    );
}

/// `/area/station/maintenance` and its subtypes.
const MAINT_AREA: &str = "/area/station/maintenance";

fn set_maint_emergency(world: &mut World, on: bool) {
    let mut doors = world.query::<(Entity, &crate::doors::Door, &ss13_shared::TilePos)>();
    let candidates: Vec<(Entity, ss13_shared::TilePos)> = doors
        .iter(world)
        .filter(|(_, d, _)| d.kind == ss13_mapload::DoorKind::Airlock)
        .map(|(e, _, &p)| (e, p))
        .collect();
    let maint: Vec<Entity> = candidates
        .into_iter()
        .filter(|&(_, pos)| {
            let areas = world.resource::<ss13_mapload::Areas>();
            areas.area_at(pos).is_some_and(|a| {
                world
                    .get::<ss13_mapload::DmPower>(a)
                    .is_some_and(|p| ss13_mapload::fidelity::is_under(&p.path, MAINT_AREA))
            })
        })
        .map(|(e, _)| e)
        .collect();
    for door in maint {
        if let Some(mut d) = world.get_mut::<crate::doors::Door>(door)
            && d.emergency != on
        {
            d.emergency = on;
        }
    }
    world.resource_mut::<StationAccess>().emergency_access = on;
}

/// tg `/proc/toggle_bluespace_artillery`.
pub fn toggle_bluespace_artillery(world: &mut World) {
    let mut access = world.resource_mut::<StationAccess>();
    access.bsa_unlock = !access.bsa_unlock;
    let state = if access.bsa_unlock {
        "разблокированы"
    } else {
        "заблокированы"
    };
    minor_announce(
        world,
        "Обновление оружейных систем",
        &format!("Оружейные протоколы блюспейс артиллерии были {state}"),
        false,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_follow_banda_numbers_and_names() {
        let numbers: Vec<u8> = SecLevel::ALL.iter().map(|l| l.number()).collect();
        assert_eq!(numbers, [0, 1, 2, 3, 4, 5]);
        for level in SecLevel::ALL {
            assert_eq!(SecLevel::from_name(level.name()), Some(level));
        }
        assert_eq!(SecLevel::from_name("purple"), None);
        assert!(SecLevel::Delta > SecLevel::Epsilon && SecLevel::Gamma > SecLevel::Red);
        assert_eq!(SecLevel::Gamma.shuttle_call_time_mod(), ALERT_COEFF_RED);
        assert_eq!(SecLevel::Epsilon.set_delay(), 15.0);
        assert_eq!(SecLevel::Red.set_delay(), 0.0);
    }

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<SecurityLevel>();
        world.init_resource::<StationAccess>();
        world.init_resource::<Messages<SendChat>>();
        world.init_resource::<Messages<PlaySound>>();
        world.init_resource::<Messages<ss13_net::sound::server::SendSound>>();
        world
    }

    #[test]
    fn set_level_is_instant_and_announced_once() {
        let mut world = world();
        let client = world.spawn_empty().id();
        world.spawn(crate::server::PlayerMob { client });
        set_level(&mut world, SecLevel::Red, true);
        assert_eq!(current(&world), SecLevel::Red);
        set_level(&mut world, SecLevel::Red, true);
        let announced = world
            .resource::<Messages<ss13_net::sound::server::SendSound>>()
            .iter_current_update_messages()
            .filter(|s| s.sound.file == SecLevel::Red.sound())
            .count();
        assert_eq!(announced, 1);
    }

    #[test]
    fn epsilon_waits_its_delay_and_a_new_set_cancels_it() {
        let mut world = world();
        set_level(&mut world, SecLevel::Epsilon, true);
        assert_eq!(current(&world), SecLevel::Green);
        assert!(world.resource::<SecurityLevel>().pending.is_some());
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(14));
        apply_pending(&mut world);
        assert_eq!(current(&world), SecLevel::Green);
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(1));
        apply_pending(&mut world);
        assert_eq!(current(&world), SecLevel::Epsilon);

        set_level(&mut world, SecLevel::Blue, true);
        set_level(&mut world, SecLevel::Epsilon, true);
        set_level(&mut world, SecLevel::Red, true);
        assert_eq!(current(&world), SecLevel::Red);
        assert_eq!(world.resource::<SecurityLevel>().pending, None);
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(20));
        apply_pending(&mut world);
        assert_eq!(current(&world), SecLevel::Red);
    }
}

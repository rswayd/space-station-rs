//! The `SimpleBot` window (server): tg `/mob/living/basic/bot/ui_data`, `ui_act` and the
//! kinds' custom controls (`secbot.dm`, `medbot.dm`, `cleanbot.dm`, `firebot.dm`).
//!
//! Every act needs `allowed(user)` (unlocked, or the bot's access on the user's ID, or a
//! silicon); the custom controls also need the controls unlocked unless a silicon acts; `hack`
//! is silicons' only.
// shortcut: no rename, pAI eject or personality acts (no pAIs or ghost possession here).

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_tgui::UiInterface;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::Bot;
use super::{
    BotKind, BotSettings, CLEANBOT_CLEAN_BLOOD, CLEANBOT_CLEAN_DRAWINGS, CLEANBOT_CLEAN_PESTS,
    CLEANBOT_CLEAN_TRASH, COVER_EMAGGED, COVER_HACKED, COVER_LOCKED, COVER_MAINTS_OPEN,
    FIREBOT_EXTINGUISH_FLAMES, FIREBOT_EXTINGUISH_PEOPLE, FIREBOT_STATIONARY_MODE, HEAL_THRESHOLD,
    MEDBOT_DECLARE_CRIT, MEDBOT_SPEAK_MODE, MEDBOT_STATIONARY_MODE, MODE_AUTOPATROL, MODE_ON,
    MODE_REMOTE_ENABLED, SECBOT_CHECK_IDS, SECBOT_CHECK_RECORDS, SECBOT_CHECK_WEAPONS,
    SECBOT_DECLARE_ARRESTS, SECBOT_HANDCUFF_TARGET, SimpleBotData, SimpleBotUi,
};
use crate::inventory::{IdCards, Inventory};
use crate::silicon::interact::SiliconUsers;

/// The server half of `SimpleBot`.
pub struct SimpleBotWindow;

impl UiInterface for SimpleBotWindow {
    const NAME: &'static str = SimpleBotUi::NAME;
    type Data = SimpleBotData;
    type StaticData = ();
}

#[derive(Clone, Debug, PartialEq)]
pub enum BotAct {
    Power,
    Maintenance,
    Patrol,
    Airplane,
    Lock,
    Hack,
    /// A custom toggle by its tg name.
    Toggle(String),
    /// The medbot's `heal_threshold` (`params["threshold"]`).
    Threshold(f64),
}

/// The toggles of a kind: name and flag field and bit.
fn toggles(kind: BotKind) -> &'static [(&'static str, Flags, u32)] {
    match kind {
        BotKind::Sec => &[
            ("check_id", Flags::Security, SECBOT_CHECK_IDS),
            ("check_weapons", Flags::Security, SECBOT_CHECK_WEAPONS),
            ("check_warrants", Flags::Security, SECBOT_CHECK_RECORDS),
            ("handcuff_targets", Flags::Security, SECBOT_HANDCUFF_TARGET),
            ("arrest_alert", Flags::Security, SECBOT_DECLARE_ARRESTS),
        ],
        BotKind::Med => &[
            ("speaker", Flags::Medical, MEDBOT_SPEAK_MODE),
            ("crit_alerts", Flags::Medical, MEDBOT_DECLARE_CRIT),
            ("stationary_mode", Flags::Medical, MEDBOT_STATIONARY_MODE),
        ],
        BotKind::Clean => &[
            ("clean_blood", Flags::Janitor, CLEANBOT_CLEAN_BLOOD),
            ("clean_trash", Flags::Janitor, CLEANBOT_CLEAN_TRASH),
            ("clean_graffiti", Flags::Janitor, CLEANBOT_CLEAN_DRAWINGS),
            ("pest_control", Flags::Janitor, CLEANBOT_CLEAN_PESTS),
        ],
        BotKind::Fire => &[
            (
                "extinguish_fires",
                Flags::Firebot,
                FIREBOT_EXTINGUISH_FLAMES,
            ),
            (
                "extinguish_people",
                Flags::Firebot,
                FIREBOT_EXTINGUISH_PEOPLE,
            ),
            ("stationary_mode", Flags::Firebot, FIREBOT_STATIONARY_MODE),
        ],
        BotKind::Other => &[],
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Flags {
    Security,
    Medical,
    Janitor,
    Firebot,
}

fn flags_mut(bot: &mut Bot, which: Flags) -> &mut u32 {
    match which {
        Flags::Security => &mut bot.security_flags,
        Flags::Medical => &mut bot.medical_flags,
        Flags::Janitor => &mut bot.janitor_flags,
        Flags::Firebot => &mut bot.firebot_flags,
    }
}

fn flags(bot: &Bot, which: Flags) -> u32 {
    match which {
        Flags::Security => bot.security_flags,
        Flags::Medical => bot.medical_flags,
        Flags::Janitor => bot.janitor_flags,
        Flags::Firebot => bot.firebot_flags,
    }
}

/// `ui_data` of `bot` for a user with `access` (`None`: a silicon).
pub fn data(bot: &Bot, access: Option<&[String]>) -> SimpleBotData {
    let silicon = access.is_none();
    let has_access = silicon
        || !bot.locked()
        || crate::inventory::check_access(&[], &bot.req_one_access, access.unwrap_or_default());
    let open = !bot.locked() || silicon;
    let mut custom_controls = Vec::new();
    if open {
        for &(name, which, bit) in toggles(bot.kind) {
            custom_controls.push((name.to_owned(), i32::from(flags(bot, which) & bit != 0)));
        }
        if bot.kind == BotKind::Med {
            custom_controls.insert(0, (HEAL_THRESHOLD.to_owned(), bot.heal_threshold as i32));
        }
    }
    SimpleBotData {
        can_hack: silicon,
        emagged: bot.emagged(),
        has_access,
        locked: bot.locked(),
        settings: open.then_some(BotSettings {
            airplane_mode: bot.mode_flags & MODE_REMOTE_ENABLED == 0,
            maintenance_lock: bot.cover & COVER_MAINTS_OPEN == 0,
            power: bot.mode_flags & MODE_ON != 0,
            patrol_station: bot.mode_flags & MODE_AUTOPATROL != 0,
        }),
        custom_controls,
    }
}

impl ServerUi for SimpleBotWindow {
    type Source = &'static Bot;
    type UserParam = (
        Query<'static, 'static, &'static Inventory>,
        IdCards<'static, 'static>,
        SiliconUsers<'static, 'static>,
    );
    type Act = BotAct;

    fn static_data(_: &Bot) {}

    fn data(
        bot: &Bot,
        user: Entity,
        (inventories, cards, silicons): &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> SimpleBotData {
        if silicons.contains(user) {
            return data(bot, None);
        }
        let access = inventories
            .get(user)
            .ok()
            .and_then(|i| i.access(cards))
            .unwrap_or_default();
        data(bot, Some(access))
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<BotAct, UiReject> {
        Ok(match action {
            "power" => BotAct::Power,
            "maintenance" => BotAct::Maintenance,
            "patrol" => BotAct::Patrol,
            "airplane" => BotAct::Airplane,
            "lock" => BotAct::Lock,
            "hack" => BotAct::Hack,
            HEAL_THRESHOLD => BotAct::Threshold(params.number("threshold")?),
            other
                if [BotKind::Sec, BotKind::Med, BotKind::Clean, BotKind::Fire]
                    .into_iter()
                    .any(|k| toggles(k).iter().any(|t| t.0 == other)) =>
            {
                BotAct::Toggle(other.to_owned())
            }
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

pub(super) fn build(app: &mut App) {
    app.register_ui::<SimpleBotWindow>()
        .add_systems(Update, bot_acts.in_set(UiSystems::Handle));
}

/// tg `attack_hand` / `attack_ai` -> `ui_interact`: an emagged bot's controls do not answer
/// (`topic_denied`) unless a silicon hacked it.
pub fn open(world: &mut World, client: Entity, actor: Entity, bot: Entity) {
    let Some(b) = world.get::<Bot>(bot) else {
        return;
    };
    let silicon = crate::silicon::interact::has_silicon_access(world, actor);
    if b.emagged() && !(b.cover & COVER_HACKED != 0 && silicon) {
        if let Some(c) = world
            .get::<crate::server::PlayerMob>(actor)
            .map(|p| p.client)
        {
            let name = super::bot_name(world, bot);
            crate::tools::tell(
                world,
                c,
                ss13_shared::chat::Span::Warning,
                format!("Интерфейс {name} не отвечает!"),
            );
        }
        return;
    }
    let title = super::bot_name(world, bot);
    world.write_message(OpenUi::new::<SimpleBotWindow>(client, actor, bot, title));
}

fn bot_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<SimpleBotWindow>>>) {
    let pending: Vec<(Entity, Entity, BotAct)> = acts
        .read(world.resource::<Messages<UiActed<SimpleBotWindow>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (user, bot, act) in pending {
        act_on(world, user, bot, act);
    }
}

/// tg `ui_act` of the bot and its kind, validated at the moment of effect.
pub fn act_on(world: &mut World, user: Entity, mob: Entity, act: BotAct) {
    if world.get::<Bot>(mob).is_none() {
        return;
    }
    if !super::allowed(world, mob, user) {
        if let Some(c) = world
            .get::<crate::server::PlayerMob>(user)
            .map(|p| p.client)
        {
            crate::tools::tell(
                world,
                c,
                ss13_shared::chat::Span::Warning,
                "В доступе отказано!",
            );
        }
        return;
    }
    let silicon = crate::silicon::interact::has_silicon_access(world, user);
    match act {
        BotAct::Power => {
            let on = world.get::<Bot>(mob).is_some_and(Bot::on);
            if on {
                super::turn_off(world, mob);
            } else {
                super::turn_on(world, mob);
            }
        }
        BotAct::Lock => world.get_mut::<Bot>(mob).expect("checked").cover ^= COVER_LOCKED,
        BotAct::Maintenance => {
            world.get_mut::<Bot>(mob).expect("checked").cover ^= COVER_MAINTS_OPEN;
        }
        BotAct::Patrol => {
            world.get_mut::<Bot>(mob).expect("checked").mode_flags ^= MODE_AUTOPATROL;
            super::bot_reset(world, mob);
        }
        BotAct::Airplane => {
            world.get_mut::<Bot>(mob).expect("checked").mode_flags ^= MODE_REMOTE_ENABLED;
        }
        BotAct::Hack => {
            if !silicon {
                return;
            }
            let cover = world.get::<Bot>(mob).expect("checked").cover;
            if cover & COVER_EMAGGED == 0 {
                world.get_mut::<Bot>(mob).expect("checked").cover |=
                    COVER_LOCKED | COVER_EMAGGED | COVER_HACKED;
                super::bot_reset(world, mob);
            } else if cover & COVER_HACKED != 0 {
                world.get_mut::<Bot>(mob).expect("checked").cover &=
                    !(COVER_EMAGGED | COVER_HACKED);
                super::bot_reset(world, mob);
            }
        }
        BotAct::Toggle(name) => {
            let mut bot = world.get_mut::<Bot>(mob).expect("checked");
            if bot.locked() && !silicon {
                return;
            }
            if let Some(&(_, which, bit)) = toggles(bot.kind).iter().find(|t| t.0 == name) {
                *flags_mut(&mut bot, which) ^= bit;
            }
            if bot.kind == BotKind::Fire {
                super::refresh_look(world, mob);
            }
        }
        BotAct::Threshold(value) => {
            let mut bot = world.get_mut::<Bot>(mob).expect("checked");
            if bot.kind != BotKind::Med || (bot.locked() && !silicon) {
                return;
            }
            // `round(text2num(params["threshold"]))` clamped to 5..75.
            bot.heal_threshold = value.round().clamp(5.0, 75.0) as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bots::{BOT_IDLE, Healer, MODE_ON};

    fn medbot() -> Bot {
        Bot {
            kind: BotKind::Med,
            mode_flags: MODE_ON,
            cover: COVER_LOCKED,
            mode: BOT_IDLE,
            req_one_access: vec!["medical".into()],
            initial_access: Vec::new(),
            access: Vec::new(),
            radio: None,
            base_icon: "medbot".into(),
            commissioned: false,
            security_flags: 0,
            medical_flags: MEDBOT_SPEAK_MODE,
            heal_amount: 2.5,
            heal_threshold: 10.0,
            healer: Healer::All,
            skin: "generic".into(),
            janitor_flags: 0,
            firebot_flags: 0,
            robot_arm: String::new(),
            stun_ready_at: 0.0,
            threat: 0,
            medkit: String::new(),
            task: None,
        }
    }

    #[test]
    fn locked_controls_hide_settings_from_humans() {
        let bot = medbot();
        let human = data(&bot, Some(&["medical".to_owned()]));
        assert!(human.locked && human.has_access);
        assert!(human.settings.is_none() && human.custom_controls.is_empty());
        let none: [String; 0] = [];
        assert!(!data_for(&bot, &none).has_access);
        let silicon = data(&bot, None);
        assert!(silicon.can_hack && silicon.settings.is_some());
        assert_eq!(silicon.custom_controls[0], (HEAL_THRESHOLD.to_owned(), 10));
        assert_eq!(silicon.custom_controls[1], ("speaker".to_owned(), 1));
    }

    fn data_for(bot: &Bot, access: &[String]) -> SimpleBotData {
        data(bot, Some(access))
    }

    #[test]
    fn acts_parse_only_known_names() {
        let params: Vec<(String, String)> = vec![("threshold".into(), "33".into())];
        let p = UiParams::new(&params);
        assert_eq!(
            SimpleBotWindow::parse_act("heal_threshold", &p),
            Ok(BotAct::Threshold(33.0))
        );
        assert_eq!(
            SimpleBotWindow::parse_act("check_id", &p),
            Ok(BotAct::Toggle("check_id".into()))
        );
        assert!(SimpleBotWindow::parse_act("rm -rf", &p).is_err());
    }
}

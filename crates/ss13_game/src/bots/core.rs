//! The bot mob itself (server); see the module docs of [`super`].

use super::{ai, construction, ui};
use std::collections::HashMap;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmName, DmNavBeacon};
use ss13_net::chat::server::SendChat;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};

use crate::click::{ClickResolved, ResolveClicks};
use crate::combat::CombatMode;
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType, Health, Stat};
use crate::inventory::{IdCard, Inventory};
use crate::mobs::BasicMob;
use crate::security::{name_of, show};
use crate::tools::{ToolKind, tell};

/// `BOT_MODE_*`.
pub const MODE_ON: u32 = 1 << 0;
pub const MODE_AUTOPATROL: u32 = 1 << 1;
pub const MODE_REMOTE_ENABLED: u32 = 1 << 2;
pub const MODE_CAN_BE_SAPIENT: u32 = 1 << 3;
/// `BOT_COVER_*`.
pub const COVER_MAINTS_OPEN: u32 = 1 << 0;
pub const COVER_LOCKED: u32 = 1 << 1;
pub const COVER_EMAGGED: u32 = 1 << 2;
pub const COVER_HACKED: u32 = 1 << 3;
/// `SECBOT_*`.
pub const SECBOT_DECLARE_ARRESTS: u32 = 1 << 0;
pub const SECBOT_CHECK_IDS: u32 = 1 << 1;
pub const SECBOT_CHECK_WEAPONS: u32 = 1 << 2;
pub const SECBOT_CHECK_RECORDS: u32 = 1 << 3;
pub const SECBOT_HANDCUFF_TARGET: u32 = 1 << 4;
/// `MEDBOT_*`.
pub const MEDBOT_DECLARE_CRIT: u32 = 1 << 0;
pub const MEDBOT_STATIONARY_MODE: u32 = 1 << 1;
pub const MEDBOT_SPEAK_MODE: u32 = 1 << 2;
/// `FIREBOT_*`.
pub const FIREBOT_STATIONARY_MODE: u32 = 1 << 0;
pub const FIREBOT_EXTINGUISH_PEOPLE: u32 = 1 << 1;
pub const FIREBOT_EXTINGUISH_FLAMES: u32 = 1 << 2;
/// `CLEANBOT_*`.
pub const CLEANBOT_CLEAN_BLOOD: u32 = 1 << 0;
pub const CLEANBOT_CLEAN_TRASH: u32 = 1 << 1;
pub const CLEANBOT_CLEAN_PESTS: u32 = 1 << 2;
pub const CLEANBOT_CLEAN_DRAWINGS: u32 = 1 << 3;

/// tg bot modes (`mode`), as the window and examine show them.
pub const BOT_IDLE: &str = "Idle";
pub const BOT_HUNT: &str = "In Pursuit";
pub const BOT_PATROL: &str = "Patrolling";
pub const BOT_SUMMON: &str = "Summoned by PDA";
pub const BOT_PREP_ARREST: &str = "Preparing to Arrest";
pub const BOT_ARREST: &str = "Arresting";
pub const BOT_CLEANING: &str = "Cleaning";
pub const BOT_HEALING: &str = "Healing";

pub const BOT_ROOT: &str = "/mob/living/basic/bot";
const EMAG: &str = "/obj/item/card/emag";
/// tg `robot_arm`: dropped in half the deaths.
pub const ROBOT_ARM: &str = "/obj/item/bodypart/arm/right/robot";
const PROX_SENSOR: &str = "/obj/item/assembly/prox_sensor";
const OIL: &str = "/obj/effect/decal/cleanable/blood/oil";

/// What a bot is (its AI tree and settings).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BotKind {
    Clean,
    Med,
    Sec,
    Fire,
    /// Bots whose own behavior is not ported (MULEbot, repairbot, honkbot, hygienebot,
    /// vibebot, ED-209, dedbot): the base bot tree.
    Other,
}

impl BotKind {
    fn of(path: &str) -> Self {
        let under = |root: &str| is_under(path, &format!("{BOT_ROOT}/{root}"));
        if under("cleanbot") {
            Self::Clean
        } else if under("medbot") {
            Self::Med
        } else if under("secbot/honkbot") || under("secbot/ed209") || under("secbot/grievous") {
            Self::Other
        } else if under("secbot") {
            Self::Sec
        } else if under("firebot") {
            Self::Fire
        } else {
            Self::Other
        }
    }
}

/// tg `damage_type_healer`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Healer {
    Type(DamageType),
    /// `HEAL_ALL_DAMAGE`: brute, burn, toxin, oxygen in that order.
    All,
}

impl Healer {
    fn from_dm(value: &str) -> Self {
        match value {
            "all_damage" => Self::All,
            other => Self::Type(DamageType::from_dm(other).unwrap_or(DamageType::Brute)),
        }
    }

    /// tg `get_current_damage_of_type` / `get_total_damage`.
    pub fn damage(self, health: &Health) -> f32 {
        match self {
            Self::All => health.brute + health.burn + health.tox + health.oxy,
            Self::Type(DamageType::Brute) => health.brute,
            Self::Type(DamageType::Burn) => health.burn,
            Self::Type(DamageType::Tox) => health.tox,
            Self::Type(DamageType::Oxy) => health.oxy,
            Self::Type(DamageType::Stamina) => 0.0,
        }
    }
}

/// A timed action of the bot (its `do_after`): ends at `ends_at` unless the bot or the target
/// moved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BotTask {
    pub job: BotJob,
    pub target: Entity,
    pub bot_pos: TilePos,
    pub target_pos: TilePos,
    pub ends_at: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BotJob {
    /// The cleaner component: wash the target (`base_cleaning_duration` 2 s).
    Clean,
    /// `medicate_patient` (2 s).
    Tend,
    /// `stun_n_cuff/cuff_target` (`handcuff_timer` 4 s).
    Cuff,
}

/// A station bot's live state (tg `/mob/living/basic/bot` vars).
#[derive(Component, Clone, Debug)]
pub struct Bot {
    pub kind: BotKind,
    /// `bot_mode_flags`.
    pub mode_flags: u32,
    /// `bot_access_flags`.
    pub cover: u32,
    /// `mode`.
    pub mode: &'static str,
    pub req_one_access: Vec<String>,
    /// `initial_access`: what `bot_reset` gives the card back.
    pub initial_access: Vec<String>,
    /// The access card's access.
    pub access: Vec<String>,
    /// `radio_channel`'s frequency, when the bot's radio has that channel (tg talks only into
    /// channels its key gives; Common has none).
    pub radio: Option<u16>,
    /// `base_icon_state` (or the kind's base).
    pub base_icon: String,
    /// `TRAIT_COMMISSIONED` (Beepsky): other bots salute it.
    pub commissioned: bool,
    pub security_flags: u32,
    pub medical_flags: u32,
    pub heal_amount: f32,
    pub heal_threshold: f32,
    pub healer: Healer,
    pub skin: String,
    pub janitor_flags: u32,
    pub firebot_flags: u32,
    /// `robot_arm` type.
    pub robot_arm: String,
    /// The secbot's `stun_cooldown` (game time it may stun again) and the threat level of its
    /// target (`BB_CURRENT_CRIMINAL_ASSESSMENT`).
    pub stun_ready_at: f64,
    pub threat: i32,
    /// The medkit a medbot was built from (`medkit_type`, dropped on death).
    pub medkit: String,
    pub task: Option<BotTask>,
}

impl Bot {
    pub fn on(&self) -> bool {
        self.mode_flags & MODE_ON != 0
    }

    pub fn emagged(&self) -> bool {
        self.cover & COVER_EMAGGED != 0
    }

    pub fn locked(&self) -> bool {
        self.cover & COVER_LOCKED != 0
    }
}

/// Whether the bot is busy with a [`BotTask`] (its AI pauses).
pub fn busy(world: &World, mob: Entity) -> bool {
    world.get::<Bot>(mob).is_some_and(|b| b.task.is_some())
}

/// A patrol or delivery beacon (`/obj/machinery/navbeacon` after `set_codes`).
#[derive(Clone, Debug, PartialEq)]
pub struct Beacon {
    pub entity: Entity,
    pub pos: TilePos,
    pub location: String,
    /// `codes[NAVBEACON_PATROL_NEXT]`.
    pub next: Option<String>,
    pub patrol: bool,
    pub delivery: bool,
}

/// `GLOB.navbeacons` (patrol beacons by z) and `GLOB.deliverybeacons`.
#[derive(Resource, Default, Debug)]
pub struct NavBeacons(pub HashMap<Entity, Beacon>);

impl NavBeacons {
    /// Patrol beacons on level `z`.
    pub fn patrol_on(&self, z: u16) -> impl Iterator<Item = &Beacon> {
        self.0.values().filter(move |b| b.patrol && b.pos.z == z)
    }
}

/// `set_codes`: `"tag1;tag2=value"` into its tags.
pub fn parse_codes(codes_txt: &str) -> Vec<(String, String)> {
    codes_txt
        .split(';')
        .filter(|e| !e.is_empty())
        .map(|entry| match entry.split_once('=') {
            Some((k, v)) => (k.to_owned(), v.to_owned()),
            None => (entry.to_owned(), "1".to_owned()),
        })
        .collect()
}

pub struct BotsServerPlugin;

impl Plugin for BotsServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NavBeacons>()
            .add_observer(on_basic_mob)
            .add_observer(on_beacon)
            .add_observer(forget_beacon)
            .add_systems(Startup, register_mapped_beacons)
            .add_systems(
                Update,
                (
                    bot_clicks
                        .after(ResolveClicks)
                        .after(crate::security::security_clicks)
                        .before(crate::combat::MeleeSystems)
                        .before(crate::inventory::inventory_intents),
                    run_tasks.before(crate::health::HealthSystems),
                ),
            );
        ui::build(app);
        construction::build(app);
    }
}

fn on_beacon(add: On<Add<DmNavBeacon>>, mut commands: Commands) {
    let entity = add.entity;
    commands.queue(move |world: &mut World| register_beacon(world, entity));
}

/// The map's beacons (the load spawns them before the observers run on their final tiles).
fn register_mapped_beacons(world: &mut World) {
    let beacons: Vec<Entity> = world
        .query_filtered::<Entity, With<DmNavBeacon>>()
        .iter(world)
        .collect();
    for beacon in beacons {
        register_beacon(world, beacon);
    }
}

fn register_beacon(world: &mut World, entity: Entity) {
    let (Some(dm), Some(&pos)) = (
        world.get::<DmNavBeacon>(entity).cloned(),
        world.get::<TilePos>(entity),
    ) else {
        return;
    };
    let codes = parse_codes(&dm.codes_txt);
    let has = |k: &str| codes.iter().any(|(c, _)| c == k);
    let beacon = Beacon {
        entity,
        pos,
        location: dm.location.clone(),
        next: codes
            .iter()
            .find(|(c, _)| c == "next_patrol")
            .map(|(_, v)| v.clone()),
        patrol: has("patrol"),
        delivery: has("delivery"),
    };
    if beacon.patrol || beacon.delivery {
        world.resource_mut::<NavBeacons>().0.insert(entity, beacon);
    }
}

fn forget_beacon(remove: On<Remove<DmNavBeacon>>, mut beacons: ResMut<NavBeacons>) {
    beacons.0.remove(&remove.entity);
}

/// A new basic mob with bot vars becomes a [`Bot`].
fn on_basic_mob(add: On<Add<BasicMob>>, mobs: Query<&BasicMob>, mut commands: Commands) {
    let Ok(basic) = mobs.get(add.entity) else {
        return;
    };
    if basic.def.bot.is_none() {
        return;
    }
    let entity = add.entity;
    commands.queue(move |world: &mut World| attach(world, entity));
}

/// tg `/mob/living/basic/bot/Initialize` and its kinds' for what is ported.
pub fn attach(world: &mut World, entity: Entity) {
    let Some(basic) = world.get::<BasicMob>(entity).cloned() else {
        return;
    };
    let Some(def) = basic.def.bot.clone() else {
        return;
    };
    if world.get::<Bot>(entity).is_some() {
        return;
    }
    let kind = BotKind::of(&basic.def.path);
    let base_icon = if def.base_icon_state.is_empty() {
        def.icon_state.trim_end_matches(['0', '1']).to_owned()
    } else {
        def.base_icon_state.clone()
    };
    // The internal radio's key gives these channels (`radio_key`).
    let radio = ss13_chat::radio::channel_freq(&def.radio_channel)
        .filter(|_| def.radio_channel != ss13_chat::radio::RADIO_CHANNEL_COMMON);
    let bot = Bot {
        kind,
        mode_flags: def.mode_flags,
        cover: def.access_flags,
        mode: BOT_IDLE,
        req_one_access: def.req_one_access.clone(),
        initial_access: def.access.clone(),
        access: def.access.clone(),
        radio,
        base_icon,
        commissioned: def.commissioned,
        security_flags: def.security_mode_flags,
        medical_flags: def.medical_mode_flags,
        heal_amount: def.heal_amount,
        heal_threshold: def.heal_threshold,
        healer: Healer::from_dm(&def.damage_type_healer),
        skin: if def.skin.is_empty() {
            "generic".to_owned()
        } else {
            def.skin.clone()
        },
        janitor_flags: def.janitor_mode_flags,
        firebot_flags: FIREBOT_EXTINGUISH_PEOPLE | FIREBOT_EXTINGUISH_FLAMES,
        robot_arm: ROBOT_ARM.to_owned(),
        stun_ready_at: 0.0,
        threat: 0,
        medkit: "/obj/item/storage/medkit".to_owned(),
        task: None,
    };
    world.entity_mut(entity).insert(bot);
    refresh_look(world, entity);
}

/// tg `update_icon_state` of each kind.
pub fn refresh_look(world: &mut World, mob: Entity) {
    let Some(bot) = world.get::<Bot>(mob) else {
        return;
    };
    let Some(icon) = world.get::<BasicMob>(mob).map(|b| b.def.icon.clone()) else {
        return;
    };
    let on = u8::from(bot.on());
    let base = &bot.base_icon;
    let state = match bot.kind {
        BotKind::Clean if bot.mode == BOT_CLEANING => format!("{base}-c"),
        BotKind::Med => {
            let suffix = if bot.mode == BOT_HEALING {
                "active"
            } else {
                "idle"
            };
            format!("{base}_{}_{suffix}", bot.skin)
        }
        BotKind::Sec if bot.mode == BOT_HUNT && bot.on() => format!("{base}-c"),
        BotKind::Fire if !bot.on() => "firebot0".to_owned(),
        BotKind::Fire if bot.firebot_flags & FIREBOT_STATIONARY_MODE != 0 => "firebots1".to_owned(),
        BotKind::Fire => "firebot1".to_owned(),
        _ => format!("{base}{on}"),
    };
    crate::mobs::set_icon_state(world, mob, &icon, &state);
}

/// tg `update_bot_mode`.
pub fn set_mode(world: &mut World, mob: Entity, mode: &'static str) {
    let Some(mut bot) = world.get_mut::<Bot>(mob) else {
        return;
    };
    if bot.mode == mode {
        return;
    }
    bot.mode = mode;
    refresh_look(world, mob);
}

/// tg `turn_on`.
pub fn turn_on(world: &mut World, mob: Entity) {
    if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return;
    }
    if let Some(mut bot) = world.get_mut::<Bot>(mob) {
        bot.mode_flags |= MODE_ON;
    }
    refresh_look(world, mob);
}

/// tg `turn_off`: the AI stops (`get_able_to_run`), the bot resets.
pub fn turn_off(world: &mut World, mob: Entity) {
    if let Some(mut bot) = world.get_mut::<Bot>(mob) {
        bot.mode_flags &= !MODE_ON;
    }
    bot_reset(world, mob);
    refresh_look(world, mob);
}

/// tg `bot_reset`: the card's access back, idle, the plan and the reset keys cleared.
pub fn bot_reset(world: &mut World, mob: Entity) {
    if let Some(mut bot) = world.get_mut::<Bot>(mob) {
        bot.access = bot.initial_access.clone();
        bot.mode = BOT_IDLE;
        bot.task = None;
    }
    ai::reset(world, mob);
    refresh_look(world, mob);
}

/// The bot's name as players see it.
pub fn bot_name(world: &World, mob: Entity) -> String {
    name_of(world, mob)
}

/// tg `speak` -> `bot_announcement/announce`: says `line` while on, and into its radio channel
/// with `radio`.
// shortcut: spoken with the default say verbs (`говорит`), not the bot's `verb_say` ("states",
// BandaStation `сообщает`); add when `ss13_chat` takes a speaker's verbs.
pub fn speak(world: &mut World, mob: Entity, line: &str, radio: bool) {
    let Some(bot) = world.get::<Bot>(mob) else {
        return;
    };
    if !bot.on() || line.is_empty() {
        return;
    }
    // `/datum/action/cooldown/bot_announcement/medbot`: silent without the speaker.
    if bot.kind == BotKind::Med && bot.medical_flags & MEDBOT_SPEAK_MODE == 0 {
        return;
    }
    let freq = bot.radio.filter(|_| radio);
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let name = bot_name(world, mob);
    let clients = crate::obj_damage::clients_in_range(world, pos, DEFAULT_MESSAGE_RANGE, &[]);
    if !clients.is_empty() {
        world.write_message(SendChat {
            clients,
            message: ss13_chat::server::say_message(&name, line),
        });
    }
    if let Some(freq) = freq {
        world.write_message(crate::radio::MachineRadio {
            freq,
            z: pos.z,
            name,
            text: line.to_owned(),
        });
    }
}

/// tg `allowed(user)` of a bot: unlocked, or the user's ID has one of `req_one_access`.
pub fn allowed(world: &World, mob: Entity, user: Entity) -> bool {
    let Some(bot) = world.get::<Bot>(mob) else {
        return false;
    };
    if !bot.locked() || crate::silicon::interact::has_silicon_access(world, user) {
        return true;
    }
    let access = user_access(world, user);
    crate::inventory::check_access(&[], &bot.req_one_access, &access)
}

/// The access of the ID tg's `get_idcard(hand_first = TRUE)` finds on `user`.
pub fn user_access(world: &World, user: Entity) -> Vec<String> {
    world
        .get::<Inventory>(user)
        .and_then(|i| i.id_card(world))
        .and_then(|card| world.get::<IdCard>(card))
        .map(|c| c.access.clone())
        .unwrap_or_default()
}

fn tell_mob(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

/// tg `unlock_with_id`.
pub fn unlock_with_id(world: &mut World, mob: Entity, user: Entity) {
    let Some(bot) = world.get::<Bot>(mob) else {
        return;
    };
    if bot.emagged() {
        tell_mob(world, user, Span::Warning, "Ошибка!");
        return;
    }
    if bot.cover & COVER_MAINTS_OPEN != 0 {
        tell_mob(
            world,
            user,
            Span::Warning,
            "Панель доступа должна быть закрыта!",
        );
        return;
    }
    let has = crate::inventory::check_access(&[], &bot.req_one_access, &user_access(world, user));
    if !has {
        tell_mob(world, user, Span::Warning, "Нет доступа!");
        return;
    }
    let mut bot = world.get_mut::<Bot>(mob).expect("checked");
    bot.cover ^= COVER_LOCKED;
    let text = if bot.locked() {
        "Управление заблокировано."
    } else {
        "Управление разблокировано."
    };
    tell_mob(world, user, Span::Notice, text);
}

/// tg `screwdriver_act`.
fn screwdriver(world: &mut World, mob: Entity, user: Entity) {
    let Some(mut bot) = world.get_mut::<Bot>(mob) else {
        return;
    };
    if bot.locked() {
        tell_mob(
            world,
            user,
            Span::Warning,
            "Панель техобслуживания заблокирована!",
        );
        return;
    }
    bot.cover ^= COVER_MAINTS_OPEN;
    let text = if bot.cover & COVER_MAINTS_OPEN != 0 {
        "Панель техобслуживания открыта."
    } else {
        "Панель техобслуживания закрыта."
    };
    tell_mob(world, user, Span::Notice, text);
}

/// tg `welder_act` out of combat mode: heals 10 with the panel open.
fn weld(world: &mut World, mob: Entity, user: Entity, welder: Entity) {
    let (Some(bot), Some(health)) = (world.get::<Bot>(mob), world.get::<Health>(mob)) else {
        return;
    };
    if health.health() >= health.max {
        tell_mob(world, user, Span::Warning, "Ремонт не требуется!");
        return;
    }
    if bot.cover & COVER_MAINTS_OPEN == 0 {
        tell_mob(
            world,
            user,
            Span::Warning,
            "Панель техобслуживания закрыта!",
        );
        return;
    }
    // `use_tool(src, user, 0 SECONDS)`: lit, nothing spent.
    if !crate::tools::tool_usable(world, welder, 0, 0.0) {
        let text = crate::tools::unusable_message(world, welder, 0);
        tell_mob(world, user, Span::Warning, text);
        return;
    }
    if let Some(mut health) = world.get_mut::<Health>(mob) {
        health.adjust(DamageType::Brute, -10.0);
    }
    let (who, what) = (name_of(world, user), name_of(world, mob));
    if let Some(&pos) = world.get::<TilePos>(mob) {
        show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (Span::Notice, format!("{who} чинит {what}!")),
            &[(user, Span::Notice, format!("Вы чините {what}."))],
        );
    }
}

/// tg `emag_act`: the first use unlocks the controls; with the panel open, the bot is emagged
/// (locked, no remote control, reset and turned on) and its kind's `emag_effects` run.
pub fn emag(world: &mut World, mob: Entity, user: Entity) {
    let Some(bot) = world.get::<Bot>(mob) else {
        return;
    };
    if bot.locked() && !bot.emagged() {
        world.get_mut::<Bot>(mob).expect("checked").cover &= !COVER_LOCKED;
        tell_mob(world, user, Span::Notice, "Крышка разблокирована.");
        return;
    }
    if bot.cover & COVER_MAINTS_OPEN == 0 {
        tell_mob(
            world,
            user,
            Span::Warning,
            "Сначала откройте панель техобслуживания!",
        );
        return;
    }
    {
        let mut bot = world.get_mut::<Bot>(mob).expect("checked");
        bot.cover |= COVER_EMAGGED | COVER_LOCKED;
        bot.mode_flags &= !MODE_REMOTE_ENABLED;
    }
    bot_reset(world, mob);
    turn_on(world, mob);
    emag_effects(world, mob);
}

/// tg `emag_effects` of the kinds: the safeties of the kind's circuits are gone.
fn emag_effects(world: &mut World, mob: Entity) {
    let Some(kind) = world.get::<Bot>(mob).map(|b| b.kind) else {
        return;
    };
    match kind {
        BotKind::Med => {
            world.get_mut::<Bot>(mob).expect("checked").medical_flags &= !MEDBOT_DECLARE_CRIT;
        }
        BotKind::Sec => {
            world.get_mut::<Bot>(mob).expect("checked").security_flags &= !SECBOT_DECLARE_ARRESTS;
        }
        _ => {}
    }
    let name = bot_name(world, mob);
    if let Some(&pos) = world.get::<TilePos>(mob) {
        show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (Span::Danger, format!("{name} странно жужжит!")),
            &[],
        );
    }
}

/// Clicks of conscious humans on bots that a bot takes before melee (tg `attack_hand` out of
/// combat mode, `item_interaction` with IDs, `screwdriver_act`, `welder_act`, `emag_act`).
fn bot_clicks(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        let (user, mob) = (click.mob, click.target);
        if click.modifiers.right || world.get::<Bot>(mob).is_none() || user == mob {
            continue;
        }
        if world
            .resource::<crate::security::MeleeHandled>()
            .0
            .contains(&user)
        {
            continue;
        }
        let combat = world.get::<CombatMode>(user).is_some_and(|c| c.0);
        let held = world
            .get::<Inventory>(user)
            .and_then(Inventory::active_item);
        let handled = match held {
            None if !combat => {
                ui::open(world, click.client, user, mob);
                true
            }
            None => false,
            Some(item) => item_interaction(world, user, item, mob, combat),
        };
        if handled {
            world
                .resource_mut::<crate::security::MeleeHandled>()
                .0
                .insert(user);
        }
    }
}

/// tg `/mob/living/basic/bot/item_interaction` and the tool acts. True when the item was
/// used on the bot (no attack follows).
fn item_interaction(
    world: &mut World,
    user: Entity,
    item: Entity,
    mob: Entity,
    combat: bool,
) -> bool {
    if crate::inventory::get_id(world, item).is_some() {
        unlock_with_id(world, mob, user);
        return true;
    }
    if world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, EMAG))
    {
        emag(world, mob, user);
        return true;
    }
    match crate::tools::tool_of(world, item).map(|t| t.0) {
        Some(ToolKind::Screwdriver) => {
            screwdriver(world, mob, user);
            true
        }
        Some(ToolKind::Welder) if !combat => {
            weld(world, mob, user, item);
            true
        }
        _ => false,
    }
}

/// tg `click_alt` -> `unlock_with_id`. True when `target` is a bot.
pub fn click_alt(world: &mut World, user: Entity, target: Entity) -> bool {
    if world.get::<Bot>(target).is_none() {
        return false;
    }
    unlock_with_id(world, target, user);
    true
}

/// tg `/mob/living/basic/bot/examine` (and the medbot's, secbot's lines).
pub fn examine_lines(world: &World, mob: Entity, lines: &mut Vec<crate::examine::Line>) {
    let (Some(bot), Some(health)) = (world.get::<Bot>(mob), world.get::<Health>(mob)) else {
        return;
    };
    let name = bot_name(world, mob);
    let state = if health.health() >= health.max {
        format!("{name} в идеальном состоянии.")
    } else if health.health() > health.max * 0.3 {
        format!("Детали {name} выглядят расшатанными.")
    } else {
        format!("Детали {name} выглядят очень расшатанными!")
    };
    lines.push((Span::Plain, state));
    let open = bot.cover & COVER_MAINTS_OPEN != 0;
    lines.push((
        Span::Notice,
        format!(
            "Панель техобслуживания {}.",
            if open {
                "открыта"
            } else {
                "закрыта"
            }
        ),
    ));
    lines.push((
        Span::Info,
        format!(
            "Её можно {} отвёрткой.",
            if open {
                "закрыть"
            } else {
                "открыть"
            }
        ),
    ));
    if open {
        lines.push((
            Span::Notice,
            format!(
                "Панель управления {}.",
                if bot.locked() {
                    "заблокирована"
                } else {
                    "разблокирована"
                }
            ),
        ));
    }
}

/// tg `/mob/living/basic/bot/death` -> `explode`: the parts of the bot's kind drop. Called by
/// `crate::mobs` before the bot is deleted.
pub fn explode(world: &mut World, mob: Entity) {
    let (Some(bot), Some(&pos)) = (world.get::<Bot>(mob).cloned(), world.get::<TilePos>(mob))
    else {
        return;
    };
    let name = bot_name(world, mob);
    show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Info,
        (Span::BoldNotice, format!("{name} разлетается на куски!")),
        &[],
    );
    let mut drops: Vec<&str> = Vec::new();
    match bot.kind {
        BotKind::Clean => drops.extend(["/obj/item/reagent_containers/cup/bucket", PROX_SENSOR]),
        BotKind::Med => drops.extend([bot.medkit.as_str(), "/obj/item/healthanalyzer"]),
        BotKind::Sec => {
            drops.extend([PROX_SENSOR, "/obj/item/melee/baton/security", OIL]);
        }
        BotKind::Fire => drops.extend([PROX_SENSOR, "/obj/item/clothing/head/utility/hardhat/red"]),
        BotKind::Other => {}
    }
    for path in drops {
        crate::construction::spawn_obj(world, path, pos);
    }
    if bot.kind == BotKind::Sec {
        // `retrieve_secbot_drops`: the assembly at its first step, with the hole welded.
        if let Some(assembly) =
            crate::construction::spawn_obj(world, construction::SECBOT_ASSEMBLY, pos)
        {
            construction::set_secbot_step(world, assembly, 2, &name);
        }
    }
    if world
        .resource_mut::<crate::combat::CombatRng>()
        .chance(50.0)
    {
        crate::construction::spawn_obj(world, &bot.robot_arm, pos);
    }
}

/// Every tick: finished bot tasks take effect, interrupted ones end.
fn run_tasks(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &Bot)>();
    let pending: Vec<(Entity, BotTask)> = query
        .iter(world)
        .filter_map(|(e, b)| b.task.map(|t| (e, t)))
        .collect();
    for (mob, task) in pending {
        let valid = world.get::<TilePos>(mob) == Some(&task.bot_pos)
            && world.get::<TilePos>(task.target) == Some(&task.target_pos)
            && world.get::<Stat>(mob) == Some(&Stat::Conscious)
            && world.get::<Bot>(mob).is_some_and(Bot::on);
        if !valid {
            if let Some(mut bot) = world.get_mut::<Bot>(mob) {
                bot.task = None;
            }
            ai::task_interrupted(world, mob, task);
            continue;
        }
        if now < task.ends_at {
            continue;
        }
        if let Some(mut bot) = world.get_mut::<Bot>(mob) {
            bot.task = None;
        }
        ai::task_done(world, mob, task);
    }
}

/// Starts a [`BotTask`] of `secs` on `target`; false when the bot is busy.
pub fn start_task(world: &mut World, mob: Entity, job: BotJob, target: Entity, secs: f64) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let (Some(&bot_pos), Some(&target_pos)) =
        (world.get::<TilePos>(mob), world.get::<TilePos>(target))
    else {
        return false;
    };
    let Some(mut bot) = world.get_mut::<Bot>(mob) else {
        return false;
    };
    if bot.task.is_some() {
        return false;
    }
    bot.task = Some(BotTask {
        job,
        target,
        bot_pos,
        target_pos,
        ends_at: now + secs,
    });
    true
}

/// `get_area_name` of the area at `pos`.
pub fn area_name(world: &World, pos: TilePos) -> String {
    world
        .get_resource::<ss13_mapload::Areas>()
        .and_then(|a| a.area_at(pos))
        .and_then(|area| world.get::<DmName>(area))
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navbeacon_codes_parse_like_set_codes() {
        assert_eq!(
            parse_codes("patrol;next_patrol=9.4-Escape-4"),
            vec![
                ("patrol".to_owned(), "1".to_owned()),
                ("next_patrol".to_owned(), "9.4-Escape-4".to_owned()),
            ]
        );
        assert!(parse_codes("").is_empty());
        assert_eq!(parse_codes("delivery;dir=4")[1].1, "4");
    }

    #[test]
    fn kinds_follow_the_type_tree() {
        assert_eq!(
            BotKind::of("/mob/living/basic/bot/secbot/beepsky/officer"),
            BotKind::Sec
        );
        assert_eq!(
            BotKind::of("/mob/living/basic/bot/secbot/honkbot"),
            BotKind::Other
        );
        assert_eq!(
            BotKind::of("/mob/living/basic/bot/medbot/autopatrol"),
            BotKind::Med
        );
        assert_eq!(
            BotKind::of("/mob/living/basic/bot/cleanbot"),
            BotKind::Clean
        );
        assert_eq!(BotKind::of("/mob/living/basic/bot/mulebot"), BotKind::Other);
    }

    #[test]
    fn healer_damage_by_type() {
        let mut h = Health::default();
        h.adjust(DamageType::Brute, 12.0);
        h.adjust(DamageType::Tox, 5.0);
        assert_eq!(Healer::from_dm("brute").damage(&h), 12.0);
        assert_eq!(Healer::from_dm("all_damage").damage(&h), 17.0);
        assert_eq!(Healer::from_dm("toxin").damage(&h), 5.0);
    }
}

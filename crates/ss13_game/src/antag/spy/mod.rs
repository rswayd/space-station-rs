//! The spy (server) after tg `/datum/antagonist/spy` and `/datum/component/spy_uplink`
//! (`code/modules/antagonists/spy/`, BandaStation strings).
//!
//! [`add_spy`] is `add_antag_datum(/datum/antagonist/spy)` on a crew body: the spy uplink goes
//! into its PDA, else its radio, else a pen (`get_uplink_location(UPLINK_PDA)`, the preference's
//! default), 1-3 flavour objectives from `SPY_OBJECTIVE_FILE`, sometimes a protect objective, and
//! an ending one (martyr, hijack, exile or escape); the greeting, the stinger, the info panel
//! (`AntagInfoSpy`), the antag HUD icon `spy`.
//!
//! The uplink ([`SpyUplink`]) used in hand by a spy opens `SpyUplink`: every bounty of the shared
//! handler ([`bounty`]) with its reward and the time to the next refresh. A right click with it
//! on a bounty's target (a spy that may claim it) scans the target for the bounty's
//! `theft_time` (`do_after`); at the end, checked again, the target is stolen: sparks, an item
//! goes to the black market's "Fenced Goods" (`black_market_prob`, always when indestructible)
//! or is deleted, a machine is taken apart and its frame goes, a bot is deleted, a person
//! target's item is taken while they fall unconscious for 10 s. The bounty is claimed for every
//! spy, its key counts against later dupes, and the reward spawns in the spy's hands or at
//! their feet. At round end [`roundend_report`] lists the thefts and the loot.
// shortcut: no backup "Create Uplink" action when the spy carries no PDA, radio or pen; no
// moodlet, custom objective button, admin commands, PDA bomb block, scan hologram and cone,
// `COMSIG_MOVABLE_SPY_STEALING`; the jailbreak objectives (not ported) are not rolled; a stolen
// machine's circuit board is not put on the market; stealing from one's own other hand is not
// possible (hand slot clicks carry no right click).

pub mod bounty;
mod strings;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{DmItem, DmObj, IconIndex};
use ss13_net::server::ClientToken;
use ss13_net::sound::server::SendSound;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::SoundEvent;
use ss13_tgui::UiStatus;
use ss13_tgui::server::{AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiSystems, UiWindow};

use self::bounty::{Bounty, BountyKind, Difficulty, SpyBounties};
use super::dynamic::DynamicRng;
use super::objectives::{Objective, ObjectiveKind, RoundEnd, find_target, objective, protect};
use super::{
    AntagHudData, AntagHudUi, AntagInfoSpyUi, ObjectiveInfo, SpyBountyRecord, SpyInfo,
    SpyUplinkData, SpyUplinkUi,
};
use crate::construction::Job;
use crate::health::Stat;
use crate::inventory::CarriedBy;
use crate::jobs::{Jobs, PlayerJob};
use crate::server::PlayerMob;
use crate::storage::StoredIn;
use crate::tools::{Actor, DoAfter, do_after, tell_mob};

/// `name = "\proper Шпион"`.
pub const SPY_NAME: &str = "Шпион";
/// `roundend_category`.
pub const ROUNDEND_CATEGORY: &str = "Шпионы";
/// `antag_hud_name`.
const ANTAG_HUD: (&str, &str) = ("icons/mob/huds/antag_hud.dmi", "spy");
const STINGER: &str = "sound/music/antag/spy.ogg";
const SCAN_SOUND: &str = "sound/items/pshoom/pshoom.ogg";
const DONE_SOUND: &str = "sound/machines/wewewew.ogg";
const PDA: &str = "/obj/item/modular_computer/pda";
const PEN: &str = "/obj/item/pen";
const MACHINERY: &str = "/obj/machinery";
const FRAME: &str = "/obj/structure/frame";
/// The bounty list's title (`SpyUplink.tsx`'s section).
const UPLINK_TITLE: &str = "Шпионские заказы";
/// `PAYCHECK_COMMAND`.
const PAYCHECK_COMMAND: i64 = 100;
/// `/datum/objective` `objective_name` in the report.
const OBJECTIVE_NAME: &str = "Задача";

/// The spy antagonist datum, on the body.
#[derive(Component, Clone, Debug)]
pub struct Spy {
    /// The player's key for the report.
    pub key: String,
    pub objectives: Vec<Objective>,
    pub uplink: Option<Entity>,
    /// `uplink_location`: where the panel says the uplink is.
    pub uplink_location: Option<String>,
    /// `bounties_claimed`.
    pub bounties_claimed: u32,
    /// `all_loot`: the rewards' names.
    pub all_loot: Vec<String>,
}

impl Spy {
    fn info(&self) -> SpyInfo {
        SpyInfo {
            antag_name: SPY_NAME.to_owned(),
            uplink_location: self.uplink_location.clone(),
            objectives: self
                .objectives
                .iter()
                .enumerate()
                .map(|(i, o)| ObjectiveInfo {
                    count: i as u32 + 1,
                    explanation: o.explanation.clone(),
                    complete: false,
                })
                .collect(),
        }
    }
}

/// tg `/datum/component/spy_uplink` on its device.
#[derive(Component, Clone, Copy, Debug)]
pub struct SpyUplink {
    /// The spy who got it (`spy_ref`).
    pub owner: Entity,
}

/// A scan in progress: the bounty and what is being stolen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpyJob {
    pub bounty: u32,
    pub stealing: Entity,
}

pub struct SpyPlugin;

impl Plugin for SpyPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.register_ui::<AntagInfoSpyUi>()
            .register_ui::<SpyUplinkUi>()
            // The uplink's window; the device's own use goes on (`return NONE`).
            .add_click_handler(ClickPhase::AttackSelf, 49, |w, c| {
                attack_self(w, c.client, c.mob, c.target)
            })
            // `COMSIG_ITEM_INTERACTING_WITH_ATOM_SECONDARY`.
            .add_click_handler(ClickPhase::HeldOnTarget, 9, |w, c| {
                c.modifiers.right
                    && c.held
                        .is_some_and(|h| try_steal(w, c.client, c.mob, h, c.target))
            })
            .add_systems(
                Update,
                (
                    open_info.in_set(UiSystems::Handle),
                    uplink_status.in_set(UiSystems::StatusHook),
                    bounty::tick_refresh.run_if(resource_exists::<SpyBounties>),
                ),
            );
    }
}

impl ServerUi for AntagInfoSpyUi {
    type Source = &'static Spy;
    type UserParam = ();
    type Act = ();

    fn static_data(spy: &Spy) -> SpyInfo {
        spy.info()
    }

    fn data(_: &Spy, _: Entity, _: &()) {}
}

impl ServerUi for SpyUplinkUi {
    type Source = &'static SpyUplink;
    type UserParam = (
        Option<Res<'static, SpyBounties>>,
        Res<'static, Time>,
        Query<'static, 'static, &'static PlayerJob>,
        Option<Res<'static, Jobs>>,
    );
    type Act = ();

    fn static_data(_: &SpyUplink) {}

    fn data(
        _: &SpyUplink,
        user: Entity,
        (handler, time, jobs_of, jobs): &bevy::ecs::system::SystemParamItem<
            '_,
            '_,
            Self::UserParam,
        >,
    ) -> SpyUplinkData {
        let Some(handler) = handler else {
            return SpyUplinkData::default();
        };
        let job = jobs_of.get(user).ok().map(|j| j.0);
        let jobs = jobs.as_deref();
        SpyUplinkData {
            bounties: handler
                .bounties
                .iter()
                .map(|b| SpyBountyRecord {
                    name: b.name.clone(),
                    help: b.help.clone(),
                    difficulty: b.difficulty.name().to_owned(),
                    reward: b.reward_name.clone(),
                    claimed: b.claimed,
                    can_claim: b.can_claim(user, job, jobs),
                })
                .collect(),
            time_left: ((handler.refresh_at - time.elapsed_secs_f64()).max(0.0) * 10.0) as u32,
        }
    }
}

fn with_rng<R>(world: &mut World, f: impl FnOnce(&mut Rng) -> R) -> R {
    if !world.contains_resource::<DynamicRng>() {
        world.insert_resource(DynamicRng(Rng::new(0)));
    }
    f(&mut world.resource_mut::<DynamicRng>().0)
}

/// `pick_list_replacements(SPY_OBJECTIVE_FILE, key)`: a line of `key`, each `@pick(other)` a
/// line of `other` (one level).
pub fn pick_objective_line(rng: &mut Rng, key: &str) -> String {
    let pick = |rng: &mut Rng, key: &str| -> String {
        let Some((_, lines)) = strings::SPY_OBJECTIVES.iter().find(|(k, _)| *k == key) else {
            return String::new();
        };
        if lines.is_empty() {
            return String::new();
        }
        lines[rng.range(0, lines.len() as i64 - 1) as usize].to_owned()
    };
    let line = pick(rng, key);
    let mut out = String::new();
    let mut rest = line.as_str();
    while let Some(start) = rest.find("@pick(") {
        out.push_str(&rest[..start]);
        let after = &rest[start + "@pick(".len()..];
        let Some(end) = after.find(')') else {
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        out.push_str(&pick(rng, &after[..end]));
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// `give_random_objectives`.
fn give_random_objectives(world: &mut World, mob: Entity) -> Vec<Objective> {
    let mut objectives: Vec<Objective> = Vec::new();
    let count = with_rng(world, |rng| rng.range(1, 3));
    for _ in 0..count {
        let text = with_rng(world, |rng| pick_objective_line(rng, "objective_body"));
        objectives.push(Objective {
            kind: ObjectiveKind::Custom,
            explanation: text,
        });
    }
    if objectives.len() < 3 && with_rng(world, |rng| rng.prob(25.0)) {
        // 1 and 2: protect (human and not); 3 and 4: the jailbreak objectives, not ported.
        if with_rng(world, |rng| rng.range(1, 4)) <= 2 {
            let mut rng = world
                .remove_resource::<DynamicRng>()
                .map_or_else(|| Rng::new(mob.to_bits()), |r| r.0);
            let target = find_target(world, &[mob], &objectives, protect, &mut rng);
            world.insert_resource(DynamicRng(rng));
            objectives.push(objective(world, protect(target)));
        }
    }
    let ending = with_rng(world, |rng| {
        if rng.prob(10.0) {
            ObjectiveKind::Martyr
        } else if rng.prob(3.0) {
            ObjectiveKind::Hijack
        } else if rng.prob(10.0) {
            ObjectiveKind::Exile
        } else {
            ObjectiveKind::Escape
        }
    });
    objectives.push(objective(world, ending));
    objectives
}

/// `get_uplink_location(UPLINK_PDA)`: the PDA, else a radio, else a pen (the PDA's first).
fn uplink_device(world: &World, mob: Entity) -> Option<Entity> {
    let contents = super::objectives::all_contents(world, mob);
    let path_under = |e: Entity, root: &str| {
        world
            .get::<DmItem>(e)
            .is_some_and(|dm| is_under(&dm.path, root))
    };
    let pda = contents.iter().copied().find(|&e| {
        world
            .get::<crate::modpc::pda::ModularComputer>(e)
            .is_some_and(|c| is_under(&c.path, PDA))
    });
    let radio = || {
        contents
            .iter()
            .copied()
            .find(|&e| world.get::<crate::radio::Radio>(e).is_some())
    };
    let pen = || {
        let in_pda = pda.and_then(|p| {
            contents
                .iter()
                .copied()
                .find(|&e| path_under(e, PEN) && world.get::<StoredIn>(e).is_some_and(|s| s.0 == p))
        });
        in_pda.or_else(|| contents.iter().copied().find(|&e| path_under(e, PEN)))
    };
    pda.or_else(radio).or_else(pen)
}

/// `create_spy_uplink`'s `uplink_location`.
fn location_text(world: &World, device: Entity) -> Option<String> {
    if world
        .get::<crate::modpc::pda::ModularComputer>(device)
        .is_some()
    {
        return Some("ваш КПК".to_owned());
    }
    let path = world.get::<DmItem>(device).map(|d| d.path.clone())?;
    if is_under(&path, PEN) {
        let in_pda = world.get::<StoredIn>(device).is_some_and(|s| {
            world
                .get::<crate::modpc::pda::ModularComputer>(s.0)
                .is_some()
        });
        return Some(
            if in_pda {
                "ручку вашего КПК"
            } else {
                "ручку"
            }
            .to_owned(),
        );
    }
    world
        .get::<crate::radio::Radio>(device)
        .map(|_| "ваш наушник".to_owned())
}

/// `add_antag_datum(/datum/antagonist/spy)` on `mob`. `false` if it already is one.
pub fn add_spy(world: &mut World, mob: Entity) -> bool {
    if world.get::<Spy>(mob).is_some() || world.get_entity(mob).is_err() {
        return false;
    }
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    // `auto_create_spy_uplink`.
    let device = uplink_device(world, mob);
    let uplink_location = device.and_then(|d| location_text(world, d));
    if let Some(device) = device {
        // The window follows the device in the spy's hands ([`uplink_status`]).
        world
            .entity_mut(device)
            .insert((SpyUplink { owner: mob }, AlwaysState));
        bounty::ensure_handler(world);
    } else if let Some(client) = client {
        crate::tools::tell(
            world,
            client,
            Span::BoldNotice,
            "Нам не удалось предоставить вам аплинк.",
        );
    }
    let objectives = give_random_objectives(world, mob);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let hud_icon = world
        .get_resource::<IconIndex>()
        .and_then(|index| index.get(ANTAG_HUD.0, ANTAG_HUD.1));
    world.entity_mut(mob).insert((
        Spy {
            key,
            objectives,
            uplink: device,
            uplink_location,
            bounties_claimed: 0,
            all_loot: Vec::new(),
        },
        AntagHudData {
            hud_icon,
            ..default()
        },
    ));
    if let Some(client) = client {
        // The base `on_gain`: `greet()`, the panel note, the panel; then `SEND_SOUND`.
        crate::tools::tell(world, client, Span::Bold, format!("Вы - {SPY_NAME}."));
        crate::tools::tell(
            world,
            client,
            Span::BoldNotice,
            "Для большей информации - прочтите панель. Вы можете вернуться снова, нажав кнопку \
             сверху слева.",
        );
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
        world.write_message(OpenUi::new::<AntagInfoSpyUi>(client, mob, mob, SPY_NAME));
        world.write_message(SendSound {
            client,
            sound: SoundEvent {
                file: STINGER.to_owned(),
                volume: 100.0,
                speed: 1.0,
                offset: None,
            },
        });
    }
    info!(
        "{mob} is a spy{}",
        if device.is_some() {
            ""
        } else {
            " without an uplink"
        }
    );
    true
}

/// `on_attack_self`: a spy using the device opens the bounty list. Never takes the click.
fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<SpyUplink>(item).is_some() && world.get::<Spy>(mob).is_some() {
        world.write_message(OpenUi::new::<SpyUplinkUi>(client, mob, item, UPLINK_TITLE));
    }
    false
}

/// `inventory_state` of the bounty list: the device in the actor's hands or on them (or in
/// something they carry); a spy out of it sees it disabled.
fn uplink_status(
    mut windows: Query<&mut UiWindow>,
    carried: Query<&CarriedBy>,
    stored: Query<&StoredIn>,
    stats: Query<&Stat>,
) {
    for mut window in &mut windows {
        if window.interface != <SpyUplinkUi as ss13_tgui::UiInterface>::NAME {
            continue;
        }
        let mut e = window.source;
        let mut holder = None;
        for _ in 0..8 {
            if let Ok(c) = carried.get(e) {
                holder = Some(c.0);
                break;
            }
            match stored.get(e) {
                Ok(s) => e = s.0,
                Err(_) => break,
            }
        }
        if holder != Some(window.actor) {
            window.cap_status(UiStatus::Close);
        } else if stats.get(window.actor).is_ok_and(|s| *s != Stat::Conscious) {
            window.cap_status(UiStatus::Disabled);
        }
    }
}

/// The antag info button of a spy.
fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if world.get::<Spy>(source).is_some() {
            world.write_message(OpenUi::new::<AntagInfoSpyUi>(
                client, actor, source, SPY_NAME,
            ));
        }
    }
}

fn name_of(world: &World, e: Entity) -> String {
    crate::security::name_of(world, e)
}

/// A spy's job, for `can_claim`.
fn job_of(world: &World, mob: Entity) -> Option<usize> {
    world.get::<PlayerJob>(mob).map(|j| j.0)
}

/// The bounties `spy` may claim that `target` satisfies.
fn matching_bounty(world: &World, spy: Entity, target: Entity) -> Option<Bounty> {
    let handler = world.get_resource::<SpyBounties>()?;
    let job = job_of(world, spy);
    let jobs = world.get_resource::<Jobs>();
    handler
        .bounties
        .iter()
        .find(|b| b.can_claim(spy, job, jobs) && bounty::is_stealable(world, b, target))
        .cloned()
}

/// `try_steal`: a right click with the uplink on a bounty's target starts the scan. `false`
/// when nothing here is wanted (the click goes on).
pub fn try_steal(
    world: &mut World,
    client: Entity,
    spy: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<SpyUplink>(held).is_none() || world.get::<Spy>(spy).is_none() {
        return false;
    }
    // `ismovable(target)`.
    let movable = world.get::<DmItem>(target).is_some()
        || world.get::<DmObj>(target).is_some()
        || world.get::<Stat>(target).is_some();
    if !movable || target == held {
        return false;
    }
    let Some(bounty) = matching_bounty(world, spy, target) else {
        return false;
    };
    if bounty.claimed {
        crate::balloon::balloon_alert(world, spy, target, "заказ уже выполнен!");
        return true;
    }
    if world
        .get::<DoAfter>(spy)
        .is_some_and(|d| matches!(d.job, Job::Spy(_)))
    {
        crate::balloon::balloon_alert(world, spy, spy, "уже заняты сканированием!");
        return true;
    }
    start_stealing(world, client, spy, held, target, &bounty);
    true
}

/// `start_stealing` + `steal_process` up to the `do_after`.
fn start_stealing(
    world: &mut World,
    client: Entity,
    spy: Entity,
    uplink: Entity,
    stealing: Entity,
    bounty: &Bounty,
) {
    let on_turf = world.get::<TilePos>(stealing).is_some();
    let held_by_spy = world.get::<CarriedBy>(stealing).is_some_and(|c| c.0 == spy);
    let thing = name_of(world, stealing);
    if !on_turf && !held_by_spy {
        crate::tools::tell(
            world,
            client,
            Span::Warning,
            format!("Ваш аплинк мигает красным: нельзя украсть {thing} отсюда."),
        );
        return;
    }
    info!(
        "{spy} started stealing {stealing} for the spy bounty {}",
        bounty.name
    );
    if let Some(pos) = crate::explosions::turf_of(world, stealing) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, SCAN_SOUND, 33.0, true),
        );
    }
    let user = name_of(world, spy);
    if let Some(pos) = crate::explosions::turf_of(world, spy) {
        let own = [(
            spy,
            Span::Notice,
            format!("Вы начинаете сканировать {thing}, приготавливая его к отправке."),
        )];
        for m in crate::pull::visible_message(
            world,
            pos,
            ChatChannel::Warning,
            (
                Span::Warning,
                format!(
                    "{} начинает сканировать {thing} странным устройством...",
                    ss13_shared::capitalize(&user)
                ),
            ),
            &own,
        ) {
            world.write_message(m);
        }
    }
    // A held target has no tile of its own: the scan follows the spy.
    let anchor = if on_turf { stealing } else { spy };
    do_after(
        world,
        Actor {
            client,
            mob: spy,
            item: Some(uplink),
        },
        anchor,
        bounty.theft_time_ds,
        0,
        Job::Spy(SpyJob {
            bounty: bounty.id,
            stealing,
        }),
    );
}

/// The scan finished (`steal_process` after the `do_after`): checked again, the theft, the
/// claim and the reward.
pub fn complete(world: &mut World, spy: Entity, action: &DoAfter, job: SpyJob) {
    let stealing = job.stealing;
    if world.get::<Spy>(spy).is_none() || world.get_entity(stealing).is_err() {
        return;
    }
    let Some(bounty) = world
        .get_resource::<SpyBounties>()
        .and_then(|h| h.bounty(job.bounty))
        .cloned()
    else {
        return;
    };
    let thing = name_of(world, stealing);
    if bounty.claimed {
        tell_mob(
            world,
            spy,
            Span::Warning,
            format!(
                "Ваш аплинк мигает красным: награда за {thing} уже была принята другим шпионом!"
            ),
        );
        return;
    }
    if !bounty::is_stealable(world, &bounty, stealing) {
        return;
    }
    if world.get::<CarriedBy>(stealing).is_some_and(|c| c.0 == spy)
        && crate::inventory::no_drop(world, stealing)
    {
        tell_mob(
            world,
            spy,
            Span::Warning,
            format!("Ваш аплинк мигает красным: кажется, что-то прилепило вас к {thing}!"),
        );
        return;
    }
    let key = bounty.dupe_key();
    {
        let mut handler = world.resource_mut::<SpyBounties>();
        *handler.all_claimed.entry(key.clone()).or_insert(0) += 1;
        handler.claimed_last_pool.insert(key);
        if let Some(b) = handler.bounty_mut(bounty.id) {
            b.claimed = true;
        }
    }
    clean_up_stolen(world, &bounty, stealing);
    // The rewarded item's ammunition joins the pools.
    let children = world.resource::<SpyBounties>().children_of(&bounty.reward);
    world.resource_scope(|world, mut handler: Mut<SpyBounties>| {
        if let Some(catalog) = world.get_resource::<super::uplink::UplinkCatalog>() {
            for child in &children {
                handler.try_add_to_loot_pool(catalog, child);
            }
        }
    });
    let reward = super::uplink::spawn_for_generic_use(world, spy, &bounty.reward);
    let reward_name = reward
        .map(|(e, _)| name_of(world, e))
        .unwrap_or_else(|| bounty.reward_name.clone());
    let at_feet = reward.is_some_and(|(_, in_hand)| !in_hand);
    tell_mob(
        world,
        spy,
        Span::Notice,
        format!(
            "Заказ выполнен! Ваша награда - {reward_name}.{}",
            if at_feet {
                " Поищите ее у ваших ног."
            } else {
                ""
            }
        ),
    );
    if let Some(pos) = action
        .item
        .and_then(|u| crate::explosions::turf_of(world, u))
    {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, DONE_SOUND, 50.0, true),
        );
    }
    info!(
        "{spy} completed the spy bounty {} of difficulty {} by stealing {thing} for {}",
        bounty.name,
        bounty.difficulty.name(),
        bounty.reward
    );
    if let Some(mut spy) = world.get_mut::<Spy>(spy) {
        spy.bounties_claimed += 1;
        spy.all_loot.push(bounty.reward_name.clone());
    }
}

/// `clean_up_stolen_item` by kind of bounty.
fn clean_up_stolen(world: &mut World, bounty: &Bounty, stealing: Entity) {
    match &bounty.kind {
        BountyKind::Person {
            target,
            item,
            original,
        } if stealing == *target && stealing != *original => {
            // `some_item/clean_up_stolen_item` of the scanned owner.
            if let Some(pos) = crate::explosions::turf_of(world, stealing) {
                crate::effects::particles::do_sparks(world, 2, false, pos);
            }
            let name = world
                .get::<Name>(stealing)
                .map(|n| n.to_string())
                .unwrap_or_default();
            let Some(real) = bounty::find_desired(world, stealing, &name, *item) else {
                return;
            };
            crate::status_effects::adjust(
                world,
                stealing,
                crate::status_effects::Incap::Unconscious,
                10.0,
            );
            let real_name = name_of(world, real);
            tell_mob(
                world,
                stealing,
                Span::Warning,
                format!(
                    "Вы чувствуете, что чего-то не хватает там, где ранее находился ваш - {real_name}."
                ),
            );
            take_away(world, bounty, real);
        }
        BountyKind::Bot { .. } => {
            // `some_bot/finish_cleanup` (`black_market_prob = 0`).
            if world.get::<PlayerMob>(stealing).is_some() {
                tell_mob(
                    world,
                    stealing,
                    Span::Warning,
                    "Вас украли! Вас отправляют на черный рынок и разбирают на запчасти...",
                );
                crate::health::ghostize_gone(world, stealing);
            }
            take_away(world, bounty, stealing);
        }
        _ => take_away(world, bounty, stealing),
    }
}

/// The base `clean_up_stolen_item`: sparks, then the black market (`black_market_prob`, always
/// for an indestructible item) or gone; a machine is taken apart and its frame removed.
// shortcut: no half-second shrink animation; it goes at once.
fn take_away(world: &mut World, bounty: &Bounty, stealing: Entity) {
    let pos = crate::explosions::turf_of(world, stealing);
    if let Some(pos) = pos {
        crate::effects::particles::do_sparks(world, 3, false, pos);
    }
    let roll = world
        .resource_mut::<SpyBounties>()
        .rng
        .prob(bounty.black_market_prob);
    if world
        .get::<DmObj>(stealing)
        .is_some_and(|o| is_under(&o.path, MACHINERY))
    {
        // `machine/send_to_black_market` and `finish_cleanup` both leave the contents behind.
        let Some(pos) = world.get::<TilePos>(stealing).copied() else {
            crate::power::singularity::qdel(world, stealing);
            return;
        };
        let frames_before = frames_at(world, pos);
        crate::obj_damage::deconstruct(world, stealing, false);
        for frame in frames_at(world, pos) {
            if !frames_before.contains(&frame) {
                world.despawn(frame);
            }
        }
        return;
    }
    let indestructible = world
        .get::<DmItem>(stealing)
        .is_some_and(|i| i.resistance_flags & crate::obj_damage::INDESTRUCTIBLE != 0);
    if world.get::<DmItem>(stealing).is_some() && (indestructible || roll) {
        // `send_to_black_market`.
        let mut price = match bounty.difficulty {
            Difficulty::Easy => PAYCHECK_COMMAND * 5 / 2,
            Difficulty::Medium => PAYCHECK_COMMAND * 5,
            Difficulty::Hard => PAYCHECK_COMMAND * 10,
        };
        price += world
            .resource_mut::<SpyBounties>()
            .rng
            .range(0, PAYCHECK_COMMAND * 5);
        if indestructible {
            price *= 2;
        }
        if crate::cargo::black_market::add_stolen_good(world, stealing, price) {
            return;
        }
    }
    crate::power::singularity::qdel(world, stealing);
}

fn frames_at(world: &mut World, pos: TilePos) -> Vec<Entity> {
    let mut q = world.query::<(Entity, &DmObj, &TilePos)>();
    q.iter(world)
        .filter(|(_, o, p)| **p == pos && is_under(&o.path, FRAME))
        .map(|(e, ..)| e)
        .collect()
}

/// The roundend section of the spies: each spy, the thefts, the loot, the objectives.
pub fn roundend_report(world: &mut World, end: RoundEnd) -> Vec<ChatMessage> {
    let mut query = world.query::<(Entity, &Spy)>();
    let mut spies: Vec<(Entity, Spy)> = query.iter(world).map(|(e, s)| (e, s.clone())).collect();
    if spies.is_empty() {
        return Vec::new();
    }
    spies.sort_by_key(|(e, _)| *e);
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = vec![line(vec![ChatSegment::new(
        Span::Bold,
        format!("{ROUNDEND_CATEGORY} были:"),
    )])];
    for (mob, spy) in spies {
        out.push(super::traitor::print_player(world, mob, &spy.key));
        out.push(line(vec![
            ChatSegment::new(Span::Plain, " - Количество совершенных краж "),
            ChatSegment::new(Span::Bold, spy.bounties_claimed.to_string()),
            ChatSegment::new(Span::Plain, "."),
        ]));
        if spy.bounties_claimed > 0 {
            out.push(line(vec![ChatSegment::new(
                Span::Plain,
                format!(
                    " - Получены следующие награды: {}",
                    crate::clothing::accessories::english_list(&spy.all_loot)
                ),
            )]));
        }
        for (i, objective) in spy.objectives.iter().enumerate() {
            let mut segments = vec![
                ChatSegment::new(Span::Bold, format!("{OBJECTIVE_NAME} #{}", i + 1)),
                ChatSegment::new(Span::Plain, format!(": {} ", objective.explanation)),
            ];
            // `no_failure`: the custom objectives and the spy's protect objective.
            if !matches!(
                objective.kind,
                ObjectiveKind::Custom | ObjectiveKind::Protect { .. }
            ) {
                segments.push(if objective.check_completion(world, &[mob], end) {
                    ChatSegment::new(Span::GreenText, "Успех!")
                } else {
                    ChatSegment::new(Span::RedText, "Неудача.")
                });
            }
            out.push(line(segments));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objective_lines_fill_every_pick() {
        for seed in 0..200 {
            let mut rng = Rng::new(seed);
            let line = pick_objective_line(&mut rng, "objective_body");
            assert!(!line.is_empty());
            assert!(!line.contains("@pick"), "{line}");
        }
    }

    #[test]
    fn spies_get_flavour_and_an_ending() {
        let mut world = World::new();
        let mob = world.spawn((Stat::Conscious, Name::new("Агент"))).id();
        for seed in 0..50 {
            world.insert_resource(DynamicRng(Rng::new(seed)));
            let objectives = give_random_objectives(&mut world, mob);
            let custom = objectives
                .iter()
                .filter(|o| o.kind == ObjectiveKind::Custom)
                .count();
            assert!((1..=3).contains(&custom), "{objectives:?}");
            assert!(matches!(
                objectives.last().unwrap().kind,
                ObjectiveKind::Escape
                    | ObjectiveKind::Martyr
                    | ObjectiveKind::Hijack
                    | ObjectiveKind::Exile
            ));
        }
    }

    #[test]
    fn report_counts_thefts_and_skips_no_failure_verdicts() {
        let mut world = World::new();
        let mob = world.spawn((Stat::Conscious, Name::new("Агент"))).id();
        world.entity_mut(mob).insert(Spy {
            key: "p0".into(),
            objectives: vec![
                Objective {
                    kind: ObjectiveKind::Custom,
                    explanation: "Украдите всё.".into(),
                },
                Objective {
                    kind: ObjectiveKind::Martyr,
                    explanation: "Умрите славной смертью!".into(),
                },
            ],
            uplink: None,
            uplink_location: None,
            bounties_claimed: 2,
            all_loot: vec!["Megaphone".into(), "Combat Gloves".into()],
        });
        let report = roundend_report(&mut world, RoundEnd { force_ending: true });
        let text: Vec<String> = report.iter().map(ChatMessage::plain_text).collect();
        assert_eq!(text[0], "Шпионы были:");
        assert_eq!(text[2], " - Количество совершенных краж 2.");
        assert_eq!(
            text[3],
            " - Получены следующие награды: Megaphone и Combat Gloves"
        );
        assert_eq!(text[4], "Задача #1: Украдите всё. ");
        assert_eq!(text[5], "Задача #2: Умрите славной смертью! Неудача.");
    }
}

//! BandaStation's cryopods and the cryo console (`modular_bandastation/cryosleep`).
//!
//! - A mob dragged onto an open pod ([`Cryopod`], on the shared [`crate::medical::pods::Pod`]) is
//!   asked to confirm (twice for important jobs, a third time for antagonists) and goes in after
//!   5 s. A body with a player stays until the player leaves it; then, or if it had no player, it
//!   leaves the round after [`DESPAWN_SECS`] unless the player comes back.
//! - Another mob can be put in only when its player left it [`CryoConfig::min_ssd_secs`] ago.
//! - Leaving the round: what it carries (not `NoDrop`) goes to the console of the area, or to the
//!   floor without one; the console lists the crew member and announces it on the common channel,
//!   and for a head of staff on the department's and the command channels; the manifest forgets
//!   the record. Despawning the body frees the job slot ([`crate::jobs`]'s `free_slot`).
//! - The console's window ([`CryopodConsoleUi`]) lists the crew and hands stored items out to
//!   holders of command or armory access. tg only hides the list in the window; the access is
//!   checked here at the moment of effect.
//! - The AI leaves through a verb (`ai_cryo`): the core is replaced by an empty one.
// shortcut: balloon alerts are chat lines; no flick animations, gender declension ("был
// перемещён"), PDA messenger hiding, `send_to_cryo` admin button; the nuclear disk lands on the
// console's tile instead of an adjacent open turf; `COMSIG_GLOB_MIND_ENTERED_CRYO` has no
// listeners yet (objective replacement, docs/parity/antag). Add with those systems.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// The console window (`CryopodConsole.jsx`).
pub struct CryopodConsoleUi;

impl UiInterface for CryopodConsoleUi {
    const NAME: &'static str = "CryopodConsole";
    type Data = CryopodConsoleData;
    type StaticData = ();
}

/// `act('item_get', { item_get: ref })`.
pub const ITEM_GET: &str = "item_get";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CryopodConsoleData {
    pub frozen_crew: Vec<FrozenCrew>,
    pub items: Vec<FrozenItem>,
    pub item_retrieval_allowed: bool,
    pub account_name: Option<String>,
}

/// One line of `frozen_crew`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FrozenCrew {
    pub name: String,
    pub job: String,
}

/// One stored item: the act's id and its name.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FrozenItem {
    pub id: String,
    pub name: String,
}

#[cfg(feature = "server")]
pub use server::*;

#[cfg(feature = "server")]
mod server {
    use bevy::ecs::message::{MessageCursor, Messages};
    use bevy::ecs::system::SystemParamItem;
    use bevy::prelude::*;
    use ss13_chat::radio::{
        FREQ_COMMAND, FREQ_COMMON, FREQ_ENGINEERING, FREQ_MEDICAL, FREQ_SCIENCE, FREQ_SECURITY,
        FREQ_SERVICE, FREQ_SUPPLY,
    };
    use ss13_mapload::{Areas, DmItem, DmObj};
    use ss13_shared::TilePos;
    use ss13_shared::chat::Span;
    use ss13_tgui::interfaces::ru_jobs::job_ru;
    use ss13_tgui::prompt::{
        Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world,
    };
    use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
    use ss13_world::Density;

    use super::*;
    use crate::closets::InCloset;
    use crate::ghost::{Ghost, ghostize_client};
    use crate::health::Stat;
    use crate::human::HumanLook;
    use crate::inventory::{ALL_SLOTS, IdCards, Inventory, check_access, no_drop, put_down};
    use crate::jobs::{Jobs, PlayerJob, display_time_text};
    use crate::medical::pods::{Pod, close_pod, occupant, open_pod, operational};
    use crate::medical::surgery::Buckled;
    use crate::medical::{name_of, visible};
    use crate::obj_damage::{is_under, set_icon_state};
    use crate::outfit::IdRegistration;
    use crate::records::manifest::Manifest;
    use crate::server::{PlayerMob, Players};
    use crate::silicon::ai::AiCore;
    use crate::silicon::cyborg::Cyborg;
    use crate::silicon::interact::{SiliconUsers, has_silicon_access};
    use crate::ssd::LoggedOutAt;
    use crate::statpanel::{AI_TAB, AI_VERB_UNLOAD, StatPanelHost, StatPanelUi};
    use crate::storage::{StoredIn, put_inside, take_out};
    use crate::tools::{tell, tell_mob};
    use crate::votes::AdminNotice;

    pub const CRYOPOD_ROOT: &str = "/obj/machinery/cryopod";
    pub const CONSOLE_PATH: &str = "/obj/machinery/computer/cryopod";
    const PRISON_POD: &str = "/obj/machinery/cryopod/prison";
    const SYNDICATE_POD: &str = "/obj/machinery/cryopod/syndicate";
    const NUKE_DISK: &str = "/obj/item/disk/nuclear";
    const LATEJOIN_CORE: &str = "/obj/structure/ai_core/latejoin_inactive";
    /// `time_till_despawn`.
    pub const DESPAWN_SECS: f64 = 30.0;
    /// `place_inside`'s `do_after`.
    pub const ENTER_SECS: f64 = 5.0;
    /// `req_admin_notify` jobs.
    const IMPORTANT_JOBS: [&str; 7] = [
        "Captain",
        "Head of Security",
        "Head of Personnel",
        "Chief Engineer",
        "Chief Medical Officer",
        "Research Director",
        "AI",
    ];
    /// `default_radio_channel` of the heads of staff.
    const JOB_RADIO: [(&str, u16); 7] = [
        ("Chief Medical Officer", FREQ_MEDICAL),
        ("Chief Engineer", FREQ_ENGINEERING),
        ("Head of Security", FREQ_SECURITY),
        ("Head of Personnel", FREQ_SERVICE),
        ("Research Director", FREQ_SCIENCE),
        ("Quartermaster", FREQ_SUPPLY),
        ("Captain", FREQ_COMMAND),
    ];

    /// `cryo_min_ssd_time`, from the environment (seconds) as the other config resources.
    #[derive(Resource, Clone, Debug, PartialEq)]
    pub struct CryoConfig {
        pub min_ssd_secs: f64,
    }

    impl Default for CryoConfig {
        fn default() -> Self {
            Self {
                min_ssd_secs: std::env::var("SS13_CRYO_MIN_SSD_SECONDS")
                    .ok()
                    .and_then(|v| v.trim().parse::<f64>().ok())
                    .filter(|v| v.is_finite() && *v >= 0.0)
                    .unwrap_or(1800.0),
            }
        }
    }

    /// `/obj/machinery/cryopod` and its prison and syndicate subtypes.
    #[derive(Component, Clone, Debug, PartialEq)]
    pub struct Cryopod {
        pub open_state: &'static str,
        pub closed_state: &'static str,
        /// `prison`: wall-mounted, never dense.
        pub prison: bool,
        /// When the player-less occupant leaves the round (game seconds).
        pub despawn_at: Option<f64>,
        /// A `do_after` in progress.
        pub entering: Option<Entering>,
        /// `src::name`, kept while the name carries the occupant.
        base_name: Option<String>,
        /// Examine's "Внутри находится ...".
        occupant_name: Option<String>,
    }

    impl Cryopod {
        pub fn of_path(path: &str) -> Self {
            let (open_state, closed_state, prison) = if is_under(path, PRISON_POD) {
                ("prisonpod", "prisonpod", true)
            } else if is_under(path, SYNDICATE_POD) {
                ("cryopod-open-s", "cryopod-s", false)
            } else {
                ("cryopod-open", "cryopod", false)
            };
            Self {
                open_state,
                closed_state,
                prison,
                despawn_at: None,
                entering: None,
                base_name: None,
                occupant_name: None,
            }
        }
    }

    /// `place_inside`'s `do_after`: cancelled when either mob moves.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Entering {
        pub user: Entity,
        pub target: Entity,
        pub ends: f64,
        user_pos: TilePos,
        target_pos: TilePos,
    }

    /// `/obj/machinery/computer/cryopod`.
    #[derive(Component, Clone, Debug, Default, PartialEq)]
    pub struct CryoConsole {
        pub crew: Vec<FrozenCrew>,
        pub items: Vec<(Entity, String)>,
    }

    /// The confirmation an open prompt is for (`tgui_alert`s of `put_self_inside`,
    /// `put_other_inside`).
    #[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
    struct CryoAsk {
        pod: Entity,
        user: Entity,
        target: Entity,
        stage: Stage,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Stage {
        SelfConfirm,
        SelfImportant,
        SelfAntag,
        OtherConfirm,
        OtherImportant,
    }

    /// The "AI Unload" confirmation.
    #[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
    struct AiUnloadAsk {
        client: Entity,
        ai: Entity,
    }

    pub struct CryosleepPlugin;

    impl Plugin for CryosleepPlugin {
        fn build(&self, app: &mut App) {
            use crate::click_handlers::{AddClickHandler, ClickPhase};
            app.add_click_handler(ClickPhase::EmptyHand, 2653, |w, c| {
                interact(w, c.client, c.mob, c.target)
            })
            .register_ui::<CryopodConsoleUi>()
            .init_resource::<CryoConfig>()
            .add_observer(attach_console)
            .add_systems(Update, tick.run_if(any_with_component::<Cryopod>))
            .add_systems(
                Update,
                (
                    answers.after(PromptSystems::Answer),
                    retrieve_items,
                    ai_verbs,
                )
                    .in_set(UiSystems::Handle),
            );
        }
    }

    fn attach_console(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
        if objs
            .get(add.entity)
            .is_ok_and(|o| is_under(&o.path, CONSOLE_PATH))
        {
            commands
                .entity(add.entity)
                .insert_if_new(CryoConsole::default());
        }
    }

    /// `update_icon_state`.
    pub fn refresh_look(world: &mut World, pod: Entity) {
        let Some(c) = world.get::<Cryopod>(pod) else {
            return;
        };
        let open = world.get::<Pod>(pod).is_some_and(|p| p.open);
        let state = if open { c.open_state } else { c.closed_state };
        set_icon_state(world, pod, state);
    }

    /// `examine`.
    pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
        if let Some(name) = world
            .get::<Cryopod>(target)
            .and_then(|c| c.occupant_name.as_ref())
        {
            lines.push((Span::Notice, format!("Внутри находится {name}.")));
        }
    }

    fn set_occupant_name(world: &mut World, pod: Entity, occupant: Option<String>) {
        let current = name_of(world, pod);
        let Some(mut c) = world.get_mut::<Cryopod>(pod) else {
            return;
        };
        let base = c.base_name.get_or_insert(current).clone();
        c.occupant_name.clone_from(&occupant);
        let name = occupant.map_or_else(|| base.clone(), |o| format!("{base} ({o})"));
        world.entity_mut(pod).insert(Name::new(name));
    }

    /// Whether `mob` holds a `req_admin_notify` job.
    fn important_job(world: &World, mob: Entity) -> bool {
        job_title(world, mob).is_some_and(|t| IMPORTANT_JOBS.contains(&t.as_str()))
    }

    /// tg `mind.get_special_roles()` non-empty.
    fn is_antag(world: &World, mob: Entity) -> bool {
        world.get::<crate::antag::AntagHudData>(mob).is_some()
    }

    fn job_title(world: &World, mob: Entity) -> Option<String> {
        let &PlayerJob(job) = world.get::<PlayerJob>(mob)?;
        Some(
            world
                .get_resource::<Jobs>()?
                .data
                .jobs
                .get(job)?
                .title
                .clone(),
        )
    }

    /// `validate_put_inside`: the reason why not.
    fn validate(world: &mut World, pod: Entity, target: Entity) -> Option<&'static str> {
        if world.get::<HumanLook>(target).is_none() && world.get::<Cyborg>(target).is_none() {
            return Some("только люди и киборги");
        }
        if occupant(world, pod).is_some() {
            return Some("внутри уже кто-то есть");
        }
        if !world.get::<Pod>(pod).is_some_and(|p| p.open) {
            return Some("должен быть открыт");
        }
        if world.get::<Stat>(target) == Some(&Stat::Dead) {
            return Some("существо мертво");
        }
        None
    }

    fn admins(world: &mut World, text: String) {
        world.write_message(AdminNotice(text));
    }

    /// `mouse_drop_receive`: `dragged` onto the open pod, the check of `put_mob_inside`.
    pub fn mouse_drop_receive(world: &mut World, user: Entity, pod: Entity, dragged: Entity) {
        let Some(client) = world.get::<PlayerMob>(user).map(|p| p.client) else {
            return;
        };
        if let Some(reason) = validate(world, pod, dragged) {
            tell(world, client, Span::Warning, reason);
            return;
        }
        let time = display_time_text(DESPAWN_SECS);
        if dragged == user {
            if world.get::<Buckled>(user).is_some() {
                tell(world, client, Span::Warning, "Вы к чему-то пристегнуты.");
                return;
            }
            ask(
                world,
                client,
                CryoAsk {
                    pod,
                    user,
                    target: dragged,
                    stage: Stage::SelfConfirm,
                },
                format!(
                    "Вы уверены что хотите погрузиться в криогенный стазис? Через {time} после \
                     выхода из игры или тела ваш персонаж будет удален и больше не сможет \
                     вернуться в этом раунде."
                ),
            );
            return;
        }
        if world.get::<PlayerMob>(dragged).is_some() {
            tell(world, client, Span::Warning, "У существа есть душа.");
            let (who, whom) = (name_of(world, user), name_of(world, dragged));
            admins(
                world,
                format!("{who} пытается переместить в криопод {whom} с подключённым игроком."),
            );
            return;
        }
        let min_ssd = world.resource::<CryoConfig>().min_ssd_secs;
        let logged_out = world.get::<LoggedOutAt>(dragged).map_or(0.0, |l| l.0);
        if logged_out + min_ssd > crate::tools::now(world) {
            tell(world, client, Span::Warning, "Слишком недавно в SSD.");
            return;
        }
        if world.get::<Buckled>(dragged).is_some() {
            let name = name_of(world, dragged);
            tell(
                world,
                client,
                Span::Warning,
                format!("Тело {name} к чему-то пристегнуто."),
            );
            return;
        }
        let name = name_of(world, dragged);
        ask(
            world,
            client,
            CryoAsk {
                pod,
                user,
                target: dragged,
                stage: Stage::OtherConfirm,
            },
            format!(
                "Вы уверены что хотите переместить {name} в криогенный стазис? Через {time} \
                 персонаж будет удален из игры и больше не сможет вернуться в этом раунде."
            ),
        );
    }

    fn ask(world: &mut World, client: Entity, what: CryoAsk, message: String) {
        let self_stage = matches!(
            what.stage,
            Stage::SelfConfirm | Stage::SelfImportant | Stage::SelfAntag
        );
        let title = if self_stage {
            "Погрузиться в криогенный стазис?".to_owned()
        } else {
            format!(
                "Поместить {} в криогенный стазис?",
                name_of(world, what.target)
            )
        };
        let prompt = ask_world(
            world,
            Ask::new(
                client,
                what.user,
                title,
                message,
                PromptKind::Alert {
                    buttons: vec!["Да".to_owned(), "Нет".to_owned()],
                    autofocus: false,
                },
            ),
        );
        world.entity_mut(prompt).insert(what);
    }

    /// The stage after a "Да": the next `tgui_alert`, else `place_inside`.
    fn advance(world: &mut World, client: Entity, what: CryoAsk) {
        let important = important_job(world, what.target);
        let antag = is_antag(world, what.target);
        let name = name_of(world, what.target);
        let next = match what.stage {
            Stage::SelfConfirm if important => Some((
                Stage::SelfImportant,
                "Вы ТОЧНО уверены что хотите погрузиться в криогенный стазис? Вы занимаете важную \
                 для раунда профессию."
                    .to_owned(),
            )),
            Stage::SelfConfirm | Stage::SelfImportant if antag => Some((
                Stage::SelfAntag,
                "Вы ТОЧНО уверены что хотите погрузиться в криогенный стазис? Вы играете на \
                 специальной роли."
                    .to_owned(),
            )),
            Stage::OtherConfirm if important => Some((
                Stage::OtherImportant,
                format!(
                    "Вы ТОЧНО уверены что хотите переместить {name} в криогенный стазис? Он \
                     занимает важную для раунда профессию."
                ),
            )),
            _ => None,
        };
        if let Some((stage, message)) = next {
            ask(world, client, CryoAsk { stage, ..what }, message);
            return;
        }
        if important {
            admins(
                world,
                format!("{name} с важной профессией отправляется в криопод."),
            );
        }
        if antag {
            admins(
                world,
                format!("{name} со специальной ролью отправляется в криопод."),
            );
        }
        place_inside(world, what);
    }

    /// `place_inside`: the `do_after`, checked again when it ends ([`tick`]).
    fn place_inside(world: &mut World, what: CryoAsk) {
        let (Some(&user_pos), Some(&target_pos)) = (
            world.get::<TilePos>(what.user),
            world.get::<TilePos>(what.target),
        ) else {
            return;
        };
        let now = crate::tools::now(world);
        if let Some(mut c) = world.get_mut::<Cryopod>(what.pod)
            && c.entering.is_none()
        {
            c.entering = Some(Entering {
                user: what.user,
                target: what.target,
                ends: now + ENTER_SECS,
                user_pos,
                target_pos,
            });
        }
    }

    fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
        let Some(messages) = world.get_resource::<Messages<PromptAnswered>>() else {
            return;
        };
        let pending: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
            .read(messages)
            .map(|a| (a.prompt, a.client, a.answer.clone()))
            .collect();
        for (prompt, client, answer) in pending {
            let yes = matches!(answer, Some(PromptAnswer::Button(0)));
            if let Some(unload) = world.get::<AiUnloadAsk>(prompt).copied() {
                if yes {
                    ai_unload(world, unload.client, unload.ai);
                }
                continue;
            }
            let Some(what) = world.get::<CryoAsk>(prompt).copied() else {
                continue;
            };
            // The prompts took a while: the user is still that player and the pod still free.
            if !yes
                || world.get::<PlayerMob>(what.user).map(|p| p.client) != Some(client)
                || world.get_entity(what.target).is_err()
                || validate(world, what.pod, what.target).is_some()
            {
                continue;
            }
            // `put_other_inside`: the body's player may have come back.
            if what.user != what.target && world.get::<PlayerMob>(what.target).is_some() {
                continue;
            }
            advance(world, client, what);
        }
    }

    /// The `do_after`s that ended, and every closed pod's occupant (`on_occupant_death`, the
    /// despawn timer that runs while the occupant has no player).
    // Cost: per tick a pass over the pods (a handful per map); closed ones also scan `InCloset`.
    fn tick(world: &mut World) {
        let now = crate::tools::now(world);
        let mut pods = world.query::<(Entity, &Pod, &Cryopod)>();
        let list: Vec<(Entity, bool, Option<Entering>, Option<f64>)> = pods
            .iter(world)
            .map(|(e, p, c)| (e, p.open, c.entering, c.despawn_at))
            .collect();
        for (pod, open, entering, despawn_at) in list {
            if let Some(entering) = entering
                && now >= entering.ends
            {
                if let Some(mut c) = world.get_mut::<Cryopod>(pod) {
                    c.entering = None;
                }
                finish_entering(world, pod, entering);
            }
            if open {
                continue;
            }
            let Some(mob) = occupant(world, pod) else {
                open_pod_back(world, pod);
                continue;
            };
            if world.get::<Stat>(mob) == Some(&Stat::Dead) {
                open_pod_back(world, pod);
                continue;
            }
            let has_player = world.get::<PlayerMob>(mob).is_some();
            let next = match (has_player, despawn_at) {
                (true, _) => None,
                (false, None) => Some(now + DESPAWN_SECS),
                (false, Some(at)) if now >= at => {
                    despawn_occupant(world, pod, mob);
                    continue;
                }
                (false, Some(at)) => Some(at),
            };
            if let Some(mut c) = world.get_mut::<Cryopod>(pod) {
                c.despawn_at = next;
            }
        }
    }

    fn finish_entering(world: &mut World, pod: Entity, e: Entering) {
        let moved = world.get::<TilePos>(e.user) != Some(&e.user_pos)
            || world.get::<TilePos>(e.target) != Some(&e.target_pos);
        if moved || validate(world, pod, e.target).is_some() {
            return;
        }
        // The body's player came back during the `do_after`.
        if e.user != e.target && world.get::<PlayerMob>(e.target).is_some() {
            return;
        }
        if world.get::<Buckled>(e.target).is_some() || world.get::<InCloset>(e.target).is_some() {
            return;
        }
        let Some(mob) = close_pod(world, pod, Some(e.target)) else {
            return;
        };
        if world.get::<Cryopod>(pod).is_some_and(|c| c.prison) {
            world.entity_mut(pod).remove::<Density>();
        }
        let name = name_of(world, mob);
        set_occupant_name(world, pod, Some(name));
        tell_mob(
            world,
            mob,
            Span::Notice,
            "Вы чувствуете, как холодный воздух обволакивает вас. Чувства затухают и ваше тело \
             немеет.",
        );
    }

    /// `open_machine(density_to_set = TRUE)`; a prison pod stays undense.
    pub fn open_pod_back(world: &mut World, pod: Entity) {
        open_pod(world, pod);
        let prison = world.get::<Cryopod>(pod).is_some_and(|c| c.prison);
        if !prison {
            world.entity_mut(pod).insert(Density);
        }
        set_occupant_name(world, pod, None);
        if let Some(mut c) = world.get_mut::<Cryopod>(pod) {
            c.despawn_at = None;
        }
    }

    /// `container_resist_act`: the occupant climbs out.
    pub fn resist(world: &mut World, mob: Entity, pod: Entity) {
        let (who, what) = (name_of(world, mob), name_of(world, pod));
        visible(
            world,
            mob,
            &format!("Вы вылезаете из {what}!"),
            &format!("{who} вылезает из {what}!"),
            Span::Notice,
        );
        open_pod_back(world, pod);
    }

    /// `get_linked_control_computer`: the console of the pod's area.
    fn find_console(world: &mut World, pod: Entity) -> Option<Entity> {
        let &pos = world.get::<TilePos>(pod)?;
        let area = world.resource::<Areas>().area_at(pos)?;
        let mut q = world.query_filtered::<(Entity, &TilePos), With<CryoConsole>>();
        q.iter(world)
            .find(|(_, p)| world.resource::<Areas>().area_at(**p) == Some(area))
            .map(|(e, _)| e)
    }

    /// `handle_occupant_items`: everything but `NoDrop` to the console, else to the floor.
    pub fn store_items(world: &mut World, mob: Entity, console: Option<Entity>, pos: TilePos) {
        let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
            return;
        };
        let slots: Vec<_> = ALL_SLOTS
            .iter()
            .filter_map(|&s| inventory.slot(s).map(|item| (s, item)))
            .collect();
        for (slot, item) in slots {
            if no_drop(world, item) {
                continue;
            }
            if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
                *inventory.slot_mut(slot) = None;
            }
            match console.filter(|&c| world.get::<CryoConsole>(c).is_some()) {
                Some(console) => {
                    put_inside(world, item, console);
                    let name = name_of(world, item);
                    if let Some(mut c) = world.get_mut::<CryoConsole>(console) {
                        c.items.push((item, name));
                    }
                }
                None => put_down(world, item, pos),
            }
        }
    }

    /// The nuclear disk does not stay in storage (`handle_occupant_items`).
    fn eject_disk(world: &mut World, console: Entity) {
        let Some(&pos) = world.get::<TilePos>(console) else {
            return;
        };
        let mut q = world.query::<(Entity, &StoredIn)>();
        let stored: Vec<(Entity, Entity)> = q.iter(world).map(|(e, s)| (e, s.0)).collect();
        let mut inside: Vec<Entity> = vec![console];
        let mut disk = None;
        while let Some(container) = inside.pop() {
            for &(item, _) in stored.iter().filter(|(_, c)| *c == container) {
                inside.push(item);
                if world
                    .get::<DmItem>(item)
                    .is_some_and(|i| is_under(&i.path, NUKE_DISK))
                {
                    disk = Some(item);
                }
            }
        }
        if let Some(disk) = disk {
            take_out(world, disk, pos);
            if let Some(mut c) = world.get_mut::<CryoConsole>(console) {
                c.items.retain(|(item, _)| *item != disk);
            }
        }
    }

    /// `announce`: the frequencies of the "moved to cryo" line for this job.
    pub fn radio_freqs(title: Option<&str>, command: bool) -> Vec<u16> {
        let job_radio =
            title.and_then(|t| JOB_RADIO.iter().find(|(j, _)| *j == t).map(|(_, f)| *f));
        let mut freqs = Vec::new();
        if let Some(freq) = job_radio {
            if command && freq != FREQ_COMMAND {
                freqs.push(FREQ_COMMAND);
            }
            freqs.push(freq);
        }
        freqs.push(FREQ_COMMON);
        freqs
    }

    /// `on_crewmember_frozen`: the list and the announcement.
    fn crewmember_frozen(world: &mut World, console: Entity, name: &str, title: Option<&str>) {
        let rank = title.map_or_else(
            || "Неизвестная должность".to_owned(),
            |t| job_ru(t).map_or_else(|| t.to_owned(), str::to_owned),
        );
        if let Some(mut c) = world.get_mut::<CryoConsole>(console) {
            c.crew.push(FrozenCrew {
                name: name.to_owned(),
                job: rank.clone(),
            });
        }
        let Some(z) = world.get::<TilePos>(console).map(|p| p.z) else {
            return;
        };
        let command = title.is_some_and(|t| {
            world
                .get_resource::<Jobs>()
                .and_then(|j| j.data.jobs.iter().find(|job| job.title == t))
                .is_some_and(|job| job.command)
        });
        let speaker = name_of(world, console);
        let text = format!("{name}, {rank}, был перемещён в криогенное хранилище.");
        for freq in radio_freqs(title, command) {
            world.write_message(crate::radio::MachineRadio {
                freq,
                z,
                name: speaker.clone(),
                text: text.clone(),
            });
        }
    }

    /// `despawn_occupant`: cannot be undone.
    fn despawn_occupant(world: &mut World, pod: Entity, mob: Entity) {
        let Some(&pos) = world.get::<TilePos>(pod) else {
            return;
        };
        let console = find_console(world, pod);
        if world.get::<Cyborg>(mob).is_none() {
            store_items(world, mob, console, pos);
        }
        let name = name_of(world, mob);
        let title = job_title(world, mob);
        if let Some(console) = console {
            eject_disk(world, console);
            crewmember_frozen(world, console, &name, title.as_deref());
        }
        let what = name_of(world, pod);
        visible(
            world,
            pod,
            "",
            &format!("{what} гудит и шипит, перемещая {name} в хранилище."),
            Span::Notice,
        );
        if let Some(mut manifest) = world.get_resource_mut::<Manifest>() {
            manifest.general.retain(|r| r.name != name);
        }
        let mut ghosts = world.query::<&mut Ghost>();
        for mut ghost in ghosts.iter_mut(world) {
            if ghost.body == Some(mob) {
                ghost.body = None;
            }
        }
        // Despawning the body frees its `PlayerJob` slot.
        world.despawn(mob);
        open_pod_back(world, pod);
    }

    /// `interact`: an empty hand opens the working console's window.
    pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
        if world.get::<CryoConsole>(target).is_none() {
            return false;
        }
        if operational(world, target) {
            let title = name_of(world, target);
            world.write_message(OpenUi::new::<CryopodConsoleUi>(client, mob, target, title));
        }
        true
    }

    fn access_list() -> [String; 2] {
        ["command".to_owned(), "armory".to_owned()]
    }

    fn allowed(world: &World, mob: Entity) -> bool {
        has_silicon_access(world, mob)
            || crate::inventory::id_allowed(world, mob, &[], &access_list())
    }

    /// `ui_act("item_get")`: tg checks only that the item is stored; the access is checked here.
    fn retrieve_items(
        world: &mut World,
        mut acts: Local<MessageCursor<UiActed<CryopodConsoleUi>>>,
    ) {
        let pending: Vec<(Entity, Entity, String)> = acts
            .read(world.resource::<Messages<UiActed<CryopodConsoleUi>>>())
            .map(|a| (a.source, a.actor, a.act.clone()))
            .collect();
        for (console, actor, wanted) in pending {
            if !allowed(world, actor) {
                continue;
            }
            let Some(&pos) = world.get::<TilePos>(console) else {
                continue;
            };
            let Some((item, item_name)) = world.get::<CryoConsole>(console).and_then(|c| {
                c.items
                    .iter()
                    .find(|(item, _)| item.to_bits().to_string() == wanted)
                    .cloned()
            }) else {
                continue;
            };
            if let Some(mut c) = world.get_mut::<CryoConsole>(console) {
                c.items.retain(|(i, _)| *i != item);
            }
            take_out(world, item, pos);
            let name = name_of(world, console);
            visible(
                world,
                console,
                "",
                &format!("{name} выдаёт {item_name}."),
                Span::Notice,
            );
            admins(
                world,
                format!("Из криохранилища извлечён предмет: {item_name}."),
            );
        }
    }

    /// `/mob/living/silicon/ai/ai_cryo`, the statpanel verb: asks, then [`ai_unload`].
    fn ai_verbs(world: &mut World, mut acts: Local<MessageCursor<UiActed<StatPanelUi>>>) {
        let pending: Vec<(Entity, Entity)> = acts
            .read(world.resource::<Messages<UiActed<StatPanelUi>>>())
            .filter(|a| a.act.tab == AI_TAB && a.act.name == AI_VERB_UNLOAD)
            .map(|a| (a.client, a.source))
            .collect();
        for (client, source) in pending {
            if world.get::<StatPanelHost>(source).map(|h| h.client) != Some(client) {
                continue;
            }
            let Some(ai) = world.resource::<Players>().mob(client) else {
                continue;
            };
            if world.get::<AiCore>(ai).is_none()
                || crate::status_effects::incapacitation(world, ai).incapacitated
            {
                continue;
            }
            let prompt = ask_world(
                world,
                Ask::new(
                    client,
                    ai,
                    "Выгрузиться на сервер хранения",
                    "Вы точно хотите выгрузиться из ядра ИИ и покинуть раунд?",
                    PromptKind::Alert {
                        buttons: vec!["Да".to_owned(), "Нет".to_owned()],
                        autofocus: false,
                    },
                ),
            );
            world.entity_mut(prompt).insert(AiUnloadAsk { client, ai });
        }
    }

    /// The effect of `ai_cryo`, checked again after the alert.
    fn ai_unload(world: &mut World, client: Entity, ai: Entity) {
        if world.resource::<Players>().mob(client) != Some(ai)
            || world.get::<AiCore>(ai).is_none()
            || crate::status_effects::incapacitation(world, ai).incapacitated
        {
            return;
        }
        let Some(&pos) = world.get::<TilePos>(ai) else {
            return;
        };
        let Some(ghost) = ghostize_client(world, client) else {
            tell(world, client, Span::Warning, "Не удаётся выгрузить ядро.");
            return;
        };
        if let Some(mut g) = world.get_mut::<Ghost>(ghost) {
            g.body = None;
        }
        crate::security_level::minor_announce(
            world,
            "Станционный ИИ",
            "Станционный ИИ был отключён от внутренних систем и был перемещён на хранение. \
             Производится подготовка для загрузки нового ИИ.",
            false,
        );
        crate::construction::spawn_obj(world, LATEJOIN_CORE, pos);
        // Despawning the core frees its `PlayerJob` slot.
        world.despawn(ai);
    }

    impl ServerUi for CryopodConsoleUi {
        type Source = &'static CryoConsole;
        type UserParam = (
            Query<'static, 'static, &'static Inventory>,
            IdCards<'static, 'static>,
            Query<'static, 'static, &'static IdRegistration>,
            SiliconUsers<'static, 'static>,
        );
        type Act = String;

        fn static_data(_: &CryoConsole) {}

        fn data(
            console: &CryoConsole,
            user: Entity,
            params: &SystemParamItem<'_, '_, Self::UserParam>,
        ) -> CryopodConsoleData {
            let (inventories, cards, registrations, silicons) = params;
            let inventory = inventories.get(user).ok();
            let access: &[String] = inventory.and_then(|i| i.access(cards)).unwrap_or(&[]);
            let account_name = inventory
                .and_then(|i| i.id_search().find_map(|item| cards.id_of(item)))
                .and_then(|card| registrations.get(card).ok())
                .map(|r| r.registered_name.clone());
            CryopodConsoleData {
                frozen_crew: console.crew.clone(),
                items: console
                    .items
                    .iter()
                    .map(|(item, name)| FrozenItem {
                        id: item.to_bits().to_string(),
                        name: name.clone(),
                    })
                    .collect(),
                item_retrieval_allowed: silicons.contains(user)
                    || check_access(&[], &access_list(), access),
                account_name,
            }
        }

        fn parse_act(action: &str, params: &UiParams) -> Result<String, UiReject> {
            match action {
                ITEM_GET => params.text(ITEM_GET),
                _ => Err(UiReject::UnknownAction),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::storage::StoredIn;

        #[test]
        fn subtypes_have_their_sprites_and_density() {
            assert_eq!(Cryopod::of_path(CRYOPOD_ROOT).open_state, "cryopod-open");
            let prison = Cryopod::of_path(PRISON_POD);
            assert!(prison.prison);
            assert_eq!(prison.closed_state, "prisonpod");
            assert_eq!(Cryopod::of_path(SYNDICATE_POD).closed_state, "cryopod-s");
        }

        #[test]
        fn heads_are_announced_on_their_channels() {
            assert_eq!(
                radio_freqs(Some("Chief Engineer"), true),
                vec![FREQ_COMMAND, FREQ_ENGINEERING, FREQ_COMMON]
            );
            assert_eq!(
                radio_freqs(Some("Captain"), true),
                vec![FREQ_COMMAND, FREQ_COMMON]
            );
            assert_eq!(radio_freqs(Some("Assistant"), false), vec![FREQ_COMMON]);
            assert_eq!(radio_freqs(None, false), vec![FREQ_COMMON]);
        }

        fn world_with_mob() -> (World, Entity, Entity, Entity) {
            let mut world = World::new();
            let mob = world.spawn_empty().id();
            let (item, kept) = (
                world
                    .spawn((crate::inventory::CarriedBy(mob), Name::new("ящик")))
                    .id(),
                world
                    .spawn((crate::inventory::CarriedBy(mob), crate::inventory::NoDrop))
                    .id(),
            );
            let mut inventory = Inventory::default();
            inventory.hands = [Some(item), Some(kept)];
            world.entity_mut(mob).insert(inventory);
            let console = world.spawn(CryoConsole::default()).id();
            (world, mob, item, console)
        }

        #[test]
        fn stored_items_outlive_the_despawned_body() {
            let (mut world, mob, item, console) = world_with_mob();
            let pos = TilePos { x: 0, y: 0, z: 0 };
            store_items(&mut world, mob, Some(console), pos);
            assert_eq!(world.get::<StoredIn>(item).map(|s| s.0), Some(console));
            let kept = world.get::<Inventory>(mob).unwrap().hands[1].unwrap();
            assert_eq!(world.get::<CryoConsole>(console).unwrap().items.len(), 1);
            world.despawn(mob);
            // The item went with the console, the `NoDrop` one with its owner.
            assert!(world.get_entity(item).is_ok());
            assert!(world.get_entity(kept).is_err());
        }

        #[test]
        fn without_a_console_items_fall_to_the_floor() {
            let (mut world, mob, item, _) = world_with_mob();
            let pos = TilePos { x: 1, y: 1, z: 0 };
            store_items(&mut world, mob, None, pos);
            assert_eq!(world.get::<TilePos>(item), Some(&pos));
        }
    }
}

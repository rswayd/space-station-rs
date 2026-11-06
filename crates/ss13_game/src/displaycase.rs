//! Display cases and their chassis (server), after tg `code/game/objects/structures/displaycase.dm`
//! and BandaStation's `modular_bandastation/objects/code/structures/displaycase.dm`.
//!
//! - A display case ([`DisplayCase`]) holds one showpiece, drawn over its base under the glass;
//!   the captain's (antique laser), the lab cage (Lamarr) start with theirs. An ID card (or a
//!   PDA holding one) with the case's access opens or closes it; open and empty, any item goes
//!   on display; an empty hand takes the showpiece out of an open or broken case onto the floor,
//!   otherwise looks at the case (out of combat mode) or kicks it for 2 damage.
//! - At a quarter of its integrity the glass breaks (a shard, the shatter sound, not dense); a
//!   broken case takes 2 glass sheets (2 s) to fix, a damaged whole one a welder (4 s). A case
//!   without the anti-theft system opens with a crowbar (2 s) and a broken one is pried off.
//! - The anti-theft system (every mapped case but `noalert` and built ones): breaking or
//!   destroying the case closes and bolts the airlocks of its area, sounds the alert and raises a
//!   burglar alarm for silicons for a minute.
//! - The chassis ([`CHASSIS`]): airlock electronics go in (3 s) and give the case their access,
//!   10 glass sheets (2 s) finish a case without the alarm, a wrench (3 s) takes it apart into
//!   5 planks (and its electronics).
// shortcut: the showpiece is drawn at full size (overlays have no `transform` scale), no red
// "fire" lights in the alarmed area (`set_fire_effect` here also means a fire alarm), only
// airlocks are locked down (tg also closes the other doors), no trophy case persistence or its
// `Trophycase` window and curator key, no Vend-A-Tray, no HoS case's random donut, no attack
// sounds of the glass. Add each with its mechanic.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{Areas, DmAtom, DmObj, DoorKind, IconIndex};
use ss13_shared::appearance::Overlay;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};
use ss13_world::Density;

use crate::construction::Job;
use crate::examine::Line;
use crate::medical::name_of;
use crate::obj_damage::Integrity;
use crate::sound::{PlaySound, playsound};
use crate::storage::StoredIn;
use crate::tools::{Actor, ToolKind, do_after, tell, tool_of, use_tool};

pub const DISPLAYCASE: &str = "/obj/structure/displaycase";
pub const CHASSIS: &str = "/obj/structure/displaycase_chassis";
const NOALERT: &str = "/obj/structure/displaycase/noalert";
const TROPHY: &str = "/obj/structure/displaycase/trophy";
const SHARD: &str = "/obj/item/shard";
const GLASS: &str = "/obj/item/stack/sheet/glass";
const WOOD: &str = "/obj/item/stack/sheet/mineral/wood";
const CLICK_SOUND: &str = "sound/machines/click.ogg";
const ALERT_SOUND: &str = "sound/effects/alert.ogg";
const DECONSTRUCT_SOUND: &str = "sound/items/deconstruct.ogg";
const SHATTER: &str = "shatter";
/// `addtimer(clear_alarm, 1 MINUTES)`.
const ALARM_SECS: f64 = 60.0;

/// `/obj/structure/displaycase`: `open`, `showpiece`, `alert`, the access it opens for.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct DisplayCase {
    pub open: bool,
    /// `showpiece`; [`showpiece_of`] checks it is still inside.
    pub showpiece: Option<Entity>,
    pub alert: bool,
    pub req_access: Vec<String>,
    pub req_one_access: Vec<String>,
}

/// `/obj/structure/displaycase_chassis`: `electronics`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct DisplayChassis {
    pub electronics: Option<Entity>,
}

/// The burglar alarm a case's `alarm_manager` sends, until `clear_at`
/// (`crate::silicon::alarms` lists it for silicons).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct BurglarAlarm {
    pub area: Option<Entity>,
    pub clear_at: f64,
}

/// Timed steps on a case or its chassis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseJob {
    /// Two glass sheets on a broken case.
    FixGlass,
    /// A welder on a damaged case.
    Weld,
    /// A crowbar on a case without the alarm.
    Pry,
    /// Airlock electronics into the chassis.
    Electronics,
    /// Ten glass sheets on the chassis.
    Finish,
    /// A wrench on the chassis.
    Disassemble,
}

pub struct DisplayCasePlugin;

impl Plugin for DisplayCasePlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::EmptyHand, 3156, |w, c| {
            attack_hand(w, c.client, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 3441, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.add_observer(add_case)
            .add_systems(ss13_world::MapInit, fill_mapped_cases)
            .add_systems(Update, clear_alarms);
    }
}

/// What [`crate::obj_damage`] gives integrity to here.
pub fn breakable(path: &str) -> bool {
    is_under(path, DISPLAYCASE) || is_under(path, CHASSIS)
}

/// `req_access` of the mapped types (BandaStation gives the captain's case `ACCESS_CAPTAIN`).
fn access_of(path: &str) -> Vec<String> {
    const ACCESS: [(&str, &str); 4] = [
        ("/obj/structure/displaycase/captain", "captain"),
        ("/obj/structure/displaycase/labcage", "rd"),
        ("/obj/structure/displaycase/trophy", "library"),
        ("/obj/structure/displaycase/hos", "hos"),
    ];
    ACCESS
        .iter()
        .filter(|(root, _)| is_under(path, root))
        .map(|&(_, a)| a.to_owned())
        .collect()
}

/// `start_showpiece_type`.
fn start_showpiece(path: &str) -> Option<&'static str> {
    if is_under(path, "/obj/structure/displaycase/captain") {
        Some("/obj/item/gun/energy/laser/captain")
    } else if is_under(path, "/obj/structure/displaycase/labcage") {
        Some("/obj/item/clothing/mask/facehugger/lamarr")
    } else {
        None
    }
}

fn add_case(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(dm) = objs.get(add.entity) else {
        return;
    };
    let entity = add.entity;
    if is_under(&dm.path, DISPLAYCASE) {
        commands.entity(entity).insert(DisplayCase {
            open: false,
            showpiece: None,
            alert: !is_under(&dm.path, NOALERT),
            req_access: access_of(&dm.path),
            req_one_access: Vec::new(),
        });
        commands.queue(move |world: &mut World| refresh(world, entity));
    } else if is_under(&dm.path, CHASSIS) {
        commands.entity(entity).insert(DisplayChassis::default());
    }
}

/// `Initialize`: `showpiece = new start_showpiece_type(src)`, once the map's prototypes exist.
fn fill_mapped_cases(world: &mut World) {
    let cases: Vec<(Entity, &'static str)> = world
        .query_filtered::<(Entity, &DmObj, &DisplayCase), With<ss13_world::Mapped>>()
        .iter(world)
        .filter(|(.., c)| c.showpiece.is_none())
        .filter_map(|(e, dm, _)| start_showpiece(&dm.path).map(|p| (e, p)))
        .collect();
    for (case, path) in cases {
        if let Some(item) = crate::construction::spawn_detached(world, path) {
            crate::storage::put_inside(world, item, case);
            if let Some(mut c) = world.get_mut::<DisplayCase>(case) {
                c.showpiece = Some(item);
            }
        }
        refresh(world, case);
    }
}

/// The showpiece, if it is still inside (tg `Exited` clears the var otherwise).
pub fn showpiece_of(world: &World, case: Entity) -> Option<Entity> {
    world
        .get::<DisplayCase>(case)?
        .showpiece
        .filter(|&s| world.get::<StoredIn>(s).is_some_and(|i| i.0 == case))
}

fn broken(world: &World, e: Entity) -> bool {
    world.get::<Integrity>(e).is_some_and(|i| i.broken)
}

fn sound(world: &mut World, e: Entity, file: &str, volume: f32) {
    if let Some(&pos) = world.get::<TilePos>(e) {
        playsound(world, PlaySound::new(pos, file, volume, true));
    }
}

/// `update_overlays`: the showpiece, then the closed or broken glass.
fn refresh(world: &mut World, case: Entity) {
    let Some(c) = world.get::<DisplayCase>(case) else {
        return;
    };
    let open = c.open;
    let piece = showpiece_of(world, case)
        .and_then(|s| world.get::<Appearance>(s))
        .map(|a| Overlay::new(a.icon_state));
    let glass = if broken(world, case) {
        Some("glassbox_broken")
    } else if !open {
        Some("glassbox_closed")
    } else {
        None
    };
    let glass = glass.and_then(|state| {
        let icon = world.get::<DmAtom>(case)?.icon.clone();
        world
            .get_resource::<IconIndex>()?
            .get(&icon, state)
            .map(Overlay::new)
    });
    let overlays: Vec<Overlay> = piece.into_iter().chain(glass).collect();
    if let Some(mut appearance) = world.get_mut::<Appearance>(case)
        && appearance.overlays != overlays
    {
        appearance.overlays = overlays;
    }
}

/// `dump`: the showpiece onto the case's tile.
fn dump(world: &mut World, case: Entity) {
    let (Some(piece), Some(&pos)) = (showpiece_of(world, case), world.get::<TilePos>(case)) else {
        return;
    };
    crate::storage::take_out(world, piece, pos);
    if let Some(mut c) = world.get_mut::<DisplayCase>(case) {
        c.showpiece = None;
    }
    refresh(world, case);
}

/// `toggle_lock`.
fn toggle_lock(world: &mut World, case: Entity) {
    sound(world, case, CLICK_SOUND, 20.0);
    if let Some(mut c) = world.get_mut::<DisplayCase>(case) {
        c.open = !c.open;
    }
    refresh(world, case);
}

/// `atom_break` of a case: true when `target` was one.
pub fn atom_break(world: &mut World, target: Entity) -> bool {
    if world.get::<DisplayCase>(target).is_none() {
        return false;
    }
    // obj_damage only breaks a whole case; it marks it broken here.
    if let Some(mut i) = world.get_mut::<Integrity>(target) {
        i.broken = true;
    }
    world.entity_mut(target).remove::<Density>();
    if let Some(&pos) = world.get::<TilePos>(target) {
        crate::construction::spawn_obj(world, SHARD, pos);
    }
    sound(world, target, SHATTER, 70.0);
    refresh(world, target);
    trigger_alarm(world, target);
    true
}

/// `atom_deconstruct` of a case (the showpiece out; destroyed, a shard and the alarm) and of the
/// chassis (taken apart, 5 planks and its electronics). True when `dm` was one.
pub(crate) fn deconstruct(
    world: &mut World,
    target: Entity,
    dm: &DmObj,
    pos: TilePos,
    disassembled: bool,
) -> bool {
    if is_under(&dm.path, DISPLAYCASE) {
        dump(world, target);
        if !disassembled {
            crate::construction::spawn_obj(world, SHARD, pos);
            trigger_alarm(world, target);
        }
        return true;
    }
    if is_under(&dm.path, CHASSIS) {
        // `wrench_act`; destroyed, it leaves nothing.
        if disassembled {
            crate::construction::spawn_stack(world, WOOD, pos, 5);
            if let Some(e) = chassis_electronics(world, target) {
                crate::storage::take_out(world, e, pos);
            }
        }
        return true;
    }
    false
}

/// `trigger_alarm` -> `area/burglaralert` and `alarm_manager.send_alarm(ALARM_BURGLAR)`.
fn trigger_alarm(world: &mut World, case: Entity) {
    if !world.get::<DisplayCase>(case).is_some_and(|c| c.alert) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(case) else {
        return;
    };
    let area = world.get_resource::<Areas>().and_then(|a| a.area_at(pos));
    if let Some(area) = area {
        lock_down(world, area);
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(case).insert(BurglarAlarm {
        area,
        clear_at: now + ALARM_SECS,
    });
    sound(world, case, ALERT_SOUND, 50.0);
}

/// `close_and_lock_door` of every airlock in `area`.
// Cost: every door on the map once per alarm (a thousand position lookups, rarely).
fn lock_down(world: &mut World, area: Entity) {
    let airlocks: Vec<(Entity, TilePos)> = world
        .query::<(Entity, &crate::doors::Door, &TilePos)>()
        .iter(world)
        .filter(|(_, d, _)| d.kind == DoorKind::Airlock)
        .map(|(e, _, &p)| (e, p))
        .collect();
    let Some(areas) = world.get_resource::<Areas>() else {
        return;
    };
    let doors: Vec<Entity> = airlocks
        .into_iter()
        .filter(|&(_, p)| areas.area_at(p) == Some(area))
        .map(|(e, _)| e)
        .collect();
    use crate::doors::DoorState;
    for door in doors {
        let shut = crate::doors::with_door(world, door, |d, appearance, ctx| {
            let closing = d.state == DoorState::Open && ctx.close(d, appearance, false);
            let shut = matches!(d.state, DoorState::Closed | DoorState::Closing { .. });
            (shut, closing)
        });
        if shut == Some(true) {
            crate::wires::set_bolts(world, door, true);
        }
    }
}

/// `clear_alarm` a minute after it was sent.
fn clear_alarms(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let done: Vec<Entity> = world
        .query::<(Entity, &BurglarAlarm)>()
        .iter(world)
        .filter(|(_, a)| a.clear_at <= now)
        .map(|(e, _)| e)
        .collect();
    for e in done {
        world.entity_mut(e).remove::<BurglarAlarm>();
    }
}

fn combat(world: &World, mob: Entity) -> bool {
    world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
}

/// `attack_hand`.
fn attack_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(c) = world.get::<DisplayCase>(target) else {
        return false;
    };
    let open = c.open;
    let path = world
        .get::<DmObj>(target)
        .map(|d| d.path.clone())
        .unwrap_or_default();
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut next) = world.get_mut::<crate::combat::NextMove>(mob) {
        next.0 = now + crate::combat::CLICK_CD_MELEE;
    }
    if showpiece_of(world, target).is_some() && (broken(world, target) || open) {
        tell(
            world,
            client,
            Span::Notice,
            "Вы отключаете парящее поле витрины.",
        );
        dump(world, target);
        return true;
    }
    if !combat(world, mob) {
        // `autoexamine_while_closed`: the trophy case shows its window instead.
        if !open && is_under(&path, TROPHY) {
            return true;
        }
        let (name, lines) = crate::examine::examine_lines(world, mob, target);
        for message in crate::examine::examine_messages(&name, lines) {
            world.write_message(ss13_net::chat::server::SendChat {
                clients: vec![client],
                message,
            });
        }
        return true;
    }
    let user = name_of(world, mob);
    crate::medical::visible(
        world,
        mob,
        "",
        &format!("{user} пинает витрину."),
        Span::Danger,
    );
    crate::animation::do_attack_animation(world, mob, target);
    crate::obj_damage::take_damage(world, target, 2.0, crate::health::DamageType::Brute);
    true
}

fn chassis_electronics(world: &World, chassis: Entity) -> Option<Entity> {
    world
        .get::<DisplayChassis>(chassis)?
        .electronics
        .filter(|&e| world.get::<StoredIn>(e).is_some_and(|i| i.0 == chassis))
}

fn item_path(world: &World, item: Entity) -> String {
    world
        .get::<ss13_mapload::DmItem>(item)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

/// `item_interaction` and the tool acts of a case and its chassis.
fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    if world.get::<DisplayChassis>(target).is_some() {
        return chassis_interaction(world, actor, held, target);
    }
    let Some(case) = world.get::<DisplayCase>(target).cloned() else {
        return false;
    };
    let broken = broken(world, target);
    if crate::inventory::get_id(world, held).is_some() && !broken {
        // `allowed(user)`: the user's ID, not necessarily the one clicked with.
        let access = crate::bots::user_access(world, mob);
        let allowed = crate::silicon::interact::has_silicon_access(world, mob)
            || crate::inventory::check_access(&case.req_access, &case.req_one_access, &access);
        if !allowed {
            tell(world, client, Span::Warning, "Доступ запрещён.");
            return true;
        }
        let text = if case.open {
            "Вы закрываете витрину."
        } else {
            "Вы открываете витрину."
        };
        tell(world, client, Span::Notice, text);
        toggle_lock(world, target);
        return true;
    }
    if case.open && showpiece_of(world, target).is_none() {
        // `insert_showpiece` (`transferItemToLoc` fails on a stuck item).
        if crate::inventory::no_drop(world, held) {
            return true;
        }
        let name = name_of(world, held);
        crate::storage::put_inside(world, held, target);
        if let Some(mut c) = world.get_mut::<DisplayCase>(target) {
            c.showpiece = Some(held);
        }
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы выставляете {name} на витрину."),
        );
        refresh(world, target);
        return true;
    }
    if broken && is_under(&item_path(world, held), GLASS) {
        if crate::tools::stack_amount(world, held) < 2 {
            tell(
                world,
                client,
                Span::Warning,
                "Чтобы починить витрину, нужно два листа стекла!",
            );
            return true;
        }
        if do_after(
            world,
            actor,
            target,
            20.0,
            2,
            Job::DisplayCase(CaseJob::FixGlass),
        ) {
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете чинить витрину...",
            );
        }
        return true;
    }
    if combat(world, mob) {
        return false;
    }
    match tool_of(world, held).map(|(t, _)| t) {
        Some(ToolKind::Welder) if !broken => {
            let max = world.get::<DmObj>(target).map_or(0.0, |o| o.max_integrity);
            if world
                .get::<Integrity>(target)
                .is_some_and(|i| i.current >= max)
            {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Витрина уже в хорошем состоянии!",
                );
                return true;
            }
            if use_tool(
                world,
                actor,
                target,
                40.0,
                1,
                Job::DisplayCase(CaseJob::Weld),
            ) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы начинаете чинить витрину...",
                );
            }
            true
        }
        // `crowbar_act`: only the lab cage and built cases (no alarm).
        Some(ToolKind::Crowbar) if !case.alert => {
            if broken {
                if showpiece_of(world, target).is_some() {
                    tell(
                        world,
                        client,
                        Span::Warning,
                        "Сначала уберите выставленный предмет!",
                    );
                } else {
                    tell(world, client, Span::Notice, "Вы снимаете разбитую витрину.");
                    let pos = world.get::<TilePos>(target).copied();
                    world.despawn(target);
                    if let Some(pos) = pos {
                        crate::construction::tile_changed(world, pos);
                    }
                }
                return true;
            }
            let text = if case.open {
                "Вы начинаете закрывать витрину..."
            } else {
                "Вы начинаете открывать витрину..."
            };
            if use_tool(
                world,
                actor,
                target,
                20.0,
                0,
                Job::DisplayCase(CaseJob::Pry),
            ) {
                tell(world, client, Span::Notice, text);
            }
            true
        }
        _ => false,
    }
}

/// The chassis' `item_interaction` and `wrench_act`.
fn chassis_interaction(world: &mut World, actor: Actor, held: Entity, target: Entity) -> bool {
    let client = actor.client;
    let path = item_path(world, held);
    if is_under(&path, crate::airlock_construction::ELECTRONICS) {
        // tg takes a second set and drops the first's reference; one is enough.
        if chassis_electronics(world, target).is_some() {
            return true;
        }
        if do_after(
            world,
            actor,
            target,
            30.0,
            0,
            Job::DisplayCase(CaseJob::Electronics),
        ) {
            tell(world, client, Span::Notice, "установка электроники...");
        }
        return true;
    }
    if is_under(&path, GLASS) {
        if crate::tools::stack_amount(world, held) < 10 {
            tell(world, client, Span::Warning, "нужно 10 листов!");
            return true;
        }
        if do_after(
            world,
            actor,
            target,
            20.0,
            10,
            Job::DisplayCase(CaseJob::Finish),
        ) {
            tell(world, client, Span::Notice, "установка стекла...");
        }
        return true;
    }
    if tool_of(world, held).is_some_and(|(t, _)| t == ToolKind::Wrench) {
        if use_tool(
            world,
            actor,
            target,
            30.0,
            0,
            Job::DisplayCase(CaseJob::Disassemble),
        ) {
            tell(world, client, Span::Notice, "разборка...");
        }
        return true;
    }
    false
}

/// End of a [`CaseJob`] (`crate::tools` checked nobody moved), every check again.
pub fn complete(world: &mut World, _mob: Entity, action: &crate::tools::DoAfter, job: CaseJob) {
    let (client, target) = (action.client, action.target);
    let consume = |world: &mut World| {
        action
            .item
            .is_none_or(|i| crate::tools::consume(world, i, action.amount))
    };
    match job {
        CaseJob::FixGlass => {
            if !broken(world, target) || world.get::<DisplayCase>(target).is_none() {
                return;
            }
            if !consume(world) {
                return;
            }
            let max = world.get::<DmObj>(target).map_or(0.0, |o| o.max_integrity);
            world.entity_mut(target).insert((
                Integrity {
                    current: max,
                    broken: false,
                },
                // tg leaves a fixed case passable; a whole case is dense.
                Density,
            ));
            refresh(world, target);
        }
        CaseJob::Weld => {
            if broken(world, target) || !consume(world) {
                return;
            }
            let max = world.get::<DmObj>(target).map_or(0.0, |o| o.max_integrity);
            if let Some(mut i) = world.get_mut::<Integrity>(target) {
                i.current = max;
            }
            tell(world, client, Span::Notice, "Вы чините витрину.");
        }
        CaseJob::Pry => {
            let Some(open) = world.get::<DisplayCase>(target).map(|c| c.open) else {
                return;
            };
            if broken(world, target) {
                return;
            }
            let text = if open {
                "Вы закрываете витрину."
            } else {
                "Вы открываете витрину."
            };
            tell(world, client, Span::Notice, text);
            toggle_lock(world, target);
        }
        CaseJob::Electronics => {
            let Some(item) = action.item else {
                return;
            };
            if world.get::<DisplayChassis>(target).is_none()
                || chassis_electronics(world, target).is_some()
                || crate::inventory::no_drop(world, item)
            {
                return;
            }
            crate::storage::put_inside(world, item, target);
            world.entity_mut(target).insert(DisplayChassis {
                electronics: Some(item),
            });
            tell(world, client, Span::Notice, "электроника установлена");
        }
        CaseJob::Finish => {
            let Some(&pos) = world.get::<TilePos>(target) else {
                return;
            };
            if world.get::<DisplayChassis>(target).is_none() || !consume(world) {
                return;
            }
            finish(world, target, pos);
        }
        CaseJob::Disassemble => {
            if world.get::<DisplayChassis>(target).is_none() {
                return;
            }
            sound(world, target, DECONSTRUCT_SOUND, 50.0);
            crate::obj_damage::deconstruct(world, target, true);
        }
    }
}

/// `make_final_result(/obj/structure/displaycase/noalert)`: the electronics' access goes over.
fn finish(world: &mut World, chassis: Entity, pos: TilePos) {
    let electronics = chassis_electronics(world, chassis);
    let access = electronics
        .and_then(|e| world.get::<crate::airlock_construction::AirlockElectronics>(e))
        .cloned();
    world.despawn(chassis);
    let Some(case) = crate::construction::spawn_obj(world, NOALERT, pos) else {
        return;
    };
    if let Some(e) = electronics {
        crate::storage::put_inside(world, e, case);
    }
    // After the case's own `add_case` insert, which is queued by its observer.
    if let Some(access) = access {
        world.commands().queue(move |world: &mut World| {
            if let Some(mut c) = world.get_mut::<DisplayCase>(case) {
                if access.one_access {
                    c.req_one_access = access.accesses;
                } else {
                    c.req_access = access.accesses;
                }
            }
        });
        world.flush();
    }
    crate::construction::tile_changed(world, pos);
}

/// `examine` of a case and its chassis.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<Line>) {
    if let Some(c) = world.get::<DisplayCase>(target) {
        if c.alert {
            lines.push((
                Span::Notice,
                "Подключена к противоугонной системе.".to_owned(),
            ));
        }
        if let Some(piece) = showpiece_of(world, target) {
            lines.push((
                Span::Notice,
                format!("Внутри находится {}.", name_of(world, piece)),
            ));
        }
        return;
    }
    if world.get::<DisplayChassis>(target).is_none() {
        return;
    }
    if chassis_electronics(world, target).is_none() {
        lines.push((
            Span::Notice,
            "Можно установить <b>электронику шлюза</b>, чтобы ограничить доступ.".to_owned(),
        ));
    }
    lines.push((
        Span::Notice,
        "Её можно достроить <b>10 листами стекла</b>.".to_owned(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapped_cases_lock_to_their_heads() {
        assert_eq!(
            access_of("/obj/structure/displaycase/captain"),
            vec!["captain"]
        );
        assert_eq!(access_of("/obj/structure/displaycase/labcage"), vec!["rd"]);
        assert!(access_of("/obj/structure/displaycase").is_empty());
        assert!(access_of(NOALERT).is_empty());
        assert_eq!(
            start_showpiece("/obj/structure/displaycase/captain"),
            Some("/obj/item/gun/energy/laser/captain")
        );
        assert_eq!(start_showpiece(TROPHY), None);
    }
}

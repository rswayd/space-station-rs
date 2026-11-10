//! The space ninja after tg `/datum/antagonist/ninja` (`space_ninja.dm`), its outfit
//! (`outfit.dm`), the energy katana (`energy_katana.dm`, `/datum/action/innate/dash/ninja`), the
//! spider charge (`ninja_explosive.dm`), the hacker module's `ninjadrain_act`s
//! (`modules_ninja.dm`) and the "Space Ninja" heavy midround
//! (`/datum/dynamic_ruleset/midround/from_ghosts/space_ninja`).
//!
//! - **Selection** ([`can_spawn`], [`spawn_ninjas`], from `super::super::midround`): ghosts who
//!   want [`ROLE_NINJA`]; the body appears at a carp spawn (`find_space_spawn`) named
//!   "[title] [name]" (`GLOB.ninja_titles`, `GLOB.ninja_names`).
//! - **The antagonist** ([`make_ninja`]): the objectives (hijack a cyborg, sabotage research,
//!   hack 15-40 doors, blow the spider charge in one station area, scramble security, send the
//!   message of terror, survive), the outfit (`/datum/outfit/ninja`: the Spider Clan MODsuit,
//!   the katana sheath, the spider charge), `post_equip` (the reinforced DNA lock takes the
//!   ninja, the weapon recall links the katana, the charge learns its area), the job, the antag
//!   HUD, the greeting and its sound.
//! - **The katana** ([`katana_clicks`]): a right click far away dashes onto the clicked tile (in
//!   sight, not blocked; 3 charges, each back 20 s later); on a living target the ninja lands
//!   next to it and strikes.
//! - **The spider charge** ([`spider_plant_check`], [`spider_detonation`]): only a ninja plants
//!   it, only in its objective's area; moved out of the area it fizzles into a new charge; going
//!   off completes the objective.
//! - **The hacker module** ([`hack`]): with the gloves sealed, a right click with an empty hand
//!   drains or hacks what it touches: airlocks and windoors are emagged (doors count for the
//!   objective), APCs, SMESes and power cells are drained into the suit (APCs emagged), the
//!   security records console arrests everyone (20 s), the communications console calls a new
//!   threat (30 s, once), an R&D server loses its points (30 s), a cyborg becomes the clan's
//!   (6 s: SpiderOS laws, cut from its AI), cameras get an EMP, bots overload and blow up,
//!   carbons are knocked down by an electric touch. Consoles warn the AIs.
//! - [`roundend_report`]: each ninja and its objectives.
// shortcut: no explosive implant, freerunning and light step quirks, antag info panel (tg
// `AntagInfoNinja`) or respawn; the hijacked cyborg keeps its model (tg turns it into a
// syndicate one) and the master R&D server's source code drive is not destroyed; the hacker
// does not touch firelocks, mechs, energy guns, vending machines, recyclers, elevator or tram
// controls and buttons; an interrupted drain says nothing (the suit keeps what it got).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{Areas, DmPower, IconIndex};
use ss13_net::sound::server::SendSound;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::{ClickModifiers, SoundEvent};
use ss13_tgui::server::OpenUi;

use super::super::objectives::RoundEnd;
use super::super::{AntagHudData, AntagHudUi};
use super::{RaiderJob, announce_objectives, key_of, line, objective_line, pick, with_rng};
use crate::click::{ClickOn, RangedClick, ResolveClicks};
use crate::click_handlers::ClickCtx;
use crate::construction::{Job, path_of};
use crate::health::{Health, Stat};
use crate::inventory::Inventory;
use crate::modsuit::modules::{Kind, ModModule};
use crate::modsuit::server::{ModSuit, add_charge, charge, max_charge, subtract_charge, worn_suit};
use crate::obj_damage::is_under;
use crate::server::{PlayerMob, SpawnPoints};
use crate::tools::{Actor, DoAfter, now, tell, tell_mob};

pub use super::super::ROLE_NINJA;
/// `/datum/antagonist/ninja` `name`.
pub const ANTAG_NAME: &str = "Космический Ниндзя";
pub const NINJA_OUTFIT: &str = "/datum/outfit/ninja";
pub const KATANA: &str = "/obj/item/energy_katana";
pub const SPIDER_CHARGE: &str = "/obj/item/grenade/c4/ninja";
const JOB: &str = "/datum/job/space_ninja";
const GREETING: &str = "sound/music/antag/ninja_greeting.ogg";
const DASH_SOUND: &str = "sound/effects/magic/blink.ogg";
const SPARKS_SOUND: &str = "sound/effects/sparks/sparks1.ogg";
const BUZZER: &str = "sound/machines/warning-buzzer.ogg";
const ANTAG_HUD: &str = "icons/mob/huds/antag_hud.dmi";
/// `/datum/ai_laws/ninja_override` `id`.
const NINJA_LAWS: &str = "ninja";
/// `VALID_TERRITORY`.
const VALID_TERRITORY: u32 = 1 << 0;
/// `/datum/action/innate/dash/ninja`: `max_charges`, `charge_rate` (seconds).
pub const DASH_CHARGES: u32 = 3;
pub const DASH_RECHARGE_SECS: f64 = 20.0;
/// `DEFAULT_CHARGE_DRAIN` (`0.005 * STANDARD_CELL_CHARGE`), joules.
pub const DEFAULT_CHARGE_DRAIN: f64 = 50.0;
/// `NINJA_MIN_DRAIN`, `NINJA_MAX_DRAIN`: one drain step, joules.
pub const NINJA_MIN_DRAIN: f64 = 2000.0;
pub const NINJA_MAX_DRAIN: f64 = 4000.0;
/// The bot overload's `addtimer(explosion(src, 0, 1, 2, 3), 2.5 SECONDS)`.
const BOT_BLAST_SECS: f64 = 2.5;

/// One of the ninja's objectives (`/datum/objective/*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NinjaGoal {
    CyborgHijack,
    ResearchSecrets,
    /// `doors_required`.
    DoorJack(u32),
    /// `detonation_location`.
    PlantExplosive(Entity),
    SecurityScramble,
    TerrorMessage,
    Survive,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NinjaObjective {
    pub goal: NinjaGoal,
    pub text: String,
    /// `completed` (the survive objective is checked at the end instead).
    pub completed: bool,
}

/// `/datum/antagonist/ninja` on its body.
#[derive(Component, Clone, Debug)]
pub struct Ninja {
    pub key: String,
    pub objectives: Vec<NinjaObjective>,
}

/// The katana's dash (`/datum/action/innate/dash/ninja`): charges and when the spent ones
/// come back.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct KatanaDash {
    pub charges: u32,
    pub recharge_at: Vec<f64>,
}

impl Default for KatanaDash {
    fn default() -> Self {
        Self {
            charges: DASH_CHARGES,
            recharge_at: Vec::new(),
        }
    }
}

impl KatanaDash {
    /// `charge()` of every timer that ran out.
    pub fn recharge(&mut self, now: f64) {
        let before = self.recharge_at.len();
        self.recharge_at.retain(|&at| at > now);
        let back = (before - self.recharge_at.len()) as u32;
        self.charges = (self.charges + back).min(DASH_CHARGES);
    }

    /// One charge spent: false without one.
    pub fn spend(&mut self, now: f64) -> bool {
        self.recharge(now);
        if self.charges == 0 {
            return false;
        }
        self.charges -= 1;
        self.recharge_at.push(now + DASH_RECHARGE_SECS);
        true
    }
}

/// The spider charge's `detonation_area` and `detonator`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpiderCharge {
    pub area: Option<Entity>,
    pub detonator: Option<Entity>,
}

/// The hacker module's `door_hack_counter` and `communication_console_hack_success`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HackerState {
    pub doors: u32,
    pub comms_hacked: bool,
}

/// A bot the hacker overloaded: it blows up at this game time.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Overloaded(pub f64);

pub struct NinjaPlugin;

impl Plugin for NinjaPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // The hacker module's right click with an empty hand (before the rest of `attack_hand`).
        app.add_click_handler(ClickPhase::EmptyHand, 13, hack);
        app.add_systems(
            Update,
            (
                katana_clicks
                    .after(ResolveClicks)
                    .run_if(resource_exists::<Messages<RangedClick>>),
                overloaded_bots,
            ),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Selection and the body

/// `find_space_spawn`'s candidates.
fn space_spawns(world: &World) -> Vec<TilePos> {
    world
        .get_resource::<SpawnPoints>()
        .map(|s| s.carp.clone())
        .unwrap_or_default()
}

/// `can_be_selected`: a carp spawn.
pub fn can_spawn(world: &mut World) -> bool {
    !space_spawns(world).is_empty()
}

/// `assign_role` for each of `clients` (their ghosts' clients): the bodies.
pub fn spawn_ninjas(world: &mut World, clients: &[Entity]) -> Vec<Entity> {
    let mut out = Vec::new();
    for &client in clients {
        let spawns = space_spawns(world);
        let Some(pos) = pick(world, &spawns) else {
            break;
        };
        let (titles, names) = world
            .get_resource::<super::RaiderData>()
            .map(|d| (d.ninja_titles.clone(), d.ninja_names.clone()))
            .unwrap_or_default();
        let title = pick(world, &titles).unwrap_or_else(|| "Ниндзя".to_owned());
        let name = pick(world, &names).unwrap_or_default();
        let full = format!("{title} {name}").trim().to_owned();
        let mob = crate::server::spawn_mob(world, client, pos, full);
        world.entity_mut(client).remove::<crate::round::NewPlayer>();
        make_ninja(world, mob);
        info!("{mob} ({client}) is a space ninja at {pos:?}");
        out.push(mob);
    }
    out
}

/// Station areas a spider charge may go off in (`is_station_level`, `VALID_TERRITORY`).
fn bomb_areas(world: &World) -> Vec<(Entity, String)> {
    let Some(areas) = world.get_resource::<Areas>() else {
        return Vec::new();
    };
    let mut out: Vec<(Entity, String)> = areas
        .entities()
        .iter()
        .copied()
        .filter(|&a| {
            world.get::<DmPower>(a).is_some_and(|p| {
                p.path.starts_with("/area/station") && (p.area_flags as u32) & VALID_TERRITORY != 0
            })
        })
        .map(|a| (a, crate::shuttle::area_name(world, a)))
        .collect();
    out.sort_by_key(|(a, _)| *a);
    out.dedup_by_key(|(a, _)| *a);
    out
}

/// `addObjectives`.
pub fn forge_objectives(world: &mut World) -> Vec<NinjaObjective> {
    let objective = |goal, text: &str| NinjaObjective {
        goal,
        text: text.to_owned(),
        completed: false,
    };
    let mut out = vec![
        objective(
            NinjaGoal::CyborgHijack,
            "Используйте свои перчатки, чтобы конвертировать хотя бы одного киборга, для \
             оказания помощи в ваших задачах.",
        ),
        objective(
            NinjaGoal::ResearchSecrets,
            "Используйте свои перчатки на сервере РНД, чтобы саботировать исследовательские \
             проекты. Помните, что ИИ будет предупрежден, как только вы начнете!",
        ),
    ];
    let doors = with_rng(world, |rng| rng.range(15, 40)) as u32;
    out.push(NinjaObjective {
        goal: NinjaGoal::DoorJack(doors),
        text: format!("Используйте свои перчатки, чтобы взломать {doors} шлюзов на станции."),
        completed: false,
    });
    let areas = bomb_areas(world);
    if let Some((area, name)) = pick(world, &areas) {
        out.push(NinjaObjective {
            goal: NinjaGoal::PlantExplosive(area),
            text: format!(
                "Взорвите выданную вам бомбу в {name}.  Учтите, что бомба не будет работать в \
                 любом другом месте!"
            ),
            completed: false,
        });
    }
    out.push(objective(
        NinjaGoal::SecurityScramble,
        "Используйте перчатки на консоли безопасности хотя бы один раз, чтобы установить арест \
         на всех. Помните, что ИИ будет предупрежден, как только вы начнете!",
    ));
    out.push(objective(
        NinjaGoal::TerrorMessage,
        "Используйте свои перчатки на консоли коммуникации, чтобы навлечь на станцию еще одну \
         угрозу. Помните, что ИИ будет предупрежден, как только вы начнете!",
    ));
    out.push(objective(
        NinjaGoal::Survive,
        "Оставайтесь в живых до самого конца.",
    ));
    out
}

/// `add_antag_datum(/datum/antagonist/ninja)`: objectives, outfit, job, HUD, greeting. False
/// when `mob` already is one.
pub fn make_ninja(world: &mut World, mob: Entity) -> bool {
    if world.get::<Ninja>(mob).is_some() || world.get_entity(mob).is_err() {
        return false;
    }
    let objectives = forge_objectives(world);
    let key = key_of(world, mob);
    let hud_icon = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(ANTAG_HUD, "ninja"));
    world.entity_mut(mob).insert((
        Ninja {
            key,
            objectives: objectives.clone(),
        },
        AntagHudData {
            hud_icon,
            ..default()
        },
        crate::mobs::Faction(vec![ROLE_NINJA.to_owned()]),
    ));
    if super::equip_outfit(world, mob, NINJA_OUTFIT).is_none() {
        warn!("{NINJA_OUTFIT}: no outfit data (run ss13-mapload convert)");
    }
    post_equip(world, mob);
    crate::noncrew_jobs::set_assigned_role(world, mob, JOB);
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        greet(world, client, mob, &objectives);
    }
    true
}

/// `/datum/outfit/ninja/post_equip`: the charge's area, the DNA lock, the katana link.
fn post_equip(world: &mut World, mob: Entity) {
    let contents = super::super::objectives::all_contents(world, mob);
    let area = world.get::<Ninja>(mob).and_then(|n| {
        n.objectives.iter().find_map(|o| match o.goal {
            NinjaGoal::PlantExplosive(a) => Some(a),
            _ => None,
        })
    });
    for &e in &contents {
        if path_of(world, e).is_some_and(|p| is_under(&p, SPIDER_CHARGE)) {
            world.entity_mut(e).insert(SpiderCharge {
                area,
                detonator: None,
            });
        }
    }
    // The sheath's blade; one in hand when the sheath came empty.
    let katana = contents
        .iter()
        .copied()
        .find(|&e| path_of(world, e).is_some_and(|p| is_under(&p, KATANA)))
        .or_else(|| {
            let blade = crate::construction::spawn_detached(world, KATANA)?;
            world.entity_mut(blade).insert(ss13_net::server::Networked);
            let inv = world.get::<Inventory>(mob)?.clone();
            crate::storage::put_in_hands(world, mob, &inv, blade);
            Some(blade)
        });
    if let Some(katana) = katana {
        world.entity_mut(katana).insert(KatanaDash::default());
    }
    // `internals_slot = ITEM_SLOT_RPOCKET`: the oxygen tank is opened.
    let tank = world.get::<Inventory>(mob).and_then(|i| i.pockets[1]);
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    if let (Some(tank), Some(client)) = (tank, client)
        && path_of(world, tank).is_some_and(|p| is_under(&p, "/obj/item/tank"))
    {
        crate::internals::toggle(world, client, mob, tank);
    }
    let Some(control) = world
        .get::<Inventory>(mob)
        .and_then(|i| i.back)
        .filter(|&b| world.get::<ModSuit>(b).is_some())
    else {
        return;
    };
    let modules = world
        .get::<ModSuit>(control)
        .map(|s| s.modules.clone())
        .unwrap_or_default();
    for module in modules {
        match world.get::<ModModule>(module).map(|m| m.kind) {
            Some(Kind::WeaponRecall) => {
                if let Some(katana) = katana {
                    crate::modsuit::ninja::set_weapon(world, module, katana);
                }
            }
            Some(Kind::DnaLock { .. }) => crate::modsuit::ninja::set_dna_owner(world, module, mob),
            _ => {}
        }
    }
}

/// `greet` + `announce_objectives`.
fn greet(world: &mut World, client: Entity, mob: Entity, objectives: &[NinjaObjective]) {
    world.write_message(SendSound {
        client,
        sound: SoundEvent {
            file: GREETING.to_owned(),
            volume: 100.0,
            speed: 1.0,
            offset: None,
        },
    });
    tell(
        world,
        client,
        Span::Danger,
        "Я элитный наёмник из могущественного клана Паука!",
    );
    tell(
        world,
        client,
        Span::Warning,
        "Неожиданность - мое оружие. Тень - моя броня. Без них, Я - ничто.",
    );
    if let Some(&pos) = world.get::<TilePos>(mob) {
        let (w, h) = world
            .get_resource::<ss13_world::SpatialIndex>()
            .and_then(|s| s.level_size(pos.z))
            .unwrap_or((255, 255));
        let center = TilePos::new(w as i32 / 2, h as i32 / 2, pos.z);
        let dir = crate::pull::dir_between(pos, center).map_or("центре", dir_text);
        tell(
            world,
            client,
            Span::Notice,
            format!(
                "Станция находится в {dir}. Брошенный сюрикен станет отличным способом попасть \
                 туда."
            ),
        );
    }
    let texts: Vec<String> = objectives.iter().map(|o| o.text.clone()).collect();
    announce_objectives(world, client, &texts);
    world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
}

/// `dir2text` of the greeting, in Russian (the prepositional case).
fn dir_text(dir: ss13_shared::Dir) -> &'static str {
    use ss13_shared::Dir;
    match dir {
        Dir::North => "северном направлении",
        Dir::South => "южном направлении",
        Dir::East => "восточном направлении",
        Dir::West => "западном направлении",
        Dir::NorthEast => "северо-восточном направлении",
        Dir::NorthWest => "северо-западном направлении",
        Dir::SouthEast => "юго-восточном направлении",
        Dir::SouthWest => "юго-западном направлении",
    }
}

/// `IS_SPACE_NINJA(mob)`.
pub fn is_ninja(world: &World, mob: Entity) -> bool {
    world.get::<Ninja>(mob).is_some()
}

/// `objective.completed = TRUE` for the ninja's objective matching `goal`.
fn complete(world: &mut World, ninja: Entity, matches: impl Fn(NinjaGoal) -> bool) {
    if let Some(mut n) = world.get_mut::<Ninja>(ninja) {
        for o in &mut n.objectives {
            if matches(o.goal) {
                o.completed = true;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The katana

/// `ranged_interact_with_atom_secondary` of a held katana: the dash.
pub fn katana_clicks(world: &mut World, mut cursor: Local<MessageCursor<RangedClick>>) {
    let clicks: Vec<RangedClick> = cursor
        .read(world.resource::<Messages<RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        if !click.modifiers.right {
            continue;
        }
        let Some(katana) = world
            .get::<Inventory>(click.mob)
            .and_then(Inventory::active_item)
            .filter(|&k| world.get::<KatanaDash>(k).is_some())
        else {
            continue;
        };
        if world.get::<Health>(click.target).is_some() {
            attack_teleport(world, click, katana);
        } else {
            teleport(world, click.mob, katana, click.target_pos);
        }
    }
}

/// `/datum/action/innate/dash/teleport`: true when the user landed on `to`.
fn teleport(world: &mut World, user: Entity, katana: Entity, to: TilePos) -> bool {
    let t = now(world);
    let has_charge = world.get_mut::<KatanaDash>(katana).is_some_and(|mut d| {
        d.recharge(t);
        d.charges > 0
    });
    if !has_charge {
        crate::balloon::balloon_alert(world, user, user, "нет зарядов!");
        return false;
    }
    let Some(&from) = world.get::<TilePos>(user) else {
        return false;
    };
    if from.z != to.z {
        crate::balloon::balloon_alert(world, user, user, "вне поля зрения!");
        return false;
    }
    if crate::construction::tile_blocked(world, to) {
        crate::balloon::balloon_alert(world, user, user, "место назначения заблокировано!");
        return false;
    }
    let tele = crate::teleport::Tele {
        no_effects: true,
        ..default()
    };
    if !crate::teleport::do_teleport(world, user, to, tele) {
        crate::balloon::balloon_alert(world, user, user, "скачок заблокирован!");
        return false;
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(to, DASH_SOUND, 25.0, true),
    );
    if let Some(mut d) = world.get_mut::<KatanaDash>(katana) {
        d.spend(t);
    }
    true
}

/// `attack_teleport`: onto the tile before the target on the line, then a strike.
fn attack_teleport(world: &mut World, click: RangedClick, katana: Entity) {
    let line = crate::radiation::get_line(click.mob_pos, click.target_pos);
    if line.len() <= 1 {
        return;
    }
    if !teleport(world, click.mob, katana, line[line.len() - 2]) {
        return;
    }
    // `melee_attack_chain`: a click on the target from the new tile.
    world.write_message(ClickOn {
        client: click.client,
        target: click.target,
        modifiers: ClickModifiers::default(),
    });
}

// ---------------------------------------------------------------------------------------------
// The spider charge

/// Whether `item` is a spider charge.
pub fn is_spider_charge(world: &World, item: Entity) -> bool {
    path_of(world, item).is_some_and(|p| is_under(&p, SPIDER_CHARGE))
}

/// `plant_c4` + `check_loc` of a spider charge: true when the planting may start.
pub fn spider_plant_check(world: &mut World, actor: Actor, item: Entity, target: Entity) -> bool {
    if !is_ninja(world, actor.mob) {
        crate::atmos_machinery::machine_say(world, item, "Доступ запрещен.");
        return false;
    }
    let area = world.get::<SpiderCharge>(item).and_then(|c| c.area);
    let Some(area) = area else {
        crate::balloon::balloon_alert(world, actor.mob, actor.mob, "локация не установлена!");
        return false;
    };
    let here = crate::explosions::turf_of(world, target)
        .and_then(|p| world.resource::<Areas>().area_at(p));
    if here != Some(area) {
        crate::balloon::balloon_alert(world, actor.mob, actor.mob, "неверное место!");
        return false;
    }
    true
}

/// The charge is planted by `mob` (`detonator = WEAKREF(user)`).
pub fn spider_planted(world: &mut World, item: Entity, mob: Entity) {
    if let Some(mut c) = world.get_mut::<SpiderCharge>(item) {
        c.detonator = Some(mob);
    }
}

/// `c4/ninja/detonate` before the blast: false when it moved out of its area (a fresh charge
/// is left where it was and this one is gone, nothing explodes).
pub fn spider_detonation(world: &mut World, item: Entity, target: Entity) -> bool {
    let Some(charge) = world.get::<SpiderCharge>(item).copied() else {
        return true;
    };
    let pos = crate::explosions::turf_of(world, target);
    let here = pos.and_then(|p| world.resource::<Areas>().area_at(p));
    if charge.area.is_none() || here != charge.area || charge.detonator.is_none() {
        if let Some(pos) = pos
            && let Some(fresh) = crate::construction::spawn_obj(world, SPIDER_CHARGE, pos)
        {
            world.entity_mut(fresh).insert(SpiderCharge {
                area: charge.area,
                detonator: None,
            });
            crate::atmos_machinery::machine_say(world, fresh, "недопустимая локация!");
        }
        return false;
    }
    true
}

/// After the blast: the planter's objective is done.
pub fn spider_exploded(world: &mut World, charge: SpiderCharge) {
    if let Some(ninja) = charge.detonator {
        complete(world, ninja, |g| matches!(g, NinjaGoal::PlantExplosive(_)));
    }
}

// ---------------------------------------------------------------------------------------------
// The hacker module (`ninjadrain_act`)

/// The timed hacks (`RaiderJob::Hack`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HackJob {
    SecurityRecords,
    Comms,
    ResearchServer,
    Cyborg,
    Bot,
    /// A drain step; `total` joules so far.
    Apc {
        total: u64,
    },
    Smes {
        total: u64,
    },
    Cell,
}

/// The worn suit and its hacker module, if the gloves are sealed.
fn active_hacker(world: &World, mob: Entity) -> Option<(Entity, Entity)> {
    let control = worn_suit(world, mob)?;
    let suit = world.get::<ModSuit>(control)?;
    let module = suit.modules.iter().copied().find(|&m| {
        world
            .get::<ModModule>(m)
            .is_some_and(|s| s.kind == Kind::Hacker && s.part_activated)
    })?;
    Some((control, module))
}

/// `AI_notify_hack`: every AI hears of the attempt.
fn ai_notify_hack(world: &mut World, machine: Entity) {
    let area = crate::explosions::turf_of(world, machine)
        .and_then(|p| world.resource::<Areas>().area_at(p))
        .map(|a| crate::shuttle::area_name(world, a));
    let text = match area {
        Some(name) => format!("Сетевая тревога: Обнаружена попытка взлома в {name}."),
        None => "Сетевая тревога: Обнаружена попытка взлома. Невозможно определить местоположение."
            .to_owned(),
    };
    let mut ais = world.query_filtered::<Entity, With<crate::silicon::ai::AiCore>>();
    let ais: Vec<Entity> = ais.iter(world).collect();
    for ai in ais {
        tell_mob(world, ai, Span::Danger, text.clone());
    }
}

/// `charge_message`.
fn charge_message(world: &mut World, mob: Entity, drained: Entity, amount: f64) {
    let name = crate::medical::name_of(world, drained);
    if amount > 0.0 {
        tell_mob(
            world,
            mob,
            Span::Notice,
            format!("Получено {} единиц энергии из {name}.", amount.round()),
        );
    } else {
        tell_mob(
            world,
            mob,
            Span::Warning,
            format!("У {name} закончилась энергия, вы должны найти другой источник!"),
        );
    }
}

fn sparks(world: &mut World, at: Entity) {
    if let Some(pos) = crate::explosions::turf_of(world, at) {
        crate::effects::particles::do_sparks(world, 5, false, pos);
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, SPARKS_SOUND, 50.0, true),
        );
    }
}

/// `can_hack` of the consoles: powered and in a station area.
fn can_hack(world: &mut World, mob: Entity, machine: Entity) -> bool {
    if world.get::<crate::power::Unpowered>(machine).is_some() {
        crate::balloon::balloon_alert(world, mob, machine, "нельзя взломать!");
        return false;
    }
    let valid = crate::explosions::turf_of(world, machine)
        .map(|p| crate::teleport::area_flags(world, p).1 & VALID_TERRITORY != 0)
        .unwrap_or(false);
    if !valid {
        crate::balloon::balloon_alert(world, mob, machine, "сигнал слишком слабый!");
        return false;
    }
    true
}

fn start(world: &mut World, ctx: &ClickCtx, delay_ds: f32, job: HackJob) {
    let actor = Actor {
        client: ctx.client,
        mob: ctx.mob,
        item: None,
    };
    crate::tools::do_after(
        world,
        actor,
        ctx.target,
        delay_ds,
        0,
        Job::Raiders(RaiderJob::Hack(job)),
    );
}

/// `hacker/hack` on `COMSIG_LIVING_UNARMED_ATTACK` with a right click: `ninjadrain_act`.
pub fn hack(world: &mut World, ctx: &ClickCtx) -> bool {
    if !ctx.modifiers.right || ctx.held.is_some() || ctx.target == ctx.mob {
        return false;
    }
    let Some((control, module)) = active_hacker(world, ctx.mob) else {
        return false;
    };
    let target = ctx.target;
    crate::forensics::server::add_fingerprint(world, target, ctx.mob, false);
    let door_kind = world.get::<crate::doors::Door>(target).map(|d| d.kind);
    if let Some(kind) = door_kind {
        use ss13_mapload::door::DoorKind;
        if !matches!(kind, DoorKind::Airlock | DoorKind::Windoor) {
            return false;
        }
        let cost = DEFAULT_CHARGE_DRAIN * 5.0;
        let actor = Actor {
            client: ctx.client,
            mob: ctx.mob,
            item: None,
        };
        if crate::modsuit::server::check_charge(world, control, cost)
            && crate::emag::emag_act(world, actor, module, target)
        {
            subtract_charge(world, control, cost);
            if kind == DoorKind::Airlock {
                door_hacked(world, ctx.mob, module);
            }
        }
        return true;
    }
    if world
        .get::<crate::records::console::RecordsConsole>(target)
        .is_some_and(|c| c.kind == crate::records::console::RecordsKind::Security)
    {
        if can_hack(world, ctx.mob, target) {
            ai_notify_hack(world, target);
            start(world, ctx, 200.0, HackJob::SecurityRecords);
        }
        return true;
    }
    if world
        .get::<crate::comms_server::CommsConsole>(target)
        .is_some()
    {
        if world
            .get::<HackerState>(module)
            .is_some_and(|s| s.comms_hacked)
        {
            return false;
        }
        if can_hack(world, ctx.mob, target) {
            ai_notify_hack(world, target);
            start(world, ctx, 300.0, HackJob::Comms);
        }
        return true;
    }
    if world
        .get::<crate::science::points::ResearchServer>(target)
        .is_some()
    {
        tell(
            world,
            ctx.client,
            Span::Notice,
            "Обнаружены исследовательские заметки. Повреждение данных...",
        );
        start(world, ctx, 300.0, HackJob::ResearchServer);
        return true;
    }
    if world
        .get::<crate::silicon::cyborg::Cyborg>(target)
        .is_some()
    {
        let ours = world
            .get::<crate::mobs::Faction>(target)
            .is_some_and(|f| f.0.iter().any(|x| x == ROLE_NINJA));
        if ours {
            return false;
        }
        tell_mob(
            world,
            target,
            Span::Danger,
            "Внима-***БЗЗЗЗЗЗЗЗЗЗТ*** ЗАГРУЗКА SPYDERPATCHER ВЕРСИИ 9.5.2...",
        );
        start(world, ctx, 60.0, HackJob::Cyborg);
        return true;
    }
    if world.get::<crate::power::Apc>(target).is_some() {
        let has = world
            .get::<crate::power::Apc>(target)
            .and_then(|a| a.cell)
            .is_some_and(|c| c.charge > 0.0);
        if !has {
            charge_message(world, ctx.mob, target, 0.0);
        } else {
            start(world, ctx, 10.0, HackJob::Apc { total: 0 });
        }
        return true;
    }
    if world.get::<crate::power::Smes>(target).is_some() {
        start(world, ctx, 10.0, HackJob::Smes { total: 0 });
        return true;
    }
    if world.get::<crate::mecha::core::ItemCell>(target).is_some() {
        start(world, ctx, 30.0, HackJob::Cell);
        return true;
    }
    if world
        .get::<crate::silicon::cameras::Camera>(target)
        .is_some()
    {
        let shielded = world
            .get::<crate::emp::EmpProtection>(target)
            .is_some_and(|p| p.0 & crate::emp::PROTECT_SELF != 0);
        if shielded {
            crate::balloon::balloon_alert(world, ctx.mob, target, "камера экранирована!");
            return true;
        }
        if subtract_charge(world, control, DEFAULT_CHARGE_DRAIN * 5.0) {
            crate::emp::emp_act(world, target, 1);
        }
        return true;
    }
    if world.get::<crate::bots::Bot>(target).is_some() {
        tell_mob(
            world,
            target,
            Span::BoldWarning,
            "Ваша схема внезапно начинает нагреваться!",
        );
        start(world, ctx, 15.0, HackJob::Bot);
        return true;
    }
    if world.get::<crate::human::HumanLook>(target).is_some() {
        // `/mob/living/carbon/ninjadrain_act`: the shove goes on (`return NONE`).
        if subtract_charge(world, control, DEFAULT_CHARGE_DRAIN * 10.0) {
            sparks(world, target);
            let (ninja, victim) = (
                crate::medical::name_of(world, ctx.mob),
                crate::medical::name_of(world, target),
            );
            let others = format!("{ninja} бьёт {victim} током своим касанием!");
            let own = format!("{ninja} бьёт вас током своим касанием!");
            if let Some(pos) = crate::explosions::turf_of(world, target) {
                let msgs = crate::pull::visible_message(
                    world,
                    pos,
                    ChatChannel::Combat,
                    (Span::Danger, others),
                    &[(target, Span::Danger, own)],
                );
                for m in msgs {
                    world.write_message(m);
                }
            }
            crate::status_effects::knockdown(world, target, 3.0, 0.0);
            crate::status_effects::set_timed(
                world,
                target,
                crate::status_effects::Timed::Jitter,
                3.0,
                true,
            );
        }
        return false;
    }
    false
}

/// An airlock emagged by the gloves: the counter and the door objective.
fn door_hacked(world: &mut World, mob: Entity, module: Entity) {
    let doors = {
        if world.get::<HackerState>(module).is_none() {
            world.entity_mut(module).insert(HackerState::default());
        }
        let mut s = world.get_mut::<HackerState>(module).expect("inserted");
        s.doors += 1;
        s.doors
    };
    let required = world.get::<Ninja>(mob).and_then(|n| {
        n.objectives.iter().find_map(|o| match o.goal {
            NinjaGoal::DoorJack(r) => Some(r),
            _ => None,
        })
    });
    let Some(required) = required else {
        return;
    };
    if doors == required {
        crate::balloon::balloon_alert(world, mob, mob, "все двери взломаны");
    }
    if doors >= required {
        complete(world, mob, |g| matches!(g, NinjaGoal::DoorJack(_)));
    }
}

/// One drain step's amount: `rand(NINJA_MIN_DRAIN, NINJA_MAX_DRAIN)` capped by the source and
/// the suit's room. (amount, the suit is full).
pub fn drain_step(roll: f64, available: f64, suit_charge: f64, suit_max: f64) -> (f64, bool) {
    let mut drain = roll.min(available);
    let mut full = false;
    if suit_charge + drain > suit_max {
        drain = (suit_max - suit_charge).max(0.0);
        full = true;
    }
    (drain, full)
}

/// The end of a [`HackJob`] `do_after`.
pub fn complete_hack(world: &mut World, mob: Entity, action: &DoAfter, job: HackJob) {
    let target = action.target;
    if world.get_entity(target).is_err() {
        return;
    }
    let Some((control, module)) = active_hacker(world, mob) else {
        return;
    };
    match job {
        HackJob::SecurityRecords => {
            if !can_hack(world, mob, target) {
                return;
            }
            if let Some(mut manifest) =
                world.get_resource_mut::<crate::records::manifest::Manifest>()
            {
                for record in &mut manifest.general {
                    record.wanted_status = crate::records::WANTED_ARREST.to_owned();
                }
            }
            complete(world, mob, |g| g == NinjaGoal::SecurityScramble);
        }
        HackJob::Comms => {
            if !can_hack(world, mob, target) {
                return;
            }
            if world.get::<HackerState>(module).is_none() {
                world.entity_mut(module).insert(HackerState::default());
            }
            if let Some(mut s) = world.get_mut::<HackerState>(module) {
                if s.comms_hacked {
                    return;
                }
                s.comms_hacked = true;
            }
            hack_console(world, mob);
            complete(world, mob, |g| g == NinjaGoal::TerrorMessage);
        }
        HackJob::ResearchServer => {
            if let Some(mut web) = world.get_resource_mut::<crate::science::techweb::Techweb>() {
                for points in web.points.values_mut() {
                    *points = 0.0;
                }
            }
            tell(
                world,
                action.client,
                Span::Notice,
                "Саботаж завершён. Исследовательские заметки повреждены.",
            );
            complete(world, mob, |g| g == NinjaGoal::ResearchSecrets);
        }
        HackJob::Cyborg => {
            hijack_cyborg(world, target);
            complete(world, mob, |g| g == NinjaGoal::CyborgHijack);
        }
        HackJob::Bot => {
            if !subtract_charge(world, control, DEFAULT_CHARGE_DRAIN * 7.0) {
                return;
            }
            sparks(world, target);
            if let Some(pos) = crate::explosions::turf_of(world, target) {
                crate::sound::playsound(
                    world,
                    crate::sound::PlaySound::new(pos, BUZZER, 35.0, true),
                );
            }
            crate::balloon::balloon_alert(world, mob, target, "отойдите!");
            let at = now(world) + BOT_BLAST_SECS;
            world.entity_mut(target).insert(Overloaded(at));
        }
        HackJob::Apc { total } => {
            let available = world
                .get::<crate::power::Apc>(target)
                .and_then(|a| a.cell)
                .map_or(0.0, |c| c.charge);
            let total = drain_into(world, mob, control, target, available, total, |w, t, d| {
                if let Some(mut apc) = w.get_mut::<crate::power::Apc>(t)
                    && let Some(cell) = apc.cell.as_mut()
                {
                    cell.take(d, true);
                }
            });
            if let Some(total) = total {
                start_step(world, mob, action, HackJob::Apc { total });
                return;
            }
            // The loop ended: the APC is emagged and unlocked.
            if let Some(mut apc) = world.get_mut::<crate::power::Apc>(target)
                && !apc.emagged
            {
                apc.emagged = true;
                apc.locked = false;
            }
        }
        HackJob::Smes { total } => {
            let available = world
                .get::<crate::power::Smes>(target)
                .map_or(0.0, |s| s.charge);
            let total = drain_into(world, mob, control, target, available, total, |w, t, d| {
                if let Some(mut smes) = w.get_mut::<crate::power::Smes>(t) {
                    smes.charge = (smes.charge - d).max(0.0);
                }
            });
            if let Some(total) = total {
                start_step(world, mob, action, HackJob::Smes { total });
            }
        }
        HackJob::Cell => {
            let available = world
                .get::<crate::mecha::core::ItemCell>(target)
                .map_or(0.0, |c| c.0.charge);
            let room = (max_charge(world, control) - charge(world, control)).max(0.0);
            let drained = available.min(room);
            add_charge(world, control, drained);
            if let Some(mut c) = world.get_mut::<crate::mecha::core::ItemCell>(target) {
                c.0.take(drained, true);
            }
            charge_message(world, mob, target, drained);
        }
    }
}

/// One step of an APC or SMES drain into the suit: `Some(total)` to go on, `None` when it ended
/// (the message is told).
fn drain_into(
    world: &mut World,
    mob: Entity,
    control: Entity,
    source: Entity,
    available: f64,
    total: u64,
    take: impl FnOnce(&mut World, Entity, f64),
) -> Option<u64> {
    let mut total = total as f64;
    if available <= 0.0 {
        charge_message(world, mob, source, total);
        return None;
    }
    let roll = with_rng(world, |rng| {
        rng.range(NINJA_MIN_DRAIN as i64, NINJA_MAX_DRAIN as i64)
    }) as f64;
    let (drain, full) = drain_step(
        roll,
        available,
        charge(world, control),
        max_charge(world, control),
    );
    sparks(world, source);
    take(world, source, drain);
    add_charge(world, control, drain);
    total += drain;
    if full || available - drain <= 0.0 {
        charge_message(world, mob, source, total);
        return None;
    }
    Some(total as u64)
}

/// The next step of a drain loop on the same target.
fn start_step(world: &mut World, mob: Entity, action: &DoAfter, job: HackJob) {
    let actor = Actor {
        client: action.client,
        mob,
        item: None,
    };
    crate::tools::do_after(
        world,
        actor,
        action.target,
        10.0,
        0,
        Job::Raiders(RaiderJob::Hack(job)),
    );
}

/// `/mob/living/silicon/robot/ninjadrain_charge`: the clan's laws and faction, cut from its
/// AI.
fn hijack_cyborg(world: &mut World, borg: Entity) {
    sparks(world, borg);
    tell_mob(
        world,
        borg,
        Span::Danger,
        "ЗАГРУЗКА ЗАВЕРШЕНА. ОБНАРУЖЕНА НОВАЯ МОДЕЛЬ КИБОРГА. УСТАНОВКА...",
    );
    world
        .entity_mut(borg)
        .insert(crate::mobs::Faction(vec![ROLE_NINJA.to_owned()]));
    if let Some(mut c) = world.get_mut::<crate::silicon::cyborg::Cyborg>(borg) {
        c.lawupdate = false;
    }
    crate::silicon::cyborg::disconnect(world, borg);
    let laws = world
        .get_resource::<crate::silicon::Silicons>()
        .and_then(|s| s.0.lawset(NINJA_LAWS).cloned());
    if let Some(set) = laws {
        world
            .entity_mut(borg)
            .insert(crate::silicon::laws::Laws::from_lawset(&set));
    }
}

/// `/obj/machinery/computer/communications/hack_console`: a sleeper agent, or pirates or
/// fugitives when enough ghosts watch and the shuttle is idle.
fn hack_console(world: &mut World, hacker: Entity) {
    use super::super::midround;
    let ghosts = crate::ghost_poll::ghosts(world).len();
    let mut options = vec![Hack::Sleeper];
    if midround::shuttle_idle_or_recalled(world) {
        if ghosts >= 4 {
            options.push(Hack::Pirate);
        }
        if ghosts >= 6 {
            options.push(Hack::Fugitives);
        }
    }
    let picked = pick(world, &options).unwrap_or(Hack::Sleeper);
    info!("{hacker} hacked a communications console, resulting in: {picked:?}");
    let command = crate::events::COMMAND_NAME;
    match picked {
        Hack::Pirate => {
            let tag = pick(world, &[pirate_tags::LIGHT, pirate_tags::HEAVY])
                .unwrap_or(pirate_tags::LIGHT);
            let _ = midround::force_midround(world, tag);
        }
        Hack::Fugitives => {
            crate::announce::Priority::new(
                "Внимание экипажу, система секторного мониторинга сообщает о массивном прыжковом \
                 следе неизвестного космического судна, направляющегося в вашу систему. \
                 Приготовьтесь к возможному контакту.",
                format!("{command}: Высокоприоритетное оповещение"),
            )
            .send(world);
            let _ = midround::force_midround(world, midround::FUGITIVES);
        }
        Hack::Sleeper => {
            crate::announce::Priority::new(
                "Внимание экипажу, похоже, зафиксирован взлом системы телекоммуникаций с \
                 последующей передачей неизвестного сигнала.",
                format!("{command}: Высокоприоритетное оповещение"),
            )
            .send(world);
            if midround::force_midround(world, midround::MIDROUND_TRAITOR).is_err() {
                world
                    .resource_mut::<midround::Midround>()
                    .queued_latejoin
                    .push(midround::LATEJOIN_TRAITOR);
            }
        }
    }
}

mod pirate_tags {
    pub const LIGHT: &str = super::super::super::midround::LIGHT_PIRATES;
    pub const HEAVY: &str = super::super::super::midround::HEAVY_PIRATES;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hack {
    Pirate,
    Fugitives,
    Sleeper,
}

/// Bots the gloves overloaded go off (`explosion(src, 0, 1, 2, 3)`).
fn overloaded_bots(world: &mut World) {
    let t = now(world);
    let mut q = world.query::<(Entity, &Overloaded)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, o)| o.0 <= t)
        .map(|(e, _)| e)
        .collect();
    for bot in due {
        world.entity_mut(bot).remove::<Overloaded>();
        if let Some(pos) = crate::explosions::turf_of(world, bot) {
            crate::explosions::explosion(
                world,
                pos,
                crate::explosions::Explosion::new(0, 1, 2).flash(3),
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The round end

/// `check_completion` of one objective.
fn done(world: &World, mob: Entity, o: &NinjaObjective) -> bool {
    match o.goal {
        NinjaGoal::Survive => world.get::<Stat>(mob).is_some_and(|s| *s != Stat::Dead),
        _ => o.completed,
    }
}

/// Each ninja and its objectives.
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let mut q = world.query::<(Entity, &Ninja)>();
    let mut all: Vec<(Entity, Ninja)> = q.iter(world).map(|(e, n)| (e, n.clone())).collect();
    if all.is_empty() {
        return Vec::new();
    }
    all.sort_by_key(|(e, _)| *e);
    let mut out = vec![line(vec![ChatSegment::new(
        Span::Bold,
        "Космическими ниндзя были:",
    )])];
    for (mob, ninja) in all {
        out.push(super::super::traitor::print_player(world, mob, &ninja.key));
        let mut all_done = true;
        for (i, o) in ninja.objectives.iter().enumerate() {
            let ok = done(world, mob, o);
            all_done &= ok;
            out.push(objective_line(i + 1, &o.text, ok));
        }
        out.push(line(vec![if all_done {
            ChatSegment::new(Span::GreenText, format!("{ANTAG_NAME} был успешен!"))
        } else {
            ChatSegment::new(Span::RedText, format!("{ANTAG_NAME} провалился!"))
        }]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dash_holds_three_charges_back_after_twenty_seconds() {
        let mut dash = KatanaDash::default();
        assert!(dash.spend(0.0));
        assert!(dash.spend(1.0));
        assert!(dash.spend(2.0));
        assert!(!dash.spend(3.0), "out of charges");
        dash.recharge(20.5);
        assert_eq!(dash.charges, 1);
        dash.recharge(30.0);
        assert_eq!(dash.charges, 3);
        assert!(dash.recharge_at.is_empty());
    }

    #[test]
    fn a_drain_step_stops_at_the_source_and_the_suit() {
        // Plenty on both sides: the roll.
        assert_eq!(drain_step(3000.0, 10_000.0, 0.0, 20_000.0), (3000.0, false));
        // The APC has less than the roll.
        assert_eq!(drain_step(3000.0, 500.0, 0.0, 20_000.0), (500.0, false));
        // The suit fills up.
        assert_eq!(
            drain_step(3000.0, 10_000.0, 19_000.0, 20_000.0),
            (1000.0, true)
        );
    }

    #[test]
    fn objectives_follow_tg() {
        let mut world = World::new();
        let objectives = forge_objectives(&mut world);
        // No station areas here: no bomb objective.
        let goals: Vec<NinjaGoal> = objectives.iter().map(|o| o.goal).collect();
        assert_eq!(goals[0], NinjaGoal::CyborgHijack);
        assert_eq!(goals[1], NinjaGoal::ResearchSecrets);
        let NinjaGoal::DoorJack(doors) = goals[2] else {
            panic!("door objective third");
        };
        assert!((15..=40).contains(&doors));
        assert!(objectives[2].text.contains(&doors.to_string()));
        assert_eq!(
            &goals[3..],
            &[
                NinjaGoal::SecurityScramble,
                NinjaGoal::TerrorMessage,
                NinjaGoal::Survive
            ]
        );
    }
}

//! The contractor after tg `code/modules/antagonists/traitor/contractor/` (`contractor_hub.dm`,
//! `syndicate_contract.dm`), `/datum/objective/contract`, the contract kit's uplink items
//! (`uplink/uplink_items/contractor.dm`) and the Syndicate Contractor Uplink program
//! (`crate::modpc::programs::contractor`).
//!
//! - A traitor logging into the contractor tablet's program gets a [`ContractorHub`] with six
//!   contracts ([`create_contracts`]): a living crew human each (not kidnapped before, not on the
//!   arrival shuttle, no one twice), a random station area as the dropoff, a TC payout plus an
//!   alive bonus (large 9-13, medium 6-8, small 2-4, the total lifted to 30 TC), a ransom of
//!   1800-4500 credits and a wanted message from `strings/wanted_message.json`.
//! - One contract at a time is accepted; aborting it loses it. With the contractor and the
//!   target in the dropoff area, the extraction pod lands 3.4 s later on a free tile within 3
//!   tiles ([`call_extraction`]); the target dragged into it ([`pod_entered`]) pays the payout
//!   (with the bonus if alive). Anyone else aborts the contract.
//! - The kidnapped ([`Captive`]) loses everything but the jumpsuit and shoes, goes through tg's
//!   five steps of "the pod did something to you" (omnizine, flashes, blur, confusion, dizziness,
//!   20 s of sleep) and comes back after 6 minutes to a free tile of the dropoff area with the
//!   belongings. Cargo pays the ransom (×1.1-1.3) with an announcement, the contractor's ID
//!   account gets 35% of the ransom.
//! - Telecrystals are redeemed into the contractor's hands; the contract kit unlocks the
//!   contractor shop in the uplink (contract reroll, contractor pinpointer).
//! - The traitor's round-end report counts the completed contracts and their TC.
// shortcut: no black market hostage listing (the black market sells no hostages), no Fulton kit
// or contractor reinforcement (no Fulton or antag spawner), no species survival kit for the
// kidnapped, no souvenir achievement; the pod is an ordinary closet (it can be opened and closed
// by hand, no landing animation); the victim waits hidden inside the departed pod.

use bevy::prelude::*;
use ss13_mapload::spawner::Rng;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::InventorySlot;

use crate::closets::InCloset;
use crate::health::Stat;
use crate::human::HumanLook;
use crate::inventory::{ALL_SLOTS, Inventory};
use crate::jobs::{Jobs, PlayerJob};
use crate::tools::{now, tell_mob};

pub const CONTRACT_KIT_ITEM: &str = "/datum/uplink_item/bundles_tc/contract_kit";
pub const REROLL_ITEM: &str = "/datum/uplink_item/contractor/reroll";
/// The contractor shop the kit unlocks (`subtypesof(/datum/uplink_item/contractor)` this port
/// has).
pub const SHOP_ITEMS: [&str; 2] = [REROLL_ITEM, "/datum/uplink_item/contractor/pinpointer"];
pub const SHOP_ROOT: &str = "/datum/uplink_item/contractor";
pub const EXTRACTION_POD: &str = "/obj/structure/closet/supplypod/extractionpod";
/// `lowest_TC_threshold`.
const LOWEST_TC_THRESHOLD: u32 = 30;
/// `COME_BACK_FROM_CAPTURE_TIME`.
pub const COME_BACK_SECS: f64 = 6.0 * 60.0;
/// `find_obstruction_free_location(3, ...)`.
const LANDING_RANGE: i32 = 3;
/// `finish_enter` and the first victim step, after the pod leaves.
const AFTER_ENTER_SECS: f64 = 3.0;
const STARTUP_SOUND: &str = "sound/music/antag/contractstartup.ogg";
const CONFIRM_SOUND: &str = "sound/effects/confirmdropoff.ogg";
const ERROR_SOUND: &str = "sound/machines/uplink/uplinkerror.ogg";
const OMNIZINE: &str = "/datum/reagent/medicine/omnizine";
const WANTED_FILE: &str = "strings/wanted_message.json";

/// `CONTRACT_STATUS_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractStatus {
    Inactive = 1,
    Active = 2,
    Extracting = 4,
    Complete = 5,
    Aborted = 6,
}

/// `CONTRACT_PAYOUT_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Payout {
    Large,
    Medium,
    Small,
}

/// `/datum/syndicate_contract` with its `/datum/objective/contract`.
#[derive(Clone, Debug, PartialEq)]
pub struct Contract {
    /// 1-based, as tgui sends it.
    pub id: u32,
    pub payout_type: Payout,
    pub target: Option<Entity>,
    pub target_name: String,
    pub target_rank: String,
    pub payout: u32,
    pub payout_bonus: u32,
    /// The dropoff area, its name and a tile of it (for the direction).
    pub dropoff: Option<Entity>,
    pub dropoff_name: String,
    pub dropoff_pos: Option<TilePos>,
    pub ransom: u32,
    pub status: ContractStatus,
    pub wanted_message: String,
}

/// `/datum/contractor_hub`, on the contractor's body.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ContractorHub {
    pub contracts: Vec<Contract>,
    /// Index into `contracts`.
    pub current: Option<usize>,
    pub assigned_targets: Vec<Entity>,
    pub completed: u32,
    pub tc_paid: u32,
    pub tc_to_redeem: u32,
}

/// `TRAIT_HAS_BEEN_KIDNAPPED`: never a contract target again.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Kidnapped;

/// A landed extraction pod (`/obj/structure/closet/supplypod/extractionpod`).
#[derive(Component, Clone, Copy, Debug)]
pub struct ExtractionPod {
    pub contractor: Entity,
    pub contract: usize,
}

/// Pods on their way: (land at, tile, contractor, contract index).
#[derive(Resource, Clone, Debug, Default)]
pub struct IncomingPods(pub Vec<(f64, TilePos, Entity, usize)>);

/// A kidnapped mob waiting in the departed pod.
#[derive(Component, Clone, Debug)]
pub struct Captive {
    pub pod: Entity,
    pub contractor: Entity,
    pub contract: usize,
    /// `handle_victim_experience` level of the next step and when.
    pub stage: u8,
    pub next_stage: f64,
    pub return_at: f64,
    /// `finish_enter`: the ransom share, once.
    pub pay_at: Option<f64>,
    pub belongings: Vec<Entity>,
    pub dropoff: Option<Entity>,
    pub dropoff_pos: TilePos,
}

pub struct ContractorPlugin;

impl Plugin for ContractorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<IncomingPods>().add_systems(
            Update,
            (land_pods, pods_entered, captives).run_if(
                |p: Res<IncomingPods>,
                 pods: Query<(), Or<(With<ExtractionPod>, With<Captive>)>>| {
                    !p.0.is_empty() || !pods.is_empty()
                },
            ),
        );
    }
}

fn with_rng<R>(world: &mut World, f: impl FnOnce(&mut World, &mut Rng) -> R) -> R {
    let mut rng = world
        .remove_resource::<super::dynamic::DynamicRng>()
        .map_or_else(|| Rng::new(0), |r| r.0);
    let out = f(world, &mut rng);
    world.insert_resource(super::dynamic::DynamicRng(rng));
    out
}

fn pick<'a, T>(list: &'a [T], rng: &mut Rng) -> Option<&'a T> {
    (!list.is_empty()).then(|| &list[rng.range(0, list.len() as i64 - 1) as usize])
}

fn area_type(world: &World, area: Entity) -> Option<String> {
    world
        .get::<ss13_mapload::DmPower>(area)
        .map(|p| p.path.clone())
}

fn area_at(world: &World, pos: TilePos) -> Option<Entity> {
    world.get_resource::<ss13_mapload::Areas>()?.area_at(pos)
}

/// `GLOB.the_station_areas` with their open, unblocked tiles.
// Cost: one scan of the station levels per contract generation (a login or a reroll).
fn station_areas(world: &World) -> Vec<(Entity, Vec<TilePos>)> {
    let levels = crate::events::controller::station_levels(world);
    let index = world.resource::<ss13_world::SpatialIndex>();
    let Some(areas) = world.get_resource::<ss13_mapload::Areas>() else {
        return Vec::new();
    };
    let mut out: Vec<(Entity, Vec<TilePos>)> = Vec::new();
    for z in levels {
        let Some((w, h)) = index.level_size(z) else {
            continue;
        };
        for y in 1..=h as i32 {
            for x in 1..=w as i32 {
                let pos = TilePos::new(x, y, z);
                let Some(area) = areas.area_at(pos) else {
                    continue;
                };
                if !area_type(world, area).is_some_and(|t| is_under(&t, "/area/station"))
                    || crate::construction::is_space(world, pos)
                    || crate::construction::tile_blocked(world, pos)
                {
                    continue;
                }
                match out.iter_mut().find(|(a, _)| *a == area) {
                    Some((_, tiles)) => tiles.push(pos),
                    None => out.push((area, vec![pos])),
                }
            }
        }
    }
    out
}

fn is_under(path: &str, root: &str) -> bool {
    ss13_mapload::fidelity::is_under(path, root)
}

/// The words of `wanted_message.json`: (base, verb, (noun, weight), (location, weight)).
type Wanted = (
    Vec<String>,
    Vec<String>,
    Vec<(String, u32)>,
    Vec<(String, u32)>,
);

fn wanted_words(world: &World) -> Option<Wanted> {
    let source = world
        .get_resource::<ss13_mapload::MapLoadPlugin>()?
        .source
        .clone();
    let text = ss13_data::read_to_string(&source.join(WANTED_FILE)).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let list = |key: &str| -> Vec<String> {
        json.get(key)
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let weighted = |key: &str| -> Vec<(String, u32)> {
        json.get(key)
            .and_then(|v| v.as_object())
            .map(|o| {
                o.iter()
                    .map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(1) as u32))
                    .collect()
            })
            .unwrap_or_default()
    };
    Some((
        list("basemessage"),
        list("verb"),
        weighted("noun"),
        weighted("location"),
    ))
}

fn pick_weighted(list: &[(String, u32)], rng: &mut Rng) -> String {
    let weights: Vec<u32> = list.iter().map(|(_, w)| *w).collect();
    super::dynamic::pick_weight(&weights, rng)
        .map(|i| list[i].0.clone())
        .unwrap_or_default()
}

/// `/datum/objective/contract/find_target` + `is_valid_target`: living crew humans who are not
/// the contractor, not kidnapped before, not on the arrival shuttle and not in `blacklist`.
fn possible_targets(world: &mut World, owner: Entity, blacklist: &[Entity]) -> Vec<Entity> {
    let mut q = world.query_filtered::<(Entity, &Stat), (With<PlayerJob>, With<HumanLook>)>();
    let mut out: Vec<Entity> = q
        .iter(world)
        .filter(|(e, stat)| *e != owner && **stat != Stat::Dead && !blacklist.contains(e))
        .map(|(e, _)| e)
        .collect();
    out.retain(|&e| {
        world.get::<Kidnapped>(e).is_none()
            && !crate::explosions::turf_of(world, e)
                .and_then(|p| area_at(world, p))
                .and_then(|a| area_type(world, a))
                .is_some_and(|t| is_under(&t, "/area/shuttle/arrival"))
    });
    out.sort();
    out
}

fn job_title(world: &World, mob: Entity) -> Option<String> {
    let job = world.get::<PlayerJob>(mob)?.0;
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

/// `/datum/syndicate_contract/proc/generate`.
fn generate(
    world: &mut World,
    owner: Entity,
    blacklist: &[Entity],
    payout_type: Payout,
    areas: &[(Entity, Vec<TilePos>)],
    wanted: Option<&Wanted>,
    rng: &mut Rng,
) -> Contract {
    let targets = possible_targets(world, owner, blacklist);
    let target = pick(&targets, rng).copied();
    let target_name = target
        .map(|t| crate::security::name_of(world, t))
        .unwrap_or_default();
    // `find_record(target.name).rank`.
    let target_rank = target
        .and_then(|t| job_title(world, t))
        .map(|title| {
            ss13_tgui::interfaces::ru_jobs::job_ru(&title).map_or(title.clone(), str::to_owned)
        })
        .unwrap_or_else(|| "Неизвестно".to_owned());
    let payout_bonus = match payout_type {
        Payout::Large => rng.range(9, 13),
        Payout::Medium => rng.range(6, 8),
        Payout::Small => rng.range(2, 4),
    } as u32;
    let payout = rng.range(0, 2) as u32;
    let (dropoff, dropoff_pos) = pick(areas, rng)
        .map(|(a, tiles)| (Some(*a), pick(tiles, rng).copied()))
        .unwrap_or((None, None));
    let dropoff_name = dropoff_pos
        .map(|p| crate::bots::area_name(world, p))
        .unwrap_or_default();
    let ransom = 100 * rng.range(18, 45) as u32;
    let wanted_message = wanted
        .map(|(base, verb, noun, location)| {
            format!(
                "{} {} {} {}.",
                pick(base, rng).cloned().unwrap_or_default(),
                pick(verb, rng).cloned().unwrap_or_default(),
                pick_weighted(noun, rng),
                pick_weighted(location, rng)
            )
        })
        .unwrap_or_default();
    Contract {
        id: 0,
        payout_type,
        target,
        target_name,
        target_rank,
        payout,
        payout_bonus,
        dropoff,
        dropoff_name,
        dropoff_pos,
        ransom,
        status: ContractStatus::Inactive,
        wanted_message,
    }
}

/// `/datum/contractor_hub/proc/create_contracts`.
pub fn create_contracts(world: &mut World, owner: Entity, hub: &mut ContractorHub) {
    let areas = station_areas(world);
    let wanted = wanted_words(world);
    with_rng(world, |world, rng| {
        let mut to_generate = vec![
            Payout::Large,
            Payout::Medium,
            Payout::Small,
            Payout::Small,
            Payout::Small,
            Payout::Small,
        ];
        // "What the fuck": tg cuts the list to the crew count this way.
        let crew = possible_targets(world, owner, &[]).len();
        if to_generate.len() > crew {
            to_generate.drain(0..crew.saturating_sub(1).min(to_generate.len()));
        }
        super::dynamic::shuffle(&mut to_generate, rng);
        let mut total = 0;
        let mut lowest: Option<(u32, usize)> = None;
        for payout_type in to_generate {
            let blacklist = hub.assigned_targets.clone();
            let mut contract = generate(
                world,
                owner,
                &blacklist,
                payout_type,
                &areas,
                wanted.as_ref(),
                rng,
            );
            if let Some(t) = contract.target {
                hub.assigned_targets.push(t);
            }
            let sum = contract.payout + contract.payout_bonus;
            let index = hub.contracts.len();
            if lowest.is_none_or(|(s, _)| sum < s) {
                lowest = Some((sum, index));
            }
            total += sum;
            contract.id = index as u32 + 1;
            hub.contracts.push(contract);
        }
        if total < LOWEST_TC_THRESHOLD
            && let Some((_, i)) = lowest
        {
            hub.contracts[i].payout_bonus += LOWEST_TC_THRESHOLD - total;
        }
    });
}

/// `PRG_login`: a traitor gets a hub (and the startup jingle) the first time.
pub fn login(world: &mut World, user: Entity) -> bool {
    if world.get::<super::traitor::Traitor>(user).is_none() {
        return false;
    }
    if world.get::<ContractorHub>(user).is_none() {
        let mut hub = ContractorHub::default();
        create_contracts(world, user, &mut hub);
        world.entity_mut(user).insert(hub);
        play_local(world, user, STARTUP_SOUND, 100.0);
    }
    true
}

/// `user.playsound_local(user, ...)`: here the sound plays on the user's tile.
fn play_local(world: &mut World, user: Entity, sound: &str, vol: f32) {
    if let Some(pos) = crate::explosions::turf_of(world, user) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, vol, false));
    }
}

/// `PRG_contract-accept`: an inactive contract becomes the current one.
pub fn accept(world: &mut World, contractor: Entity, id: u32) -> bool {
    let Some(mut hub) = world.get_mut::<ContractorHub>(contractor) else {
        return false;
    };
    // tg accepts any id while another runs; the window offers it only with none running.
    if hub.current.is_some() {
        return false;
    }
    let Some(i) = hub.contracts.iter().position(|c| c.id == id) else {
        return false;
    };
    if hub.contracts[i].status != ContractStatus::Inactive {
        return false;
    }
    hub.contracts[i].status = ContractStatus::Active;
    hub.current = Some(i);
    true
}

/// `PRG_contract_abort`.
pub fn abort(world: &mut World, contractor: Entity) -> bool {
    let Some(mut hub) = world.get_mut::<ContractorHub>(contractor) else {
        return false;
    };
    let Some(i) = hub.current.take() else {
        return false;
    };
    hub.contracts[i].status = ContractStatus::Aborted;
    true
}

/// `PRG_redeem_TC`: the telecrystals into the contractor's hands, else at their feet.
pub fn redeem(world: &mut World, contractor: Entity) -> bool {
    let amount = world
        .get::<ContractorHub>(contractor)
        .map_or(0, |h| h.tc_to_redeem);
    if amount == 0 {
        play_local(world, contractor, ERROR_SOUND, 50.0);
        return false;
    }
    let Some(stack) = crate::construction::spawn_detached(world, super::uplink::TELECRYSTAL) else {
        warn!("no prototype for {}", super::uplink::TELECRYSTAL);
        return false;
    };
    world.flush();
    world
        .entity_mut(stack)
        .insert(crate::tools::Stack { amount });
    let in_hand =
        super::uplink::hand_or_floor(world, contractor, stack, super::uplink::TELECRYSTAL);
    tell_mob(
        world,
        contractor,
        Span::Notice,
        if in_hand == Some(true) {
            "Your payment materializes into your hands!"
        } else {
            "Your payment materializes onto the floor."
        },
    );
    if let Some(mut hub) = world.get_mut::<ContractorHub>(contractor) {
        hub.tc_paid += amount;
        hub.tc_to_redeem = 0;
    }
    true
}

/// `PRG_call_extraction`: `Err` is the error the window shows.
pub fn call_extraction(world: &mut World, contractor: Entity) -> Result<(), &'static str> {
    let Some(hub) = world.get::<ContractorHub>(contractor) else {
        return Err("no hub");
    };
    let Some(i) = hub.current else {
        return Err("no contract");
    };
    let contract = hub.contracts[i].clone();
    if contract.status == ContractStatus::Extracting {
        play_local(world, contractor, ERROR_SOUND, 50.0);
        return Err(
            "Already extracting... Place the target into the pod. If the pod was destroyed, this \
             contract is no longer possible.",
        );
    }
    let failed = "Either both you or your target aren't at the dropoff location, or the pod \
                  hasn't got a valid place to land. Clear space, or make sure you're both inside.";
    // `dropoff_check`: both in the dropoff area.
    let here = crate::explosions::turf_of(world, contractor);
    let in_dropoff = |world: &World, pos: Option<TilePos>| {
        pos.and_then(|p| area_at(world, p)).is_some()
            && pos.and_then(|p| area_at(world, p)) == contract.dropoff
    };
    let target_pos = contract
        .target
        .and_then(|t| crate::explosions::turf_of(world, t));
    if !in_dropoff(&*world, here) || !in_dropoff(&*world, target_pos) {
        play_local(world, contractor, ERROR_SOUND, 50.0);
        return Err(failed);
    }
    let center = here.expect("checked above");
    // `find_obstruction_free_location(3, user, dropoff)`.
    let mut free = Vec::new();
    for dy in -LANDING_RANGE..=LANDING_RANGE {
        for dx in -LANDING_RANGE..=LANDING_RANGE {
            let pos = TilePos::new(center.x + dx, center.y + dy, center.z);
            if area_at(world, pos) == contract.dropoff
                && !crate::construction::is_space(world, pos)
                && !crate::construction::tile_blocked(world, pos)
            {
                free.push(pos);
            }
        }
    }
    let Some(pos) = with_rng(world, |_, rng| pick(&free, rng).copied()) else {
        play_local(world, contractor, ERROR_SOUND, 50.0);
        return Err(failed);
    };
    let land_at = now(world) + crate::cargo::supplypod::TRANSIT_SECS;
    world
        .resource_mut::<IncomingPods>()
        .0
        .push((land_at, pos, contractor, i));
    if let Some(mut hub) = world.get_mut::<ContractorHub>(contractor) {
        hub.contracts[i].status = ContractStatus::Extracting;
    }
    play_local(world, contractor, CONFIRM_SOUND, 100.0);
    Ok(())
}

/// The pod lands open on its tile (`/obj/effect/pod_landingzone`).
fn land_pods(world: &mut World) {
    let t = now(world);
    let due: Vec<(f64, TilePos, Entity, usize)> = {
        let mut pods = world.resource_mut::<IncomingPods>();
        let (due, rest) = pods.0.drain(..).partition(|(at, ..)| *at <= t);
        pods.0 = rest;
        due
    };
    for (_, pos, contractor, contract) in due {
        let Some(pod) = crate::construction::spawn_obj(world, EXTRACTION_POD, pos) else {
            warn!("no prototype for {EXTRACTION_POD}");
            continue;
        };
        world.flush();
        // A spawned closet builds its state itself (as `jobs::security::spawn_locker`).
        let closet = {
            let icons = world.get_resource::<ss13_mapload::IconIndex>();
            let entity = world.entity(pod);
            match (
                entity.get::<ss13_mapload::DmCloset>(),
                entity.get::<ss13_mapload::DmAtom>(),
            ) {
                (Some(dm), Some(atom)) => Some(crate::closets::Closet::from_dm(dm, atom, icons)),
                _ => None,
            }
        };
        if let Some(closet) = closet {
            world.entity_mut(pod).insert(closet);
        }
        world.entity_mut(pod).insert(ExtractionPod {
            contractor,
            contract,
        });
        crate::closets::open(world, pod);
        crate::closets::refresh_look(world, pod);
    }
}

/// `COMSIG_ATOM_ENTERED` of a landed pod: a living mob inside.
fn pods_entered(world: &mut World) {
    let mut q = world.query::<(Entity, &InCloset, &Stat)>();
    let inside: Vec<(Entity, Entity)> = q.iter(world).map(|(mob, c, _)| (mob, c.closet)).collect();
    for (mob, closet) in inside {
        let Some(pod) = world.get::<ExtractionPod>(closet).copied() else {
            continue;
        };
        if world.get::<Captive>(mob).is_some() {
            continue;
        }
        enter_check(world, closet, pod, mob);
    }
}

/// `/datum/syndicate_contract/proc/enter_check`.
pub fn enter_check(world: &mut World, pod_entity: Entity, pod: ExtractionPod, victim: Entity) {
    let Some(mut hub) = world.get::<ContractorHub>(pod.contractor).cloned() else {
        return;
    };
    let Some(contract) = hub.contracts.get(pod.contract).cloned() else {
        return;
    };
    let alive = world.get::<Stat>(victim).is_some_and(|s| *s != Stat::Dead);
    let done = contract.target == Some(victim);
    {
        let c = &mut hub.contracts[pod.contract];
        if done {
            hub.tc_to_redeem += contract.payout;
            hub.completed += 1;
            if alive {
                hub.tc_to_redeem += contract.payout_bonus;
            }
            c.status = ContractStatus::Complete;
        } else {
            // "Sending a target that wasn't even yours is as good as just aborting it".
            c.status = ContractStatus::Aborted;
        }
    }
    if hub.current == Some(pod.contract) {
        hub.current = None;
    }
    world.entity_mut(pod.contractor).insert(hub);
    // Everything but the jumpsuit and shoes goes into the pod.
    let mut belongings = Vec::new();
    if let Some(pos) = world.get::<TilePos>(victim).copied() {
        for slot in ALL_SLOTS {
            if matches!(slot, InventorySlot::Uniform | InventorySlot::Shoes) {
                continue;
            }
            let Some(item) = world.get::<Inventory>(victim).and_then(|i| i.slot(slot)) else {
                continue;
            };
            crate::inventory::unequip(world, victim, pos, slot);
            crate::storage::put_inside(world, item, pod_entity);
            belongings.push(item);
        }
        crate::inventory::refresh_look(world, victim);
    }
    // `start_exit_sequence`: the pod leaves with the victim.
    world.entity_mut(pod_entity).remove::<TilePos>();
    world.entity_mut(pod_entity).remove::<ExtractionPod>();
    let t = now(world);
    let dropoff_pos = contract
        .dropoff_pos
        .or_else(|| world.get::<TilePos>(victim).copied())
        .unwrap_or_default();
    world.entity_mut(victim).insert((
        Kidnapped,
        Captive {
            pod: pod_entity,
            contractor: pod.contractor,
            contract: pod.contract,
            stage: 0,
            next_stage: t + AFTER_ENTER_SECS,
            return_at: t + AFTER_ENTER_SECS + COME_BACK_SECS,
            pay_at: done.then_some(t + AFTER_ENTER_SECS),
            belongings,
            dropoff: contract.dropoff,
            dropoff_pos,
        },
    ));
    // `process_capture(ransom * rand(11, 13) / 10)`: cargo pays.
    let price = with_rng(world, |_, rng| {
        contract.ransom as i64 * rng.range(11, 13) / 10
    });
    process_capture(world, price);
    info!(
        "{victim} was extracted for contract {} of {}",
        contract.id, pod.contractor
    );
}

/// `/mob/living/proc/process_capture`: the ransom off the cargo budget, announced.
fn process_capture(world: &mut World, ransom: i64) {
    if ransom <= 0 {
        return;
    }
    if let Some(mut economy) = world.get_resource_mut::<crate::cargo::economy::Economy>()
        && let Some(cargo) = economy.department_mut(crate::cargo::economy::ACCOUNT_CAR)
    {
        let taken = ransom.min(cargo.balance);
        cargo.adjust_money(-taken);
    }
    crate::announce::Priority::new(
        "Один из членов экипажа был захвачен конкурирущей организацией - нам пришлось заплатить \
         выкуп, чтобы вернуть его назад. В соответствии с политикой компании, часть средств \
         станции была изъята для компенсации.",
        "Защита активов Нанотрейзен",
    )
    .important()
    .send(world);
}

/// The kidnapped: the ransom share, the five steps, the return.
fn captives(world: &mut World) {
    let t = now(world);
    let mut q = world.query::<(Entity, &Captive)>();
    let list: Vec<(Entity, Captive)> = q.iter(world).map(|(e, c)| (e, c.clone())).collect();
    for (victim, captive) in list {
        if captive.pay_at.is_some_and(|at| at <= t) {
            pay_share(world, &captive);
            if let Some(mut c) = world.get_mut::<Captive>(victim) {
                c.pay_at = None;
            }
        }
        if t >= captive.return_at {
            return_victim(world, victim, &captive);
            continue;
        }
        if captive.stage <= 4 && t >= captive.next_stage {
            let next = victim_step(world, victim, captive.stage);
            if let Some(mut c) = world.get_mut::<Captive>(victim) {
                c.stage += 1;
                c.next_stage = next.map_or(f64::INFINITY, |secs| t + secs);
            }
        }
    }
}

/// `finish_enter`: 35% of the ransom to the contractor's ID account.
fn pay_share(world: &mut World, captive: &Captive) {
    let Some(ransom) = world
        .get::<ContractorHub>(captive.contractor)
        .and_then(|h| h.contracts.get(captive.contract))
        .map(|c| c.ransom)
    else {
        return;
    };
    let Some(account) = id_account(world, captive.contractor) else {
        return;
    };
    let share = (f64::from(ransom) * 0.35) as i64;
    let balance = {
        let Some(mut economy) = world.get_resource_mut::<crate::cargo::economy::Economy>() else {
            return;
        };
        let Some(a) = economy.account_mut(account) else {
            return;
        };
        a.adjust_money(share);
        a.balance
    };
    crate::cargo::economy::bank_card_talk(
        world,
        account,
        &format!("Мы получили выкуп, агент. Вот ваша доля - на вашем балансе теперь {balance}cr."),
    );
}

/// `get_idcard(TRUE)?.registered_account`: the hands first, then the ID slot.
fn id_account(world: &World, mob: Entity) -> Option<u32> {
    let inventory = world.get::<Inventory>(mob)?;
    [
        inventory.hands[inventory.active],
        inventory.hands[1 - inventory.active],
        inventory.id,
    ]
    .into_iter()
    .flatten()
    .find_map(|item| crate::cargo::economy::account_of(world, item))
}

/// `handle_victim_experience(level)`: the step's effects; the seconds to the next one.
fn victim_step(world: &mut World, victim: Entity, level: u8) -> Option<f64> {
    use crate::status_effects::{Incap, Timed, adjust_timed, apply};
    use crate::vision::{adjust_eye_blur, flash_act};
    if world.get::<Stat>(victim).is_none_or(|s| *s == Stat::Dead) {
        return None;
    }
    let say = |world: &mut World, span: Span, text: &str| tell_mob(world, victim, span, text);
    match level {
        0 => {
            add_omnizine(world, victim, 20.0);
            flash_act(world, victim, 1, false, false, 2.5);
            adjust_timed(world, victim, Timed::Confusion, 1.0, None);
            adjust_eye_blur(world, victim, 5.0);
            say(world, Span::Warning, "Вы чувствуете себя странно...");
            Some(6.0)
        }
        1 => {
            say(world, Span::Warning, "Эта капсула что-то сделала с вами...");
            adjust_timed(world, victim, Timed::Dizziness, 3.5, None);
            Some(6.5)
        }
        2 => {
            say(
                world,
                Span::Warning,
                "Ваша голова раскалывается... Такое ощущение, что ваш череп сейчас лопнет!",
            );
            flash_act(world, victim, 1, false, false, 2.5);
            adjust_timed(world, victim, Timed::Confusion, 2.0, None);
            adjust_eye_blur(world, victim, 3.0);
            Some(3.0)
        }
        3 => {
            say(world, Span::Warning, "Ваша голова болит...");
            Some(10.0)
        }
        4 => {
            flash_act(world, victim, 1, false, false, 2.5);
            apply(world, victim, Incap::Unconscious, 20.0);
            say(
                world,
                Span::Hypnophrase,
                "Множество голосов отдаются эхом в вашей голове... \"Вам разум хранил множество \
                 важных секретов - мы благодарим вас за предоставления их нам. Ваша ценность \
                 упала, и вы будете выкуплены обратно на свою станцию. Мы всегда получаем свое, \
                 поэтому это только дело времени когда вы вернетесь обратно на ваш корабль...\"",
            );
            adjust_eye_blur(world, victim, 10.0);
            adjust_timed(world, victim, Timed::Dizziness, 1.5, None);
            adjust_timed(world, victim, Timed::Confusion, 2.0, None);
            None
        }
        _ => None,
    }
}

fn add_omnizine(world: &mut World, mob: Entity, amount: f32) {
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return;
    };
    let Some(id) = chem.id(OMNIZINE) else {
        return;
    };
    if let Some(mut reagents) = world.get_mut::<crate::chem::MobReagents>(mob) {
        reagents
            .blood
            .add_reagent(&chem, id, amount, 300.0, None, None, false, false);
    }
}

/// `return_victim` + `returnal_side_effects`: a free tile of the dropoff area (near the
/// remembered one), the belongings with them.
fn return_victim(world: &mut World, victim: Entity, captive: &Captive) {
    let center = captive.dropoff_pos;
    let mut free = Vec::new();
    for dy in -7..=7 {
        for dx in -7..=7 {
            let pos = TilePos::new(center.x + dx, center.y + dy, center.z);
            if (captive.dropoff.is_none() || area_at(world, pos) == captive.dropoff)
                && !crate::construction::is_space(world, pos)
                && !crate::construction::tile_blocked(world, pos)
            {
                free.push(pos);
            }
        }
    }
    let dest = with_rng(world, |_, rng| pick(&free, rng).copied()).unwrap_or(center);
    world.entity_mut(victim).remove::<Captive>();
    crate::closets::let_out(world, victim);
    crate::teleport::do_teleport(
        world,
        victim,
        dest,
        crate::teleport::Tele {
            forced: true,
            no_effects: true,
            ..default()
        },
    );
    for item in captive.belongings.iter().copied() {
        if world.get_entity(item).is_ok() {
            crate::storage::take_out(world, item, dest);
        }
    }
    if world.get_entity(captive.pod).is_ok() {
        world.despawn(captive.pod);
    }
    if world.get::<Stat>(victim).is_some_and(|s| *s != Stat::Dead) {
        use crate::status_effects::{Timed, adjust_timed};
        crate::vision::flash_act(world, victim, 1, false, false, 2.5);
        crate::vision::adjust_eye_blur(world, victim, 3.0);
        adjust_timed(world, victim, Timed::Dizziness, 3.5, None);
        adjust_timed(world, victim, Timed::Confusion, 2.0, None);
    }
    info!("{victim} came back from capture to {dest:?}");
}

/// The contract reroll (`/datum/uplink_item/contractor/reroll/spawn_item`): the inactive and
/// active contracts get new targets, payouts and dropoffs, no target repeated.
pub fn reroll(world: &mut World, contractor: Entity) -> bool {
    let Some(mut hub) = world.get::<ContractorHub>(contractor).cloned() else {
        return false;
    };
    let mut targets: Vec<Entity> = hub
        .contracts
        .iter()
        .filter(|c| !matches!(c.status, ContractStatus::Active | ContractStatus::Inactive))
        .filter_map(|c| c.target)
        .collect();
    let areas = station_areas(world);
    let wanted = wanted_words(world);
    with_rng(world, |world, rng| {
        for c in &mut hub.contracts {
            if !matches!(c.status, ContractStatus::Active | ContractStatus::Inactive) {
                continue;
            }
            let mut fresh = generate(
                world,
                contractor,
                &targets,
                c.payout_type,
                &areas,
                wanted.as_ref(),
                rng,
            );
            fresh.id = c.id;
            fresh.status = c.status;
            if let Some(t) = fresh.target {
                targets.push(t);
            }
            *c = fresh;
        }
    });
    hub.assigned_targets = targets;
    world.entity_mut(contractor).insert(hub);
    true
}

/// `contractor_round_end`: the completed contracts and their TC, for the traitor's report.
pub fn roundend_lines(world: &World, traitor: Entity) -> Vec<ChatMessage> {
    let Some(hub) = world.get::<ContractorHub>(traitor) else {
        return Vec::new();
    };
    if hub.completed == 0 {
        return Vec::new();
    }
    let plural = if hub.completed > 1 {
        "контракты"
    } else {
        "контракт"
    };
    vec![ChatMessage {
        channel: ChatChannel::System,
        segments: vec![
            ChatSegment::new(Span::Plain, "Выполнил "),
            ChatSegment::new(Span::GreenText, hub.completed.to_string()),
            ChatSegment::new(Span::Plain, format!(" {plural} на общую сумму в ")),
            ChatSegment::new(
                Span::GreenText,
                format!("{} ТК", hub.tc_paid + hub.tc_to_redeem),
            ),
            ChatSegment::new(Span::Plain, "!"),
        ],
    }]
}

/// `dir2text(get_dir(from, to))`, upper case.
pub fn dir_word(from: TilePos, to: TilePos) -> &'static str {
    match ((to.x - from.x).signum(), (to.y - from.y).signum()) {
        (0, 1) => "NORTH",
        (0, -1) => "SOUTH",
        (1, 0) => "EAST",
        (-1, 0) => "WEST",
        (1, 1) => "NORTHEAST",
        (-1, 1) => "NORTHWEST",
        (1, -1) => "SOUTHEAST",
        (-1, -1) => "SOUTHWEST",
        _ => "",
    }
}

/// The area `pos` stands in.
pub fn area_of(world: &World, pos: TilePos) -> Option<Entity> {
    area_at(world, pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract(id: u32, status: ContractStatus) -> Contract {
        Contract {
            id,
            payout_type: Payout::Small,
            target: None,
            target_name: String::new(),
            target_rank: String::new(),
            payout: 1,
            payout_bonus: 3,
            dropoff: None,
            dropoff_name: String::new(),
            dropoff_pos: None,
            // No ransom: nothing to announce in a bare world.
            ransom: 0,
            status,
            wanted_message: String::new(),
        }
    }

    #[test]
    fn one_contract_at_a_time_and_abort_loses_it() {
        let mut world = World::new();
        let hub = ContractorHub {
            contracts: vec![
                contract(1, ContractStatus::Inactive),
                contract(2, ContractStatus::Inactive),
            ],
            ..Default::default()
        };
        let c = world.spawn(hub).id();
        assert!(accept(&mut world, c, 2));
        assert!(!accept(&mut world, c, 1), "another contract runs");
        assert_eq!(world.get::<ContractorHub>(c).unwrap().current, Some(1));
        assert!(abort(&mut world, c));
        let hub = world.get::<ContractorHub>(c).unwrap();
        assert_eq!(hub.contracts[1].status, ContractStatus::Aborted);
        assert_eq!(hub.current, None);
        assert!(!accept(&mut world, c, 2), "an aborted contract is gone");
    }

    #[test]
    fn the_target_pays_out_with_the_alive_bonus_and_others_abort() {
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        world.insert_resource(Time::<()>::default());
        let target = world.spawn((Stat::Conscious, TilePos::new(1, 1, 1))).id();
        let other = world.spawn((Stat::Dead, TilePos::new(1, 1, 1))).id();
        let mut first = contract(1, ContractStatus::Extracting);
        first.target = Some(target);
        let mut second = contract(2, ContractStatus::Extracting);
        second.target = Some(target);
        let c = world
            .spawn(ContractorHub {
                contracts: vec![first, second],
                current: Some(0),
                ..Default::default()
            })
            .id();
        let pod = world.spawn_empty().id();
        let ext = |contract| ExtractionPod {
            contractor: c,
            contract,
        };
        enter_check(&mut world, pod, ext(0), target);
        let hub = world.get::<ContractorHub>(c).unwrap();
        assert_eq!(hub.contracts[0].status, ContractStatus::Complete);
        assert_eq!(hub.tc_to_redeem, 4, "payout 1 + alive bonus 3");
        assert_eq!(hub.completed, 1);
        assert_eq!(hub.current, None);
        assert!(world.get::<Kidnapped>(target).is_some());
        assert!(world.get::<Captive>(target).is_some());
        let pod2 = world.spawn_empty().id();
        enter_check(&mut world, pod2, ext(1), other);
        let hub = world.get::<ContractorHub>(c).unwrap();
        assert_eq!(hub.contracts[1].status, ContractStatus::Aborted);
        assert_eq!(hub.tc_to_redeem, 4);
        assert_eq!(roundend_lines(&world, c).len(), 1);
    }

    #[test]
    fn compass_words() {
        let a = TilePos::new(5, 5, 1);
        assert_eq!(dir_word(a, TilePos::new(5, 9, 1)), "NORTH");
        assert_eq!(dir_word(a, TilePos::new(1, 1, 1)), "SOUTHWEST");
    }
}

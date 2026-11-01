//! Test hooks for the functional test bots (`ss13_bots`, docs/RUNNING.md "Functional test bots"):
//! [`DebugRequest::Test`](ss13_shared::debug::DebugRequest::Test) carries a JSON command, the
//! answer is JSON. Reachable only through [`crate::server::DebugGate`] with dev tools on (a debug
//! server build), like the other dev tools, and not rate-limited: bots poll conditions.
//!
//! Commands act for the asking client (its mob, its `NetId`s):
//! - `{"op":"setup","pos":[x,y,z],"job":"Security Officer","give":["/obj/item/..."]}`: a body of
//!   that job (tg title; omitted: the current body stays), moved to `pos`, the items spawned into
//!   its hands (left first) or at its feet when the hands are full; `"antag":"brother"` then makes
//!   it a blood brother with a team of its own, `"vampire"` a vampire, `"changeling"` a
//!   changeling (an admin's "make antag");
//! - `{"op":"spawn","path":"/obj/...","pos":[x,y,z]}`;
//! - `{"op":"tile","pos":[x,y,z]}`: turf, area, air and every entity on the tile;
//! - `{"op":"mob"}`: the asking client's mob with health, hunger, fire, inventory;
//! - `{"op":"time_scale","scale":2.0}`: the server's virtual time speed;
//! - `{"op":"hurt","kind":"oxy","amount":150}`: damage to the asking client's mob (spread over
//!   the body for brute and burn), then its stat;
//! - `{"op":"event","name":"Grid Check","announce":true}`: forces a random event by its tg name
//!   (the admin Force Event; `announce` overrides its announcement chance).
//!
//! Positions in answers are `[x, y]` with `z` apart, as the case files write tiles.

use bevy::prelude::*;
use serde_json::{Map, Value, json};
use ss13_atmos::Atmos;
use ss13_game::chem::{Chem, MobReagents, Reagents};
use ss13_game::closets::Closet;
use ss13_game::doors::{Door, DoorState};
use ss13_game::fire::{Burning, FireStacks};
use ss13_game::health::{DamageType, Health, Stat};
use ss13_game::hunger::Hunger;
use ss13_game::inventory::{ALL_SLOTS, CarriedBy, Inventory};
use ss13_game::jobs::Jobs;
use ss13_game::power::accumulator::Accumulator;
use ss13_game::power::emitter::Emitter;
use ss13_game::power::supermatter::Supermatter;
use ss13_game::power::{Grid, MACHINE_TICK_SECS, Smes};
use ss13_game::server::PlayerMob;
use ss13_game::storage::{Storage, StoredIn};
use ss13_game::vending::Vendor;
use ss13_game::wires::Wires;
use ss13_game::zone::BodyZone;
use ss13_mapload::{DmItem, DmName};
use ss13_net::server::{ControlledEntity, Interest};
use ss13_shared::TilePos;
use ss13_shared::protocol::InventorySlot;
use ss13_world::SpatialIndex;

use crate::server::{TIME_SCALE_RANGE, on_map, tile_report};

/// Answers one command for `client`.
pub fn answer(world: &mut World, client: Entity, command: &str) -> Result<String, String> {
    let command: Value = serde_json::from_str(command).map_err(|e| format!("bad command: {e}"))?;
    let op = command["op"].as_str().unwrap_or_default();
    let value = match op {
        "setup" => setup(world, client, &command)?,
        "spawn" => {
            let path = str_field(&command, "path")?;
            let pos = pos_field(world, &command)?;
            spawn(world, path, pos)?;
            Value::Null
        }
        "tile" => {
            let pos = pos_field(world, &command)?;
            tile(world, client, pos)
        }
        "mob" => {
            let mob = mob_of(world, client)?;
            entity(world, client, mob)
        }
        "event" => {
            let name = str_field(&command, "name")?;
            ss13_game::events::force_event(world, name, command["announce"].as_bool())?;
            Value::Null
        }
        "time_scale" => {
            let scale = command["scale"].as_f64().unwrap_or(f64::NAN) as f32;
            if !(TIME_SCALE_RANGE.0..=TIME_SCALE_RANGE.1).contains(&scale) {
                return Err(format!("time scale must be in {TIME_SCALE_RANGE:?}"));
            }
            world
                .resource_mut::<Time<Virtual>>()
                .set_relative_speed(scale);
            Value::Null
        }
        "hurt" => {
            let mob = mob_of(world, client)?;
            let kind = match str_field(&command, "kind")? {
                "brute" => DamageType::Brute,
                "burn" => DamageType::Burn,
                "tox" => DamageType::Tox,
                "oxy" => DamageType::Oxy,
                other => return Err(format!("unknown damage type {other:?}")),
            };
            let amount = command["amount"].as_f64().unwrap_or(f64::NAN) as f32;
            if !amount.is_finite() {
                return Err("amount: expected a number".into());
            }
            let (Some(mut health), Some(&stat)) =
                (world.get::<Health>(mob).cloned(), world.get::<Stat>(mob))
            else {
                return Err("not a living mob".into());
            };
            let mut new = stat;
            ss13_game::health::apply_damage(&mut health, &mut new, kind, amount);
            world.entity_mut(mob).insert((health, new));
            Value::Null
        }
        _ => return Err(format!("unknown test hook {op:?}")),
    };
    Ok(value.to_string())
}

fn str_field<'a>(command: &'a Value, key: &str) -> Result<&'a str, String> {
    command[key]
        .as_str()
        .ok_or_else(|| format!("{key}: expected a string"))
}

fn pos_field(world: &World, command: &Value) -> Result<TilePos, String> {
    let v: Vec<i64> = command["pos"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default();
    let [x, y, z] = v[..] else {
        return Err("pos: expected [x, y, z]".into());
    };
    let pos = TilePos::new(x as i32, y as i32, z as u16);
    if !on_map(world, pos) {
        return Err(format!("({x}, {y}, {z}) is off the map"));
    }
    Ok(pos)
}

fn mob_of(world: &World, client: Entity) -> Result<Entity, String> {
    world
        .get::<ControlledEntity>(client)
        .map(|c| c.0)
        .filter(|&mob| world.get_entity(mob).is_ok())
        .ok_or_else(|| "the client has no mob".into())
}

fn spawn(world: &mut World, path: &str, pos: TilePos) -> Result<Entity, String> {
    if !path.starts_with("/obj") {
        return Err(format!("{path} is not an /obj"));
    }
    ss13_game::antag::heretic::server::spawn_obj_or_rune(world, path, pos)
        .ok_or_else(|| format!("{path} could not be spawned (no prototype?)"))
}

fn setup(world: &mut World, client: Entity, command: &Value) -> Result<Value, String> {
    let pos = pos_field(world, command)?;
    if let Some(title) = command["job"].as_str() {
        let job = world
            .resource::<Jobs>()
            .data
            .jobs
            .iter()
            .position(|j| j.title == title)
            .ok_or_else(|| format!("no job titled {title:?}"))?;
        ss13_game::jobs::join(world, client, job, true);
    }
    let mob = mob_of(world, client)?;
    world.entity_mut(mob).insert(pos);
    // The AI's body is its core; the client controls the eye.
    if let Some(eye) = world.get::<ss13_game::silicon::ai::AiEye>(mob).copied() {
        world.entity_mut(eye.ai).insert(pos);
    }
    for path in command["give"].as_array().into_iter().flatten() {
        let path = path.as_str().ok_or("give: expected paths")?;
        let item = spawn(world, path, pos)?;
        // tg `put_in_hands`: the first free hand, else it stays at the feet.
        let Some(mut inventory) = world.get_mut::<Inventory>(mob) else {
            continue;
        };
        let Some(hand) = inventory.hands.iter().position(Option::is_none) else {
            continue;
        };
        inventory.hands[hand] = Some(item);
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
    }
    match command["antag"].as_str() {
        None => {}
        Some("brother") => {
            ss13_game::antag::brother::add_brother(world, mob, None);
        }
        Some("vampire") => {
            ss13_game::antag::vampire::server::add_vampire(world, mob);
        }
        Some("changeling") => {
            ss13_game::antag::changeling::server::add_changeling(world, mob);
        }
        // With the roundstart equipment (the dagger and runed metal).
        Some("cult") => {
            ss13_game::antag::cult::server::add_cultist_equipped(world, mob);
        }
        Some("heretic") => {
            ss13_game::antag::heretic::server::add_heretic(world, mob);
        }
        Some("headrev") => {
            ss13_game::antag::revolution::server::add_revolutionary(world, mob, true);
        }
        Some(other) => return Err(format!("antag: unknown role {other:?}")),
    }
    info!("test: {client} set up at {pos:?}");
    Ok(Value::Null)
}

fn tile(world: &World, client: Entity, pos: TilePos) -> Value {
    let report = tile_report(world, client, pos);
    let entities: Vec<Value> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .map(|&e| entity(world, client, e))
        .collect();
    let air = world
        .get_resource::<Atmos>()
        .and_then(|a| a.air(pos))
        .map(|_| {
            let air = report.air.clone().unwrap_or_default();
            let gases: Map<String, Value> = air
                .gases
                .iter()
                .map(|(id, moles, _)| (id.clone(), json!(moles)))
                .collect();
            json!({
                "pressure": air.pressure_kpa,
                "temperature": air.temperature_k,
                "moles": air.total_moles,
                "gases": gases,
            })
        });
    let area = report.area.map(|a| {
        json!({
            "name": a.name,
            "powered": a.powered,
            "requires_power": a.requires_power,
            "has_apc": a.has_apc,
        })
    });
    json!({
        "pos": [pos.x, pos.y],
        "z": pos.z,
        "turf": report.turf,
        "area": area,
        "air": air,
        // `TURF_WET_*` of the strongest wetness, 0 dry.
        "wet": ss13_game::construction::turf_at(world, pos)
            .and_then(|t| world.get::<ss13_game::janitor::WetFloor>(t))
            .map_or(0, ss13_game::janitor::WetFloor::highest),
        "entities": entities,
    })
}

/// DM path of an entity, as far as the server keeps one.
fn path_of(world: &World, e: Entity) -> Option<String> {
    let entity = world.get_entity(e).ok()?;
    if entity.contains::<ss13_game::silicon::ai::AiCore>() {
        return Some(ss13_mapload::silicon::AI_TYPE.into());
    }
    if entity.contains::<ss13_game::silicon::cyborg::Cyborg>() {
        return Some(ss13_mapload::silicon::ROBOT_TYPE.into());
    }
    if entity.contains::<ss13_game::silicon::ai::AiEye>() {
        return Some(ss13_mapload::silicon::AI_EYE_TYPE.into());
    }
    if let Some(mob) = entity.get::<ss13_mapload::DmMob>() {
        return Some(mob.path.clone());
    }
    if entity.contains::<PlayerMob>() || entity.contains::<Health>() {
        return Some("/mob/living/carbon/human".into());
    }
    entity
        .get::<ss13_mapload::DmObj>()
        .map(|o| o.path.clone())
        .or_else(|| entity.get::<DmItem>().map(|i| i.path.clone()))
        .or_else(|| {
            entity
                .get::<ss13_mapload::DmTurfBuild>()
                .map(|t| t.path.clone())
        })
        .or_else(|| {
            entity
                .get::<ss13_mapload::DmCleanable>()
                .map(|c| c.path.clone())
        })
        .or_else(|| {
            entity
                .get::<ss13_game::antag::cult::server::Rune>()
                .map(|r| r.kind.def().path.to_owned())
        })
        .or_else(|| {
            use ss13_game::antag::heretic::server as heretic;
            if entity.contains::<heretic::Influence>() {
                Some(heretic::INFLUENCE.to_owned())
            } else if entity.contains::<heretic::VisibleInfluence>() {
                Some(heretic::VISIBLE_INFLUENCE.to_owned())
            } else if entity.contains::<heretic::TransmutationRune>() {
                Some(heretic::RUNE.to_owned())
            } else {
                None
            }
        })
}

/// What the tests read about one entity: identity plus the state of the systems it has.
fn entity(world: &World, client: Entity, e: Entity) -> Value {
    let mut out = Map::new();
    let name = world
        .get::<Name>(e)
        .map(|n| n.to_string())
        .or_else(|| world.get::<DmName>(e).map(|n| n.name.clone()))
        .unwrap_or_default();
    out.insert("path".into(), json!(path_of(world, e)));
    out.insert("name".into(), json!(name));
    let net_id = world
        .get_resource::<Interest>()
        .and_then(|i| i.net_id(client, e));
    out.insert("net_id".into(), json!(net_id.map(|id| id.0)));
    if let Some(pos) = world.get::<TilePos>(e) {
        out.insert("pos".into(), json!([pos.x, pos.y]));
        out.insert("z".into(), json!(pos.z));
    }
    if let Some(door) = world.get::<Door>(e) {
        out.insert(
            "door".into(),
            json!({
                "open": door.state == DoorState::Open,
                "closed": door.state == DoorState::Closed,
                "bolted": door.locked,
                "welded": door.welded,
                "panel_open": door.panel_open,
            }),
        );
    }
    if let Some(wires) = world.get::<Wires>(e) {
        // Wire name (tg `WIRE_*`, e.g. `Bolts`) -> its color on this holder.
        let colors: Map<String, Value> = wires
            .colors
            .iter()
            .map(|(color, wire)| (wire.clone(), json!(color)))
            .collect();
        out.insert(
            "wires".into(),
            json!({ "colors": colors, "cut": wires.cut }),
        );
    }
    if let Some(atmos) = atmos_machine(world, e) {
        out.insert("atmos".into(), atmos);
    }
    disposal(world, e, &mut out);
    if let Some(closet) = world.get::<Closet>(e) {
        out.insert(
            "closet".into(),
            json!({ "open": closet.opened, "locked": closet.locked, "welded": closet.welded }),
        );
    }
    let chem = world.get_resource::<Chem>();
    let reagents = |holder: &ss13_reagents::Holder| {
        let list: Map<String, Value> = holder
            .reagents
            .iter()
            .map(|r| {
                let id = chem
                    .and_then(|c| c.0.reagents.get(usize::from(r.id)))
                    .map_or_else(|| r.id.to_string(), |d| d.path.clone());
                (id, json!(r.volume))
            })
            .collect();
        json!({
            "total": holder.total_volume,
            "max": holder.maximum_volume,
            "temperature": holder.chem_temp,
            "list": list,
        })
    };
    if let Some(Reagents(holder)) = world.get::<Reagents>(e) {
        out.insert("reagents".into(), reagents(holder));
    }
    if let Some(mob) = world.get::<MobReagents>(e) {
        out.insert("blood".into(), reagents(&mob.blood));
        out.insert("stomach".into(), reagents(&mob.stomach));
    }
    if let Some(health) = world.get::<Health>(e) {
        let parts: Map<String, Value> = BodyZone::ALL
            .iter()
            .map(|&zone| {
                let part = health.parts[zone.index()];
                let name = format!("{zone:?}").to_lowercase();
                (name, json!({ "brute": part.brute, "burn": part.burn }))
            })
            .collect();
        out.insert(
            "health".into(),
            json!({
                "health": health.health(),
                "max": health.max,
                "brute": health.brute,
                "burn": health.burn,
                "tox": health.tox,
                "oxy": health.oxy,
                "parts": parts,
            }),
        );
    }
    if let Some(eye) = world.get::<ss13_game::silicon::ai::AiEye>(e) {
        out.insert("ai".into(), entity(world, client, eye.ai));
    }
    if let Some(borg) = world.get::<ss13_game::silicon::cyborg::Cyborg>(e) {
        out.insert(
            "cyborg".into(),
            json!({
                "model": borg.model.map(|i| world
                    .resource::<ss13_game::silicon::Silicons>()
                    .0
                    .models[i]
                    .name
                    .clone()),
                "charge": borg.charge,
                "max_charge": borg.max_charge,
                "low_power": borg.low_power,
                "connected_ai": borg.connected_ai.is_some(),
            }),
        );
    }
    if let Some(laws) = world.get::<ss13_game::silicon::laws::Laws>(e) {
        out.insert(
            "laws".into(),
            json!({ "name": laws.name, "list": laws.law_list(true) }),
        );
    }
    if let Some(m) = world.get::<ss13_game::mecha::Mech>(e) {
        out.insert(
            "mech".into(),
            json!({
                "integrity": m.integrity,
                "charge": ss13_game::mecha::core::charge(world, e),
                "occupied": m.occupant.is_some(),
                "cargo": m.cargo.len(),
                "l_arm": m.l_arm.is_some(),
                "modules": m.modules.len(),
                "strafe": m.strafe,
                "facing": world.get::<ss13_shared::Facing>(e).map(|f| format!("{:?}", f.0)),
            }),
        );
    }
    if let Some(p) = world.get::<ss13_game::mecha::Pilot>(e) {
        out.insert("pilot".into(), entity(world, client, p.mech));
    }
    if let Some(frame) = world.get::<ss13_game::mecha::construction::Frame>(e) {
        out.insert(
            "frame".into(),
            match frame {
                ss13_game::mecha::construction::Frame::Parts { missing, .. } => {
                    json!({ "missing": missing.len() })
                }
                ss13_game::mecha::construction::Frame::Steps { index, .. } => {
                    json!({ "step": index })
                }
            },
        );
    }
    if let Some(station) = world.get::<ss13_game::silicon::cyborg::RechargeStation>(e) {
        out.insert(
            "station".into(),
            json!({ "open": station.open, "occupied": station.occupant.is_some() }),
        );
    }
    if let Some(stat) = world.get::<Stat>(e) {
        out.insert("stat".into(), json!(format!("{stat:?}")));
    }
    if world.get::<Stat>(e).is_some()
        && let Some(manifest) = world.get_resource::<ss13_game::records::manifest::Manifest>()
        && let Some(r) = world.get::<Name>(e).and_then(|n| manifest.find(n.as_str()))
    {
        out.insert(
            "record".into(),
            json!({
                "crew_ref": r.id.to_string(),
                "rank": r.rank,
                "wanted_status": r.wanted_status,
                "crimes": r.crimes.len(),
            }),
        );
    }
    if let Some(card) = world.get::<ss13_game::inventory::IdCard>(e) {
        out.insert("id_access".into(), json!(card.access));
    }
    medical(world, e, &mut out);
    if let Some(hunger) = world.get::<Hunger>(e) {
        out.insert("nutrition".into(), json!(hunger.nutrition));
    }
    if let Some(fire) = world.get::<FireStacks>(e) {
        out.insert("fire_stacks".into(), json!(fire.stacks));
        out.insert("on_fire".into(), json!(fire.on_fire));
    }
    if world.get::<Burning>(e).is_some() {
        out.insert("burning".into(), json!(true));
    }
    if let Some(vendor) = world.get::<Vendor>(e) {
        let stock: Vec<Value> = vendor
            .records
            .iter()
            .map(|r| json!({ "path": r.path, "amount": r.amount }))
            .collect();
        out.insert("stock".into(), json!(stock));
    }
    if world.get::<Storage>(e).is_some() {
        out.insert("contents".into(), json!(contents(world, e)));
    }
    if let Some(sm) = world.get::<Supermatter>(e) {
        let c = &sm.crystal;
        out.insert(
            "supermatter".into(),
            json!({ "energy": c.internal_energy, "damage": c.damage,
                    "integrity": c.integrity_percent(), "countdown": c.final_countdown }),
        );
    }
    if let Some(emitter) = world.get::<Emitter>(e) {
        out.insert(
            "emitter".into(),
            json!({ "active": emitter.active, "powered": emitter.powered, "welded": emitter.welded }),
        );
    }
    if let Some(acc) = world.get::<Accumulator>(e) {
        out.insert(
            "accumulator".into(),
            json!({ "stored": acc.stored_energy, "processed": acc.processed_energy }),
        );
    }
    if let Some(smes) = world.get::<Smes>(e) {
        out.insert(
            "smes".into(),
            json!({ "charge": smes.charge, "input": smes.input_available, "inputting": smes.inputting }),
        );
    }
    // The powernet of a power machine: what it had available and drew last tick, watts.
    let grid = world.resource::<Grid>();
    if let Some(net) = grid.node_net(e).and_then(|n| grid.net(n)) {
        out.insert(
            "powernet".into(),
            json!({ "avail": net.avail / MACHINE_TICK_SECS, "load": net.load / MACHINE_TICK_SECS }),
        );
    }
    if let Some(inventory) = world.get::<Inventory>(e) {
        if let Some(back) = inventory.back {
            out.insert("back_contents".into(), json!(contents(world, back)));
        }
        let held: Vec<Value> = inventory
            .hands
            .iter()
            .map(|h| json!(h.and_then(|h| path_of(world, h))))
            .collect();
        out.insert("held".into(), json!(held));
        let slots: Map<String, Value> = ALL_SLOTS
            .iter()
            .filter(|s| !matches!(s, InventorySlot::LeftHand | InventorySlot::RightHand))
            .filter_map(|&s| {
                let item = inventory.slot(s)?;
                Some((slot_name(s).to_owned(), json!(path_of(world, item))))
            })
            .collect();
        out.insert("slots".into(), Value::Object(slots));
    }
    if let Some(cuffs) = world.get::<ss13_game::restraints::Handcuffed>(e) {
        out.insert("handcuffed".into(), json!(path_of(world, cuffs.0)));
    }
    if world.get::<ss13_game::closets::InCloset>(e).is_some() {
        out.insert("in_closet".into(), json!(true));
    }
    if let Some(dna) = world.get::<ss13_game::genetics::Dna>(e) {
        let names = |ids: &mut dyn Iterator<Item = usize>| -> Vec<&str> {
            ids.map(|m| ss13_game::genetics::MUTATIONS[m].name)
                .collect()
        };
        out.insert(
            "dna".into(),
            json!({
                "enzymes": dna.unique_enzymes,
                "identity": dna.unique_identity,
                "stability": dna.stability,
                "index": names(&mut dna.mutation_index.iter().map(|(m, _)| *m)),
                "mutations": names(&mut dna.mutations.iter().map(|m| m.id)),
            }),
        );
    }
    if let Some(s) = world.get::<ss13_game::genetics::console::DnaScanner>(e) {
        out.insert("scanner".into(), json!({ "open": s.open }));
    }
    Value::Object(out)
}

/// Disposal bins, parcels and taggers, and whether a thing is inside a bin or a pipe
/// (`ss13_game::disposals`).
fn disposal(world: &World, e: Entity, out: &mut Map<String, Value>) {
    use ss13_game::disposals::{Delivery, DestTagger, DisposalBin, DisposalHolder};
    if let Some(bin) = world.get::<DisposalBin>(e) {
        out.insert(
            "disposal".into(),
            json!({
                "flush": bin.flush,
                "full_pressure": bin.full_pressure,
                "pressure_charging": bin.pressure_charging,
                "flushing": bin.flushing_until.is_some(),
                "pressure": bin.pressure(),
                "contents": contents(world, e),
            }),
        );
    }
    if let Some(d) = world.get::<Delivery>(e) {
        out.insert("sort_tag".into(), json!(d.sort_tag));
    }
    if let Some(t) = world.get::<DestTagger>(e) {
        out.insert("dest_tag".into(), json!(t.curr_tag));
    }
    let container = world
        .get::<ss13_game::closets::InCloset>(e)
        .map(|c| c.closet)
        .or_else(|| world.get::<ss13_game::storage::StoredIn>(e).map(|s| s.0));
    if let Some(c) = container {
        if world.get::<DisposalBin>(c).is_some() {
            out.insert("in_disposal".into(), json!("bin"));
        } else if world.get::<DisposalHolder>(c).is_some() {
            out.insert("in_disposal".into(), json!("pipe"));
        }
    }
}

/// DM paths of what a storage holds.
// Cost: a pass over every stored item; test queries only.
/// Blood, wounds, surgery state, buckling, a defibrillator's charge (`ss13_game::medical`).
fn medical(world: &World, e: Entity, out: &mut Map<String, Value>) {
    use ss13_game::medical::{blood, defib, surgery, wounds};
    if world.get::<ss13_game::human::Husk>(e).is_some() {
        out.insert("husk".into(), json!(true));
    }
    if let Some(c) = world.get::<ss13_game::antag::changeling::server::Changeling>(e) {
        out.insert(
            "changeling".into(),
            json!({
                "chemicals": c.chem_charges,
                "genetic_points": c.genetic_points,
                "absorbed": c.absorbed_count,
                "true_absorbs": c.true_absorbs,
                "profiles": c.profiles.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
                "purchased": c.purchased.iter().map(|p| p.def().id).collect::<Vec<_>>(),
                "stasis": format!("{:?}", c.stasis),
            }),
        );
    }
    if let Some(b) = world.get::<blood::Blood>(e) {
        out.insert("blood_volume".into(), json!(b.volume));
        out.insert("blood_type".into(), json!(b.blood_type.name()));
    }
    if let Some(limbs) = world.get::<wounds::Limbs>(e) {
        let lying = world
            .get::<ss13_game::health::BodyPosition>(e)
            .is_some_and(|b| b.lying());
        out.insert("bleed_rate".into(), json!(limbs.bleed_rate(lying)));
        let mut parts = Map::new();
        for zone in BodyZone::ALL {
            let limb = limbs.limb(zone);
            let wounds: Vec<Value> = limb
                .wounds
                .iter()
                .map(|w| json!({ "kind": format!("{:?}", w.kind), "blood_flow": w.blood_flow }))
                .collect();
            parts.insert(
                format!("{zone:?}").to_lowercase(),
                json!({
                    "wounds": wounds,
                    "kinds": limb.wounds.iter().map(|w| format!("{:?}", w.kind)).collect::<Vec<_>>(),
                    "gauze": limb.gauze.is_some(),
                    "surgery": limb.surgery,
                    "prepped": limb.prepped,
                }),
            );
        }
        out.insert("limbs".into(), Value::Object(parts));
    }
    if let Some(organs) = world.get::<defib::OrganDecay>(e) {
        out.insert("heart_damage".into(), json!(organs.heart));
    }
    if world.get::<surgery::Buckled>(e).is_some() {
        out.insert("buckled".into(), json!(true));
    }
    if let Some(d) = world.get::<defib::Defib>(e) {
        out.insert(
            "defib".into(),
            json!({ "charge": d.cell.map(|c| c.0), "paddles_out": d.paddles.is_some() }),
        );
    }
}

fn contents(world: &World, storage: Entity) -> Vec<Option<String>> {
    let mut q = world
        .try_query::<(Entity, &StoredIn)>()
        .expect("registered");
    q.iter(world)
        .filter(|(_, s)| s.0 == storage)
        .map(|(item, _)| path_of(world, item))
        .collect()
}

/// The slot names the case files use (`ss13_bots`).
pub fn slot_name(slot: InventorySlot) -> &'static str {
    match slot {
        InventorySlot::LeftHand => "left_hand",
        InventorySlot::RightHand => "right_hand",
        InventorySlot::Id => "id",
        InventorySlot::Uniform => "uniform",
        InventorySlot::Shoes => "shoes",
        InventorySlot::Back => "back",
        InventorySlot::Head => "head",
        InventorySlot::Mask => "mask",
        InventorySlot::Neck => "neck",
        InventorySlot::Eyes => "eyes",
        InventorySlot::Ears => "ears",
        InventorySlot::Gloves => "gloves",
        InventorySlot::Suit => "suit",
        InventorySlot::SuitStorage => "suit_storage",
        InventorySlot::Belt => "belt",
        InventorySlot::LeftPocket => "left_pocket",
        InventorySlot::RightPocket => "right_pocket",
    }
}

/// Atmos machines: on/off, portables' gas and port, air alarms' alert level and lock.
fn atmos_machine(world: &World, e: Entity) -> Option<Value> {
    use ss13_atmos::pipenet::Pipenet;
    use ss13_game::atmos_machinery::{AirAlarm, AtmosDevice, Canister, PortableAtmos, VentPump};
    let mut out = Map::new();
    if let Some(portable) = world.get::<PortableAtmos>(e) {
        out.insert("port".into(), json!(portable.port.is_some()));
        out.insert("holding".into(), json!(portable.holding.is_some()));
        out.insert(
            "pressure".into(),
            json!(ss13_game::atmos_machinery::portable::pressure(world, e)),
        );
    }
    if let Some(canister) = world.get::<Canister>(e) {
        out.insert("valve_open".into(), json!(canister.valve_open));
        out.insert("release_pressure".into(), json!(canister.release_pressure));
    }
    if let Some(vent) = world.get::<VentPump>(e) {
        out.insert("on".into(), json!(vent.on));
        out.insert("welded".into(), json!(vent.welded));
    }
    if let Some(alarm) = world.get::<AirAlarm>(e) {
        out.insert("danger".into(), json!(alarm.danger_level));
        out.insert("locked".into(), json!(alarm.locked));
    }
    if let (Some(device), Some(net)) =
        (world.get::<AtmosDevice>(e), world.get_resource::<Pipenet>())
        && let Some(air) = net.pipeline_air(device.id, 0)
    {
        out.insert("pipe_pressure".into(), json!(air.pressure()));
        out.insert("pipe_temperature".into(), json!(air.temperature()));
    }
    (!out.is_empty()).then_some(Value::Object(out))
}

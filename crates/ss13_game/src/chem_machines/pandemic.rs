//! The PanD.E.M.I.C 2200 (server): tg `/obj/machinery/computer/pandemic` (`pandemic.dm`) and its
//! `Pandemic` window, on the ported diseases (`crate::medical::diseases`).
//!
//! - An open container clicked on it goes in (the old one to the hands); a dropper or syringe
//!   works on the beaker inside (a syringe's right click draws); an empty-hand right click takes
//!   the beaker out. Unpowered, it takes no beaker.
//! - The window shows the beaker's blood (the first reagent with a blood type): its type, the
//!   viruses it carries (`crate::medical::diseases::BloodDiseases`, not the ones hidden from the
//!   PANDEMIC) with an advanced virus's properties and symptoms, and its antibodies.
//! - A culture tube (`create_culture_bottle`): 20 u of blood with a fresh copy of the virus, then
//!   5 s of cooldown; a vaccine tube (`create_vaccine_bottle`, from an antibody): 15 u of vaccine
//!   carrying that disease id, then 20 s. A vaccine swallowed or injected cures those diseases
//!   with a resistance and makes the mob resist them (`crate::medical::diseases::vaccinate`).
//! - An advanced virus still named "Unknown" can be renamed: the name is the round's name of
//!   every virus with those symptoms ([`DiseaseNames`], tg `SSdisease.archive_diseases`).
// shortcut: no symptom descriptions or threshold texts (not in the symptom table), no icon
// states or wait light; the disease names show in this window only (the analyzer keeps the
// virus's own name); a vaccine merged with another keeps the first one's ids.

use std::collections::HashMap;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmName, DmReagents};
use ss13_reagents::{Chemistry, Holder, round_to};
use ss13_shared::chat::Span;
use ss13_shared::protocol::ClickModifiers;
use ss13_tgui::server::{OpenUi, ServerUi, UiActed, UiParams, UiReject};

use super::*;
use crate::chem::{Chem, Reagents};
use crate::medical::diseases::{BloodDiseases, Disease, HIDDEN_PANDEMIC, Kind};
use crate::obj_damage::is_under;
use crate::power::Unpowered;
use crate::sound::{PlaySound, playsound};

pub const PANDEMIC: &str = "/obj/machinery/computer/pandemic";
pub const VACCINE: &str = "/datum/reagent/vaccine";
pub const TUBE: &str = "/obj/item/reagent_containers/cup/tube";
const SYRINGE: &str = "/obj/item/reagent_containers/syringe";
const DROPPER: &str = "/obj/item/reagent_containers/dropper";
const PING: &str = "sound/machines/ping.ogg";
const CULTURE_COOLDOWN: f64 = 5.0;
const VACCINE_COOLDOWN: f64 = 20.0;
/// `sanitize_name`'s `MAX_NAME_LEN`.
const MAX_NAME_LEN: usize = 42;

#[derive(Component, Clone, Debug, Default, PartialEq)]
#[require(Views<PandemicData>)]
pub struct Pandemic {
    pub beaker: Option<Entity>,
    /// `wait`: ready again at this game time.
    pub wait_until: Option<f64>,
}

/// tg `SSdisease.archive_diseases[id].name` of renamed advanced viruses, by disease id.
#[derive(Resource, Default, Clone, Debug)]
pub struct DiseaseNames(pub HashMap<String, String>);

/// The ids a vaccine's data carries.
pub fn vaccine_ids(data: &str) -> Vec<String> {
    data.split('|')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(super) fn ensure(world: &mut World, e: Entity) -> bool {
    if world.get::<Pandemic>(e).is_some() {
        return true;
    }
    if !world
        .get::<ss13_mapload::DmObj>(e)
        .is_some_and(|o| is_under(&o.path, PANDEMIC))
    {
        return false;
    }
    world.entity_mut(e).insert(Pandemic::default());
    true
}

fn beaker(world: &World, e: Entity) -> Option<Entity> {
    inside(world, e, world.get::<Pandemic>(e)?.beaker)
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn eject(world: &mut World, mob: Option<Entity>, e: Entity) {
    if let Some(b) = beaker(world, e) {
        hand_out(world, mob, e, b);
    }
    if let Some(mut p) = world.get_mut::<Pandemic>(e) {
        p.beaker = None;
    }
}

pub(super) fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    e: Entity,
    right: bool,
) -> bool {
    if world.get::<Pandemic>(e).is_none() {
        return false;
    }
    if right {
        eject(world, Some(mob), e);
    } else {
        world.write_message(OpenUi::new::<PandemicUi>(client, mob, e, title(world, e)));
    }
    true
}

pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
    modifiers: ClickModifiers,
) -> bool {
    if world.get::<Pandemic>(e).is_none() {
        return false;
    }
    let path = world
        .get::<DmItem>(held)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    // Precision instruments work on the loaded sample.
    if is_under(&path, DROPPER) || is_under(&path, SYRINGE) {
        match beaker(world, e) {
            Some(b) => {
                crate::chem::item_interaction(world, client, mob, held, b, modifiers);
            }
            None => crate::balloon::balloon_alert(world, mob, e, "нет ёмкости!"),
        }
        return true;
    }
    let open_container = world.get::<DmReagents>(held).is_some()
        && world
            .get::<Reagents>(held)
            .is_some_and(|r| r.0.is_open_container());
    if !open_container {
        return false;
    }
    if world.get::<Unpowered>(e).is_some() {
        return true;
    }
    let text = if beaker(world, e).is_some() {
        "ёмкость заменена"
    } else {
        "ёмкость загружена"
    };
    eject(world, Some(mob), e);
    take_in(world, mob, e, held);
    if let Some(mut p) = world.get_mut::<Pandemic>(e) {
        p.beaker = Some(held);
    }
    crate::balloon::balloon_alert(world, mob, e, text);
    true
}

/// `reset_replicator_cooldown`.
// Cost: one pass over the PANDEMICs per tick, only while one is waiting.
pub(super) fn process(world: &mut World) {
    let t = now(world);
    let mut q = world.query::<(Entity, &Pandemic)>();
    let ready: Vec<Entity> = q
        .iter(world)
        .filter(|(_, p)| p.wait_until.is_some_and(|w| w <= t))
        .map(|(e, _)| e)
        .collect();
    for e in ready {
        if let Some(mut p) = world.get_mut::<Pandemic>(e) {
            p.wait_until = None;
        }
        if let Some(&pos) = world.get::<ss13_shared::TilePos>(e) {
            playsound(world, PlaySound::new(pos, PING, 30.0, true));
        }
    }
}

/// `get_blood_reagent`: the blood in the beaker (a culture's blood has no type).
fn blood_sample<'a>(chem: &Chemistry, h: &'a Holder) -> Option<&'a ss13_reagents::Reagent> {
    h.has(chem.id(crate::medical::blood::BLOOD_REAGENT)?)
}

/// The blood with viruses in the beaker: its `BloodDiseases`, when its blood is the sample.
fn blood_data(world: &World, e: Entity) -> Option<BloodDiseases> {
    let b = beaker(world, e)?;
    let chem = &world.get_resource::<Chem>()?.0;
    let blood = chem.id(crate::medical::blood::BLOOD_REAGENT)?;
    let h = &world.get::<Reagents>(b)?.0;
    h.has(blood)?;
    world.get::<BloodDiseases>(b).cloned()
}

/// The shown viruses (`get_viruses_data`): every one not hidden from the PANDEMIC, numbered.
fn shown(data: &BloodDiseases) -> Vec<(u32, &Disease)> {
    data.viruses
        .iter()
        .filter(|d| d.visibility & HIDDEN_PANDEMIC == 0)
        .enumerate()
        .map(|(i, d)| (i as u32 + 1, d))
        .collect()
}

/// tg `get_disease_name`: the round's name of an advanced virus, else its own.
pub fn disease_name(names: &DiseaseNames, d: &Disease) -> String {
    names
        .0
        .get(&d.id())
        .cloned()
        .unwrap_or_else(|| d.name.clone())
}

fn virus_view(chem: &Chemistry, names: &DiseaseNames, index: u32, d: &Disease) -> VirusView {
    let def = d.def();
    let mut v = VirusView {
        index,
        name: d.name.clone(),
        agent: def.agent.to_owned(),
        description: if def.desc.is_empty() {
            "none".to_owned()
        } else {
            def.desc.to_owned()
        },
        spread: if d.spread_text.is_empty() {
            "none".to_owned()
        } else {
            d.spread_text.clone()
        },
        cure: d.cure_text(crate::medical::symptoms::SYMPTOM_LIMIT),
        ..default()
    };
    if v.cure.is_empty() {
        v.cure = "none".to_owned();
    }
    if let Some(adv) = &d.advance {
        let name = disease_name(names, d);
        v.can_rename = name == "Unknown";
        v.is_adv = true;
        v.name = name;
        v.resistance = adv.props.resistance;
        v.stage_speed = adv.props.stage_rate;
        v.stealth = adv.props.stealth;
        v.transmission = adv.props.transmittable;
        v.symptoms = adv
            .symptoms
            .iter()
            .map(|s| {
                let sd = s.kind.def();
                SymptomView {
                    name: sd.name.to_owned(),
                    stealth: sd.stealth,
                    resistance: sd.resistance,
                    stage_speed: sd.stage_speed,
                    transmission: sd.transmittable,
                    level: sd.level,
                    neutered: s.neutered,
                    cure: sd
                        .cure
                        .and_then(|p| chem.id(p))
                        .map_or_else(|| "Nothing".to_owned(), |id| chem.def(id).name.clone()),
                }
            })
            .collect();
    }
    v
}

/// The name of a resistance id (`archive_diseases[id].name`).
fn resistance_name(names: &DiseaseNames, data: &BloodDiseases, id: &str) -> Option<String> {
    if let Some(name) = names.0.get(id) {
        return Some(name.clone());
    }
    if let Some(kind) = Kind::from_path(id) {
        return Some(kind.def().name.to_owned());
    }
    data.viruses
        .iter()
        .find(|d| d.id() == id)
        .map(|d| d.name.clone())
}

pub(super) fn view(world: &mut World, e: Entity, _user: Entity) -> Option<PandemicData> {
    let p = world.get::<Pandemic>(e)?.clone();
    let chem = world.get_resource::<Chem>()?.0.clone();
    let names = world
        .get_resource::<DiseaseNames>()
        .cloned()
        .unwrap_or_default();
    let mut out = PandemicData {
        is_ready: p.wait_until.is_none(),
        ..default()
    };
    let Some(b) = beaker(world, e) else {
        return Some(out);
    };
    let h = world.get::<Reagents>(b)?.0.clone();
    let capacity = world
        .get::<DmReagents>(b)
        .map_or(h.maximum_volume, |d| d.volume);
    out.beaker = Some((round_to(h.total_volume, 0.01), capacity));
    let Some(sample) = blood_sample(&chem, &h) else {
        return Some(out);
    };
    let blood_type = sample
        .data
        .as_deref()
        .filter(|t| !t.is_empty())
        .map(|t| crate::medical::blood::type_name(t).to_owned())
        .unwrap_or_else(|| "none".to_owned());
    out.blood = Some(("none".to_owned(), blood_type));
    if let Some(data) = blood_data(world, e) {
        out.viruses = shown(&data)
            .into_iter()
            .map(|(i, d)| virus_view(&chem, &names, i, d))
            .collect();
        out.resistances = data
            .resistances
            .iter()
            .filter_map(|id| {
                Some(ResistanceView {
                    id: id.clone(),
                    name: resistance_name(&names, &data, id)?,
                })
            })
            .collect();
    }
    Some(out)
}

impl ServerUi for PandemicUi {
    type Source = &'static Views<PandemicData>;
    type UserParam = ();
    type Act = PandemicAct;

    fn static_data(_: &Views<PandemicData>) {}

    fn data(views: &Views<PandemicData>, user: Entity, _: &()) -> PandemicData {
        views.of(user)
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<PandemicAct, UiReject> {
        let index = || params.number("index").map(|n| n as u32);
        Ok(match action {
            "eject_beaker" => PandemicAct::Eject,
            "empty_beaker" => PandemicAct::Empty,
            "empty_eject_beaker" => PandemicAct::EmptyEject,
            "rename_disease" => PandemicAct::Rename(index()?, params.text("name")?),
            "create_culture_bottle" => PandemicAct::Culture(index()?),
            "create_vaccine_bottle" => PandemicAct::Vaccine(params.text("index")?),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PandemicAct {
    Eject,
    Empty,
    EmptyEject,
    /// (virus index, name).
    Rename(u32, String),
    Culture(u32),
    /// A resistance id.
    Vaccine(String),
}

pub(super) fn acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<PandemicUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, PandemicAct)> = cursor
        .read(world.resource::<Messages<UiActed<PandemicUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, e, act) in pending {
        apply_act(world, client, actor, e, act);
    }
}

fn empty(world: &mut World, e: Entity) {
    if let Some(b) = beaker(world, e)
        && let Some(mut r) = world.get_mut::<Reagents>(b)
    {
        r.0.clear();
    }
    if let Some(b) = beaker(world, e) {
        world.entity_mut(b).remove::<BloodDiseases>();
    }
}

/// tg `ui_act`, checked at the moment of effect.
pub fn apply_act(world: &mut World, client: Entity, actor: Entity, e: Entity, act: PandemicAct) {
    let Some(p) = world.get::<Pandemic>(e).cloned() else {
        return;
    };
    match act {
        PandemicAct::Eject => eject(world, Some(actor), e),
        PandemicAct::Empty => empty(world, e),
        PandemicAct::EmptyEject => {
            if beaker(world, e).is_some() {
                empty(world, e);
                eject(world, Some(actor), e);
            }
        }
        PandemicAct::Rename(index, name) => {
            let Some(data) = blood_data(world, e) else {
                return;
            };
            let Some(d) = shown(&data)
                .into_iter()
                .find(|(i, _)| *i == index)
                .map(|(_, d)| d.clone())
            else {
                return;
            };
            if d.advance.is_none() {
                return;
            }
            let name: String = name
                .trim()
                .chars()
                .filter(|c| !c.is_control() && !"<>\\".contains(*c))
                .take(MAX_NAME_LEN)
                .collect();
            if name.trim().is_empty() {
                return;
            }
            world
                .get_resource_or_init::<DiseaseNames>()
                .0
                .insert(d.id(), name.trim().to_owned());
        }
        PandemicAct::Culture(index) => {
            if p.wait_until.is_some() {
                return;
            }
            let Some(data) = blood_data(world, e) else {
                return;
            };
            let Some(d) = shown(&data)
                .into_iter()
                .find(|(i, _)| *i == index)
                .map(|(_, d)| d.copy())
            else {
                return;
            };
            if d.advance.is_none() {
                crate::tools::tell(
                    world,
                    client,
                    Span::Warning,
                    "ОШИБКА: невозможно воспроизвести этот штамм.",
                );
                return;
            }
            let names = world
                .get_resource::<DiseaseNames>()
                .cloned()
                .unwrap_or_default();
            let name = disease_name(&names, &d);
            let agent = d.def().agent;
            let Some(tube) = spawn_tube(
                world,
                e,
                &format!("{name} culture tube"),
                Some(&format!(
                    "A small test tube containing {agent} culture in synthblood medium."
                )),
                crate::medical::blood::BLOOD_REAGENT,
                20.0,
                None,
            ) else {
                return;
            };
            world.entity_mut(tube).insert(BloodDiseases {
                viruses: vec![d],
                resistances: Vec::new(),
            });
            start_wait(world, e, CULTURE_COOLDOWN);
            info!("A culture tube was printed for the virus {name}");
        }
        PandemicAct::Vaccine(id) => {
            if p.wait_until.is_some() {
                return;
            }
            let Some(data) = blood_data(world, e) else {
                return;
            };
            if !data.resistances.contains(&id) {
                return;
            }
            let names = world
                .get_resource::<DiseaseNames>()
                .cloned()
                .unwrap_or_default();
            let name = resistance_name(&names, &data, &id).unwrap_or_else(|| "Unknown".into());
            if spawn_tube(
                world,
                e,
                &format!("{name} vaccine tube"),
                None,
                VACCINE,
                15.0,
                Some(&id),
            )
            .is_some()
            {
                start_wait(world, e, VACCINE_COOLDOWN);
            }
        }
    }
}

fn start_wait(world: &mut World, e: Entity, secs: f64) {
    let t = now(world);
    let active = world
        .get::<ss13_mapload::DmPower>(e)
        .map_or(0.0, |p| p.active_power_usage);
    crate::kitchen::machine_use_energy(world, e, active);
    if let Some(mut p) = world.get_mut::<Pandemic>(e) {
        p.wait_until = Some(t + secs);
    }
}

/// A test tube by the machine with `amount` of `reagent` (and its data).
fn spawn_tube(
    world: &mut World,
    e: Entity,
    name: &str,
    desc: Option<&str>,
    reagent: &str,
    amount: f32,
    data: Option<&str>,
) -> Option<Entity> {
    let pos = *world.get::<ss13_shared::TilePos>(e)?;
    let chem = world.get_resource::<Chem>()?.0.clone();
    let id = chem.id(reagent)?;
    let tube = crate::construction::spawn_obj(world, TUBE, pos)?;
    if let Some(mut r) = world.get_mut::<Reagents>(tube) {
        r.0.add_reagent(
            &chem,
            id,
            amount,
            ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
            None,
            None,
            false,
            false,
        );
        if let Some(data) = data {
            r.0.set_data_if_none(id, data);
        }
    }
    if let Some(mut n) = world.get_mut::<DmName>(tube) {
        n.name = name.to_owned();
        if let Some(desc) = desc {
            n.desc = desc.to_owned();
        }
    }
    Some(tube)
}

/// `examine`.
pub fn examine_lines(world: &World, e: Entity, lines: &mut Vec<(Span, String)>) {
    if world.get::<Pandemic>(e).is_none() {
        return;
    }
    if beaker(world, e).is_some() {
        lines.push((Span::Notice, "Внутри есть ёмкость.".to_owned()));
        lines.push((Span::Info, "ПКМ пустой рукой — извлечь ёмкость.".to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vaccine_data_round_trips_ids() {
        assert_eq!(
            vaccine_ids("/datum/disease/cold"),
            vec!["/datum/disease/cold".to_owned()]
        );
        assert_eq!(
            vaccine_ids("3:7|12"),
            vec!["3:7".to_owned(), "12".to_owned()]
        );
        assert!(vaccine_ids("").is_empty());
    }

    #[test]
    fn a_vaccine_cures_and_immunizes() {
        use crate::medical::diseases::{Diseases, vaccinate};
        let mut world = World::new();
        let cold = Disease::new(Kind::Cold);
        let id = cold.id();
        let mob = world
            .spawn(Diseases {
                list: vec![cold, Disease::new(Kind::Flu)],
                ..Default::default()
            })
            .id();
        vaccinate(&mut world, mob, std::slice::from_ref(&id));
        let d = world.get::<Diseases>(mob).unwrap();
        assert_eq!(d.list.len(), 1, "the flu stays");
        assert!(d.resistances.contains(&id));
    }
}

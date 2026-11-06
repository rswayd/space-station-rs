//! The medical kiosk (server): tg `/obj/machinery/medical_kiosk`
//! (`code/game/machinery/medical_kiosk.dm`) and its `MedicalKiosk` window
//! (`ss13_tgui::interfaces::medical_kiosk`).
//!
//! - An empty hand of a human opens the window; the user becomes the patient when there is
//!   none, a patient more than 5 tiles away resets it ("Patient out of range"). The opener pays.
//! - Each of the four scans (general health, symptoms and blood, brain, chemicals) is paid
//!   once per patient: free for an ID whose account is medical's ("Hello, esteemed medical
//!   staff!"), else the board's 10 credits (an emagged kiosk adds 10-30) from the ID's account
//!   into the medical budget. `clearTarget` forgets the patient and the paid scans.
//! - An emag (`pandemonium`) and a hallucinating user make the numbers erratic and the status
//!   nonsense.
// shortcut: no scanner wand (the patient is always the user or whoever was set before), no
// sounds, no multitool pricing of the board, no wrench unanchoring; addictions are not modelled
// (empty list); the disease cure text is the analyzer's.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::ui::UiStatus;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::medical_kiosk::{MedicalKioskData, MedicalKioskUi};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiKind, UiParams, UiReject, UiSystems, UiWindow,
};

use super::pods::operational;
use crate::cargo::economy::{ACCOUNT_MED, BankAccountId, Economy, bank_card_talk};
use crate::chem::{Chem, MobReagents};
use crate::health::{Health, Stat};
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::tools::tell;

pub const KIOSK_ROOT: &str = "/obj/machinery/medical_kiosk";
/// `/obj/item/circuitboard/machine/medical_kiosk::custom_cost`.
pub const DEFAULT_COST: u32 = 10;
/// The patient must stay this close (`get_dist`).
const PATIENT_RANGE: u32 = 5;
/// `DRUNK_POWER_TO_BLOOD_ALCOHOL`.
const DRUNK_TO_BLOOD_ALCOHOL: f32 = 0.003;
/// `REAGENT_INVISIBLE`.
const REAGENT_INVISIBLE: u32 = 1 << 1;
const NONSENSE: [&str; 7] = [
    "The only kiosk is kiosk, but is the only patient, patient?",
    "Breathing manually.",
    "Contact NTOS site admin.",
    "97% carbon, 3% natural flavoring",
    "The ebb and flow wears us all in time.",
    "It's Lupus. You have Lupus.",
    "Undergoing monkey disease.",
];

/// A kiosk's live state.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct MedicalKiosk {
    /// `scan_active`: `KIOSK_SCANNING_*` bits of the paid scans.
    pub scan_active: u8,
    /// `pandemonium` (emagged).
    pub pandemonium: bool,
    /// `patient_ref`, `paying_ref`.
    pub patient: Option<Entity>,
    pub paying: Option<Entity>,
    pub cost: u32,
    view: MedicalKioskData,
}

impl Default for MedicalKiosk {
    fn default() -> Self {
        Self {
            scan_active: 0,
            pandemonium: false,
            patient: None,
            paying: None,
            cost: DEFAULT_COST,
            view: MedicalKioskData::default(),
        }
    }
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // tg `medical_kiosk/ui_interact`.
    app.add_click_handler(ClickPhase::EmptyHand, 2660, |w, c| {
        crate::medical::kiosk::interact(w, c.client, c.mob, c.target)
    });
    app.register_ui::<KioskWindow>()
        .add_observer(attach)
        .add_systems(
            Update,
            (
                cap_windows.in_set(UiSystems::StatusHook),
                acts.in_set(UiSystems::Handle),
                refresh_views
                    .after(UiSystems::Handle)
                    .before(UiSystems::Send),
            ),
        );
}

fn attach(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    if objs
        .get(add.entity)
        .is_ok_and(|o| is_under(&o.path, KIOSK_ROOT))
    {
        commands
            .entity(add.entity)
            .insert_if_new(MedicalKiosk::default());
    }
}

fn say(world: &mut World, e: Entity, text: &str) {
    crate::atmos_machinery::machine_say(world, e, text);
}

/// `clearScans`.
fn clear_scans(world: &mut World, e: Entity) {
    if let Some(mut k) = world.get_mut::<MedicalKiosk>(e) {
        k.scan_active = 0;
    }
    refresh_look(world, e, false);
}

/// `update_icon_state`: `kiosk_open`, `kiosk_off` or `kiosk[_active]`.
fn refresh_look(world: &mut World, e: Entity, active: bool) {
    let state = if super::pods::panel_open(world, e) {
        "kiosk_open"
    } else if !operational(world, e) {
        "kiosk_off"
    } else if active
        || world
            .get::<MedicalKiosk>(e)
            .is_some_and(|k| k.scan_active != 0)
    {
        "kiosk_active"
    } else {
        "kiosk"
    };
    crate::obj_damage::set_icon_state(world, e, state);
}

/// `ishuman`: a carbon with an inventory.
fn is_human(world: &World, mob: Entity) -> bool {
    world.get::<Inventory>(mob).is_some() && world.get::<super::wounds::Limbs>(mob).is_some()
}

/// `ui_interact` of a working kiosk. True when `target` is a kiosk.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(kiosk) = world.get::<MedicalKiosk>(target).cloned() else {
        return false;
    };
    if !operational(world, target) {
        return true;
    }
    if !is_human(world, mob) {
        let name = super::name_of(world, target);
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} не может работать с не-гуманоидами!"),
        );
        return true;
    }
    let patient = kiosk
        .patient
        .filter(|&p| world.get_entity(p).is_ok() && world.get::<Health>(p).is_some());
    match patient {
        None => {
            say(world, target, "Scanner reset.");
            if let Some(mut k) = world.get_mut::<MedicalKiosk>(target) {
                k.patient = Some(mob);
            }
        }
        Some(p) => {
            let far = match (world.get::<TilePos>(target), world.get::<TilePos>(p)) {
                (Some(a), Some(b)) => a.chebyshev(*b).is_none_or(|d| d > PATIENT_RANGE),
                _ => true,
            };
            if far {
                if let Some(mut k) = world.get_mut::<MedicalKiosk>(target) {
                    k.patient = None;
                }
                say(world, target, "Patient out of range. Resetting biometrics.");
                clear_scans(world, target);
                return true;
            }
        }
    }
    if let Some(mut k) = world.get_mut::<MedicalKiosk>(target) {
        k.paying = Some(mob);
    }
    refresh_look(world, target, true);
    refresh_view(world, target);
    let title = super::name_of(world, target);
    world.write_message(OpenUi::new::<KioskWindow>(client, mob, target, title));
    true
}

/// `emag_act`: erratic readings. False when it was emagged already.
pub fn emag_act(world: &mut World, client: Entity, e: Entity) -> Option<bool> {
    let kiosk = world.get::<MedicalKiosk>(e)?;
    if kiosk.pandemonium {
        return Some(false);
    }
    if let Some(mut k) = world.get_mut::<MedicalKiosk>(e) {
        k.pandemonium = true;
    }
    tell(world, client, Span::Notice, "сенсоры перегружены");
    Some(true)
}

/// `inuse`: the payer pays for one scan. True when it is paid for.
fn pay(world: &mut World, e: Entity) -> bool {
    let Some(kiosk) = world.get::<MedicalKiosk>(e).cloned() else {
        return false;
    };
    let Some(payer) = kiosk.paying.filter(|&p| world.get_entity(p).is_ok()) else {
        if let Some(mut k) = world.get_mut::<MedicalKiosk>(e) {
            k.paying = None;
        }
        return false;
    };
    let card = world.get::<Inventory>(payer).and_then(|i| i.id_card(world));
    let account = card
        .and_then(|c| world.get::<BankAccountId>(c))
        .map(|a| a.0);
    let department = account
        .and_then(|id| world.get_resource::<Economy>()?.account(id))
        .map(|a| a.paycheck_department.clone());
    let watts = world
        .get::<ss13_mapload::DmPower>(e)
        .map_or(0.0, |p| p.active_power_usage);
    if department.as_deref() == Some(ACCOUNT_MED) {
        crate::kitchen::machine_use_energy(world, e, watts);
        say(world, e, "Hello, esteemed medical staff!");
        return true;
    }
    let bonus = if kiosk.pandemonium {
        super::fork_rng(world).range(10, 30)
    } else {
        0
    };
    let price = i64::from(kiosk.cost + bonus);
    // The payment component's `handle_card`.
    let Some(account) = account.filter(|_| department.is_some()) else {
        tell_mob(
            world,
            payer,
            Span::Warning,
            "ID-карта с аккаунтом не найдена.",
        );
        return false;
    };
    let charged = world
        .get_resource_mut::<Economy>()
        .and_then(|mut e| e.account_mut(account).map(|a| a.adjust_money(-price)))
        .unwrap_or(false);
    if !charged {
        tell_mob(world, payer, Span::Warning, "Недостаточно средств.");
        return false;
    }
    if let Some(mut economy) = world.get_resource_mut::<Economy>()
        && let Some(budget) = economy.department_mut(ACCOUNT_MED)
    {
        budget.adjust_money(price);
    }
    bank_card_talk(
        world,
        account,
        &format!(
            "С вашего счёта списано {price}{}.",
            crate::cargo::MONEY_SYMBOL
        ),
    );
    crate::kitchen::machine_use_energy(world, e, watts);
    refresh_look(world, e, true);
    say(world, e, "Thank you for your patronage!");
    true
}

fn tell_mob(world: &mut World, mob: Entity, span: Span, text: &str) {
    if let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

/// tg's `round(x, 0.001)`.
fn thousandths(x: f32) -> f32 {
    (x * 1000.0).round() / 1000.0
}

/// tg `brain_status` by brain damage.
pub fn brain_status(damage: f32) -> &'static str {
    if damage >= 100.0 {
        "Grave brain damage detected."
    } else if damage >= 50.0 {
        "Severe brain damage detected."
    } else if damage >= 20.0 {
        "Brain damage detected."
    } else if damage >= 1.0 {
        "Mild brain damage detected."
    } else {
        "Brain patterns normal."
    }
}

/// tg `patient_status` by total damage (the death check comes first and is overwritten by the
/// damage tiers, as in tg).
pub fn patient_status(dead: bool, total_damage: f32) -> &'static str {
    if total_damage >= 80.0 {
        "Gravely Injured"
    } else if total_damage >= 40.0 {
        "Injured"
    } else if total_damage >= 20.0 {
        "Lightly Injured"
    } else if dead {
        "Dead."
    } else {
        "Alive."
    }
}

/// `ui_data`.
fn compute_view(world: &mut World, e: Entity) -> MedicalKioskData {
    let Some(kiosk) = world.get::<MedicalKiosk>(e).cloned() else {
        return MedicalKioskData::default();
    };
    let Some(patient) = kiosk
        .patient
        .filter(|&p| world.get_entity(p).is_ok() && world.get::<Health>(p).is_some())
    else {
        return MedicalKioskData::default();
    };
    let user_hallucinating = kiosk.paying.is_some_and(|u| {
        world
            .get::<crate::hallucination::Hallucinating>(u)
            .is_some()
    });
    let chaos = if kiosk.pandemonium {
        1.0
    } else if user_hallucinating {
        0.3
    } else {
        0.0
    };
    let mut rng = super::fork_rng(world);
    let mut noise = |max: u32| chaos * rng.range(1, max) as f32;
    let health = world.get::<Health>(patient).cloned().unwrap_or_default();
    let (brute, burn, tox, oxy) = (health.brute, health.burn, health.tox, health.oxy);
    let dead = world.get::<Stat>(patient) == Some(&Stat::Dead);
    let mut status = patient_status(dead, brute + burn + tox + oxy).to_owned();
    let mut pick = super::fork_rng(world);
    if chaos > 0.0 {
        let i = pick.range(0, NONSENSE.len() as u32 - 1) as usize;
        status = NONSENSE[i].to_owned();
    }
    let brain =
        super::organs::state(world, patient, super::organs::Slot::Brain).map_or(0.0, |o| o.damage);
    let trauma_status = match super::traumas::scan_line(world, patient) {
        Some(line) => format!(
            "Cerebral traumas detected: patient appears to be suffering from {}",
            line.trim_start_matches("Mental trauma: ")
        ),
        None => "Patient is free of unique brain trauma.".to_owned(),
    };
    let (illness, illness_info) = world
        .get::<super::diseases::Diseases>(patient)
        .and_then(|d| {
            d.list
                .iter()
                .rfind(|d| d.visibility & super::diseases::HIDDEN_SCANNER == 0)
        })
        .map_or(
            (
                "Patient does not show signs of disease.".to_owned(),
                "Not Applicable.".to_owned(),
            ),
            |d| {
                (
                    "Warning: Patient is harboring some form of viral disease. Seek further \
                     medical attention."
                        .to_owned(),
                    format!(
                        "\nName: {}.\nType: {}.\nStage: {}/{}.\nCure: {}",
                        d.name,
                        d.spread_text,
                        d.stage,
                        d.max_stages,
                        d.cure_text(2)
                    ),
                )
            },
        );
    let blood = world.get::<super::blood::Blood>(patient).cloned();
    let percent = blood.as_ref().map_or(0.0, |b| {
        (b.volume / super::blood::BLOOD_VOLUME_NORMAL * 100.0).round()
    });
    let bleeding = world
        .get::<super::wounds::Limbs>(patient)
        .is_some_and(|l| l.bleed_rate(false) > 0.0);
    let (blood_name, bleed_status, blood_status) = match &blood {
        Some(b) => {
            let mut warning = " ".to_owned();
            if percent <= 80.0 {
                let level = if percent <= 60.0 {
                    "DANGEROUSLY low"
                } else {
                    "low"
                };
                warning = format!(" Patient has {level} blood levels.");
                let treatment = if percent <= 60.0 {
                    "blood transfusion or iron supplements or saline-glucose immediately"
                } else {
                    "a large meal or iron supplements"
                };
                warning.push_str(&format!(" Seek {treatment}"));
                if percent <= 60.0 {
                    warning.push_str(" Ignoring treatment may lead to death!");
                }
            }
            (
                "blood".to_owned(),
                if bleeding {
                    " Patient is currently bleeding!".to_owned()
                } else {
                    "Patient is not currently bleeding.".to_owned()
                },
                format!(
                    "Patient blood levels are currently reading {percent}%. Patient has {} type \
                     blood.{warning}",
                    b.blood_type.name()
                ),
            )
        }
        None => (
            "error".to_owned(),
            "Patient is not currently bleeding.".to_owned(),
            " Patient either has no blood, or does not require it to function.".to_owned(),
        ),
    };
    let chem = world.get_resource::<Chem>().map(|c| c.0.clone());
    let reagents = world.get::<MobReagents>(patient);
    let mut chemical_list = Vec::new();
    let mut overdose_list = Vec::new();
    if let (Some(chem), Some(reagents)) = (&chem, reagents) {
        for r in reagents
            .blood
            .reagents
            .iter()
            .chain(&reagents.stomach.reagents)
        {
            let def = chem.def(r.id);
            if def.chemical_flags & REAGENT_INVISIBLE != 0 {
                continue;
            }
            chemical_list.push((def.name.clone(), (r.volume * 100.0).round() / 100.0));
            if r.overdosed {
                overdose_list.push(def.name.clone());
            }
        }
    }
    let blood_alcohol = reagents.map_or(0.0, |r| {
        (r.drunk * DRUNK_TO_BLOOD_ALCOHOL * 100.0).round() / 100.0
    });
    let hallucinating_status = if world
        .get::<crate::hallucination::Hallucinating>(patient)
        .is_some()
    {
        "Subject appears to be hallucinating. Suggested treatments: Antipsychotic medication, \
         Haloperidol or Synaptizine."
    } else {
        "Patient is not hallucinating."
    };
    let max = if health.max > 0.0 { health.max } else { 100.0 };
    let bit = |i: u8| kiosk.scan_active & (1 << i) != 0;
    MedicalKioskData {
        kiosk_cost: kiosk.cost as f32 + noise(25),
        patient_name: super::name_of(world, patient),
        patient_health: thousandths((health.health() - noise(50)) / max * 100.0),
        brute_health: thousandths(brute + noise(30)),
        burn_health: thousandths(burn + noise(30)),
        toxin_health: thousandths(tox + noise(30)),
        suffocation_health: thousandths(oxy + noise(30)),
        brain_health: brain_status(brain).to_owned(),
        brain_damage: brain + noise(30),
        patient_status: status,
        trauma_status,
        patient_illness: illness,
        illness_info,
        bleed_status,
        blood_name: ss13_chat::speech::capitalize(&blood_name),
        blood_levels: percent - noise(35),
        blood_status,
        blood_alcohol,
        chemical_list,
        overdose_list,
        addict_list: Vec::new(),
        hallucinating_status: hallucinating_status.to_owned(),
        active: [bit(0), bit(1), bit(2), bit(3)],
    }
}

fn refresh_view(world: &mut World, e: Entity) {
    if world.get::<MedicalKiosk>(e).is_none() {
        return;
    }
    let view = compute_view(world, e);
    if let Some(mut k) = world.get_mut::<MedicalKiosk>(e)
        && k.view != view
    {
        k.view = view;
    }
}

/// Kiosks with an open window get fresh data.
// Cost: per tick, one pass over the open windows; a kiosk's data is a few dozen fields.
fn refresh_views(world: &mut World) {
    let mut q = world.query_filtered::<&UiWindow, With<UiKind<KioskWindow>>>();
    let mut kiosks: Vec<Entity> = q.iter(world).map(|w| w.source).collect();
    kiosks.sort();
    kiosks.dedup();
    for e in kiosks {
        refresh_view(world, e);
    }
}

/// Closed for a broken or unpowered kiosk.
fn cap_windows(world: &mut World) {
    let windows: Vec<(Entity, Entity)> = world
        .query_filtered::<(Entity, &UiWindow), With<UiKind<KioskWindow>>>()
        .iter(world)
        .map(|(w, window)| (w, window.source))
        .collect();
    for (window, e) in windows {
        if !operational(world, e)
            && let Some(mut w) = world.get_mut::<UiWindow>(window)
        {
            w.cap_status(UiStatus::Close);
        }
    }
}

/// The server side of the `MedicalKiosk` interface.
pub struct KioskWindow;

impl UiInterface for KioskWindow {
    const NAME: &'static str = MedicalKioskUi::NAME;
    type Data = MedicalKioskData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KioskAct {
    /// `beginScan_1..4` as the bit index 0..3.
    BeginScan(u8),
    ClearTarget,
}

impl ServerUi for KioskWindow {
    type Source = &'static MedicalKiosk;
    type UserParam = ();
    type Act = KioskAct;

    fn static_data(_: &MedicalKiosk) {}

    fn data(kiosk: &MedicalKiosk, _: Entity, _: &()) -> MedicalKioskData {
        kiosk.view.clone()
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<KioskAct, UiReject> {
        Ok(match action {
            "beginScan_1" => KioskAct::BeginScan(0),
            "beginScan_2" => KioskAct::BeginScan(1),
            "beginScan_3" => KioskAct::BeginScan(2),
            "beginScan_4" => KioskAct::BeginScan(3),
            "clearTarget" => KioskAct::ClearTarget,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

fn acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<KioskWindow>>>) {
    let pending: Vec<(Entity, Entity, KioskAct)> = acts
        .read(world.resource::<Messages<UiActed<KioskWindow>>>())
        .map(|a| (a.source, a.actor, a.act))
        .collect();
    for (e, actor, act) in pending {
        apply_act(world, e, actor, act);
    }
}

/// `ui_act`: a scan not paid yet is paid by the window's user, at the moment of effect.
pub fn apply_act(world: &mut World, e: Entity, actor: Entity, act: KioskAct) {
    let Some(kiosk) = world.get::<MedicalKiosk>(e).cloned() else {
        return;
    };
    match act {
        KioskAct::BeginScan(i) => {
            let bit = 1u8 << i;
            if kiosk.scan_active & bit != 0 {
                return;
            }
            if let Some(mut k) = world.get_mut::<MedicalKiosk>(e) {
                k.paying = Some(actor);
            }
            if pay(world, e)
                && let Some(mut k) = world.get_mut::<MedicalKiosk>(e)
            {
                k.scan_active |= bit;
            }
        }
        KioskAct::ClearTarget => {
            if let Some(mut k) = world.get_mut::<MedicalKiosk>(e) {
                k.patient = None;
            }
            clear_scans(world, e);
        }
    }
    refresh_view(world, e);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_lines_follow_tg_tiers() {
        assert_eq!(patient_status(false, 0.0), "Alive.");
        assert_eq!(patient_status(true, 10.0), "Dead.");
        // tg overwrites the death line with the damage tiers.
        assert_eq!(patient_status(true, 90.0), "Gravely Injured");
        assert_eq!(patient_status(false, 45.0), "Injured");
        assert_eq!(patient_status(false, 20.0), "Lightly Injured");
        assert_eq!(brain_status(0.5), "Brain patterns normal.");
        assert_eq!(brain_status(1.0), "Mild brain damage detected.");
        assert_eq!(brain_status(150.0), "Grave brain damage detected.");
    }
}

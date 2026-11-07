//! Requirements, tool quality, speed and failure modifiers, and operation availability.

use super::Implement::{Hand, Path, Tool};
use super::*;

impl Op {
    /// tg `get_requirements()`: all of, one of, optional, blocking.
    pub fn requirements(self) -> [Vec<String>; 4] {
        let def = self.def();
        let mut all: Vec<String> = match def.target {
            Target::Basic(zone) => {
                let zone = zone_name(zone);
                vec![format!("операция на {zone} (цель: {zone})")]
            }
            Target::Organ(slot) => vec![format!(
                "операция на {} (цель {})",
                slot.spec().name,
                slot.spec().zone.dm()
            )],
            Target::Limb => Vec::new(),
        };
        all.extend(state_guides(def.all_required, "требуется"));
        if def.flags & flags::STANDING_ALLOWED == 0 {
            all.push("пациент должен лежать".into());
        }
        all.extend(def.required.iter().map(|s| (*s).to_owned()));
        let mut any = Vec::new();
        let mut parsed = def.any_required;
        for (group, text) in [
            (
                state::ALL_BONE,
                "кость должна быть распилена или просверлена",
            ),
            (state::ALL_SKIN, "кожа должна быть разрезана или вскрыта"),
            (
                state::ALL_VESSEL,
                "кровеносные сосуды должны быть зажаты или разжаты",
            ),
        ] {
            if parsed & group == group {
                parsed &= !group;
                any.push(text.to_owned());
            }
        }
        any.extend(state_guides(parsed, "требуется"));
        let optional = if def.flags & flags::SELF_OPERABLE != 0 {
            vec!["хирург может сделать это самостоятельно".to_owned()]
        } else {
            Vec::new()
        };
        let mut blocked = Vec::new();
        let mut parsed = def.any_blocked;
        for (group, text) in [
            (state::ALL_BONE, "кость должна быть целой"),
            (state::ALL_SKIN, "кожа должна быть целой"),
            (state::ALL_VESSEL, "кровеносные сосуды должны быть целы"),
        ] {
            if parsed & group == group {
                parsed &= !group;
                blocked.push(text.to_owned());
            }
        }
        blocked.extend(state_guides(parsed, "не требуется"));
        if matches!(def.target, Target::Basic(_)) {
            blocked.push("пациент не должен быть роботом".into());
        }
        blocked.extend(def.blocked.iter().map(|s| (*s).to_owned()));
        if def.flags & flags::IGNORE_CLOTHES == 0 {
            blocked.push("рабочая область не должна быть закрыта одеждой".into());
        }
        [all, any, optional, blocked]
    }
}

/// `SURGERY_STATE_READABLE`, in bit order.
pub const STATE_READABLE: [(u16, &str); 8] = [
    (state::SKIN_CUT, "Кожа разрезана"),
    (state::SKIN_OPEN, "Кожа раскрыта"),
    (state::VESSELS_UNCLAMPED, "Сосуды не зажаты"),
    (state::VESSELS_CLAMPED, "Сосуды зажаты"),
    (state::ORGANS_CUT, "Органы разрезаны"),
    (state::BONE_DRILLED, "Кость просверлена"),
    (state::BONE_SAWED, "Кость распилена"),
    (state::CAVITY_WIDENED, "Грудная полость широко раскрыта"),
];

/// `bitfield_to_list(states, SURGERY_STATE_READABLE)`.
pub fn readable_states(states: u16) -> Vec<&'static str> {
    STATE_READABLE
        .iter()
        .filter(|(bit, _)| states & bit != 0)
        .map(|(_, text)| *text)
        .collect()
}

/// `bitfield_to_list(states, SURGERY_STATE_GUIDES(must))`.
pub(super) fn state_guides(states: u16, must: &str) -> Vec<String> {
    [
        (state::SKIN_CUT, format!("{must} разрез кожи")),
        (
            state::SKIN_OPEN,
            format!("{must}, чтобы кожа была раскрыта"),
        ),
        (
            state::VESSELS_UNCLAMPED,
            format!("{must}, чтобы сосуды были разжаты"),
        ),
        (
            state::VESSELS_CLAMPED,
            format!("{must}, чтобы сосуды были зажаты"),
        ),
        (
            state::ORGANS_CUT,
            format!("{must}, чтобы орган был разрезан"),
        ),
        (
            state::BONE_DRILLED,
            format!("{must}, чтобы кость была просверлена"),
        ),
        (
            state::BONE_SAWED,
            format!("{must}, чтобы кость была распилена"),
        ),
        (
            state::CAVITY_WIDENED,
            format!("{must}, чтобы грудная полость была раскрыта"),
        ),
    ]
    .into_iter()
    .filter(|(bit, _)| states & bit != 0)
    .map(|(_, text)| text)
    .collect()
}

/// tg `is_replaced`: an operation hidden because what replaces it (directly or down the chain)
/// is offered too.
pub fn is_replaced(op: Op, pool: &[&str]) -> bool {
    let mut next = op.def().replaced_by;
    while let Some(path) = next {
        if pool.contains(&path) {
            return true;
        }
        next = Op::of_path(path).and_then(|o| o.def().replaced_by);
    }
    false
}

/// The operations a surgeon may pick from: tg `GLOB.operations.unlocked` plus `extra` (the
/// operating computer's list), without the replaced ones.
pub fn operation_pool(extra: &[&'static str]) -> Vec<Op> {
    let mut paths: Vec<&str> = Op::types()
        .into_iter()
        .filter(|op| op.def().flags & flags::LOCKED == 0)
        .map(|op| op.def().path)
        .collect();
    for path in extra {
        if !paths.contains(path) {
            paths.push(path);
        }
    }
    Op::all()
        .into_iter()
        .filter(|op| paths.contains(&op.def().path) && !is_replaced(*op, &paths))
        .collect()
}

/// The tool as an operation sees it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolInfo {
    /// `None`: the empty hand.
    pub path: Option<String>,
    pub behaviour: String,
    pub toolspeed: f32,
    pub sharpness: u32,
    pub force: f32,
    /// `get_temperature()`.
    pub temperature: f32,
    /// Revival's `tool_check`: paddles of a powered unit, wielded and off cooldown; an active
    /// stun baton.
    pub shock_ready: bool,
}

impl ToolInfo {
    pub fn read(world: &World, item: Option<Entity>) -> Self {
        let Some(item) = item else {
            return Self::default();
        };
        let tool = world.get::<DmTool>(item);
        let lit = world.get::<Welder>(item).is_none_or(|w| w.on);
        Self {
            path: world.get::<DmItem>(item).map(|i| i.path.clone()),
            behaviour: tool.map(|t| t.behaviour.clone()).unwrap_or_default(),
            toolspeed: tool.map_or(1.0, |t| t.speed),
            sharpness: world.get::<DmWounding>(item).map_or(0, |w| w.sharpness),
            force: world.get::<DmItem>(item).map_or(0.0, |i| i.force),
            temperature: if lit {
                tool.map_or(0.0, |t| t.heat)
            } else {
                0.0
            },
            shock_ready: shock_ready(world, item),
        }
    }

    fn under(&self, root: &str) -> bool {
        self.path.as_deref().is_some_and(|p| is_under(p, root))
    }
}

pub(super) fn shock_ready(world: &World, item: Entity) -> bool {
    if let Some(super::super::defib::Paddles(unit)) =
        world.get::<super::super::defib::Paddles>(item)
    {
        let now = world.resource::<Time>().elapsed_secs_f64();
        let wielded = world
            .get::<crate::inventory::CarriedBy>(item)
            .is_some_and(|c| super::super::defib::is_wielded(world, c.0));
        return wielded
            && world
                .get::<super::super::defib::Defib>(*unit)
                .is_some_and(|d| d.powered() && now >= d.cooldown_until);
    }
    world
        .get::<crate::security::Baton>(item)
        .is_some_and(|b| b.active)
}

/// tg `tool_check` overrides.
pub(super) fn tool_check(op: Op, tool: &ToolInfo) -> bool {
    let def = op.def();
    let listed = def
        .implements
        .iter()
        .any(|(i, _)| *i == Tool(leak(&tool.behaviour)));
    match op {
        Op::InciseSkin => {
            tool.sharpness & super::super::wounds::SHARP_EDGED != 0
                || listed
                || tool.under("/obj/item/pen")
                || tool.under("/obj/item/screwdriver")
        }
        Op::SawBones => {
            (tool.sharpness & super::super::wounds::SHARP_EDGED != 0 && tool.force >= 10.0)
                || listed
        }
        Op::DrillBones => {
            (tool.sharpness & super::super::wounds::SHARP_POINTY != 0 && tool.force >= 10.0)
                || listed
        }
        // Saws would rip the organs up.
        Op::InciseOrgans => {
            (tool.sharpness & super::super::wounds::SHARP_EDGED != 0 || listed)
                && tool.behaviour != "saw"
        }
        Op::CloseSkin => {
            tool.under("/obj/item/stack/medical/suture")
                || tool.temperature >= FIRE_MINIMUM_TEMPERATURE_TO_EXIST
        }
        Op::SealVeins => tool.temperature >= FIRE_MINIMUM_TEMPERATURE_TO_EXIST,
        Op::Gastrectomy | Op::Lobotomy => {
            tool.sharpness & super::super::wounds::SHARP_EDGED != 0 || listed
        }
        // shortcut: an energy gun never counts as charged with an electrode (no fire modes
        // here), so only paddles and batons shock a brain.
        Op::Revival => {
            !tool.under("/obj/item/gun/energy")
                && (tool.shock_ready || tool.under("/obj/item/melee/touch_attack/shock"))
        }
        _ => true,
    }
}

/// A `&'static str` view for comparing with [`Implement::Tool`] (only the listed behaviours
/// can match, so unknown ones become "").
pub(super) fn leak(behaviour: &str) -> &'static str {
    const KNOWN: [&str; 12] = [
        "scalpel",
        "hemostat",
        "retractor",
        "cautery",
        "saw",
        "drill",
        "bonesetter",
        "screwdriver",
        "cutters",
        "crowbar",
        "welder",
        "knife",
    ];
    KNOWN.into_iter().find(|k| *k == behaviour).unwrap_or("")
}

/// The `implements` entry `tool` matches (its behaviour first, else its type), unscaled.
pub(super) fn listed_quality(op: Op, tool: &ToolInfo) -> Option<f32> {
    let implements = op.def().implements;
    let Some(path) = &tool.path else {
        return implements.iter().find(|(i, _)| *i == Hand).map(|(_, q)| *q);
    };
    let by_tool = implements
        .iter()
        .find(|(i, _)| !tool.behaviour.is_empty() && *i == Tool(leak(&tool.behaviour)))
        .map(|(_, q)| *q);
    let by_path = implements
        .iter()
        .find(|(i, _)| matches!(i, Path(root) if is_under(path, root)))
        .map(|(_, q)| *q);
    by_tool.or(by_path)
}

/// tg `get_tool_quality`: 0 when the tool cannot do the operation.
pub fn tool_quality(op: Op, tool: &ToolInfo) -> f32 {
    if tool.path.is_none() {
        return listed_quality(op, tool).unwrap_or(0.0);
    }
    if !tool_check(op, tool) {
        return 0.0;
    }
    tool.toolspeed * listed_quality(op, tool).unwrap_or(0.0)
}

/// tg `get_location_modifier`: the best surface on the patient's tile.
pub fn location_modifier(paths: &[String]) -> f32 {
    const MODS: [(&str, f32); 6] = [
        ("/obj/structure/table/optable/abductor", 0.85),
        ("/obj/structure/table/optable", 1.0),
        ("/obj/machinery/stasis", 1.15),
        ("/obj/structure/table", 1.25),
        ("/obj/structure/bed", 1.5),
        ("/obj/structure/kitchenspike", 1.5),
    ];
    paths
        .iter()
        .filter_map(|p| {
            MODS.iter()
                .find(|(root, _)| is_under(p, root))
                .map(|(_, m)| *m)
        })
        .fold(2.0, f32::min)
}

/// tg `get_time_modifiers`: tool quality × surgeon × surface × patient, rounded to 0.01.
pub fn speed_modifier(quality: f32, surgeon: f32, location: f32, patient: f32) -> f32 {
    let total = if quality > 0.0 { quality } else { 1.0 } * surgeon * location * patient;
    (total * 100.0).round() / 100.0
}

/// `get_mob_surgery_speed_mod`: ×0.8 on a patient who feels nothing (BandaStation: analgesia or
/// `stat >= UNCONSCIOUS`).
pub fn patient_speed_mod(painless: bool) -> f32 {
    if painless { 0.8 } else { 1.0 }
}

/// `get_surgeon_surgery_speed_mod`: drunkenness slows the surgeon, except at the Ballmer peak.
pub fn surgeon_speed_mod(drunk: f32) -> f32 {
    if (BALLMER_PEAK_LOW_END..=BALLMER_PEAK_HIGH_END).contains(&drunk) {
        0.8
    } else {
        1.0 + (drunk.powf(1.5) / 90.0 * 10.0).round() / 10.0
    }
}

/// `GET_FAILURE_CHANCE(base_time, speed_mod)`, time in deciseconds.
pub fn base_failure_chance(time: f32, speed: f32) -> f32 {
    FAILURE_CHANCE_PER_SECOND * ((speed * time - SURGERY_MODIFIER_FAILURE_THRESHOLD * time) / 10.0)
}

/// BandaStation's tool grade: an improvised implement (listed above 1.15) +10, else by the type
/// path: advanced 0, alien 1, augment 0, anything else 5.
pub fn tool_mod(op: Op, tool: &ToolInfo) -> f32 {
    // tg indexes `implements` with the tool object, which is never a key, so the improvised
    // grade never applied there; the listed quality of the matched implement is what it meant.
    if listed_quality(op, tool).is_some_and(|q| q > 1.15) {
        return TOOL_MOD_GHETTO;
    }
    let Some(path) = tool.path.as_deref() else {
        return 0.0;
    };
    if path.contains("/advanced") {
        TOOL_MOD_ADVANCED
    } else if path.contains("/alien") {
        TOOL_MOD_ALIEN
    } else if path.contains("/augment") {
        TOOL_MOD_AUGMENT
    } else {
        TOOL_MOD_BASIC
    }
}

/// BandaStation `get_modified_failure_chance`: the base chance (clamped), +40 on a patient who
/// feels it, darkness (`lumcount` below 0.6), the tool's grade.
pub fn failure_chance(
    time: f32,
    speed: f32,
    painless: bool,
    lumcount: Option<f32>,
    tool_mod: f32,
) -> f32 {
    let mut chance = base_failure_chance(time, speed).clamp(0.0, 99.0);
    if !painless {
        chance += 40.0;
    }
    if let Some(light) = lumcount
        && light < 0.6
    {
        chance += (1.0 - light / 0.6).powf(1.5) * 60.0;
    }
    chance + tool_mod
}

/// Reagents with `TRAIT_ANALGESIA` in `metabolized_traits` (BandaStation adds tramadol).
pub(super) const ANALGESICS: [&str; 12] = [
    "/datum/reagent/medicine/adminordrazine",
    "/datum/reagent/medicine/mine_salve",
    "/datum/reagent/medicine/morphine",
    "/datum/reagent/medicine/stimulants",
    "/datum/reagent/medicine/muscle_stimulant",
    "/datum/reagent/medicine/granibitaluri",
    "/datum/reagent/medicine/tramadol",
    "/datum/reagent/toxin/berserker",
    "/datum/reagent/drug/bath_salts",
    "/datum/reagent/drug/happiness",
    "/datum/reagent/drug/pumpup",
    "/datum/reagent/drug/maint/sludge",
];

/// `HAS_TRAIT(mob, TRAIT_ANALGESIA)` from what is metabolizing in its blood.
pub fn analgesia(world: &World, mob: Entity) -> bool {
    let (Some(reagents), Some(chem)) = (
        world.get::<crate::chem::MobReagents>(mob),
        world.get_resource::<crate::chem::Chem>(),
    ) else {
        return false;
    };
    reagents
        .blood
        .reagents
        .iter()
        .any(|r| r.metabolizing && ANALGESICS.contains(&chem.0.def(r.id).path.as_str()))
}

pub(super) fn drunkenness(world: &World, mob: Entity) -> f32 {
    world
        .get::<crate::chem::MobReagents>(mob)
        .map_or(0.0, |r| r.drunk)
}

/// Mob-level facts operations check.
#[derive(Clone, Debug)]
pub(super) struct Patient {
    pub(super) lying: bool,
    pub(super) unconscious: bool,
    /// Analgesia or unconscious (BandaStation `stat >= UNCONSCIOUS`).
    pub(super) painless: bool,
    dead: bool,
    /// tg `TRAIT_HUSK` ([`crate::human::Husk`]).
    husk: bool,
    pub(super) covered: u32,
    prepped_any: bool,
    brute: f32,
    burn: f32,
}

pub(super) fn patient(world: &World, mob: Entity) -> Option<Patient> {
    let health = world.get::<Health>(mob)?;
    let stat = world.get::<Stat>(mob).copied().unwrap_or_default();
    let limbs = world.get::<Limbs>(mob)?;
    let covered = world.get::<Inventory>(mob).map_or(0, |inv| {
        inv.equipped()
            .filter(|&item| inv.slot_of(item) != Some(InventorySlot::Neck))
            .filter_map(|item| world.get::<ss13_mapload::DmClothing>(item))
            .fold(0, |bits, c| bits | c.body_parts_covered)
    });
    let unconscious = health.knocked_out(stat);
    // BandaStation: `TRAIT_STASIS` feels no pain; a stasis bed's `free_operation`.
    let stasis = super::super::stasis::in_stasis(world, mob);
    let free = world
        .get::<super::super::stasis::FreeOperation>(mob)
        .is_some();
    Some(Patient {
        lying: world.get::<BodyPosition>(mob).is_some_and(|b| b.lying()),
        unconscious,
        painless: unconscious || stasis || analgesia(world, mob),
        dead: stat == Stat::Dead,
        husk: world.get::<crate::human::Husk>(mob).is_some(),
        covered,
        prepped_any: free || limbs.parts.iter().any(|l| l.prepped),
        brute: health.brute,
        burn: health.burn,
    })
}

/// tg `is_available`: the target's state allows `op` on `zone` (no tool, posture or surgeon
/// checks; what the operating computer shows as a next step).
pub(crate) fn is_available(world: &World, op: Op, mob: Entity, zone: TargetZone) -> bool {
    let (Some(p), Some(limbs)) = (patient(world, mob), world.get::<Limbs>(mob)) else {
        return false;
    };
    let def = op.def();
    let body = match def.target {
        // `basic`: the exact zone, a prepared patient and bodypart.
        Target::Basic(target) => {
            if zone != target || !p.prepped_any {
                return false;
            }
            target.check_zone()
        }
        // `organ`: the organ's exact zone, its bodypart prepared; organic (`required_organ_flag`).
        Target::Organ(slot) => {
            let organic = organs::state(world, mob, slot).is_some_and(|o| !o.robotic);
            if zone != slot.spec().zone || !organic {
                return false;
            }
            slot.bodypart()
        }
        Target::Limb => zone.check_zone(),
    };
    let limb = limbs.limb(body);
    if !limb.prepped
        && world
            .get::<super::super::stasis::FreeOperation>(mob)
            .is_none()
    {
        return false;
    }
    // A stump takes only operations with `allow_stumps`.
    if limb.missing != (op == Op::AttachLimb) {
        return false;
    }
    if def.all_required != 0 && !limb.has_state(def.all_required) {
        return false;
    }
    if def.any_required != 0 && !limb.has_any_state(def.any_required) {
        return false;
    }
    if def.any_blocked != 0 && limb.has_any_state(def.any_blocked) {
        return false;
    }
    let organ = organ_target(op).and_then(|slot| organs::state(world, mob, slot));
    let damage = organ.map_or(0.0, |o| o.damage);
    let max = organ.map_or(0.0, |o| o.spec().max_health);
    // `organ/repair/state_check`: moderately damaged and not repaired before.
    let repairable = |heal_to: f32| organ.is_some_and(|o| !o.operated) && damage >= max * heal_to;
    let bones_intact = !limb.has_any_state(state::BONE_SAWED | state::BONE_DRILLED);
    let state_ok = match op {
        Op::TendWounds(_, Heal::Brute) => p.brute > 0.0,
        Op::TendWounds(_, Heal::Burn) => p.burn > 0.0,
        Op::TendWounds(_, Heal::Combo) => p.brute > 0.0 || p.burn > 0.0,
        // A wound's own broken-bone state does not offer the fix (tg avoids the confusion).
        Op::FixBones => !limb
            .wounds
            .iter()
            .any(|w| w.surgery_states & state::ALL_BONE != 0),
        Op::RepairDislocation => limb
            .wound(Series::Bone)
            .is_some_and(|w| w.kind == WoundKind::Dislocation),
        Op::RepairHairline => limb
            .wound(Series::Bone)
            .is_some_and(|w| w.kind == WoundKind::Hairline),
        Op::ResetCompound => limb
            .wound(Series::Bone)
            .is_some_and(|w| w.kind == WoundKind::Compound && !w.reset),
        Op::RepairCompound => limb
            .wound(Series::Bone)
            .is_some_and(|w| w.kind == WoundKind::Compound && w.reset),
        // `amputate/state_check` and `required_bodytype`: not the chest, not
        // `TRAIT_NODISMEMBER`; flesh limbs are cut off, robotic ones taken apart.
        Op::Amputate | Op::AmputateMechanic => {
            body != BodyZone::Chest
                && !crate::species::has_trait(world, mob, crate::species::Trait::NoDismember)
                && limb.robotic == (op == Op::AmputateMechanic)
        }
        Op::RemoveOrgan(slot) => {
            zone == slot.spec().zone
                && organs::organ(world, mob, slot).is_some()
                && organ_bone_ok(slot, limb)
        }
        Op::InsertOrgan(slot) => {
            zone == slot.spec().zone && organs::lacks(world, mob, slot) && organ_bone_ok(slot, limb)
        }
        Op::RemoveImplant(slot) => {
            zone == slot.zone()
                && cyberimps::implant_in(world, mob, slot).is_some()
                && implant_bone_ok(zone, limb)
        }
        // The held implant's slot is checked with the tool ([`available`]).
        Op::InsertImplant => cyberimps::takes_implants(zone) && implant_bone_ok(zone, limb),
        // `revival/state_check`: not a husk.
        // shortcut: no suicide or DNR flags yet.
        Op::Revival => p.dead && !p.husk,
        Op::CoronaryBypass | Op::Lobectomy => repairable(REPAIR_HEAL_TO),
        Op::Hepatectomy => repairable(HEPATECTOMY_HEAL_TO),
        Op::Gastrectomy => repairable(GASTRECTOMY_HEAL_TO),
        Op::EarSurgery | Op::EyeSurgery => bones_intact,
        Op::Debride => limb.wound(Series::Burn).is_some_and(|w| w.infection > 0.0),
        // `filter/state_check`: not a husk.
        Op::FilterBlood => body == BodyZone::Chest && !p.husk,
        // `bioware/state_check`: its zone, no bioware of the same kind yet.
        Op::Bioware(kind) => {
            body == kind.zone().check_zone() && super::super::bioware::can_gain(world, mob, kind)
        }
        Op::RepairPuncture => limb
            .wound(Series::Pierce)
            .is_some_and(|w| w.blood_flow > 0.0 && !w.mend_state),
        Op::SealVeins => limb
            .wound(Series::Pierce)
            .is_some_and(|w| w.blood_flow > 0.0 && w.mend_state),
        _ => true,
    };
    if !state_ok {
        return false;
    }
    def.flags & flags::IGNORE_CLOTHES != 0 || p.covered & crate::chem::body_part_bit(body) == 0
}

/// `organ_manipulation/internal/organ_check`: chest organs and the brain need the bone sawed.
pub(super) fn organ_bone_ok(slot: Slot, limb: &super::super::wounds::Limb) -> bool {
    let locked = slot.spec().zone == TargetZone::Chest || slot == Slot::Brain;
    !locked || limb.has_state(state::BONE_SAWED)
}

/// `organ_check` of an implant: the ones in the chest need the bone sawed.
pub(super) fn implant_bone_ok(zone: TargetZone, limb: &super::super::wounds::Limb) -> bool {
    zone != TargetZone::Chest || limb.has_state(state::BONE_SAWED)
}

/// tg `snowflake_check_availability` with the tool in hand: an organ goes into its own slot, a
/// limb onto its own stump.
pub(super) fn tool_fits(op: Op, zone: TargetZone, tool: &ToolInfo) -> bool {
    let path = tool.path.as_deref().unwrap_or_default();
    match op {
        Op::InsertOrgan(slot) => Slot::of_path(path) == Some(slot),
        Op::AttachLimb => super::super::bodyparts::zone_of_path(path) == Some(zone.check_zone()),
        _ => true,
    }
}

/// tg `check_availability` of `op` on `mob`'s `zone` with `tool`, by `surgeon`.
pub(super) fn available(
    world: &World,
    op: Op,
    mob: Entity,
    surgeon: Entity,
    zone: TargetZone,
    tool: &ToolInfo,
) -> bool {
    if tool_quality(op, tool) <= 0.0
        || !tool_fits(op, zone, tool)
        || !is_available(world, op, mob, zone)
    {
        return false;
    }
    // `is_insert_available` of an implant: a slot of the zone, empty.
    if op == Op::InsertImplant
        && !cyberimps::slot_for(tool.path.as_deref().unwrap_or_default(), zone)
            .is_some_and(|slot| cyberimps::implant_in(world, mob, slot).is_none())
    {
        return false;
    }
    let def = op.def();
    if def.flags & flags::STANDING_ALLOWED == 0 && !patient(world, mob).is_some_and(|p| p.lying) {
        return false;
    }
    def.flags & flags::SELF_OPERABLE != 0 || mob != surgeon
}

/// tg `get_available_operations` over `pool` ([`operation_pool`]), sorted by name
/// (`cmp_name_asc`).
pub fn available_ops(
    world: &World,
    pool: &[Op],
    mob: Entity,
    surgeon: Entity,
    zone: TargetZone,
    tool: &ToolInfo,
) -> Vec<Op> {
    let mut ops: Vec<Op> = pool
        .iter()
        .copied()
        .filter(|&op| available(world, op, mob, surgeon, zone, tool))
        .collect();
    ops.sort_by_key(|op| op.def().name.to_lowercase());
    ops
}

/// The operating table `mob` lies on and the operating computer next to it (tg
/// `table.patient`, `table.computer`: the first computer around the table in `alldirs` order).
pub fn table_and_computer(world: &World, mob: Entity) -> Option<(Entity, Option<Entity>)> {
    let lying = world.get::<BodyPosition>(mob).is_some_and(|b| b.lying());
    let pos = *world.get::<TilePos>(mob)?;
    if !lying {
        return None;
    }
    let index = world.resource::<ss13_world::SpatialIndex>();
    let table = index.on_tile(pos).iter().copied().find(|&e| {
        crate::construction::path_of(world, e)
            .is_some_and(|p| is_under(&p, "/obj/structure/table/optable"))
    })?;
    Some((table, computer_near(world, pos)))
}

/// tg `alldirs`.
pub const ALLDIRS: [(i32, i32); 8] = [
    (0, 1),
    (0, -1),
    (1, 0),
    (-1, 0),
    (1, 1),
    (-1, 1),
    (1, -1),
    (-1, -1),
];

/// The first operating computer around `pos` (`locate() in get_step(src, direction)`).
pub fn computer_near(world: &World, pos: TilePos) -> Option<Entity> {
    let index = world.resource::<ss13_world::SpatialIndex>();
    ALLDIRS.iter().find_map(|&(dx, dy)| {
        let tile = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
        index
            .on_tile(tile)
            .iter()
            .copied()
            .find(|&e| world.get::<OperatingComputer>(e).is_some())
    })
}

/// The operations offered on `mob`: tg `unlocked` plus, for a patient lying on an operating
/// table, its computer's `advanced_surgeries` (`get_surgeries`, no power check).
pub fn pool_for(world: &World, mob: Entity) -> Vec<Op> {
    let extra = table_and_computer(world, mob)
        .and_then(|(_, computer)| computer)
        .map(|c| advanced_surgeries(world, c))
        .unwrap_or_default();
    operation_pool(&extra)
}

//! The changeling (server) after tg `/datum/antagonist/changeling`
//! (`code/modules/antagonists/changeling/changeling.dm`, `changeling_power.dm`,
//! `cellular_emporium.dm`, `powers/*`) and its saymode (`/datum/saymode/changeling`).
//!
//! - [`add_changeling`] is `add_antag_datum(/datum/antagonist/changeling)`: the hivemind name
//!   (`Mr. Alpha`) and hive, the cellular emporium button, the innate powers, the first DNA
//!   profile (its own), the objectives (absorb, steal, kill or maroon with identity theft, escape
//!   or escape with identity), the greeting, the info panel, the antag HUD icon, the chemical
//!   counter and the hivemind (`:g`).
//! - Chemicals: 20 of 75, +1 per second each `Life()` (`SSmobs`, 2 s), a quarter of that while
//!   burning, up to half the storage while dead; a power costs its `chemical_cost` when used.
//! - Powers ([`Power`]) are action buttons ([`crate::actions`]). Innate: absorb DNA (an
//!   aggressive grab, three 15 s stages, the victim is killed and husked, its profile stored),
//!   transform (a stored profile's name and body, `ListInputWindow` for tg's radial menu),
//!   reviving stasis (40 s of fake death, then a revive button heals fully) and regenerate.
//!   The cellular emporium ([`CellularEmporiumUi`]) sells gene stim, fleshmend, the arm blade
//!   and chitinous armor for genetic points (10); readapting (once per true absorption) refunds
//!   them all.
//! - [`roundend_report`]: the changelings, their extracted genes and objectives.
// shortcut: the emporium sells the four powers above (stings, the tentacle (no projectiles),
// shields, suits, lesser form, last resort and the utility powers are not ported); flesh
// clothes are not imitated by transform (the body and name change, worn items stay), no voice
// (no TTS); absorbed victims do not join the hivemind as imaginary friends, no memories or
// recent speech are stolen (only antagonist objectives); the kill grab is the aggressive grab
// here (grabs stop there, `crate::pull`); stasis enters without tg's confirmation alert; the arm
// blade does not pry airlocks or smash tables; chitin gives no armor (no armor values yet); gene
// stim's reagent is added without its own metabolism effects.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_chat::server::{Listeners, SaySystems, Spoke};
use ss13_chat::speech::{ChangelingHivemind, RadioTarget};
use ss13_mapload::IconIndex;
use ss13_mapload::antag::StealItemDef;
use ss13_mapload::spawner::Rng;
use ss13_net::chat::server::SendChat;
use ss13_net::server::ClientToken;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::InventorySlot;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::list_input::{ListInputData, ListInputUi};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::{
    Ability, AntagInfoChangelingUi, CellularEmporiumUi, ChangelingHudData, ChangelingHudUi,
    ChangelingInfo, EVOLVE, EmporiumData, EmporiumStatic, READAPT,
};
use crate::actions::{ActionButton, ActionButtonsUi, set_buttons};
use crate::antag::dynamic::DynamicRng;
use crate::antag::objectives::{
    Objective, ObjectiveKind, RoundEnd, considered_escaped, find_steal_target, find_target,
    target_text,
};
use crate::antag::uplink::UplinkCatalog;
use crate::antag::{AntagHudData, AntagHudUi, ObjectiveInfo};
use crate::combat::{CLICK_CD_MELEE, NextMove};
use crate::construction::Job;
use crate::ghost::Ghost;
use crate::health::{DamageType, DeathComa, Health, Stat, apply_damage};
use crate::human::{HumanLook, Husk};
use crate::hunger::{Hunger, NUTRITION_LEVEL_WELL_FED};
use crate::inventory::{CarriedBy, Inventory, NoDrop, no_drop, unequip};
use crate::jobs::PlayerJob;
use crate::medical::blood::{BLOOD_VOLUME_NORMAL, Blood};
use crate::medical::{name_of, visible};
use crate::pull::{GrabState, Pulling};
use crate::server::PlayerMob;
use crate::status_effects::{Incap, effects_mut};
use crate::tools::{Actor, DoAfter, do_after, tell};

mod absorb;
mod gain;
mod life;
mod powers;
mod transform;
mod windows;
pub use absorb::*;
pub use gain::*;
pub use life::*;
pub use powers::*;
pub use transform::*;
pub use windows::*;

/// `name = "\proper Генокрад"`.
pub const CHANGELING_NAME: &str = "Генокрад";
/// `roundend_category`.
pub const ROUNDEND_CATEGORY: &str = "Генокрады";
/// `chem_charges`, `total_chem_storage`, `chem_recharge_rate`.
pub const START_CHEMICALS: f32 = 20.0;
pub const CHEM_STORAGE: f32 = 75.0;
pub const CHEM_RECHARGE_RATE: f32 = 1.0;
/// `genetic_points` and `total_genetic_points`.
pub const GENETIC_POINTS: u32 = 10;
/// `dna_max`.
pub const DNA_MAX: usize = 6;
/// `attempt_absorb`: three `do_after`s of 15 s.
pub const ABSORB_STAGES: u8 = 3;
pub const ABSORB_STAGE_DS: f32 = 150.0;
/// `fakedeath_duration`.
pub const FAKEDEATH_SECS: f64 = 40.0;
/// `/datum/status_effect/fleshmend` duration and per-second healing.
pub const FLESHMEND_SECS: f64 = 10.0;
const FLESHMEND_HEAL: [(DamageType, f32); 3] = [
    (DamageType::Brute, 4.0),
    (DamageType::Burn, 2.0),
    (DamageType::Oxy, 4.0),
];
/// Chitinous armor's `recharge_slowdown`.
const ARMOR_SLOWDOWN: f32 = 0.125;
const OBJECTIVE_NAME: &str = "Задача";

const ACTIONS_DMI: &str = "icons/mob/actions/actions_changeling.dmi";
const ANTAG_HUD: (&str, &str) = ("icons/mob/huds/antag_hud.dmi", "changeling");
/// `/datum/action/cellular_emporium` `button_icon`, `button_icon_state`.
const EMPORIUM_ICON: (&str, &str) = ("icons/obj/drinks/soda.dmi", "changelingsting");
const EMPORIUM_ID: &str = "cellular_emporium";
const ARM_BLADE: &str = "/obj/item/melee/arm_blade";
const LING_ARMOR: &str = "/obj/item/clothing/suit/armor/changeling";
const LING_HELMET: &str = "/obj/item/clothing/head/helmet/changeling";
const ADRENALINE: &str = "/datum/reagent/medicine/changelingadrenaline";

/// `strings/greek_letters.txt`.
const GREEK_LETTERS: [&str; 24] = [
    "Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta", "Eta", "Theta", "Iota", "Kappa",
    "Lambda", "Mu", "Nu", "Xi", "Omicron", "Pi", "Rho", "Sigma", "Tau", "Upsilon", "Phi", "Chi",
    "Psi", "Omega",
];
/// `strings/names/hive_names.txt` (BandaStation).
const HIVE_NAMES: [&str; 31] = [
    "Алеф",
    "Айин",
    "Бет",
    "Далет",
    "Дигг",
    "Гами",
    "Гимел",
    "Хе",
    "Хет",
    "Хлл",
    "Кап",
    "Ламед",
    "Мем",
    "Нахс",
    "Нун",
    "Пе",
    "Пит",
    "Коф",
    "Реш",
    "Самех",
    "Шин",
    "Симс",
    "Таф",
    "Тау",
    "Тет",
    "Цейд",
    "Вав",
    "Вет",
    "Йод",
    "Заин",
    "Цик",
];

/// tg `req_stat`: `STABLE` powers need a conscious, awake changeling; the rest work in any
/// state (`req_stat < IS_UNCONSCIOUS_OR_CRIT(user)` only blocks `STABLE`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReqStat {
    Stable,
    HardCrit,
    Dead,
}

/// The `/datum/action/changeling/*` powers this server has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Power {
    AbsorbDna,
    Transform,
    Fakedeath,
    Regenerate,
    Adrenaline,
    Fleshmend,
    ArmBlade,
    Armor,
}

/// A power's vars.
#[derive(Clone, Copy, Debug)]
pub struct PowerDef {
    /// The action button's id.
    pub id: &'static str,
    pub path: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub helptext: &'static str,
    pub icon: &'static str,
    pub chemical_cost: u32,
    /// `dna_cost`; `None` is `CHANGELING_POWER_INNATE`.
    pub dna_cost: Option<u32>,
    pub req_dna: u32,
    pub req_absorbs: u32,
    pub req_human: bool,
    pub req_stat: ReqStat,
    pub ignores_fakedeath: bool,
    pub disabled_by_fire: bool,
    pub category: &'static str,
}

const BASE: PowerDef = PowerDef {
    id: "",
    path: "",
    name: "",
    desc: "",
    helptext: "",
    icon: "",
    chemical_cost: 0,
    dna_cost: None,
    req_dna: 0,
    req_absorbs: 0,
    req_human: false,
    req_stat: ReqStat::Stable,
    ignores_fakedeath: false,
    disabled_by_fire: true,
    category: "utility",
};

impl Power {
    pub const ALL: [Self; 8] = [
        Self::AbsorbDna,
        Self::Transform,
        Self::Fakedeath,
        Self::Regenerate,
        Self::Adrenaline,
        Self::Fleshmend,
        Self::ArmBlade,
        Self::Armor,
    ];

    pub fn def(self) -> PowerDef {
        match self {
            Self::AbsorbDna => PowerDef {
                id: "absorb_dna",
                path: "/datum/action/changeling/absorb_dna",
                name: "Absorb DNA",
                desc: "Поглотить ДНК нашей жертвы. Для поглощения нужно душить их.",
                icon: "absorb_dna",
                req_human: true,
                ..BASE
            },
            Self::Transform => PowerDef {
                id: "transform",
                path: "/datum/action/changeling/transform",
                name: "Transform",
                desc: "Мы принимаем облик и голос того, кого поглотили. Стоит 5 химикатов.",
                icon: "transform",
                chemical_cost: 5,
                req_dna: 1,
                req_human: true,
                ..BASE
            },
            Self::Fakedeath => PowerDef {
                id: "fakedeath",
                path: "/datum/action/changeling/fakedeath",
                name: "Reviving Stasis",
                desc: "Мы впадаем в стазис, что позволяет нам регенерировать и обманывать наших \
                       врагов. Стоит 15 химикатов.",
                icon: "fake_death",
                chemical_cost: 15,
                req_dna: 1,
                req_stat: ReqStat::Dead,
                ignores_fakedeath: true,
                disabled_by_fire: false,
                ..BASE
            },
            Self::Regenerate => PowerDef {
                id: "regenerate",
                path: "/datum/action/changeling/regenerate",
                name: "Regenerate",
                desc: "Позволяет регенерировать и восстанавливать отсутствующие внешние конечности \
                       и жизненно важные внутренние органы, а также удалять осколки, заживлять \
                       серьезные раны и восстанавливать объем крови. Стоит 10 химикатов.",
                helptext: "Оповестит ближайших членов экипажа о регенерации внешних конечностей. \
                           Можно использовать в бессознательном состоянии.",
                icon: "regenerate",
                chemical_cost: 10,
                req_stat: ReqStat::HardCrit,
                ..BASE
            },
            Self::Adrenaline => PowerDef {
                id: "adrenaline",
                path: "/datum/action/changeling/adrenaline",
                name: "Gene Stim",
                desc: "Мы концентрируем наши химикаты в мощный стимулятор, делающий нашу форму \
                       потрясающе устойчивой к недееспособности. Стоит 25 химикатов",
                helptext: "Избавляет от любого оглушения и вливает в нашу форму ЧЕТЫРЕ единицы \
                           адреналина генокрада; наша форма восстанавливает огромное количество \
                           выносливости и просто не замечает боли или усталости во время его \
                           воздействия.",
                icon: "adrenaline",
                chemical_cost: 25,
                dna_cost: Some(2),
                category: "combat",
                ..BASE
            },
            Self::Fleshmend => PowerDef {
                id: "fleshmend",
                path: "/datum/action/changeling/fleshmend",
                name: "Fleshmend",
                desc: "Наша плоть быстро регенерирует, заживляя ожоги и ушибы, одышку, а также \
                       скрывая все шрамы. Стоит 20 химикатов.",
                helptext: "Если мы находимся в огне, эффект лечения не действует. Не отращивает \
                           конечности и не восстанавливает потерянную кровь. Действует в \
                           бессознательном состоянии.",
                icon: "fleshmend",
                chemical_cost: 20,
                dna_cost: Some(2),
                req_stat: ReqStat::HardCrit,
                category: "combat",
                ..BASE
            },
            Self::ArmBlade => PowerDef {
                id: "arm_blade",
                path: "/datum/action/changeling/weapon/arm_blade",
                name: "Arm Blade",
                desc: "Мы превращаем одну из наших рук в смертоносный клинок. Стоит 20 химикатов.",
                helptext: "Мы можем убрать свой клинок так же, как и сформировали его. Нельзя \
                           использовать, находясь в меньшей форме.",
                icon: "arm_blade",
                chemical_cost: 20,
                dna_cost: Some(2),
                req_human: true,
                category: "combat",
                ..BASE
            },
            Self::Armor => PowerDef {
                id: "chitinous_armor",
                path: "/datum/action/changeling/suit/armor",
                name: "Chitinous Armor",
                desc: "Мы превращаем нашу кожу в прочный хитин, чтобы защитить себя от \
                       повреждений. Стоит 20 химикатов.",
                helptext: "На поддержание брони требуется небольшой расход химикатов. Доспехи \
                           обеспечивают достойную защиту от грубой силы и энергетического оружия. \
                           Не может быть использована в меньшей форме.",
                icon: "chitinous_armor",
                chemical_cost: 20,
                dna_cost: Some(1),
                req_human: true,
                category: "combat",
                ..BASE
            },
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.def().id == id)
    }

    pub fn from_path(path: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.def().path == path)
    }

    /// `dna_cost == CHANGELING_POWER_INNATE`: given on gain, never sold.
    pub fn innate(self) -> bool {
        self.def().dna_cost.is_none()
    }
}

/// `/datum/changeling_profile`: what a DNA sample looked like.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub name: String,
    /// The body (no clothes); `None` for a body drawn from its template.
    pub look: Option<HumanLook>,
    pub protected: bool,
}

impl Profile {
    /// `dna.is_same_as`: here the name and the body.
    pub fn same_dna(&self, name: &str, look: Option<&HumanLook>) -> bool {
        self.name == name && self.look.as_ref() == look
    }
}

/// Reviving stasis (`/datum/action/changeling/fakedeath`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Stasis {
    #[default]
    None,
    /// In fake death until `ready_at`; `died` when it began on a dead body.
    Waiting { ready_at: f64, died: bool },
    /// `revive_ready`: the button revives.
    Ready { died: bool },
}

impl Stasis {
    fn died(self) -> bool {
        matches!(
            self,
            Self::Waiting { died: true, .. } | Self::Ready { died: true }
        )
    }
}

/// The changeling's objectives (`forge_objectives`).
#[derive(Clone, Debug, PartialEq)]
pub enum LingObjective {
    /// `/datum/objective/absorb`.
    Absorb {
        target: u32,
    },
    /// `/datum/objective/absorb_most`.
    AbsorbMost,
    /// `/datum/objective/absorb_changeling`.
    AbsorbChangeling,
    /// `/datum/objective/escape/escape_with_identity`.
    EscapeIdentity {
        target: Option<Entity>,
        name: String,
        text: String,
        missing_id: bool,
    },
    Generic(Objective),
}

impl LingObjective {
    pub fn explanation(&self) -> String {
        match self {
            Self::Absorb { target } => format!("Извлеките {target} совместимых генов."),
            Self::AbsorbMost => {
                "Извлеките больше совместимых генов, чем любой другой генокрад.".to_owned()
            }
            Self::AbsorbChangeling => "Поглотите другого генокрада.".to_owned(),
            Self::EscapeIdentity { text, .. } => text.clone(),
            Self::Generic(o) => o.explanation.clone(),
        }
    }

    pub fn check_completion(
        &self,
        world: &World,
        owner: Entity,
        ling: &Changeling,
        end: RoundEnd,
    ) -> bool {
        match self {
            Self::Absorb { target } => ling.absorbed_count >= *target,
            Self::AbsorbMost => {
                let mut others = world.try_query::<(Entity, &Changeling)>();
                others.as_mut().is_none_or(|q| {
                    q.iter(world)
                        .filter(|(e, _)| *e != owner)
                        .all(|(_, other)| other.absorbed_count < ling.absorbed_count)
                })
            }
            Self::AbsorbChangeling => ling.spent_points() > GENETIC_POINTS,
            Self::EscapeIdentity {
                target,
                name,
                missing_id,
                ..
            } => {
                // Without a target it is the plain escape.
                if target.is_none() || name.is_empty() {
                    return considered_escaped(world, owner, end);
                }
                considered_escaped(world, owner, end)
                    && world.get::<Name>(owner).is_some_and(|n| n.as_str() == name)
                    && (*missing_id || id_name(world, owner).as_deref() == Some(name.as_str()))
            }
            Self::Generic(o) => o.check_completion(world, &[owner], end),
        }
    }
}

/// The changeling antagonist datum, on the body it was given to (the mind here).
#[derive(Component, Clone, Debug)]
pub struct Changeling {
    /// The player's key for the report.
    pub key: String,
    /// `changelingID`: the hivemind name.
    pub id_name: String,
    pub hive_name: String,
    /// `stored_profiles`, oldest first.
    pub profiles: Vec<Profile>,
    /// `absorbed_count`: profiles gained, the own one included.
    pub absorbed_count: u32,
    /// `true_absorbs`: absorptions of a body with a player.
    pub true_absorbs: u32,
    pub chem_charges: f32,
    pub total_chem_storage: f32,
    pub chem_recharge_rate: f32,
    pub chem_recharge_slowdown: f32,
    pub genetic_points: u32,
    pub total_genetic_points: u32,
    /// `purchased_powers`, in purchase order.
    pub purchased: Vec<Power>,
    /// `can_respec`.
    pub can_respec: u32,
    pub objectives: Vec<LingObjective>,
    /// `antag_memory`.
    pub memory: String,
    /// The absorption in progress: the victim and the stage (1-3).
    pub absorbing: Option<(Entity, u8)>,
    pub stasis: Stasis,
    /// `/datum/status_effect/fleshmend` end.
    pub fleshmend_until: f64,
    /// Absorbed by another changeling (`/datum/antagonist/fallen_changeling`).
    pub fallen: bool,
}

impl Default for Changeling {
    fn default() -> Self {
        Self {
            key: String::new(),
            id_name: "Changeling".to_owned(),
            hive_name: String::new(),
            profiles: Vec::new(),
            absorbed_count: 0,
            true_absorbs: 0,
            chem_charges: START_CHEMICALS,
            total_chem_storage: CHEM_STORAGE,
            chem_recharge_rate: CHEM_RECHARGE_RATE,
            chem_recharge_slowdown: 0.0,
            genetic_points: GENETIC_POINTS,
            total_genetic_points: GENETIC_POINTS,
            purchased: Vec::new(),
            can_respec: 0,
            objectives: Vec::new(),
            memory: String::new(),
            absorbing: None,
            stasis: Stasis::None,
            fleshmend_until: 0.0,
            fallen: false,
        }
    }
}

/// What [`Changeling::check_use`] needs to know about the changeling.
#[derive(Clone, Copy, Debug, Default)]
pub struct UseContext {
    pub stat: Stat,
    /// `IS_UNCONSCIOUS`: knocked out (hard crit, oxygen loss, stasis).
    pub knocked_out: bool,
    pub human: bool,
    pub on_fire: bool,
}

impl Changeling {
    pub fn has(&self, power: Power) -> bool {
        power.innate() || self.purchased.contains(&power)
    }

    pub fn in_stasis(&self) -> bool {
        self.stasis != Stasis::None
    }

    /// `adjust_chemicals(amount, override_cap)`.
    pub fn adjust_chemicals(&mut self, amount: f32, cap: Option<f32>) {
        let cap = cap.unwrap_or(self.total_chem_storage);
        self.chem_charges = (self.chem_charges + amount).clamp(0.0, cap.max(0.0));
    }

    /// `on_life` over `secs` of `Life()`: dead up to half the storage, burning a quarter speed.
    pub fn regenerate_chemicals(&mut self, secs: f32, dead: bool, on_fire: bool) {
        if dead {
            let rate = self.chem_recharge_rate - self.chem_recharge_slowdown;
            self.adjust_chemicals(rate * secs, Some(self.total_chem_storage * 0.5));
        } else if on_fire {
            self.adjust_chemicals((self.chem_recharge_rate - 0.75) * secs, None);
        } else {
            let rate = self.chem_recharge_rate - self.chem_recharge_slowdown;
            self.adjust_chemicals(rate * secs, None);
        }
    }

    /// `can_sting` + `try_to_sting`'s fire check: the message of the first failed check.
    pub fn check_use(&self, power: Power, ctx: UseContext) -> Result<(), String> {
        let def = power.def();
        if self.fallen || !self.has(power) || !ctx.human {
            return Err(String::new());
        }
        if self.chem_charges < def.chemical_cost as f32 {
            return Err(format!("требуется {} химикатов!", def.chemical_cost));
        }
        if self.absorbed_count < def.req_dna {
            return Err(format!("требуется {} образцов ДНК!", def.req_dna));
        }
        if self.true_absorbs < def.req_absorbs {
            return Err(format!("требуется {} поглощений!", def.req_absorbs));
        }
        if def.req_stat == ReqStat::Stable && (ctx.stat >= Stat::SoftCrit || ctx.knocked_out) {
            return Err("недееспособны!".to_owned());
        }
        if self.in_stasis() && !def.ignores_fakedeath {
            return Err("притворяемся мёртвым!".to_owned());
        }
        if def.disabled_by_fire && ctx.on_fire {
            return Err("on fire!".to_owned());
        }
        Ok(())
    }

    /// `purchase_power`'s checks.
    pub fn check_purchase(&self, power: Power) -> Result<u32, &'static str> {
        if self.purchased.contains(&power) {
            return Err("Мы уже развили эту способность!");
        }
        let def = power.def();
        let Some(cost) = def.dna_cost else {
            return Err("Мы не можем развить эту способность!");
        };
        if self.genetic_points < cost {
            return Err("Мы достигли предела своих возможностей!");
        }
        if self.absorbed_count < def.req_dna {
            return Err("У нас недостаточно ДНК, чтобы развить эту способность!");
        }
        if self.true_absorbs < def.req_absorbs {
            return Err("Нам недостаточно поглощенных ДНК, чтобы развить эту способность!");
        }
        if self.in_stasis() {
            return Err("Сейчас нам не хватает энергии для развития новых способностей!");
        }
        Ok(cost)
    }

    /// `remove_changeling_powers()`: the bought powers go, every point comes back.
    pub fn remove_purchased(&mut self) -> Vec<Power> {
        let gone = std::mem::take(&mut self.purchased);
        self.genetic_points = self.total_genetic_points;
        self.chem_charges = self.chem_charges.min(self.total_chem_storage);
        self.chem_recharge_rate = CHEM_RECHARGE_RATE;
        self.chem_recharge_slowdown = 0.0;
        gone
    }

    /// The points held plus those spent on powers (`absorb_changeling/check_completion`).
    pub fn spent_points(&self) -> u32 {
        self.genetic_points
            + self
                .purchased
                .iter()
                .filter_map(|p| p.def().dna_cost)
                .sum::<u32>()
    }

    pub fn has_profile(&self, name: &str, look: Option<&HumanLook>) -> bool {
        self.profiles.iter().any(|p| p.same_dna(name, look))
    }

    /// `add_profile`: over the limit the oldest unprotected profile goes first.
    pub fn add_profile(&mut self, profile: Profile) {
        if self.profiles.len() > DNA_MAX {
            let Some(i) = self.profiles.iter().position(|p| !p.protected) else {
                return;
            };
            self.profiles.remove(i);
        }
        self.profiles.push(profile);
        self.absorbed_count += 1;
    }
}

/// The steps of one absorption (`attempt_absorb`): three `do_after`s, each announced as it
/// starts; the stab of the third hurts the victim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbsorbStep {
    /// Start stage `n` (a `do_after` of [`ABSORB_STAGE_DS`]).
    Stage(u8),
    /// The last stage ended: absorb.
    Finish,
}

/// What follows the end of stage `stage` (0: the start).
pub fn absorb_next(stage: u8) -> AbsorbStep {
    if stage < ABSORB_STAGES {
        AbsorbStep::Stage(stage + 1)
    } else {
        AbsorbStep::Finish
    }
}

/// The changeling's timed actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangelingJob {
    /// One stage of `attempt_absorb`.
    Absorb,
}

/// `possible_changeling_IDs`: the greek letters not taken yet this round.
#[derive(Resource, Clone, Debug)]
struct ChangelingIds(Vec<&'static str>);

impl Default for ChangelingIds {
    fn default() -> Self {
        Self(GREEK_LETTERS.to_vec())
    }
}

/// The emporium's abilities with their sprites, built on gain.
#[derive(Component, Clone, Debug, Default)]
pub struct EmporiumCatalog(pub Vec<Ability>);

/// The window of the transform pick (tg `select_dna`'s radial menu).
pub struct TransformListWindow;

impl UiInterface for TransformListWindow {
    const NAME: &'static str = ListInputUi::NAME;
    type Data = ListInputData;
    type StaticData = ();
}

pub struct ChangelingPlugin;

impl Plugin for ChangelingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChangelingIds>()
            .register_ui::<AntagInfoChangelingUi>()
            .register_ui::<CellularEmporiumUi>()
            .register_ui::<ChangelingHudUi>()
            .register_ui::<TransformListWindow>()
            .add_systems(
                Update,
                (open_info, trigger_powers, emporium_acts, transform_acts)
                    .in_set(UiSystems::Handle),
            )
            .add_systems(Update, hivemind_chat.after(SaySystems))
            .add_systems(
                Update,
                (changeling_life, tick_changelings)
                    .chain()
                    .run_if(any_with_component::<Changeling>)
                    .before(crate::health::HealthSystems)
                    .before(UiSystems::Open),
            );
    }
}

use crate::tools::tell_mob as chat_to;

#[cfg(test)]
mod tests;

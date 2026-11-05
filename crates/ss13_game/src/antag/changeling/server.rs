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

fn chat_to(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

fn rng_of(world: &mut World, seed: u64) -> Rng {
    world
        .remove_resource::<DynamicRng>()
        .map_or_else(|| Rng::new(seed), |r| r.0)
}

fn icon(world: &World, (dmi, state): (&str, &str)) -> Option<ss13_shared::IconStateId> {
    world
        .get_resource::<IconIndex>()
        .and_then(|index| index.get(dmi, state))
}

/// The mob's DNA as a profile sees it: its name and body.
fn dna_of(world: &World, mob: Entity) -> (String, Option<HumanLook>) {
    let look = world.get::<HumanLook>(mob).map(|l| HumanLook {
        uniform: None,
        shoes: None,
        husk: false,
        missing: 0,
        robotic: 0,
        eyeless: false,
        ..l.clone()
    });
    (name_of(world, mob), look)
}

/// `get_id_name`: the registered name of the worn ID.
fn id_name(world: &World, mob: Entity) -> Option<String> {
    let card = world
        .get::<Inventory>(mob)?
        .id
        .and_then(|e| crate::inventory::get_id(world, e))?;
    world
        .get::<crate::outfit::IdRegistration>(card)
        .map(|r| r.registered_name.clone())
}

/// `add_antag_datum(/datum/antagonist/changeling)` on `mob`. `false` if it already is one.
pub fn add_changeling(world: &mut World, mob: Entity) -> bool {
    if world.get::<Changeling>(mob).is_some() || world.get::<Health>(mob).is_none() {
        return false;
    }
    let mut rng = rng_of(world, mob.to_bits());
    // `generate_name`: an honorific and a greek letter, then numbers.
    let honorific = match world.get::<HumanLook>(mob).map(|l| l.female) {
        Some(true) => "Ms.",
        Some(false) => "Mr.",
        None => "Mx.",
    };
    let letter = {
        let mut ids = world.resource_mut::<ChangelingIds>();
        (!ids.0.is_empty()).then(|| {
            let i = rng.range(0, ids.0.len() as i64 - 1) as usize;
            ids.0.remove(i)
        })
    };
    let id_name = match letter {
        Some(letter) => format!("{honorific} {letter}"),
        None => format!("{honorific} {}", rng.range(1, 999)),
    };
    let hive_name = format!(
        "{}-hive",
        HIVE_NAMES[rng.range(0, HIVE_NAMES.len() as i64 - 1) as usize]
    );
    let objectives = forge_objectives(world, mob, &mut rng);
    world.insert_resource(DynamicRng(rng));
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let (name, look) = dna_of(world, mob);
    let mut ling = Changeling {
        key,
        id_name,
        hive_name,
        objectives,
        ..default()
    };
    // `create_initial_profile`.
    ling.add_profile(Profile {
        name,
        look,
        protected: false,
    });
    let catalog = emporium_catalog(world);
    let hud_icon = icon(world, ANTAG_HUD);
    world.entity_mut(mob).insert((
        ling,
        catalog,
        AntagHudData {
            hud_icon,
            ..default()
        },
        ChangelingHudData::default(),
        ChangelingHivemind,
    ));
    // `greet()`, the panel note and the panel.
    chat_to(world, mob, Span::Bold, format!("Вы - {CHANGELING_NAME}."));
    announce_objectives(world, mob);
    chat_to(
        world,
        mob,
        Span::BoldNotice,
        "Для большей информации - прочтите панель. Вы можете вернуться снова, нажав кнопку \
         сверху слева.",
    );
    if let Some(client) = client {
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
        world.write_message(OpenUi::new::<ChangelingHudUi>(client, mob, mob, ""));
        world.write_message(OpenUi::new::<AntagInfoChangelingUi>(
            client,
            mob,
            mob,
            CHANGELING_NAME,
        ));
    }
    refresh(world, mob);
    info!("{mob} is a changeling");
    true
}

/// `announce_objectives`.
fn announce_objectives(world: &mut World, mob: Entity) {
    let Some(lines) = world.get::<Changeling>(mob).map(|c| {
        c.objectives
            .iter()
            .enumerate()
            .map(|(i, o)| format!("{OBJECTIVE_NAME} #{}: {}", i + 1, o.explanation()))
            .collect::<Vec<_>>()
    }) else {
        return;
    };
    chat_to(world, mob, Span::Notice, "Ваши текущие задачи:");
    for line in lines {
        chat_to(world, mob, Span::Plain, line);
    }
}

fn objective(world: &World, kind: ObjectiveKind) -> Objective {
    Objective {
        explanation: kind.explanation(world),
        kind,
    }
}

fn assassinate(target: Option<Entity>) -> ObjectiveKind {
    ObjectiveKind::Assassinate { target }
}

fn maroon(target: Option<Entity>) -> ObjectiveKind {
    ObjectiveKind::Maroon { target }
}

/// `escape_with_identity/update_explanation_text` for `target`.
fn identity_theft(world: &World, target: Option<Entity>) -> LingObjective {
    let Some(target) = target else {
        return LingObjective::EscapeIdentity {
            target: None,
            name: String::new(),
            text: ObjectiveKind::Escape.explanation(world),
            missing_id: false,
        };
    };
    let name = name_of(world, target);
    let mut text = format!(
        "Сбегите на эвакуационном шаттле или челноке под личиной {}",
        target_text(world, target)
    );
    let missing_id = id_name(world, target).as_deref() != Some(name.as_str());
    if !missing_id {
        text.push_str(", при этом нося ID карту цели");
    }
    text.push('.');
    LingObjective::EscapeIdentity {
        target: Some(target),
        name,
        text,
        missing_id,
    }
}

/// `forge_objectives`.
fn forge_objectives(world: &mut World, owner: Entity, rng: &mut Rng) -> Vec<LingObjective> {
    let competitive = {
        let mut q = world.query::<(Entity, &Changeling)>();
        q.iter(world).any(|(e, _)| e != owner)
    };
    let mut objectives = Vec::new();
    match if competitive { rng.range(1, 3) } else { 1 } {
        2 => objectives.push(LingObjective::AbsorbMost),
        3 => objectives.push(LingObjective::AbsorbChangeling),
        _ => {
            // `gen_amount_goal(6, 8)`: at most the other humans with a player.
            let goal = rng.range(6, 8) as u32;
            let mut q = world.query_filtered::<Entity, (With<PlayerMob>, With<HumanLook>)>();
            let others = q
                .iter(world)
                .filter(|&e| e != owner && world.get::<Changeling>(e).is_none())
                .count() as u32;
            objectives.push(LingObjective::Absorb {
                target: goal.min(others + 1),
            });
        }
    }
    let mut existing: Vec<Objective> = Vec::new();
    if rng.prob(60.0) {
        let targets: Vec<StealItemDef> = world
            .get_resource::<UplinkCatalog>()
            .map(|c| c.data.steal_items.clone())
            .unwrap_or_default();
        let item = find_steal_target(world, &[owner], &existing, &targets, rng);
        let o = objective(world, ObjectiveKind::Steal { item });
        existing.push(o.clone());
        objectives.push(LingObjective::Generic(o));
    }
    // shortcut: no AIs to destroy (`active_ais()` is empty here).
    let mut escape_possible = true;
    if rng.prob(70.0) {
        let target = find_target(world, &[owner], &existing, assassinate, rng);
        objectives.push(LingObjective::Generic(objective(
            world,
            assassinate(target),
        )));
    } else {
        let identity_target = find_target(world, &[owner], &existing, maroon, rng);
        let identity = identity_theft(world, identity_target);
        escape_possible = false;
        let target =
            identity_target.or_else(|| find_target(world, &[owner], &existing, maroon, rng));
        objectives.push(LingObjective::Generic(objective(world, maroon(target))));
        objectives.push(identity);
    }
    if escape_possible {
        if rng.prob(50.0) {
            objectives.push(LingObjective::Generic(objective(
                world,
                ObjectiveKind::Escape,
            )));
        } else {
            let target = find_target(world, &[owner], &existing, maroon, rng);
            objectives.push(identity_theft(world, target));
        }
    }
    objectives
}

/// `/datum/cellular_emporium/ui_static_data`: the sellable powers by name.
fn emporium_catalog(world: &World) -> EmporiumCatalog {
    let background = icon(world, (ACTIONS_DMI, "bg_changeling"));
    let mut abilities: Vec<Ability> = Power::ALL
        .into_iter()
        .filter_map(|p| {
            let def = p.def();
            Some(Ability {
                name: def.name.to_owned(),
                desc: def.desc.to_owned(),
                icon: def.icon.to_owned(),
                icon_state: icon(world, (ACTIONS_DMI, def.icon)),
                background,
                helptext: def.helptext.to_owned(),
                path: def.path.to_owned(),
                genetic_point_required: def.dna_cost?,
                absorbs_required: def.req_absorbs,
                dna_required: def.req_dna,
                category: def.category.to_owned(),
            })
        })
        .collect();
    abilities.sort_by(|a, b| a.name.cmp(&b.name));
    EmporiumCatalog(abilities)
}

/// The action buttons and the chemical counter from the changeling's state
/// (`build_all_button_icons`, `update_chemical_hud`).
pub fn refresh(world: &mut World, mob: Entity) {
    let Some(ling) = world.get::<Changeling>(mob).cloned() else {
        return;
    };
    let (background, border) = (
        icon(world, (ACTIONS_DMI, "bg_changeling")),
        icon(world, (ACTIONS_DMI, "bg_changeling_border")),
    );
    let button = |id: &str, name: &str, icon: Option<ss13_shared::IconStateId>| {
        (
            ActionButton {
                id: id.to_owned(),
                name: name.to_owned(),
                icon,
                background,
                border,
                available: true,
                cooldown: 0.0,
                active: false,
                maptext: None,
            },
            0.0,
        )
    };
    let mut buttons = Vec::new();
    if !ling.fallen {
        buttons.push(button(
            EMPORIUM_ID,
            "Cellular Emporium",
            icon(world, EMPORIUM_ICON),
        ));
        let powers = Power::ALL
            .into_iter()
            .filter(|p| p.innate())
            .chain(ling.purchased.iter().copied());
        for power in powers {
            let def = power.def();
            let ready = matches!(ling.stasis, Stasis::Ready { .. });
            let (name, state) = if power == Power::Fakedeath && ready {
                ("Revive", "revive")
            } else {
                (def.name, def.icon)
            };
            buttons.push(button(def.id, name, icon(world, (ACTIONS_DMI, state))));
        }
    }
    set_buttons(world, mob, buttons);
    if let Some(mut hud) = world.get_mut::<ChangelingHudData>(mob) {
        hud.set_if_neq(ChangelingHudData {
            chem_charges: ling.chem_charges.round() as u32,
            chem_max: ling.total_chem_storage.round() as u32,
        });
    }
}

fn use_context(world: &World, mob: Entity) -> UseContext {
    let stat = world.get::<Stat>(mob).copied().unwrap_or(Stat::Dead);
    let knocked_out = world.get::<DeathComa>(mob).is_some()
        || world
            .get::<Health>(mob)
            .is_some_and(|h| h.knocked_out(stat));
    UseContext {
        stat,
        knocked_out,
        human: world.get::<HumanLook>(mob).is_some(),
        on_fire: crate::fire::on_fire(world, mob),
    }
}

/// The use checks with their feedback (tg balloon alerts).
fn checked(world: &mut World, client: Entity, mob: Entity, power: Power) -> bool {
    let ctx = use_context(world, mob);
    match world
        .get::<Changeling>(mob)
        .map(|c| c.check_use(power, ctx))
    {
        Some(Ok(())) => true,
        Some(Err(text)) => {
            if !text.is_empty() {
                tell(world, client, Span::Warning, text);
            }
            false
        }
        None => false,
    }
}

/// `try_to_sting` after `sting_action` succeeded: the chemicals and the click cooldown.
fn spend(world: &mut World, mob: Entity, power: Power) {
    let cost = power.def().chemical_cost as f32;
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.adjust_chemicals(-cost, None);
    }
    world.entity_mut(mob).insert(NextMove(now + CLICK_CD_MELEE));
    info!("{mob} used {power:?} (changeling)");
    refresh(world, mob);
}

// ---------------------------------------------------------------------------------------------
// Buttons

/// Button clicks: tg `Trigger` of the changeling's actions and the emporium's.
fn trigger_powers(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        // The client must still control the changeling (not its ghost through a stale window).
        if actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world.get::<Changeling>(actor).is_none_or(|c| c.fallen)
        {
            continue;
        }
        if id == EMPORIUM_ID {
            world.write_message(OpenUi::new::<CellularEmporiumUi>(
                client,
                actor,
                actor,
                "cellular emporium",
            ));
            continue;
        }
        let Some(power) = Power::from_id(&id) else {
            continue;
        };
        try_power(world, client, actor, power);
    }
}

/// tg `try_to_sting` of `power`, with the powers' own overrides.
fn try_power(world: &mut World, client: Entity, mob: Entity, power: Power) {
    // `weapon/try_to_sting`, `suit/try_to_sting`: a second use takes it back, for free.
    match power {
        Power::ArmBlade if retract_blade(world, mob) => return,
        Power::Armor if cast_off_armor(world, mob) => return,
        _ => {}
    }
    // `fakedeath/can_sting`: a ready revive skips the stasis checks.
    if power == Power::Fakedeath
        && !matches!(
            world.get::<Changeling>(mob).map(|c| c.stasis),
            Some(Stasis::Ready { .. })
        )
        && world
            .get::<Changeling>(mob)
            .is_some_and(Changeling::in_stasis)
    {
        tell(world, client, Span::Warning, "уже возрождаемся!");
        return;
    }
    if !checked(world, client, mob, power) {
        return;
    }
    let done = match power {
        Power::AbsorbDna => start_absorb(world, client, mob),
        Power::Transform => open_transform(world, client, mob),
        Power::Fakedeath => fakedeath(world, mob),
        Power::Regenerate => regenerate(world, mob),
        Power::Adrenaline => adrenaline(world, mob),
        Power::Fleshmend => fleshmend(world, client, mob),
        Power::ArmBlade => form_blade(world, client, mob),
        Power::Armor => form_armor(world, client, mob),
    };
    if done {
        spend(world, mob, power);
    }
}

// ---------------------------------------------------------------------------------------------
// Absorb

/// `can_absorb_dna(target, verbose)`.
fn can_absorb(world: &mut World, mob: Entity, target: Entity, verbose: bool) -> bool {
    let (Some(ling), (name, look)) = (world.get::<Changeling>(mob).cloned(), dna_of(world, mob))
    else {
        return false;
    };
    let fail = |world: &mut World, text: String| {
        if verbose {
            chat_to(world, mob, Span::Warning, text);
        }
        false
    };
    if let Some(top) = ling.profiles.first()
        && top.same_dna(&name, look.as_ref())
        && ling.profiles.len() > DNA_MAX
    {
        return fail(
            world,
            "Мы достигли предела своей способности хранить генетическую информацию! Мы должны \
             трансформироваться, прежде чем поглощать больше."
                .to_owned(),
        );
    }
    let victim = name_of(world, target);
    let (target_name, target_look) = dna_of(world, target);
    if target_look.is_none() {
        return fail(
            world,
            format!("ДНК {victim} несовместимо с нашей биологией."),
        );
    }
    if ling.has_profile(&target_name, target_look.as_ref()) {
        return fail(world, "Мы уже имеем это ДНК в нашем хранилище!".to_owned());
    }
    // `TRAIT_BADDNA` (a drained body is also `TRAIT_HUSK`).
    if world.get::<Husk>(target).is_some() {
        return fail(world, format!("ДНК {victim} разрушено до неузнаваемости!"));
    }
    true
}

/// `absorb_dna/can_sting` + `sting_action`: the grab checks, then the first stage.
fn start_absorb(world: &mut World, client: Entity, mob: Entity) -> bool {
    if world
        .get::<Changeling>(mob)
        .is_some_and(|c| c.absorbing.is_some())
    {
        tell(world, client, Span::Warning, "уже поглощаем!");
        return false;
    }
    let target = world.get::<Pulling>(mob).map(|p| p.0);
    let Some(target) = target.filter(|&t| world.get::<Health>(t).is_some()) else {
        tell(world, client, Span::Warning, "нужно взять в захват!");
        return false;
    };
    if world.get::<GrabState>(mob).copied().unwrap_or_default() < GrabState::Aggressive {
        tell(world, client, Span::Warning, "нужно усилить захват!");
        return false;
    }
    if !can_absorb(world, mob, target, true) {
        return false;
    }
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.absorbing = Some((target, 0));
    }
    info!("{mob} starts absorbing {target} (changeling)");
    absorb_stage(world, client, mob, target, 1);
    // The cost (0) and the cooldown come when it finishes.
    false
}

/// Stage `stage` of `attempt_absorb`: its message, then a 15 s `do_after`.
fn absorb_stage(world: &mut World, client: Entity, mob: Entity, target: Entity, stage: u8) {
    let (user, victim) = (name_of(world, mob), name_of(world, target));
    match stage {
        1 => chat_to(
            world,
            mob,
            Span::Notice,
            "Это существо совместимо. Мы должны стоять неподвижно...",
        ),
        2 => visible(
            world,
            mob,
            "Мы вытягиваем жало-хоботок.",
            &format!("{user} вытягивает жало-хоботок!"),
            Span::Warning,
        ),
        _ => {
            visible(
                world,
                mob,
                &format!("Мы пронзаем {victim} жалом-хоботком."),
                &format!("{user} пронзает {victim} жалом-хоботком!"),
                Span::Danger,
            );
            chat_to(
                world,
                target,
                Span::BoldWarning,
                "Вы чувствуете острую колющую боль!",
            );
            // `take_overall_damage(40)`.
            hurt(world, target, DamageType::Brute, 40.0);
        }
    }
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.absorbing = Some((target, stage));
    }
    let item = world.get::<Inventory>(mob).and_then(Inventory::active_item);
    let actor = Actor { client, mob, item };
    let job = Job::Changeling(ChangelingJob::Absorb);
    if !do_after(world, actor, target, ABSORB_STAGE_DS, 0, job) {
        absorb_interrupted(world, mob);
    }
}

/// `balloon_alert("прервано!")`: the absorption ends.
fn absorb_interrupted(world: &mut World, mob: Entity) {
    let was = world
        .get_mut::<Changeling>(mob)
        .and_then(|mut c| c.absorbing.take());
    if was.is_some() {
        chat_to(world, mob, Span::Warning, "прервано!");
    }
}

/// [`Job::Changeling`] completions.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: ChangelingJob) {
    match job {
        ChangelingJob::Absorb => {
            let Some((target, stage)) = world.get::<Changeling>(mob).and_then(|c| c.absorbing)
            else {
                return;
            };
            if target != action.target {
                return;
            }
            match absorb_next(stage) {
                AbsorbStep::Stage(next) => absorb_stage(world, action.client, mob, target, next),
                AbsorbStep::Finish => finish_absorb(world, mob, target),
            }
        }
    }
}

/// `absorb_dna/sting_action` after `attempt_absorb`: the profile, the chemicals, the victim
/// killed and drained (`death()`, `Drain()`).
fn finish_absorb(world: &mut World, mob: Entity, target: Entity) {
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.absorbing = None;
    }
    // Revalidated: another changeling may have drained it meanwhile.
    if world.get_entity(target).is_err() || !can_absorb(world, mob, target, true) {
        return;
    }
    let (user, victim) = (name_of(world, mob), name_of(world, target));
    visible(
        world,
        mob,
        &format!("We have absorbed {victim}."),
        &format!("{user} sucks the fluids from {victim}!"),
        Span::Danger,
    );
    // `client || mind || last_mind`: a crew body or a player's.
    let true_absorb =
        world.get::<PlayerJob>(target).is_some() || world.get::<PlayerMob>(target).is_some();
    if !true_absorb {
        chat_to(
            world,
            mob,
            Span::Changeling,
            format!(
                "You absorb {victim}, but their weak DNA is not enough to satisfy your hunger."
            ),
        );
    }
    let (name, look) = dna_of(world, target);
    let memory = stolen_memory(world, target, &victim);
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        if !ling.has_profile(&name, look.as_ref()) {
            ling.add_profile(Profile {
                name,
                look,
                protected: false,
            });
            if true_absorb {
                ling.true_absorbs += 1;
            }
        }
        ling.memory.push_str(&memory);
    }
    // `set_nutrition(min(nutrition + target.nutrition, NUTRITION_LEVEL_WELL_FED))`.
    let theirs = world.get::<Hunger>(target).map_or(0.0, |h| h.nutrition);
    if let Some(mut hunger) = world.get_mut::<Hunger>(mob)
        && hunger.nutrition < NUTRITION_LEVEL_WELL_FED
    {
        hunger.nutrition = (hunger.nutrition + theirs).min(NUTRITION_LEVEL_WELL_FED);
    }
    absorb_changeling(world, mob, target, &victim);
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.adjust_chemicals(10.0, None);
        if true_absorb {
            ling.can_respec += 1;
        }
    }
    info!("{mob} absorbed {target} (changeling)");
    drain(world, target);
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(mob).insert(NextMove(now + CLICK_CD_MELEE));
    refresh(world, mob);
}

/// `absorb_memories`' antagonist part: the victim's objectives.
fn stolen_memory(world: &World, target: Entity, victim: &str) -> String {
    let objectives: Vec<String> =
        if let Some(t) = world.get::<crate::antag::traitor::Traitor>(target) {
            t.objectives.iter().map(|o| o.explanation.clone()).collect()
        } else if let Some(b) = world.get::<crate::antag::brother::Brother>(target) {
            b.objectives.iter().map(|o| o.explanation.clone()).collect()
        } else if let Some(v) = world.get::<crate::antag::vampire::server::Vampire>(target) {
            v.objectives.iter().map(|o| o.explanation()).collect()
        } else if let Some(c) = world.get::<Changeling>(target) {
            c.objectives
                .iter()
                .map(LingObjective::explanation)
                .collect()
        } else {
            Vec::new()
        };
    let mut memory = String::new();
    if !objectives.is_empty() {
        memory.push_str(" Задачи:");
        for (i, text) in objectives.iter().enumerate() {
            memory.push_str(&format!(" Задача #{}: {text}.", i + 1));
        }
    }
    memory.push_str(&format!(" Это все, что имел {victim}. "));
    memory
}

/// A changeling victim gives half its points and storage and all its DNA, and falls.
fn absorb_changeling(world: &mut World, mob: Entity, target: Entity, victim: &str) {
    let Some(theirs) = world
        .get::<Changeling>(target)
        .filter(|c| !c.fallen)
        .cloned()
    else {
        return;
    };
    chat_to(
        world,
        mob,
        Span::BoldNotice,
        format!("{victim} были одними из нас. Мы поглотили их силу."),
    );
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        let points = theirs.total_genetic_points / 2;
        ling.genetic_points += points;
        ling.total_genetic_points += points;
        let chems = (theirs.total_chem_storage / 2.0).round();
        ling.adjust_chemicals(chems, None);
        ling.total_chem_storage += chems;
        ling.absorbed_count += theirs.absorbed_count;
    }
    remove_powers(world, target);
    if let Some(mut fallen) = world.get_mut::<Changeling>(target) {
        fallen.fallen = true;
        fallen.purchased.clear();
    }
    world.entity_mut(target).remove::<ChangelingHivemind>();
    refresh(world, target);
}

/// `death()` + `Drain()`: dead, husked, bloodless.
fn drain(world: &mut World, target: Entity) {
    if world.get::<Stat>(target) != Some(&Stat::Dead) {
        world.entity_mut(target).insert(Stat::Dead);
    }
    world.entity_mut(target).insert(Husk);
    if let Some(mut look) = world.get_mut::<HumanLook>(target) {
        look.husk = true;
    }
    if let Some(mut inventory) = world.get_mut::<Inventory>(target) {
        inventory.set_changed();
    }
    if let Some(mut blood) = world.get_mut::<Blood>(target) {
        blood.volume = 0.0;
    }
}

fn hurt(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
    let (Some(mut health), Some(mut stat)) = (
        world.get::<Health>(mob).cloned(),
        world.get::<Stat>(mob).copied(),
    ) else {
        return;
    };
    let changed = apply_damage(&mut health, &mut stat, kind, amount);
    world.entity_mut(mob).insert(health);
    if changed.is_some() {
        world.entity_mut(mob).insert(stat);
    }
}

// ---------------------------------------------------------------------------------------------
// Transform

/// `transform/sting_action` -> `select_dna`: the profiles to pick from.
fn open_transform(world: &mut World, client: Entity, mob: Entity) -> bool {
    world.write_message(OpenUi::new::<TransformListWindow>(
        client,
        mob,
        mob,
        "Трансформация",
    ));
    false
}

impl ServerUi for TransformListWindow {
    type Source = &'static Changeling;
    type UserParam = ();
    type Act = Option<String>;

    fn static_data(_: &Changeling) {}

    fn data(ling: &Changeling, _: Entity, _: &()) -> ListInputData {
        let items: Vec<String> = ling.profiles.iter().map(|p| p.name.clone()).collect();
        ListInputData {
            title: "Трансформация".into(),
            message: "Выберите облик.".into(),
            init_value: items.first().cloned().unwrap_or_default(),
            items,
            timeout: None,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<Option<String>, UiReject> {
        match action {
            "submit" => params
                .get("entry")
                .map(|e| Some(e.to_owned()))
                .ok_or_else(|| UiReject::BadParams("entry: expected a profile".into())),
            "cancel" => Ok(None),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// The pick: checked again at the moment of effect, then `transform(user, profile)`.
fn transform_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<TransformListWindow>>>) {
    let pending: Vec<(Entity, Entity, Entity, Entity, Option<String>)> = acts
        .read(world.resource::<Messages<UiActed<TransformListWindow>>>())
        .map(|a| (a.window, a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (window, client, actor, source, pick) in pending {
        if let Some(mut window) = world.get_mut::<ss13_tgui::server::UiWindow>(window) {
            window.cap_status(ss13_shared::ui::UiStatus::Close);
        }
        let Some(pick) = pick else {
            continue;
        };
        if actor != source || !checked(world, client, actor, Power::Transform) {
            continue;
        }
        let Some(profile) = world
            .get::<Changeling>(actor)
            .and_then(|c| c.profiles.iter().find(|p| p.name == pick).cloned())
        else {
            continue;
        };
        transform(world, actor, &profile);
        spend(world, actor, Power::Transform);
    }
}

/// `/datum/antagonist/changeling/transform`: the name and the body of `profile`; worn items and
/// the ID stay.
pub fn transform(world: &mut World, mob: Entity, profile: &Profile) {
    world
        .entity_mut(mob)
        .insert(Name::new(profile.name.clone()));
    if let (Some(look), Some(mut body)) = (&profile.look, world.get_mut::<HumanLook>(mob)) {
        *body = HumanLook {
            uniform: body.uniform.clone(),
            shoes: body.shoes.clone(),
            husk: false,
            missing: body.missing,
            robotic: body.robotic,
            eyeless: body.eyeless,
            ..look.clone()
        };
    }
    // `transform` copies the profile's species too (`hardset_dna(..., mrace)`).
    if let Some(look) = &profile.look
        && look.species != crate::species::species_of(world, mob)
    {
        crate::species::set_species(world, mob, look.species);
    }
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.set_changed();
    }
    info!("{mob} transformed into {:?} (changeling)", profile.name);
}

// ---------------------------------------------------------------------------------------------
// Stasis

/// `fakedeath/sting_action`: a ready revive revives; otherwise fake death starts.
fn fakedeath(world: &mut World, mob: Entity) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let stasis = world.get::<Changeling>(mob).map(|c| c.stasis);
    if let Some(Stasis::Ready { .. }) = stasis {
        revive(world, mob);
        // The revive costs nothing (`chemical_cost = 0` while ready).
        refresh(world, mob);
        return false;
    }
    let died = world.get::<Stat>(mob) == Some(&Stat::Dead);
    // `fakedeath(CHANGELING_TRAIT)`: the deathgasp comes with the stat (`crate::health`).
    world.entity_mut(mob).insert((Stat::Dead, DeathComa));
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.stasis = Stasis::Waiting {
            ready_at: now + FAKEDEATH_SECS,
            died,
        };
    }
    chat_to(
        world,
        mob,
        Span::Changeling,
        "Мы начинаем стазис, подготавливая энергию, чтобы восстать вновь.",
    );
    true
}

/// `fakedeath/revive`: `revive(HEAL_DAMAGE | HEAL_BODY | HEAL_STATUS | HEAL_CC_STATUS)`.
fn revive(world: &mut World, mob: Entity) {
    if world.get::<DeathComa>(mob).is_none() {
        return;
    }
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.stasis = Stasis::None;
    }
    world.entity_mut(mob).remove::<DeathComa>();
    heal_body(world, mob);
    if let Some(health) = world.get::<Health>(mob).cloned() {
        world.entity_mut(mob).insert(health.healed());
    }
    if let Some(mut effects) = effects_mut(world, mob) {
        for kind in [
            Incap::Stun,
            Incap::Knockdown,
            Incap::Paralyze,
            Incap::Immobilize,
        ] {
            effects.set(kind, 0.0);
        }
        effects.stamina = 0.0;
        effects.stamcrit = false;
    }
    world.entity_mut(mob).insert(Stat::Conscious);
    if let Some(mut alerts) = world.get_mut::<crate::breath::Alerts>(mob) {
        alerts.0.clear();
    }
    chat_to(world, mob, Span::Changeling, "Мы возродили себя.");
    info!("{mob} revived from stasis (changeling)");
}

/// `fully_heal(HEAL_BODY)`: organs, limbs, the blood and the wounds.
fn heal_body(world: &mut World, mob: Entity) {
    crate::medical::organs::regenerate(world, mob);
    crate::medical::bodyparts::regenerate_limbs(world, mob);
    if let Some(mut blood) = world.get_mut::<Blood>(mob)
        && blood.volume < BLOOD_VOLUME_NORMAL
    {
        blood.volume = BLOOD_VOLUME_NORMAL;
    }
    if let Some(mut limbs) = world.get_mut::<crate::medical::wounds::Limbs>(mob)
        && limbs.parts.iter().any(|l| !l.wounds.is_empty())
    {
        for limb in &mut limbs.parts {
            limb.wounds.clear();
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Bought and other powers

/// `regenerate/sting_action`.
fn regenerate(world: &mut World, mob: Entity) -> bool {
    chat_to(
        world,
        mob,
        Span::Notice,
        "Вы чувствуете зуд, как внутри, так и снаружи, когда ваши ткани плоти вяжутся и \
         перевязываются.",
    );
    heal_body(world, mob);
    true
}

/// `adrenaline/sting_action`: up at once, stamina reset, four units of changeling adrenaline.
fn adrenaline(world: &mut World, mob: Entity) -> bool {
    if let Some(mut effects) = effects_mut(world, mob) {
        for kind in [
            Incap::Stun,
            Incap::Knockdown,
            Incap::Paralyze,
            Incap::Immobilize,
        ] {
            effects.set(kind, 0.0);
        }
        effects.stamina = 0.0;
        effects.stamcrit = false;
    }
    if let (Some(chem), Some(mut reagents)) = (
        world
            .get_resource::<crate::chem::Chem>()
            .map(|c| c.0.clone()),
        world.get::<crate::chem::MobReagents>(mob).cloned(),
    ) && let Some(id) = chem.id(ADRENALINE)
    {
        reagents
            .blood
            .add_reagent(&chem, id, 4.0, 310.15, None, None, false, false);
        world.entity_mut(mob).insert(reagents);
    }
    chat_to(
        world,
        mob,
        Span::Changeling,
        "The staggering rush of a stimulant honed precisely to our biology is INVIGORATING. We \
         will not be subdued.",
    );
    true
}

/// `fleshmend/sting_action`: 10 s of healing (`/datum/status_effect/fleshmend`).
fn fleshmend(world: &mut World, client: Entity, mob: Entity) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if world
        .get::<Changeling>(mob)
        .is_some_and(|c| c.fleshmend_until > now)
    {
        tell(world, client, Span::Warning, "уже лечимся!");
        return false;
    }
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.fleshmend_until = now + FLESHMEND_SECS;
    }
    chat_to(
        world,
        mob,
        Span::Notice,
        "Мы начинаем быстро выздоравливать.",
    );
    true
}

/// Spawns `path` into the mob's possession, stuck to it.
fn grow(world: &mut World, mob: Entity, path: &str) -> Option<Entity> {
    let pos = *world.get::<TilePos>(mob)?;
    let item = crate::construction::spawn_obj(world, path, pos)?;
    world
        .entity_mut(item)
        .remove::<TilePos>()
        .insert((CarriedBy(mob), NoDrop));
    Some(item)
}

fn held_of(world: &World, mob: Entity, path: &str) -> Vec<Entity> {
    world
        .get::<Inventory>(mob)
        .map(|i| {
            i.hands
                .iter()
                .flatten()
                .copied()
                .filter(|&e| crate::construction::path_of(world, e).as_deref() == Some(path))
                .collect()
        })
        .unwrap_or_default()
}

/// `weapon/unequip_held`: the blade goes back into the arm. True when one was held.
fn retract_blade(world: &mut World, mob: Entity) -> bool {
    let blades = held_of(world, mob, ARM_BLADE);
    if blades.is_empty() {
        return false;
    }
    for blade in blades {
        world.despawn(blade);
    }
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        "Мы ассимилируем blade обратно в наше тело.",
        &format!("С отвратительным хрустом, {user} превращает blade в руку!"),
        Span::Warning,
    );
    true
}

/// `weapon/sting_action`: the active hand drops what it holds and becomes the blade.
fn form_blade(world: &mut World, client: Entity, mob: Entity) -> bool {
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return false;
    };
    if let Some(held) = inventory.active_item() {
        if no_drop(world, held) {
            tell(world, client, Span::Warning, "рука занята!");
            return false;
        }
        world.entity_mut(held).remove::<CarriedBy>().insert(pos);
    }
    let Some(blade) = grow(world, mob, ARM_BLADE) else {
        return false;
    };
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        let hand = inventory.active;
        inventory.hands[hand] = Some(blade);
    }
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        "Наша рука скручивается и мутирует, превращаясь в смертоносный клинок.",
        &format!("Страшный клинок формируется вокруг руки {user}!"),
        Span::Warning,
    );
    true
}

/// `suit/check_suit`: the chitin is cast off. True when it was worn.
fn cast_off_armor(world: &mut World, mob: Entity) -> bool {
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return false;
    };
    let is = |slot: Option<Entity>, path: &str| {
        slot.filter(|&e| crate::construction::path_of(world, e).as_deref() == Some(path))
    };
    let (suit, helmet) = (
        is(inventory.suit, LING_ARMOR),
        is(inventory.head, LING_HELMET),
    );
    if suit.is_none() && helmet.is_none() {
        return false;
    }
    for item in [suit, helmet].into_iter().flatten() {
        world.despawn(item);
    }
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.chem_recharge_slowdown -= ARMOR_SLOWDOWN;
    }
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        "Мы сбрасываем нашу armor.",
        &format!("{user} сбрасывает свой armor!"),
        Span::Warning,
    );
    true
}

/// `suit/sting_action`: whatever is worn there falls, the chitin grows in its place.
fn form_armor(world: &mut World, client: Entity, mob: Entity) -> bool {
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return false;
    };
    if inventory.suit.is_some_and(|e| no_drop(world, e)) {
        tell(world, client, Span::Warning, "слот тела занят!");
        return false;
    }
    if inventory.head.is_some_and(|e| no_drop(world, e)) {
        tell(world, client, Span::Warning, "слот головы занят!");
        return false;
    }
    for (slot, worn) in [
        (InventorySlot::Suit, inventory.suit),
        (InventorySlot::Head, inventory.head),
    ] {
        if let Some(worn) = worn {
            unequip(world, mob, pos, slot);
            world.entity_mut(worn).remove::<CarriedBy>().insert(pos);
        }
    }
    let (Some(suit), Some(helmet)) = (grow(world, mob, LING_ARMOR), grow(world, mob, LING_HELMET))
    else {
        return false;
    };
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.suit = Some(suit);
        inventory.head = Some(helmet);
    }
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.chem_recharge_slowdown += ARMOR_SLOWDOWN;
    }
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        "Мы закаляем свою плоть, создавая броню!",
        &format!("Плоть {user} чернеет, быстро превращаясь в твердую, хитиновую массу!"),
        Span::Warning,
    );
    true
}

/// The bought powers' `Remove()`: the blade and the chitin go.
fn remove_powers(world: &mut World, mob: Entity) {
    retract_blade(world, mob);
    cast_off_armor(world, mob);
}

// ---------------------------------------------------------------------------------------------
// The emporium

impl ServerUi for CellularEmporiumUi {
    type Source = (&'static Changeling, &'static EmporiumCatalog);
    type UserParam = ();
    type Act = EmporiumAct;

    fn static_data((_, catalog): (&Changeling, &EmporiumCatalog)) -> EmporiumStatic {
        EmporiumStatic {
            abilities: catalog.0.clone(),
        }
    }

    fn data((ling, _): (&Changeling, &EmporiumCatalog), _: Entity, _: &()) -> EmporiumData {
        EmporiumData {
            can_readapt: ling.can_respec,
            genetic_points_count: ling.genetic_points,
            owned_abilities: ling
                .purchased
                .iter()
                .map(|p| p.def().path.to_owned())
                .collect(),
            absorb_count: ling.true_absorbs,
            dna_count: ling.absorbed_count,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<EmporiumAct, UiReject> {
        match action {
            READAPT => Ok(EmporiumAct::Readapt),
            EVOLVE => params
                .get("path")
                .and_then(Power::from_path)
                .map(EmporiumAct::Evolve)
                .ok_or_else(|| UiReject::BadParams("path: not a changeling power".into())),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmporiumAct {
    Evolve(Power),
    Readapt,
}

/// `ui_act` of the emporium: `purchase_power` and `readapt`, checked at the moment of effect.
fn emporium_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<CellularEmporiumUi>>>) {
    let pending: Vec<(Entity, Entity, EmporiumAct)> = acts
        .read(world.resource::<Messages<UiActed<CellularEmporiumUi>>>())
        .map(|a| (a.actor, a.source, a.act))
        .collect();
    for (actor, source, act) in pending {
        if actor != source || world.get::<Changeling>(source).is_none_or(|c| c.fallen) {
            continue;
        }
        match act {
            EmporiumAct::Evolve(power) => purchase(world, source, power),
            EmporiumAct::Readapt => readapt(world, source),
        }
    }
}

/// `purchase_power` + `give_power`.
pub fn purchase(world: &mut World, mob: Entity, power: Power) {
    let Some(result) = world
        .get::<Changeling>(mob)
        .map(|c| c.check_purchase(power))
    else {
        return;
    };
    match result {
        Ok(cost) => {
            if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
                ling.genetic_points -= cost;
                ling.purchased.push(power);
            }
            info!("{mob} adapted {power:?} (changeling)");
            refresh(world, mob);
        }
        Err(text) => chat_to(world, mob, Span::Warning, text),
    }
}

/// `readapt()`: every bought power goes, the points come back, one respec spent.
pub fn readapt(world: &mut World, mob: Entity) {
    let Some(ling) = world.get::<Changeling>(mob).cloned() else {
        return;
    };
    if world.get::<HumanLook>(mob).is_none() {
        chat_to(
            world,
            mob,
            Span::Warning,
            "Мы не можем избавиться от нашего развития в такой форме!",
        );
        return;
    }
    if ling.in_stasis() {
        chat_to(
            world,
            mob,
            Span::Warning,
            "Мы слишком заняты реформированием себя, чтобы переадаптироваться прямо сейчас!",
        );
        return;
    }
    if ling.can_respec == 0 {
        chat_to(
            world,
            mob,
            Span::Warning,
            "Вам не хватает силы, чтобы переадаптировать ваше развитие!",
        );
        return;
    }
    chat_to(
        world,
        mob,
        Span::Notice,
        "Мы избавились от нашего развития в этой форме и теперь готовы к повторной адаптации.",
    );
    remove_powers(world, mob);
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.remove_purchased();
        ling.can_respec -= 1;
    }
    info!("{mob} readapted (changeling)");
    refresh(world, mob);
}

// ---------------------------------------------------------------------------------------------
// Life

/// tg `COMSIG_LIVING_LIFE` -> `on_life` each `SSmobs` fire: the chemicals.
// Cost: per changeling per 2 s, a few component reads.
fn changeling_life(world: &mut World, mut last_fired: Local<Option<u64>>) {
    let fired = world.resource::<crate::breath::MobsClock>().times_fired;
    let fires = fired.saturating_sub(last_fired.unwrap_or(fired));
    *last_fired = Some(fired);
    if fires == 0 {
        return;
    }
    let secs = crate::breath::SSMOBS_WAIT_SECS as f32 * fires as f32;
    let mobs: Vec<Entity> = world
        .query_filtered::<Entity, With<Changeling>>()
        .iter(world)
        .collect();
    for mob in mobs {
        let stasis = world
            .get::<Changeling>(mob)
            .map(|c| c.stasis)
            .unwrap_or_default();
        let dead = world.get::<Stat>(mob) == Some(&Stat::Dead)
            && (world.get::<DeathComa>(mob).is_none() || stasis.died());
        let on_fire = crate::fire::on_fire(world, mob);
        if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
            ling.regenerate_chemicals(secs, dead, on_fire);
        }
        refresh(world, mob);
    }
}

/// Per tick: interrupted absorptions, the stasis timer and early revivals, fleshmend's healing.
// Cost: per tick, one pass over the changelings.
fn tick_changelings(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let delta = world.resource::<Time>().delta_secs();
    let mobs: Vec<Entity> = world
        .query_filtered::<Entity, With<Changeling>>()
        .iter(world)
        .collect();
    for mob in mobs {
        let Some(ling) = world.get::<Changeling>(mob).cloned() else {
            continue;
        };
        // `do_after` failed (moved, incapacitated, target moved): "прервано!".
        let absorbing = world
            .get::<DoAfter>(mob)
            .is_some_and(|d| d.job == Job::Changeling(ChangelingJob::Absorb));
        if ling.absorbing.is_some() && !absorbing {
            absorb_interrupted(world, mob);
        }
        match ling.stasis {
            // `on_stat_change`: revived from death by other means (a defibrillator, an admin).
            Stasis::Waiting { .. } | Stasis::Ready { .. }
                if world.get::<Stat>(mob) != Some(&Stat::Dead) =>
            {
                world.entity_mut(mob).remove::<DeathComa>();
                if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
                    ling.stasis = Stasis::None;
                }
                chat_to(
                    world,
                    mob,
                    Span::Changeling,
                    "Мы выходим из стазиса раньше.",
                );
                refresh(world, mob);
            }
            // `ready_to_regenerate`.
            Stasis::Waiting { ready_at, died } if now >= ready_at => {
                if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
                    ling.stasis = Stasis::Ready { died };
                }
                chat_to(world, mob, Span::Changeling, "Мы готовы возродиться.");
                refresh(world, mob);
            }
            _ => {}
        }
        // `/datum/status_effect/fleshmend/tick`, frame-independent; paused while burning.
        if ling.fleshmend_until > now
            && world.get::<Stat>(mob) != Some(&Stat::Dead)
            && !crate::fire::on_fire(world, mob)
        {
            for (kind, rate) in FLESHMEND_HEAL {
                hurt(world, mob, kind, -rate * delta);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Hivemind

/// `/datum/saymode/changeling/handle_message`: every hivemind member and ghost reads
/// `<id>: message`.
fn hivemind_chat(
    mut spoken: MessageReader<Spoke>,
    members: Query<Entity, With<ChangelingHivemind>>,
    lings: Query<&Changeling>,
    ghosts: Query<Entity, With<Ghost>>,
    listeners: Res<Listeners>,
    mut chat: MessageWriter<SendChat>,
) {
    for spoke in spoken.read() {
        if spoke.radio != Some(RadioTarget::Changeling) {
            continue;
        }
        let id = lings
            .get(spoke.speaker)
            .map_or_else(|_| spoke.name.clone(), |c| c.id_name.clone());
        let clients: Vec<Entity> = members
            .iter()
            .chain(ghosts.iter())
            .filter_map(|e| listeners.client_of(e))
            .collect();
        info!(target: "say", "hivemind {id} ({}): {}", spoke.name, spoke.text);
        chat.write(SendChat {
            clients,
            message: hivemind_message(&id, &spoke.text),
        });
    }
}

/// `span_changeling("<b>[id]:</b> [message]")`.
pub fn hivemind_message(id: &str, text: &str) -> ChatMessage {
    ChatMessage {
        channel: ChatChannel::Radio,
        segments: vec![
            ChatSegment::new(Span::Changeling, format!("{id}: ")),
            ChatSegment::new(Span::Changeling, text),
        ],
    }
}

// ---------------------------------------------------------------------------------------------
// Windows

impl ServerUi for AntagInfoChangelingUi {
    type Source = &'static Changeling;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &Changeling) {}

    fn data(ling: &Changeling, _: Entity, _: &()) -> ChangelingInfo {
        ChangelingInfo {
            true_name: ling.id_name.clone(),
            hive_name: ling.hive_name.clone(),
            stolen_antag_info: ling.memory.clone(),
            memories: Vec::new(),
            objectives: ling
                .objectives
                .iter()
                .enumerate()
                .map(|(i, o)| ObjectiveInfo {
                    count: i as u32 + 1,
                    explanation: o.explanation(),
                    complete: false,
                })
                .collect(),
            can_change_objective: false,
            absorbed_dna: ling.absorbed_count,
        }
    }

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

impl ServerUi for ChangelingHudUi {
    type Source = &'static ChangelingHudData;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &ChangelingHudData) {}

    fn data(source: &ChangelingHudData, _: Entity, _: &()) -> ChangelingHudData {
        *source
    }

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

/// `/datum/action/antag_info/Trigger` of a changeling.
fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if world.get::<Changeling>(source).is_some() {
            world.write_message(OpenUi::new::<AntagInfoChangelingUi>(
                client,
                actor,
                source,
                CHANGELING_NAME,
            ));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Round end

/// tg `antag_report` for changelings: the header, then each one's `roundend_report`.
pub fn roundend_report(world: &mut World, end: RoundEnd) -> Vec<ChatMessage> {
    let mut query = world.query::<(Entity, &Changeling)>();
    let mut lings: Vec<(Entity, Changeling)> =
        query.iter(world).map(|(e, c)| (e, c.clone())).collect();
    if lings.is_empty() {
        return Vec::new();
    }
    lings.sort_by_key(|(e, _)| *e);
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = vec![line(vec![ChatSegment::new(
        Span::Bold,
        format!("{ROUNDEND_CATEGORY} были:"),
    )])];
    for (mob, ling) in lings {
        out.push(crate::antag::traitor::print_player(world, mob, &ling.key));
        out.push(line(vec![
            ChatSegment::new(Span::Bold, "Извлеченных генов: "),
            ChatSegment::new(Span::Plain, ling.absorbed_count.to_string()),
        ]));
        let mut won = world.get_entity(mob).is_ok();
        for (i, objective) in ling.objectives.iter().enumerate() {
            let done = objective.check_completion(world, mob, &ling, end);
            won &= done;
            out.push(line(vec![
                ChatSegment::new(Span::Bold, format!("{OBJECTIVE_NAME} #{}", i + 1)),
                ChatSegment::new(Span::Plain, format!(": {} ", objective.explanation())),
                if done {
                    ChatSegment::new(Span::GreenText, "Успех!")
                } else {
                    ChatSegment::new(Span::RedText, "Неудача.")
                },
            ]));
        }
        out.push(line(vec![if won {
            ChatSegment::new(Span::GreenText, "Генокрад был успешен!")
        } else {
            ChatSegment::new(Span::RedText, "Генокрад провалился.")
        }]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> UseContext {
        UseContext {
            human: true,
            ..default()
        }
    }

    #[test]
    fn chemicals_regenerate_per_life_tick() {
        let mut c = Changeling::default();
        assert_eq!(c.chem_charges, 20.0);
        // One `Life()` (2 s) at 1 per second.
        c.regenerate_chemicals(2.0, false, false);
        assert_eq!(c.chem_charges, 22.0);
        // Capped at the storage.
        c.regenerate_chemicals(100.0, false, false);
        assert_eq!(c.chem_charges, 75.0);
        // Dead: only up to half the storage, and a full storage is cut down to it.
        c.chem_charges = 30.0;
        c.regenerate_chemicals(20.0, true, false);
        assert_eq!(c.chem_charges, 37.5);
        // Burning: a quarter speed, slowdowns do not apply.
        c.chem_charges = 0.0;
        c.chem_recharge_slowdown = ARMOR_SLOWDOWN;
        c.regenerate_chemicals(2.0, false, true);
        assert_eq!(c.chem_charges, 0.5);
        // The chitin's slowdown.
        c.regenerate_chemicals(2.0, false, false);
        assert_eq!(c.chem_charges, 0.5 + 2.0 * (1.0 - ARMOR_SLOWDOWN));
        c.adjust_chemicals(-1000.0, None);
        assert_eq!(c.chem_charges, 0.0);
    }

    #[test]
    fn purchase_and_readapt_rules() {
        let mut c = Changeling::default();
        assert_eq!(c.check_purchase(Power::ArmBlade), Ok(2));
        assert!(
            c.check_purchase(Power::AbsorbDna).is_err(),
            "innate powers are not sold"
        );
        c.genetic_points -= 2;
        c.purchased.push(Power::ArmBlade);
        assert_eq!(
            c.check_purchase(Power::ArmBlade),
            Err("Мы уже развили эту способность!")
        );
        for power in [Power::Adrenaline, Power::Fleshmend, Power::Armor] {
            let cost = c.check_purchase(power).unwrap();
            c.genetic_points -= cost;
            c.purchased.push(power);
        }
        // 10 - 2 - 2 - 2 - 1.
        assert_eq!(c.genetic_points, 3);
        assert_eq!(c.spent_points(), 10);
        c.genetic_points = 1;
        c.purchased.retain(|p| *p != Power::Fleshmend);
        assert_eq!(
            c.check_purchase(Power::Fleshmend),
            Err("Мы достигли предела своих возможностей!")
        );
        // Stasis blocks purchases.
        c.genetic_points = 10;
        c.stasis = Stasis::Waiting {
            ready_at: 1.0,
            died: false,
        };
        assert!(c.check_purchase(Power::Fleshmend).is_err());
        c.stasis = Stasis::None;
        // Readapt refunds everything and resets the rates.
        c.chem_recharge_slowdown = ARMOR_SLOWDOWN;
        c.chem_charges = 100.0;
        let gone = c.remove_purchased();
        assert_eq!(gone.len(), 3);
        assert!(c.purchased.is_empty());
        assert_eq!(c.genetic_points, GENETIC_POINTS);
        assert_eq!((c.chem_recharge_slowdown, c.chem_charges), (0.0, 75.0));
        // The emporium lists the four powers by name with their costs.
        let names: Vec<(&str, Option<u32>)> = Power::ALL
            .into_iter()
            .filter(|p| !p.innate())
            .map(|p| (p.def().name, p.def().dna_cost))
            .collect();
        assert_eq!(
            names,
            [
                ("Gene Stim", Some(2)),
                ("Fleshmend", Some(2)),
                ("Arm Blade", Some(2)),
                ("Chitinous Armor", Some(1)),
            ]
        );
        assert_eq!(
            Power::from_path("/datum/action/changeling/weapon/arm_blade"),
            Some(Power::ArmBlade)
        );
        assert_eq!(Power::from_path("/datum/action/changeling/headcrab"), None);
    }

    #[test]
    fn use_checks_follow_can_sting() {
        let mut c = Changeling {
            absorbed_count: 1,
            ..default()
        };
        assert_eq!(c.check_use(Power::Transform, ctx()), Ok(()));
        // Fake death needs 15; 20 is there.
        assert_eq!(c.check_use(Power::Fakedeath, ctx()), Ok(()));
        c.chem_charges = 10.0;
        assert_eq!(
            c.check_use(Power::Fakedeath, ctx()),
            Err("требуется 15 химикатов!".to_owned())
        );
        // A bought power must be owned.
        c.chem_charges = 75.0;
        assert_eq!(c.check_use(Power::ArmBlade, ctx()), Err(String::new()));
        c.purchased.push(Power::ArmBlade);
        assert_eq!(c.check_use(Power::ArmBlade, ctx()), Ok(()));
        // `STABLE` powers need a conscious changeling; fleshmend works in hard crit, stasis
        // in death.
        let crit = UseContext {
            stat: Stat::HardCrit,
            knocked_out: true,
            ..ctx()
        };
        assert_eq!(
            c.check_use(Power::ArmBlade, crit),
            Err("недееспособны!".to_owned())
        );
        c.purchased.push(Power::Fleshmend);
        assert_eq!(c.check_use(Power::Fleshmend, crit), Ok(()));
        let dead = UseContext {
            stat: Stat::Dead,
            ..crit
        };
        assert_eq!(c.check_use(Power::Fakedeath, dead), Ok(()));
        // In stasis only the stasis button works.
        c.stasis = Stasis::Ready { died: false };
        assert_eq!(c.check_use(Power::Fakedeath, dead), Ok(()));
        assert_eq!(
            c.check_use(Power::Fleshmend, dead),
            Err("притворяемся мёртвым!".to_owned())
        );
        // Burning blocks most powers.
        c.stasis = Stasis::None;
        let burning = UseContext {
            on_fire: true,
            ..ctx()
        };
        assert_eq!(
            c.check_use(Power::ArmBlade, burning),
            Err("on fire!".to_owned())
        );
        // A fallen changeling has nothing.
        c.fallen = true;
        assert!(c.check_use(Power::Transform, ctx()).is_err());
    }

    #[test]
    fn absorb_takes_three_fifteen_second_stages() {
        let mut stage = 0;
        let mut stages = Vec::new();
        while let AbsorbStep::Stage(n) = absorb_next(stage) {
            stages.push(n);
            stage = n;
        }
        assert_eq!(stages, [1, 2, 3]);
        let total: f32 = stages.iter().map(|_| ABSORB_STAGE_DS / 10.0).sum();
        assert_eq!(total, 45.0);
    }

    #[test]
    fn profiles_dedupe_and_push_out_the_oldest() {
        let mut c = Changeling::default();
        let profile = |i: usize| Profile {
            name: format!("crew {i}"),
            look: Some(HumanLook::random_assistant(i as u64)),
            protected: false,
        };
        for i in 0..=DNA_MAX {
            c.add_profile(profile(i));
        }
        assert_eq!(c.profiles.len(), DNA_MAX + 1);
        assert!(c.has_profile("crew 3", profile(3).look.as_ref()));
        // The same name with another body is other DNA.
        assert!(!c.has_profile("crew 3", profile(4).look.as_ref()));
        // Over the limit the oldest goes; the count keeps every gain.
        c.add_profile(profile(DNA_MAX + 1));
        assert_eq!(c.profiles.len(), DNA_MAX + 1);
        assert_eq!(c.profiles[0].name, "crew 1");
        assert_eq!(c.absorbed_count, DNA_MAX as u32 + 2);
    }
}

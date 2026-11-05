//! Damage, stat and the body's posture (server), after tg `/mob/living/carbon` (`carbon.dm`
//! `updatehealth`/`update_stat`, `living.dm` `set_stat`).
//!
//! - [`Health`]: brute/burn per bodypart (tg `/obj/item/bodypart`, capped at `max_damage`),
//!   tox/oxy; health = `maxHealth - oxy - tox - burn - brute`, rounded to `DAMAGE_PRECISION`.
//! - [`ZoneSelected`]: tg `zone_selected`, set by the HUD's zone selector (`crate::medical`).
//! - [`Stat`]: conscious, soft crit (health <= 0), hard crit (<= -30), dead (<= -100; permanent).
//!   It replaces "living = has a `Mover`": moves, clicks and hands check it.
//! - Soft crit floors the mob (it crawls at tg speed, hands blocked); hard crit and death knock it
//!   out (no moving at all). A floored mob lies: its sprite turns 90 or 270 degrees, drops to
//!   `LYING_MOB_LAYER` and 6 px down, and it stops being dense (tg `TRAIT_UNDENSE`).
//! - Breathing, pressure and body temperature damage come from `crate::breath` (tg `Life()`).
//!
//! Damage is applied through [`apply_damage`], which updates the stat at once, so an attack in the
//! same tick sees the new stat. The rest ([`BodyPosition`], speed, HUD, death message) follows
//! from `Changed<Health>`/`Changed<Stat>` in one system (plus the pull slowdown of
//! `crate::pull`).

use bevy::prelude::*;
use ss13_chat::server::Senses;
use ss13_mapload::DmAtom;
use ss13_net::chat::server::SendChat;
use ss13_net::server::NetServerSystems;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::{Appearance, Dir, Facing, Layer, TilePos};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiParams, UiReject, UiSystems};
use ss13_world::{Density, Mover, SpatialIndex};

use crate::RUN_DELAY_SECS;
use crate::combat::{CombatMode, CombatRng, NextMove};
use crate::hud::{HealthHudData, HealthHudUi, HealthReadout};
use crate::pull::PullSlowdown;
use crate::server::PlayerMob;
use crate::status_effects::{Incapacitation, STAGGERED_SLOWDOWN};
use crate::zone::{BodyZone, TargetZone};

/// `maxHealth` of `/mob/living`.
pub const MAX_HEALTH: f32 = 100.0;
/// `HEALTH_THRESHOLD_CRIT`: soft crit at or below.
pub const HEALTH_THRESHOLD_CRIT: f32 = 0.0;
/// `HEALTH_THRESHOLD_FULLCRIT`: hard crit at or below.
pub const HEALTH_THRESHOLD_FULLCRIT: f32 = -30.0;
/// `HEALTH_THRESHOLD_DEAD`.
pub const HEALTH_THRESHOLD_DEAD: f32 = -100.0;
/// `DAMAGE_PRECISION`: health is rounded to this.
pub const DAMAGE_PRECISION: f32 = 0.1;
/// `OXYLOSS_PASSOUT_THRESHOLD`: knocked out at or above, whatever the stat.
pub const OXYLOSS_PASSOUT_THRESHOLD: f32 = 50.0;
/// `SOFTCRIT_ADD_SLOWDOWN`, `CRAWLING_ADD_SLOWDOWN`, deciseconds per tile.
pub const SOFTCRIT_ADD_SLOWDOWN: f32 = 2.0;
pub const CRAWLING_ADD_SLOWDOWN: f32 = 4.0;
/// `LYING_MOB_LAYER`, `PIXEL_Y_OFFSET_LYING`.
pub const LYING_MOB_LAYER: f32 = 3.8;
pub const PIXEL_Y_OFFSET_LYING: i16 = -6;
/// `DEFAULT_MESSAGE_RANGE` and `COMBAT_MESSAGE_RANGE`, tiles.
pub const DEFAULT_MESSAGE_RANGE: u32 = 7;
pub const COMBAT_MESSAGE_RANGE: u32 = 3;

/// tg `stat` of a living mob (`STABLE`/`CONSCIOUS`, `SOFT_CRIT`, `HARD_CRIT`, `DEAD`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stat {
    #[default]
    Conscious,
    SoftCrit,
    HardCrit,
    Dead,
}

impl Stat {
    /// The stat tg `update_stat` gives a living mob at `health`.
    pub fn for_health(health: f32) -> Self {
        if health <= HEALTH_THRESHOLD_DEAD {
            Self::Dead
        } else if health <= HEALTH_THRESHOLD_FULLCRIT {
            Self::HardCrit
        } else if health <= HEALTH_THRESHOLD_CRIT {
            Self::SoftCrit
        } else {
            Self::Conscious
        }
    }
}

/// tg damage types (`BRUTE`, `BURN`, `TOX`, `OXY`, `STAMINA`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DamageType {
    Brute,
    Burn,
    Tox,
    Oxy,
    Stamina,
}

impl DamageType {
    /// A DM `damtype` value.
    pub fn from_dm(value: &str) -> Option<Self> {
        Some(match value {
            "brute" => Self::Brute,
            "burn" => Self::Burn,
            "toxin" => Self::Tox,
            "oxygen" => Self::Oxy,
            "stamina" => Self::Stamina,
            _ => return None,
        })
    }
}

/// Brute and burn of one bodypart (tg `/obj/item/bodypart` `brute_dam`, `burn_dam`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bodypart {
    pub brute: f32,
    pub burn: f32,
}

impl Bodypart {
    /// tg `get_damage`.
    pub fn damage(self) -> f32 {
        self.brute + self.burn
    }
}

/// Damage of a carbon mob: brute and burn per bodypart (tg `/obj/item/bodypart`), toxin and
/// oxygen on the mob.
// shortcut: no prosthetics; amputated limbs are `missing` (`crate::medical::bodyparts`).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Health {
    pub max: f32,
    /// tg `get_brute_loss`/`get_fire_loss`: the bodyparts' sums, rounded to `DAMAGE_PRECISION`.
    /// Derived: changed only through [`Health::adjust`], [`Health::damage_zone`] and
    /// [`Health::heal_zone`].
    pub brute: f32,
    pub burn: f32,
    pub tox: f32,
    pub oxy: f32,
    /// Indexed by [`BodyZone::index`].
    pub parts: [Bodypart; 6],
    /// Amputated bodyparts, bits by [`BodyZone::index`]: damage aimed there hits the chest (tg
    /// `get_bodypart(zone) || get_bodypart()`).
    pub missing: u8,
    /// A basic mob (`crate::mobs`, tg `/mob/living/basic`): its `physiology` multipliers (brute,
    /// burn, toxin, oxygen, stamina). Every damage type becomes brute (tg
    /// `simple_transfer_to_brute_loss`), kept on the chest; no crit, dead at 0 health.
    pub basic: Option<[f32; 5]>,
    /// What the body's species does to damage (`crate::species`).
    pub mods: BodyMods,
}

/// The damage rules a species brings: its bodyparts' `brute_modifier`/`burn_modifier`
/// (`receive_damage`), `TRAIT_TOXINLOVER`/`TRAIT_TOXIMMUNE` (`adjust_tox_loss`) and
/// `TRAIT_NOSOFTCRIT` (`update_stat`).
// shortcut: a toxin lover's blood volume does not follow its toxin damage (tg
// `adjust_blood_volume` in `adjust_tox_loss`); per-limb modifiers are one value for the body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyMods {
    pub brute: f32,
    pub burn: f32,
    pub tox_lover: bool,
    pub tox_immune: bool,
    pub no_softcrit: bool,
}

impl Default for BodyMods {
    fn default() -> Self {
        Self {
            brute: 1.0,
            burn: 1.0,
            tox_lover: false,
            tox_immune: false,
            no_softcrit: false,
        }
    }
}

impl Default for Health {
    fn default() -> Self {
        Self {
            max: MAX_HEALTH,
            brute: 0.0,
            burn: 0.0,
            tox: 0.0,
            oxy: 0.0,
            parts: [Bodypart::default(); 6],
            missing: 0,
            basic: None,
            mods: BodyMods::default(),
        }
    }
}

/// DM `round(x, DAMAGE_PRECISION)`.
fn precise(x: f32) -> f32 {
    (x / DAMAGE_PRECISION).round() * DAMAGE_PRECISION
}

impl Health {
    /// A basic mob's damage model with its `maxHealth` and `physiology`.
    pub fn basic(max: f32, physiology: [f32; 5]) -> Self {
        Self {
            max,
            basic: Some(physiology),
            ..Self::default()
        }
    }

    /// No damage, the same body (an admin heal, a revival).
    pub fn healed(&self) -> Self {
        Self {
            max: self.max,
            basic: self.basic,
            missing: self.missing,
            mods: self.mods,
            ..Self::default()
        }
    }

    /// tg `updatehealth`.
    pub fn health(&self) -> f32 {
        let raw = self.max - self.oxy - self.tox - self.burn - self.brute;
        precise(raw)
    }

    pub fn part(&self, zone: BodyZone) -> Bodypart {
        self.parts[zone.index()]
    }

    /// The bodypart is attached (not amputated).
    pub fn has_part(&self, zone: BodyZone) -> bool {
        self.missing & (1 << zone.index()) == 0
    }

    /// Adds `amount` (negative heals) of `kind`; no damage type goes below zero. Brute and burn
    /// go over the whole body (tg `take_overall_damage`/`heal_overall_damage`).
    // shortcut: stamina damage is dropped (no stamina, stamcrit or exhaustion yet).
    pub fn adjust(&mut self, kind: DamageType, amount: f32) {
        if let Some(physiology) = self.basic {
            let i = match kind {
                DamageType::Brute => 0,
                DamageType::Burn => 1,
                DamageType::Tox => 2,
                DamageType::Oxy => 3,
                DamageType::Stamina => return,
            };
            // tg `simple_transfer_to_brute_loss`: clamp(bruteloss + amount, 0, maxHealth * 2).
            let brute = precise((self.brute + amount * physiology[i]).clamp(0.0, self.max * 2.0));
            self.parts = [Bodypart::default(); 6];
            self.parts[BodyZone::Chest.index()].brute = brute;
            (self.brute, self.burn, self.tox, self.oxy) = (brute, 0.0, 0.0, 0.0);
            return;
        }
        let value = match kind {
            DamageType::Brute | DamageType::Burn => {
                let burn = kind == DamageType::Burn;
                if amount > 0.0 {
                    self.take_overall(amount, burn);
                } else {
                    self.heal_overall(-amount, burn);
                }
                return;
            }
            DamageType::Tox => {
                // `adjust_tox_loss`: damage becomes healing for a toxin lover; immunity stops
                // damage, not healing.
                let mut amount = if self.mods.tox_lover { -amount } else { amount };
                if self.mods.tox_immune {
                    amount = amount.min(0.0);
                }
                self.tox = (self.tox + amount).max(0.0);
                return;
            }
            DamageType::Oxy => &mut self.oxy,
            DamageType::Stamina => return,
        };
        *value = (*value + amount).max(0.0);
    }

    /// tg `apply_damage` on one bodypart: `receive_damage` for a positive `amount` (capped at
    /// the part's `max_damage`), `heal_damage` for a negative one. Other kinds than brute and
    /// burn ignore the zone.
    pub fn damage_zone(&mut self, zone: BodyZone, kind: DamageType, amount: f32) {
        if self.basic.is_some() {
            return self.adjust(kind, amount);
        }
        let burn = match kind {
            DamageType::Brute => false,
            DamageType::Burn => true,
            _ => return self.adjust(kind, amount),
        };
        let zone = if self.has_part(zone) {
            zone
        } else {
            BodyZone::Chest
        };
        if amount < 0.0 {
            let (brute, burn) = if burn { (0.0, -amount) } else { (-amount, 0.0) };
            self.heal_zone(zone, brute, burn);
            return;
        }
        // `receive_damage`: the bodypart's `brute_modifier`/`burn_modifier`.
        let amount = precise(
            amount
                * if burn {
                    self.mods.burn
                } else {
                    self.mods.brute
                },
        );
        let part = &mut self.parts[zone.index()];
        let can_inflict = zone.max_damage() - part.damage();
        if can_inflict <= 0.0 || amount <= 0.0 {
            return;
        }
        let dealt = if amount > can_inflict {
            precise(amount * (can_inflict / amount))
        } else {
            amount
        };
        if burn {
            part.burn += dealt;
        } else {
            part.brute += dealt;
        }
        self.sync();
    }

    /// tg `/obj/item/bodypart/heal_damage`; returns the damage healed.
    pub fn heal_zone(&mut self, zone: BodyZone, brute: f32, burn: f32) -> f32 {
        if self.basic.is_some() {
            let before = self.brute;
            self.adjust(DamageType::Brute, -(brute + burn));
            return before - self.brute;
        }
        let part = &mut self.parts[zone.index()];
        let before = part.damage();
        if brute > 0.0 {
            part.brute = precise((part.brute - brute).max(0.0));
        }
        if burn > 0.0 {
            part.burn = precise((part.burn - burn).max(0.0));
        }
        let healed = before - part.damage();
        self.sync();
        healed
    }

    /// tg `take_overall_damage`: an even share for every part that can still take damage.
    // shortcut: tg picks the parts in random order, which decides only where a capped part's
    // excess and the rounding remainder go; here they go in `BodyZone::ALL` order.
    fn take_overall(&mut self, amount: f32, burn: bool) {
        let mut left = amount;
        let mut parts: Vec<BodyZone> = BodyZone::ALL
            .into_iter()
            .filter(|z| self.has_part(*z) && self.part(*z).damage() < z.max_damage())
            .collect();
        while let Some(zone) = parts.first().copied()
            && left > 0.0
        {
            let share = precise(left / parts.len() as f32);
            let before = self.part(zone).damage();
            let kind = if burn {
                DamageType::Burn
            } else {
                DamageType::Brute
            };
            self.damage_zone(zone, kind, share);
            left = precise(left - (self.part(zone).damage() - before));
            parts.remove(0);
        }
    }

    /// tg `heal_overall_damage`: heals the damaged parts one after another until `amount` is
    /// used up.
    // shortcut: tg picks the next part at random; here in `BodyZone::ALL` order.
    fn heal_overall(&mut self, amount: f32, burn: bool) {
        let mut left = amount;
        for zone in BodyZone::ALL {
            if left <= 0.0 {
                break;
            }
            left = precise(if burn {
                left - self.heal_zone(zone, 0.0, left)
            } else {
                left - self.heal_zone(zone, left, 0.0)
            });
        }
    }

    fn sync(&mut self) {
        self.brute = precise(self.parts.iter().map(|p| p.brute).sum());
        self.burn = precise(self.parts.iter().map(|p| p.burn).sum());
    }

    /// tg `IS_UNCONSCIOUS` from damage: hard crit, death, or `check_passout` oxyloss.
    pub fn knocked_out(&self, stat: Stat) -> bool {
        if self.basic.is_some() {
            return stat == Stat::Dead;
        }
        stat >= Stat::HardCrit || self.oxy >= OXYLOSS_PASSOUT_THRESHOLD
    }

    /// tg `TRAIT_FLOORED` from the stat: lying down.
    pub fn floored(&self, stat: Stat) -> bool {
        // A basic mob does not lie down: death changes its sprite (`crate::mobs`).
        if self.basic.is_some() {
            return false;
        }
        stat >= Stat::SoftCrit || self.knocked_out(stat)
    }

    /// Whether the mob may take a step (tg `MOBILITY_MOVE`, which `knocked_out` takes away).
    pub fn can_move(&self, stat: Stat) -> bool {
        !self.knocked_out(stat)
    }

    /// Seconds per tile: tg `RUN_DELAY` plus `damage_slowdown` (from 40 missing health, missing
    /// / 75), `carbon_softcrit` and `carbon_crawling` (lying), all in deciseconds.
    pub fn move_delay(&self, stat: Stat) -> f32 {
        let deficiency = self.max - self.health();
        let mut slowdown = 0.0;
        if deficiency >= 40.0 {
            slowdown += deficiency / 75.0;
        }
        if stat == Stat::SoftCrit {
            slowdown += SOFTCRIT_ADD_SLOWDOWN;
        }
        if self.floored(stat) {
            slowdown += CRAWLING_ADD_SLOWDOWN;
        }
        RUN_DELAY_SECS + slowdown / 10.0
    }
}

/// `amount` of `kind` to a living mob, then tg `update_stat` (death is permanent). Returns the old
/// stat when it changed.
pub fn apply_damage(
    health: &mut Health,
    stat: &mut Stat,
    kind: DamageType,
    amount: f32,
) -> Option<Stat> {
    health.adjust(kind, amount);
    update_stat(health, stat)
}

/// `amount` of `kind` to one bodypart of a living mob (tg `apply_damage` with a `def_zone`),
/// then tg `update_stat`. Returns the old stat when it changed.
pub fn apply_damage_zone(
    health: &mut Health,
    stat: &mut Stat,
    zone: BodyZone,
    kind: DamageType,
    amount: f32,
) -> Option<Stat> {
    health.damage_zone(zone, kind, amount);
    update_stat(health, stat)
}

/// tg `/mob/living/carbon/update_stat`; a basic mob's (`/mob/living/basic/update_stat`) dies at
/// 0 health and has no crit.
pub fn update_stat(health: &Health, stat: &mut Stat) -> Option<Stat> {
    if *stat == Stat::Dead {
        return None;
    }
    let new = match health.basic {
        Some(_) if health.health() <= 0.0 => Stat::Dead,
        Some(_) => Stat::Conscious,
        // `TRAIT_NOSOFTCRIT`: conscious until hard crit.
        None => match Stat::for_health(health.health()) {
            Stat::SoftCrit if health.mods.no_softcrit => Stat::Conscious,
            stat => stat,
        },
    };
    (new != *stat).then(|| std::mem::replace(stat, new))
}

/// tg `TRAIT_DEATHCOMA` with `TRAIT_FAKEDEATH` (a changeling's regenerative stasis): the mob
/// keeps [`Stat::Dead`], so it looks and examines dead and cannot act, but it is alive: its
/// steps do not ghostize it and objectives count it alive (tg gives it `UNCONSCIOUS`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeathComa;

/// The sum of a mob's ability movespeed modifiers (tg `multiplicative_slowdown`), deciseconds
/// per tile; negative is faster. Kept at 0 instead of removed, so the speed follows its changes.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct MoveSpeedModifier(pub f32);

/// tg `body_position` + `lying_angle`: 0 standing, 90 or 270 lying.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BodyPosition {
    pub lying_angle: u16,
}

impl BodyPosition {
    pub fn lying(self) -> bool {
        self.lying_angle != 0
    }
}

/// tg `zone_selected`: where the mob's attacks and treatments aim.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ZoneSelected(pub TargetZone);

impl ServerUi for HealthHudUi {
    type Source = &'static HealthHudData;
    type UserParam = ();
    /// `zone` (`zone`: a DM zone value): the zone selector (`/atom/movable/screen/zone_sel`).
    type Act = TargetZone;

    fn static_data(_: &HealthHudData) {}

    fn data(source: &HealthHudData, _: Entity, _: &()) -> HealthHudData {
        source.clone()
    }

    /// Combat mode is `ClientIntent::ToggleCombatMode`; the only act is the zone selector.
    fn parse_act(action: &str, params: &UiParams) -> Result<TargetZone, UiReject> {
        if action != "zone" {
            return Err(UiReject::UnknownAction);
        }
        params
            .get("zone")
            .and_then(TargetZone::from_dm)
            .ok_or_else(|| UiReject::BadParams("zone: expected a body zone".into()))
    }
}

/// What a living carbon mob gets at spawn.
/// tg `gib(DROP_ALL_REMAINS)`: the body is gone, its items fall (a `NoDrop` one goes with
/// it), its player becomes a ghost.
// shortcut: no gibs, organs or bodyparts left behind.
pub fn gib(world: &mut World, mob: Entity) {
    use crate::inventory::{ALL_SLOTS, Inventory, NoDrop};
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    world.entity_mut(mob).insert(Stat::Dead);
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client)
        && let Some(ghost) = crate::ghost::ghostize_client(world, client)
        && let Some(mut g) = world.get_mut::<crate::ghost::Ghost>(ghost)
    {
        g.body = None;
    }
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        let items: Vec<Entity> = ALL_SLOTS
            .iter()
            .filter_map(|&s| inventory.slot(s))
            .collect();
        for item in items {
            if world.get::<NoDrop>(item).is_some() {
                world.despawn(item);
            } else {
                crate::inventory::put_down(world, item, pos);
            }
        }
    }
    if let Some(cuffs) = world.get::<crate::restraints::Handcuffed>(mob).map(|h| h.0) {
        crate::inventory::put_down(world, cuffs, pos);
    }
    world.despawn(mob);
    crate::construction::tile_changed(world, pos);
}

pub fn living_bundle() -> impl Bundle {
    (
        Health::default(),
        Stat::Conscious,
        BodyPosition::default(),
        CombatMode::default(),
        NextMove::default(),
        HealthHudData::default(),
        Incapacitation::default(),
        ZoneSelected::default(),
    )
}

/// Applied to mobs and their HUD after the gameplay systems of a tick; consumers run after it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HealthSystems;

pub struct HealthServerPlugin;

impl Plugin for HealthServerPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<HealthHudUi>()
            .add_systems(
                Update,
                body_state.in_set(HealthSystems).before(UiSystems::Open),
            )
            .add_systems(PostUpdate, lying_appearance.before(NetServerSystems::Send));
    }
}

type BodyItem<'a> = (
    Entity,
    &'a Health,
    Ref<'a, Stat>,
    &'a mut BodyPosition,
    Option<&'a mut Mover>,
    Option<&'a mut Facing>,
    Has<Density>,
    &'a mut HealthHudData,
    &'a CombatMode,
    Option<&'a PlayerMob>,
    Option<&'a TilePos>,
    Option<&'a Name>,
    Option<&'a PullSlowdown>,
    Option<&'a Incapacitation>,
    (
        Option<&'a ZoneSelected>,
        Option<&'a crate::hunger::Fat>,
        Option<&'a crate::medical::LimbLimits>,
        Option<&'a crate::mecha::Pilot>,
        Option<&'a MoveSpeedModifier>,
        Option<&'a crate::janitor::Walking>,
        Option<&'a crate::taser::TaseSlowdown>,
        Option<&'a crate::mood::SanitySlowdown>,
        Option<&'a crate::hallucination::ScrewyHud>,
    ),
);

/// Everything that follows the damage and the stat: posture, density, speed, the owner's HUD, the
/// death message.
fn body_state(
    mut mobs: Query<
        BodyItem,
        Or<(
            Changed<Health>,
            Changed<Stat>,
            Changed<CombatMode>,
            Added<HealthHudData>,
            Changed<PullSlowdown>,
            Changed<Incapacitation>,
            Changed<ZoneSelected>,
            Changed<crate::hunger::Fat>,
            Changed<crate::medical::LimbLimits>,
            Changed<crate::mecha::Pilot>,
            Changed<MoveSpeedModifier>,
            Changed<crate::janitor::Walking>,
            Changed<crate::taser::TaseSlowdown>,
            Changed<crate::mood::SanitySlowdown>,
        )>,
    >,
    index: Res<SpatialIndex>,
    players: Query<&PlayerMob>,
    senses: Senses,
    mut rng: ResMut<CombatRng>,
    mut open: MessageWriter<OpenUi>,
    mut chat: MessageWriter<SendChat>,
    mut commands: Commands,
) {
    for (
        mob,
        health,
        stat,
        mut posture,
        mover,
        facing,
        dense,
        mut hud,
        combat,
        player,
        pos,
        name,
        drag,
        traits,
        (zone, fat, limits, pilot, speed, walking, tased, sanity, screwy),
    ) in &mut mobs
    {
        let traits = traits.copied().unwrap_or_default();
        // tg `bulky_drag` from `crate::pull`, deciseconds.
        let mut delay = health.move_delay(*stat) + drag.map_or(0.0, |d| d.0) / 10.0;
        // `/datum/movespeed_modifier/config_walk_run/walk`: WALK_DELAY instead of RUN_DELAY.
        if walking.is_some_and(|w| w.0) {
            delay += crate::janitor::WALK_DELAY_SECS - RUN_DELAY_SECS;
        }
        // `TRAIT_FLOORED` from status effects crawls like soft crit (`carbon_crawling`);
        // `/datum/movespeed_modifier/staggered`.
        if (traits.floored || traits.resting) && !health.floored(*stat) {
            delay += CRAWLING_ADD_SLOWDOWN / 10.0;
        }
        if traits.staggered {
            delay += STAGGERED_SLOWDOWN / 10.0;
        }
        if fat.is_some_and(|f| f.0) {
            delay += crate::hunger::OBESITY_SLOWDOWN / 10.0;
        }
        // `/datum/movespeed_modifier/sanity` (`crate::mood`).
        if let Some(sanity) = sanity {
            delay += sanity.0 / 10.0;
        }
        // Limping and disabled legs (`crate::medical::wounds`).
        if let Some(limits) = limits {
            delay += limits.slowdown / 10.0;
        }
        // Movespeed modifiers of abilities (negative: faster).
        if let Some(speed) = speed {
            delay = (delay + speed.0 / 10.0).max(0.0);
        }
        // `being_tased`, `tasing_someone` (`crate::taser`).
        if let Some(tased) = tased {
            delay += tased.0 / 10.0;
        }
        // A pilot walks at its mech's pace (`crate::mecha`).
        if let Some(pilot) = pilot {
            delay = pilot.move_delay;
        }
        let floored = health.floored(*stat) || traits.floored || traits.resting;
        if floored != posture.lying() {
            if floored {
                // tg `lying_angle_on_lying_down`: either side; facing east or west turns to a
                // random north or south, so the body falls on its side.
                posture.lying_angle = if rng.chance(50.0) { 90 } else { 270 };
                if let Some(mut facing) = facing
                    && matches!(facing.0, Dir::East | Dir::West)
                {
                    facing.0 = if rng.chance(50.0) {
                        Dir::North
                    } else {
                        Dir::South
                    };
                }
            } else {
                posture.lying_angle = 0;
            }
        }
        // tg `TRAIT_UNDENSE` while lying; a pilot is inside its mech.
        if (posture.lying() || pilot.is_some()) && dense {
            commands.entity(mob).remove::<Density>();
        } else if !posture.lying() && pilot.is_none() && !dense {
            commands.entity(mob).insert(Density);
        }
        if let Some(mut mover) = mover
            && mover.delay != delay
        {
            mover.delay = delay;
        }
        let mut readout = HealthReadout {
            health: health.health(),
            max: health.max,
            brute: health.brute,
            burn: health.burn,
            tox: health.tox,
            oxy: health.oxy,
            dead: *stat == Stat::Dead,
            limbs: doll_limbs(health),
        };
        // Hallucinated health HUDs (`crate::hallucination`); a change touches `Health`.
        if let Some(screwy) = screwy {
            screwy.apply(&mut readout, *stat);
        }
        hud.set_if_neq(HealthHudData {
            health: Some(readout),
            combat_mode: combat.0,
            move_delay: delay,
            immobile: !health.can_move(*stat) || traits.immobilized,
            phasing: false,
            zone_selected: zone.copied().unwrap_or_default().0,
        });
        if let Some(player) = player {
            open.write(OpenUi::new::<HealthHudUi>(player.client, mob, mob, ""));
        }
        if stat.is_changed() && *stat == Stat::Dead && !stat.is_added() {
            let Some(&pos) = pos else {
                continue;
            };
            // tg `deathgasp` (carbons have `ALWAYS_DEATHGASP`), BandaStation translation.
            let name = name.map_or("Кто-то", |n| n.as_str());
            let message = ChatMessage {
                channel: ChatChannel::Local,
                segments: vec![
                    ChatSegment::new(Span::Name, name),
                    ChatSegment::new(
                        Span::Emote,
                        " цепенеет и расслабляется, взгляд становится пустым и безжизненным...",
                    ),
                ],
            };
            let clients =
                clients_in_view(&index, &players, &senses, pos, DEFAULT_MESSAGE_RANGE, &[]);
            chat.write(SendChat { clients, message });
        }
    }
}

/// The health doll's limb states (tg `/atom/movable/screen/healthdoll/human`
/// `update_icon_state`: `ceil(damage / max_damage * 5)`), in [`crate::hud::DOLL_LIMBS`] order.
fn doll_limbs(health: &Health) -> [u8; 6] {
    crate::hud::DOLL_LIMBS.map(|zone| {
        let damage = health.part(zone).damage() / zone.max_damage();
        (damage * 5.0).ceil().clamp(0.0, 5.0) as u8
    })
}

/// Clients whose mob (or ghost) is within `range` of `pos` and perceives it (an AI eye only where
/// its cameras see), except the mobs in `exclude` (tg `visible_message` `ignored_mobs`).
// shortcut: range only, no line of sight (tg uses `viewers()`, which opaque turfs block), as for
// speech in `ss13_chat`.
pub fn clients_in_view(
    index: &SpatialIndex,
    players: &Query<&PlayerMob>,
    senses: &Senses,
    pos: TilePos,
    range: u32,
    exclude: &[Entity],
) -> Vec<Entity> {
    index
        .in_radius(pos, range)
        .filter(|e| !exclude.contains(e))
        .filter(|&e| senses.perceives(e, pos))
        .filter_map(|e| players.get(e).ok().map(|p| p.client))
        .collect()
}

/// The lying look on top of whatever built the appearance (`crate::inventory` rebuilds it from
/// the mob type's layer and offset on every inventory change): idempotent, so it only writes when
/// the result differs.
fn lying_appearance(
    mut mobs: Query<
        (
            &BodyPosition,
            &DmAtom,
            &mut Appearance,
            Option<&crate::tables::Elevated>,
        ),
        Or<(
            Changed<BodyPosition>,
            Changed<Appearance>,
            Changed<crate::tables::Elevated>,
        )>,
    >,
) {
    for (posture, atom, mut appearance, elevated) in &mut mobs {
        // tg `/datum/element/elevation`: up on a table or rack, lying or not.
        let up = elevated.map_or(0, |e| e.0);
        let (layer, pixel_y) = if posture.lying() {
            // tg `on_lying_down` lowers the layer only while it is the type's own.
            (LYING_MOB_LAYER, atom.pixel_y + PIXEL_Y_OFFSET_LYING + up)
        } else {
            (atom.layer, atom.pixel_y + up)
        };
        let wanted = (Layer(layer), [atom.pixel_x, pixel_y], posture.lying_angle);
        let current = (
            appearance.layer,
            appearance.pixel_offset,
            appearance.rotation,
        );
        if wanted != current {
            appearance.layer = wanted.0;
            appearance.pixel_offset = wanted.1;
            appearance.rotation = wanted.2;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hurt(brute: f32) -> (Health, Stat) {
        let mut health = Health::default();
        let mut stat = Stat::Conscious;
        apply_damage(&mut health, &mut stat, DamageType::Brute, brute);
        (health, stat)
    }

    #[test]
    fn thresholds_follow_tg() {
        assert_eq!(Stat::for_health(100.0), Stat::Conscious);
        assert_eq!(Stat::for_health(0.1), Stat::Conscious);
        assert_eq!(Stat::for_health(0.0), Stat::SoftCrit);
        assert_eq!(Stat::for_health(-29.9), Stat::SoftCrit);
        assert_eq!(Stat::for_health(-30.0), Stat::HardCrit);
        assert_eq!(Stat::for_health(-99.9), Stat::HardCrit);
        assert_eq!(Stat::for_health(-100.0), Stat::Dead);
        assert_eq!(hurt(99.0).1, Stat::Conscious);
        assert_eq!(hurt(100.0).1, Stat::SoftCrit);
        assert_eq!(hurt(130.0).1, Stat::HardCrit);
        assert_eq!(hurt(200.0).1, Stat::Dead);
    }

    #[test]
    fn health_sums_every_type_and_rounds_to_precision() {
        let mut health = Health::default();
        health.adjust(DamageType::Brute, 10.04);
        health.adjust(DamageType::Burn, 5.0);
        health.adjust(DamageType::Tox, 2.0);
        health.adjust(DamageType::Oxy, 3.0);
        health.adjust(DamageType::Stamina, 50.0);
        assert!((health.health() - 80.0).abs() < 1e-4, "{}", health.health());
        health.adjust(DamageType::Brute, -100.0);
        assert_eq!(health.brute, 0.0, "healing stops at zero");
    }

    #[test]
    fn death_is_permanent_and_reported_once() {
        let (mut health, mut stat) = hurt(200.0);
        assert_eq!(stat, Stat::Dead);
        assert_eq!(
            apply_damage(&mut health, &mut stat, DamageType::Brute, -150.0),
            None
        );
        assert_eq!(stat, Stat::Dead);
        let mut health = Health::default();
        let mut stat = Stat::Conscious;
        assert_eq!(
            apply_damage(&mut health, &mut stat, DamageType::Burn, 100.0),
            Some(Stat::Conscious)
        );
        assert_eq!(
            apply_damage(&mut health, &mut stat, DamageType::Burn, 1.0),
            None
        );
    }

    #[test]
    fn posture_and_speed_follow_the_stat() {
        let (health, stat) = hurt(20.0);
        assert!(!health.floored(stat) && health.can_move(stat));
        assert_eq!(health.move_delay(stat), RUN_DELAY_SECS);
        // 50 missing: damage slowdown 50/75 ds.
        let (health, stat) = hurt(50.0);
        assert!((health.move_delay(stat) - (RUN_DELAY_SECS + 50.0 / 750.0)).abs() < 1e-5);
        // Soft crit crawls: run + 100/75 + softcrit 2 + crawling 4 deciseconds.
        let (health, stat) = hurt(100.0);
        assert!(health.floored(stat) && health.can_move(stat));
        let expected = RUN_DELAY_SECS + (100.0 / 75.0 + 2.0 + 4.0) / 10.0;
        assert!((health.move_delay(stat) - expected).abs() < 1e-5);
        let (health, stat) = hurt(130.0);
        assert!(health.floored(stat) && !health.can_move(stat));
        // Passing out from oxyloss above crit.
        let mut health = Health::default();
        health.adjust(DamageType::Oxy, 50.0);
        assert!(health.knocked_out(Stat::Conscious) && !health.can_move(Stat::Conscious));
    }

    #[test]
    fn bodyparts_cap_spread_and_heal_like_tg() {
        let mut health = Health::default();
        // `receive_damage` caps a limb at `max_damage` (50).
        health.damage_zone(BodyZone::LArm, DamageType::Brute, 70.0);
        assert_eq!(health.part(BodyZone::LArm).brute, 50.0);
        health.damage_zone(BodyZone::LArm, DamageType::Burn, 10.0);
        assert_eq!(
            health.part(BodyZone::LArm).burn,
            0.0,
            "a maxed limb takes nothing"
        );
        health.damage_zone(BodyZone::Chest, DamageType::Brute, 45.0);
        assert_eq!((health.brute, health.burn), (95.0, 0.0));
        assert!((health.health() - 5.0).abs() < 1e-4);
        // A bruise pack (`heal_brute` 40) on the chest.
        assert_eq!(health.heal_zone(BodyZone::Chest, 40.0, 0.0), 40.0);
        assert_eq!(health.part(BodyZone::Chest).brute, 5.0);
        // Healing stops at zero and only heals the zone's own damage.
        assert_eq!(health.heal_zone(BodyZone::Chest, 40.0, 10.0), 5.0);
        assert_eq!(health.brute, 50.0);
        // Overall damage: an even share per part that can take it (the full arm takes none).
        health.adjust(DamageType::Burn, 10.0);
        assert_eq!(health.part(BodyZone::LArm).burn, 0.0);
        assert_eq!(health.part(BodyZone::Head).burn, 2.0);
        assert!((health.burn - 10.0).abs() < 1e-4, "{}", health.burn);
        // Overall healing takes it back from wherever it is.
        health.adjust(DamageType::Burn, -7.0);
        assert!((health.burn - 3.0).abs() < 1e-4, "{}", health.burn);
        health.adjust(DamageType::Brute, -100.0);
        assert_eq!(health.brute, 0.0);
        // A zoned negative amount heals that zone.
        health.damage_zone(BodyZone::Head, DamageType::Brute, 12.0);
        health.damage_zone(BodyZone::Head, DamageType::Brute, -2.0);
        assert_eq!(health.part(BodyZone::Head).brute, 10.0);
    }
}

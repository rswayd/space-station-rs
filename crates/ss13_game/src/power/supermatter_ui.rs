//! The supermatter's `Supermatter` window (server): tg `supermatter_crystal/ui_interact` (ghosts
//! only, from `attack_ghost`) and `sm_ui_data`. [`sm_entry`] builds one crystal's data from its
//! [`Crystal`] and the last breath's factors ([`SmFactors`]), for the NTOS monitor program too.
// shortcut: no `gas_metadata` static data (the gases' descriptions and per-gas effects under each
// gas), and the factor lists are always open; a ghost's window follows the default distance
// status (tg `observer_state`: anywhere it sees).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_atmos::gas::{PRIMARY_COLORS, byond_round};
use ss13_atmos::{GAS_COUNT, GASES};
use ss13_mapload::{Areas, DmName};
use ss13_shared::TilePos;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::supermatter::{SmEntry, SmFactor, SmGasShare, SupermatterUi};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi};

use super::supermatter::{
    Crystal, POWER_PENALTY_THRESHOLD, SM_DAMAGE_FACTORS, SM_POWER_FACTORS, SM_TEMP_LIMIT_FACTORS,
    SM_WASTE_FACTORS, SM_ZAP_FACTORS, SmFactors, Supermatter,
};

/// tg `uid`: crystals numbered from 1 as they appear.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmUid(pub u32);

pub struct SupermatterWindow;

impl UiInterface for SupermatterWindow {
    const NAME: &'static str = SupermatterUi::NAME;
    type Data = SmEntry;
    type StaticData = ();
}

impl ServerUi for SupermatterWindow {
    type Source = (
        &'static Supermatter,
        Option<&'static SmUid>,
        Option<&'static TilePos>,
    );
    type UserParam = (
        Option<Res<'static, Areas>>,
        Query<'static, 'static, &'static DmName>,
    );
    type Act = ();

    fn static_data(_: (&Supermatter, Option<&SmUid>, Option<&TilePos>)) {}

    fn data(
        (sm, uid, pos): (&Supermatter, Option<&SmUid>, Option<&TilePos>),
        _: Entity,
        (areas, names): &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> SmEntry {
        let area_name = areas
            .as_ref()
            .zip(pos)
            .and_then(|(a, &p)| a.area_at(p))
            .and_then(|area| names.get(area).ok())
            .map(|n| crate::vending::plain_name(&n.name).to_owned())
            .unwrap_or_default();
        sm_entry(&sm.crystal, uid.map_or(0, |u| u.0), area_name)
    }
}

pub struct SupermatterUiPlugin;

impl Plugin for SupermatterUiPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<SupermatterWindow>()
            .add_systems(Update, (number_crystals, ghost_opens));
    }
}

fn number_crystals(
    added: Query<Entity, (With<Supermatter>, Without<SmUid>)>,
    mut next: Local<u32>,
    mut commands: Commands,
) {
    for e in &added {
        *next += 1;
        commands.entity(e).insert(SmUid(*next));
    }
}

/// `attack_ghost` -> `ui_interact` (`isobserver(user)`).
fn ghost_opens(world: &mut World, mut clicks: Local<MessageCursor<crate::multiz::GhostClick>>) {
    let Some(messages) = world.get_resource::<Messages<crate::multiz::GhostClick>>() else {
        return;
    };
    let pending: Vec<crate::multiz::GhostClick> = clicks.read(messages).copied().collect();
    for click in pending {
        if world.get::<Supermatter>(click.target).is_none() {
            continue;
        }
        world.write_message(OpenUi::new::<SupermatterWindow>(
            click.client,
            click.ghost,
            click.target,
            SupermatterUi::NAME,
        ));
    }
}

/// `round(x, 0.01)`.
fn round2(x: f64) -> f64 {
    byond_round(x, 0.01)
}

/// The non-zero lines of a factor list (`round(amount, 0.01)` is the zero test), each value
/// through `value`.
fn lines(names: &[&str], amounts: &[f64], value: impl Fn(f64) -> f64) -> Vec<SmFactor> {
    names
        .iter()
        .zip(amounts)
        .filter(|(_, a)| round2(**a) != 0.0)
        .map(|(n, &a)| SmFactor {
            name: (*n).to_owned(),
            amount: value(a) as f32,
        })
        .collect()
}

/// The zap power past 5 GeV (`sm_ui_data`'s "High Energy Bonus"): the extra zaps and the
/// doubled power at 7 and 9 GeV, watts.
pub fn high_energy_bonus(zap_transmission: f64, internal_energy: f64) -> f64 {
    if internal_energy <= POWER_PENALTY_THRESHOLD {
        return 0.0;
    }
    let multiplier = 2f64.powf(
        ((internal_energy - POWER_PENALTY_THRESHOLD) / 2000.0)
            .floor()
            .clamp(0.0, 2.0),
    );
    // `INVERSE_LERP(1000, 3000, internal_energy)`.
    let zaps = ((internal_energy - 1000.0) / 2000.0)
        .floor()
        .clamp(1.0, 4.0);
    let additional = (internal_energy * 3200.0).clamp(6.4e6, 3.2e7) * zaps;
    (zap_transmission + additional) * multiplier - zap_transmission
}

/// `sm_ui_data()` of a crystal.
pub fn sm_entry(c: &Crystal, uid: u32, area_name: String) -> SmEntry {
    let f: &SmFactors = &c.factors;
    let energy = c.internal_energy;
    let mut zap_factors = lines(&SM_ZAP_FACTORS, &f.zap, |a| a * energy);
    zap_factors.retain(|z| z.amount != 0.0);
    let zap_transmission = c.zap_transmission_rate * energy;
    let bonus = high_energy_bonus(zap_transmission, energy);
    if bonus != 0.0 {
        zap_factors.push(SmFactor {
            name: "High Energy Bonus".to_owned(),
            amount: bonus as f32,
        });
    }
    let gas_composition = (0..GAS_COUNT)
        .map(|i| SmGasShare {
            name: GASES[i].name.to_owned(),
            color: PRIMARY_COLORS[i],
            amount: c.gas_percentage[i] as f32,
        })
        .collect();
    SmEntry {
        uid,
        area_name,
        integrity: c.integrity_percent() as f32,
        integrity_factors: lines(&SM_DAMAGE_FACTORS, &f.damage, |a| -round2(a)),
        internal_energy: energy as f32,
        internal_energy_factors: lines(&SM_POWER_FACTORS, &f.internal_energy, |a| a),
        zap_transmission: (zap_transmission + bonus) as f32,
        zap_transmission_factors: zap_factors,
        temp_limit: c.temp_limit as f32,
        temp_limit_factors: lines(&SM_TEMP_LIMIT_FACTORS, &f.temp_limit, round2),
        waste_multiplier: c.waste_multiplier as f32,
        waste_multiplier_factors: lines(&SM_WASTE_FACTORS, &f.waste_multiplier, round2),
        absorbed_ratio: c.absorption_ratio as f32,
        gas_composition,
        gas_temperature: c.gas_temperature as f32,
        gas_total_moles: c.gas_total_moles as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factors_skip_zeros_and_damage_reads_as_lost_integrity() {
        let mut c = Crystal::default();
        c.calculate_zap_transmission_rate();
        c.calculate_temp_limit(500.0);
        c.calculate_waste_multiplier();
        c.internal_energy = 1000.0;
        c.factors.damage = [0.0, 0.5, 0.0, 0.001, 0.0, 0.0];
        let entry = sm_entry(&c, 1, "Engine".into());
        assert_eq!(entry.integrity_factors.len(), 1, "0.001 rounds to 0");
        assert_eq!(entry.integrity_factors[0].amount, -0.5);
        // The base transmission only (no gas): 1040 W/MeV * 1000 MeV.
        assert_eq!(entry.zap_transmission_factors.len(), 1);
        assert_eq!(entry.zap_transmission, 1_040_000.0);
        assert_eq!(
            entry.temp_limit_factors.len(),
            1,
            "500 moles: no low-mole bonus"
        );
        assert_eq!(entry.waste_multiplier_factors.len(), 1);
        assert_eq!(entry.gas_composition.len(), GAS_COUNT);
    }

    #[test]
    fn the_high_energy_bonus_starts_past_five_gev() {
        assert_eq!(high_energy_bonus(1e6, 5000.0), 0.0);
        // 6 GeV: no doubling yet, two extra zaps of 19.2 MW.
        let bonus = high_energy_bonus(1e6, 6000.0);
        assert!((bonus - 2.0 * 19.2e6).abs() < 1.0, "{bonus}");
    }
}

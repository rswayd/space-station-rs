//! The ammo counter (server): BandaStation's `/datum/component/gunhud`
//! (`modular_bandastation/gunhud/code/gunhud_component.dm`). While a human holds a gun or a
//! welding tool, its HUD's counter (`ui_gunhud`) shows the ballistic gun's rounds (three digits,
//! "no_mag" or a flashing empty), the energy gun's cell and shot cost in percent, or the welder's
//! fuel and flame. [`GunHudData`] reaches the owner as the headless tgui interface
//! [`GunHudUi`]; `crate::gunhud_client` draws it.
// shortcut: the counter follows the active hand's gun or welder, else the other hand's (tg: the
// last one picked up). The welder's fuel is shown whole (tg prints `num2text` of a fractional
// amount, which overflows to "999").
// Cost: one pass over player humans per tick, a few comparisons each; a UI update only when the
// shown data changes.

use bevy::prelude::*;
use ss13_mapload::DmGun;
use ss13_mapload::fidelity::is_under;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiParams, UiReject, UiSystems};

use crate::guns::Gun;
use crate::hud::{GunHudData, GunHudUi};
use crate::inventory::Inventory;
use crate::server::PlayerMob;

/// `COLOR_CYAN`, `COLOR_TAN_ORANGE`, `COLOR_RED`, `COLOR_YELLOW`, `COLOR_VIBRANT_LIME`.
const CYAN: [u8; 3] = [0x00, 0xFF, 0xFF];
const TAN_ORANGE: [u8; 3] = [0xFF, 0x7B, 0x00];
const RED: [u8; 3] = [0xFF, 0x00, 0x00];
const YELLOW: [u8; 3] = [0xFF, 0xFF, 0x00];
const LIME: [u8; 3] = [0x00, 0xFF, 0x00];
const REVOLVER: &str = "/obj/item/gun/ballistic/revolver";

impl ServerUi for GunHudUi {
    type Source = &'static GunHudData;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &GunHudData) {}

    fn data(source: &GunHudData, _: Entity, _: &()) -> GunHudData {
        source.clone()
    }

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

pub fn plugin(app: &mut App) {
    app.register_ui::<GunHudUi>()
        .add_systems(Update, sync_gunhud.before(UiSystems::Open));
}

/// `set_hud(color, oth_o, oth_t, oth_h, indicator)` with the `oth_light` backlight: the
/// overlays that are set, in tg's order.
fn counter(color: [u8; 3], digits: [&str; 3], indicator: Option<&str>) -> GunHudData {
    let layers = std::iter::once("oth_light")
        .chain(digits.into_iter().filter(|d| !d.is_empty()))
        .chain(indicator)
        .map(|state| (state.to_owned(), color))
        .collect();
    GunHudData {
        shown: true,
        base: "backing".to_owned(),
        layers,
        energy: None,
    }
}

/// The digit overlays of `n` (`o` ones, `t` tens, `h` hundreds); over 999 all nines.
pub fn digits(n: u32) -> [String; 3] {
    if n > 999 {
        return ["o9".into(), "t9".into(), "h9".into()];
    }
    let text = n.to_string();
    let d: Vec<char> = text.chars().rev().collect();
    let at = |i: usize, prefix: &str| d.get(i).map_or(String::new(), |c| format!("{prefix}{c}"));
    [at(0, "o"), at(1, "t"), at(2, "h")]
}

fn with_digits(color: [u8; 3], n: u32, indicator: Option<&str>) -> GunHudData {
    let [o, t, h] = digits(n);
    counter(color, [o.as_str(), t.as_str(), h.as_str()], indicator)
}

/// `update_hud` of a ballistic gun.
pub fn ballistic_hud(dm: &DmGun, gun: &Gun) -> GunHudData {
    let Some(magazine) = &gun.magazine else {
        return counter(CYAN, ["oe", "te", "he"], Some("no_mag"));
    };
    // `get_ammo()`: the chambered casing (spent or not) and every casing in the magazine.
    let all = usize::from(gun.chambered.is_some()) + magazine.ammo_count(true);
    if all == 0 {
        return counter(CYAN, ["oe", "te", "he"], Some("empty_flash"));
    }
    // Revolvers count their live rounds only (`get_ammo(FALSE, FALSE)`).
    let rounds = if is_under(&dm.path, REVOLVER) {
        magazine.ammo_count(false)
    } else {
        all
    };
    with_digits(CYAN, rounds as u32, None)
}

/// `update_hud` of an energy gun: `FLOOR(percent, 1)` of the cell and of the selected shot.
pub fn energy_hud(dm: &DmGun, gun: &Gun) -> GunHudData {
    let max = dm.cell_maxcharge.max(1e-9);
    let cost = dm.shots.get(gun.select).map_or(0.0, |s| s.e_cost);
    let battery = ((gun.charge / max).clamp(0.0, 1.0) * 100.0).floor() as u32;
    let shot = ((cost / max).clamp(0.0, 1.0) * 100.0).floor() as u32;
    let can_shoot = dm.shots.get(gun.select).is_some() && gun.charge >= cost;
    let (base, color) = if !can_shoot {
        ("eammo_counter_empty", RED)
    } else if battery <= 25 {
        ("eammo_counter", YELLOW)
    } else {
        ("eammo_counter", LIME)
    };
    GunHudData {
        shown: true,
        base: base.to_owned(),
        layers: Vec::new(),
        energy: Some((battery, color, shot)),
    }
}

/// `update_hud` of a welding tool.
pub fn welder_hud(welder: &crate::tools::Welder) -> GunHudData {
    if welder.fuel < 1.0 {
        return counter(TAN_ORANGE, ["oe", "te", "he"], Some("empty_flash"));
    }
    let flame = if welder.on { "flame_on" } else { "flame_off" };
    with_digits(TAN_ORANGE, welder.fuel.floor() as u32, Some(flame))
}

/// The counter for `item`, when it has one.
fn hud_of(world: &World, item: Entity) -> Option<GunHudData> {
    if let (Some(dm), Some(gun)) = (world.get::<DmGun>(item), world.get::<Gun>(item)) {
        if dm.ballistic {
            return Some(ballistic_hud(dm, gun));
        }
        if dm.energy && dm.cell_maxcharge > 0.0 {
            return Some(energy_hud(dm, gun));
        }
        return None;
    }
    world.get::<crate::tools::Welder>(item).map(welder_hud)
}

/// `wake_up` / `turn_off` / `update_hud`: the counter of a held gun or welder, for humans.
fn sync_gunhud(world: &mut World) {
    let mut q =
        world.query_filtered::<(Entity, &PlayerMob, &Inventory), With<crate::human::HumanLook>>();
    let mobs: Vec<(Entity, Entity, Inventory)> = q
        .iter(world)
        .map(|(e, p, i)| (e, p.client, i.clone()))
        .collect();
    for (mob, client, inventory) in mobs {
        let active = inventory.hands[inventory.active];
        let other = inventory.hands[1 - inventory.active];
        let data = [active, other]
            .into_iter()
            .flatten()
            .find_map(|item| hud_of(world, item))
            .unwrap_or_default();
        let changed = match world.get::<GunHudData>(mob) {
            Some(old) => *old != data,
            // Nothing held and never shown: no window yet.
            None => data.shown,
        };
        if !changed {
            continue;
        }
        world.entity_mut(mob).insert(data);
        world.write_message(OpenUi::new::<GunHudUi>(client, mob, mob, ""));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_mapload::{CasingData, MagazineData};

    #[test]
    fn digits_follow_the_oth_overlays() {
        assert_eq!(digits(7), ["o7".to_owned(), String::new(), String::new()]);
        assert_eq!(
            digits(12),
            ["o2".to_owned(), "t1".to_owned(), String::new()]
        );
        assert_eq!(
            digits(305),
            ["o5".to_owned(), "t0".to_owned(), "h3".to_owned()]
        );
        assert_eq!(
            digits(1000),
            ["o9".to_owned(), "t9".to_owned(), "h9".to_owned()]
        );
    }

    #[test]
    fn a_pistol_counts_its_chambered_round_and_an_energy_gun_its_cell() {
        let dm = DmGun {
            ballistic: true,
            ..default()
        };
        let data = MagazineData {
            max_ammo: 12,
            ..default()
        };
        let mut gun = Gun {
            magazine: Some(crate::guns::Magazine::new(&data)),
            ..default()
        };
        gun.chambered = gun.magazine.as_mut().unwrap().rounds.pop();
        let hud = ballistic_hud(&dm, &gun);
        let states: Vec<&str> = hud.layers.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(states, ["oth_light", "o2", "t1"]);
        assert!(hud.layers.iter().all(|(_, c)| *c == CYAN));
        gun.magazine = None;
        assert_eq!(ballistic_hud(&dm, &gun).layers.last().unwrap().0, "no_mag");

        let egun = DmGun {
            energy: true,
            cell_maxcharge: 1000.0,
            shots: vec![CasingData {
                e_cost: 100.0,
                ..default()
            }],
            ..default()
        };
        let mut state = Gun {
            charge: 200.0,
            ..default()
        };
        let hud = energy_hud(&egun, &state);
        assert_eq!(hud.base, "eammo_counter");
        assert_eq!(hud.energy, Some((20, YELLOW, 10)));
        state.charge = 50.0;
        assert_eq!(energy_hud(&egun, &state).base, "eammo_counter_empty");
    }
}

use super::*;

/// `add_duds(n)`: `__dud<n-1>` .. `__dud0`.
pub(crate) fn with_duds(wires: &[&str], duds: usize) -> Vec<String> {
    let mut out: Vec<String> = wires.iter().map(|w| (*w).to_owned()).collect();
    out.extend((0..duds).rev().map(|i| format!("{WIRE_DUD_PREFIX}{i}")));
    out
}

/// tg `area.airlock_wires`, most specific area type first wins.
const AIRLOCK_AREA_WIRES: [(&str, &str, &str); 17] = [
    ("/area/station/ai", "/datum/wires/airlock/ai", "AI Airlock"),
    (
        "/area/station/cargo",
        "/datum/wires/airlock/cargo",
        "Cargo Airlock",
    ),
    (
        "/area/station/command",
        "/datum/wires/airlock/command",
        "Command Airlock",
    ),
    (
        "/area/station/engineering",
        "/datum/wires/airlock/engineering",
        "Engineering Airlock",
    ),
    (
        "/area/station/maintenance",
        "/datum/wires/airlock/maint",
        "Maintenance Airlock",
    ),
    (
        "/area/station/medical",
        "/datum/wires/airlock/medbay",
        "Medbay Airlock",
    ),
    (
        "/area/station/science",
        "/datum/wires/airlock/science",
        "Science Airlock",
    ),
    (
        "/area/station/security",
        "/datum/wires/airlock/security",
        "Security Airlock",
    ),
    (
        "/area/station/service",
        "/datum/wires/airlock/service",
        "Service Airlock",
    ),
    (
        "/area/station/solars",
        "/datum/wires/airlock/engineering",
        "Engineering Airlock",
    ),
    (
        "/area/station/tcommsat",
        "/datum/wires/airlock/engineering",
        "Engineering Airlock",
    ),
    (
        "/area/ruin/space/solars",
        "/datum/wires/airlock/engineering",
        "Engineering Airlock",
    ),
    (
        "/area/ruin/space/tcommsat_oldaisat",
        "/datum/wires/airlock/engineering",
        "Engineering Airlock",
    ),
    (
        "/area/ruin/space/has_grav/hauntedtradingpost/employees",
        "/datum/wires/airlock/engineering",
        "Engineering Airlock",
    ),
    (
        "/area/ruin/space/has_grav/hauntedtradingpost/maint",
        "/datum/wires/airlock/maint",
        "Maintenance Airlock",
    ),
    (
        "/area/ruin/space/has_grav/hauntedtradingpost/office",
        "/datum/wires/airlock/cargo",
        "Cargo Airlock",
    ),
    (
        "/area/ruin/space/has_grav/hauntedtradingpost/aicore",
        "/datum/wires/airlock/security",
        "Security Airlock",
    ),
];

/// `/datum/wires/airlock` (or the subtype the airlock's area names, tg `get_wires()`).
pub fn airlock_set(area_path: &str) -> WireSet {
    let (key, proper_name) = AIRLOCK_AREA_WIRES
        .iter()
        .filter(|(area, ..)| is_under(area_path, area))
        .max_by_key(|(area, ..)| area.len())
        .map_or(
            ("/obj/machinery/door/airlock", "Generic Airlock"),
            |(_, k, n)| (*k, *n),
        );
    WireSet {
        key,
        proper_name,
        wires: with_duds(
            &[
                WIRE_AI,
                WIRE_BACKUP1,
                WIRE_BACKUP2,
                WIRE_BOLTS,
                WIRE_IDSCAN,
                WIRE_FEEDBACK,
                WIRE_OPEN,
                WIRE_POWER1,
                WIRE_POWER2,
                WIRE_SAFETY,
                WIRE_SHOCK,
                WIRE_TIMING,
                WIRE_UNRESTRICTED_EXIT,
                WIRE_ZAP1,
                WIRE_ZAP2,
            ],
            2,
        ),
    }
}

/// `/datum/wires/apc`.
pub fn apc_set() -> WireSet {
    WireSet {
        key: "/obj/machinery/power/apc",
        proper_name: "APC",
        wires: with_duds(
            &[
                WIRE_EQUIPMENT,
                WIRE_LIGHT,
                WIRE_ENVIRONMENT,
                WIRE_POWER1,
                WIRE_POWER2,
                WIRE_INTERFACE,
                WIRE_AI,
            ],
            6,
        ),
    }
}

/// `/datum/wires/vending`.
pub fn vending_set() -> WireSet {
    WireSet {
        key: "/obj/machinery/vending",
        proper_name: "Vending Unit",
        wires: with_duds(
            &[
                WIRE_THROW,
                WIRE_SHOCK,
                WIRE_SPEAKER,
                WIRE_CONTRABAND,
                WIRE_IDSCAN,
            ],
            1,
        ),
    }
}

/// `/datum/wires/autolathe`.
pub fn autolathe_set() -> WireSet {
    WireSet {
        key: crate::science::autolathe::AUTOLATHE_ROOT,
        proper_name: "Autolathe",
        wires: with_duds(&[WIRE_HACK, WIRE_DISABLE, WIRE_SHOCK, WIRE_ZAP], 6),
    }
}

/// `/datum/wires/explosive/chem_grenade`: one dud wire (`duds_number = 1`), every instance
/// random (`randomize`).
pub fn chem_grenade_set() -> WireSet {
    WireSet {
        key: "/obj/item/grenade/chem_grenade",
        proper_name: "Explosive Device",
        wires: with_duds(&[], 1),
    }
}

/// [`Wires`] on an autolathe (map load replaces them with the round's colors in
/// [`init_wires`]).
pub fn add_autolathe_wires(world: &mut World, e: Entity) {
    let wires = world
        .get_resource_or_init::<WireColors>()
        .wires_for(WireHolder::Autolathe, &autolathe_set());
    world.entity_mut(e).insert(wires);
}

/// [`Wires`] and [`VendorHack`] on a vendor built from a frame.
pub fn add_vending_wires(world: &mut World, e: Entity) {
    let wires = world
        .get_resource_or_init::<WireColors>()
        .wires_for(WireHolder::Vending, &vending_set());
    world.entity_mut(e).insert((wires, VendorHack::default()));
}

/// FNV-1a, to give every wire set key its own stream from the round seed.
fn key_hash(key: &str) -> u64 {
    key.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// tg `randomize()` for a set: `shuffle(wires)`, then each wire takes `pick_n_take` of the
/// remaining colors (refilled when they run out). Deterministic in `(seed, key)`.
pub fn colors_for(seed: u64, set: &WireSet) -> Vec<(&'static str, String)> {
    let mut rng = Rng::new(seed ^ key_hash(set.key));
    let mut wires = set.wires.clone();
    // tg `/proc/shuffle`: `Swap(i, rand(i, len))`.
    for i in 0..wires.len().saturating_sub(1) {
        let j = rng.range(i as i64, wires.len() as i64 - 1) as usize;
        wires.swap(i, j);
    }
    let mut possible: Vec<&'static str> = Vec::new();
    let mut colors: Vec<(&'static str, String)> = Vec::new();
    for wire in wires {
        if possible.is_empty() {
            possible = POSSIBLE_COLORS.iter().map(|(c, _)| *c).collect();
        }
        let color = possible.remove(rng.range(0, possible.len() as i64 - 1) as usize);
        // `LAZYSET`: an assigned color keeps its place and takes the new wire.
        match colors.iter_mut().find(|(c, _)| *c == color) {
            Some(entry) => entry.1 = wire,
            None => colors.push((color, wire)),
        }
    }
    colors
}

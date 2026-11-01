//! Classic tg recipes with the generated data (`SS13_ASSETS/reagents.ron` from
//! `ss13-mapload convert`):
//! `SS13_ASSETS=... cargo test -p ss13_reagents --test real -- --ignored --nocapture`.

use std::path::PathBuf;

use ss13_reagents::{
    CHEM_FILE, ChemData, Chemistry, DEFAULT_REAGENT_TEMPERATURE, Holder, Owner, flags,
};

fn chem() -> Chemistry {
    let assets = std::env::var("SS13_ASSETS").unwrap_or_else(|_| "assets/generated".into());
    let data = ChemData::read(&PathBuf::from(assets).join(CHEM_FILE)).unwrap();
    let chem = Chemistry::new(data);
    eprintln!(
        "{} reagents, {} reactions, unresolved: {:?}",
        chem.reagents.len(),
        chem.reactions.len(),
        chem.unresolved_reactions
    );
    chem
}

/// Adds `list` to a beaker at room temperature, steps reactions (0.25 s) until they stop.
fn mix(chem: &Chemistry, list: &[(&str, f32)]) -> Holder {
    let mut beaker = Holder::new(
        100.0,
        flags::OPENCONTAINER,
        Owner {
            path: "/obj/item/reagent_containers/cup/beaker/large".into(),
            living: false,
        },
    );
    for &(path, amount) in list {
        let id = chem.id(path).unwrap_or_else(|| panic!("{path}"));
        beaker.add_reagent(
            chem,
            id,
            amount,
            DEFAULT_REAGENT_TEMPERATURE,
            None,
            None,
            false,
            false,
        );
    }
    let mut steps = 0;
    while beaker.is_reacting {
        beaker.process(chem, 0.25);
        steps += 1;
        assert!(steps < 2000, "never stops: {beaker:?}");
    }
    let shown: Vec<String> = beaker
        .reagents
        .iter()
        .map(|r| format!("{} {:.2}", chem.def(r.id).name, r.volume))
        .collect();
    eprintln!(
        "{list:?} -> {shown:?} at {:.1} K after {steps} steps, {:?}",
        beaker.chem_temp, beaker.messages
    );
    beaker
}

fn amount(chem: &Chemistry, h: &Holder, path: &str) -> f32 {
    h.amount(chem.id(path).unwrap())
}

#[test]
#[ignore = "needs generated assets"]
fn classic_recipes() {
    let chem = chem();
    let h = mix(
        &chem,
        &[
            ("/datum/reagent/hydrogen", 30.0),
            ("/datum/reagent/nitrogen", 10.0),
        ],
    );
    assert!((amount(&chem, &h, "/datum/reagent/ammonia") - 30.0).abs() < 0.1);
    assert_eq!(h.messages, ["The solution begins to bubble."]);
    let h = mix(
        &chem,
        &[
            ("/datum/reagent/phenol", 10.0),
            ("/datum/reagent/oxygen", 10.0),
            ("/datum/reagent/nitrogen", 10.0),
        ],
    );
    assert!(amount(&chem, &h, "/datum/reagent/medicine/c2/libital") > 25.0);
    let h = mix(
        &chem,
        &[
            ("/datum/reagent/consumable/ethanol/vodka", 10.0),
            ("/datum/reagent/consumable/orangejuice", 20.0),
        ],
    );
    assert!(
        amount(
            &chem,
            &h,
            "/datum/reagent/consumable/ethanol/screwdrivercocktail"
        ) > 29.0
    );
    // Water + potassium: tg's explosion (not modelled) still uses up the reagents at once.
    let h = mix(
        &chem,
        &[
            ("/datum/reagent/water", 5.0),
            ("/datum/reagent/potassium", 5.0),
        ],
    );
    assert_eq!(h.total_volume, 0.0);
}

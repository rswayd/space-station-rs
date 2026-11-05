//! Plant genes (tg `/datum/plant_gene`, `plant_genes.dm`): trait genes (types from
//! `plants.ron`) and reagent genes, what a seed may take (`can_add`) and the reagent gene
//! upgrades of cross-pollination (`try_upgrade_gene`, `perform_reagent_pollination`).
//!
//! The traits the tray, the harvest and the seed stats read have their effects here (see the
//! constants); product traits act in [`super::produce`].

use ss13_mapload::plants::TraitDef;
use ss13_reagents::is_under;

use super::seeds::{Dice, Seed};

/// `PLANT_GENE_REMOVABLE`, `PLANT_GENE_MUTATABLE`, `PLANT_GENE_GRAFTABLE`.
pub const PLANT_GENE_REMOVABLE: u32 = 1 << 0;
pub const PLANT_GENE_MUTATABLE: u32 = 1 << 1;
/// `TRAIT_HALVES_YIELD`, `TRAIT_NO_POLLINATION`.
pub const TRAIT_HALVES_YIELD: u32 = 1 << 0;
pub const TRAIT_NO_POLLINATION: u32 = 1 << 1;

pub const REPEATED_HARVEST: &str = "/datum/plant_gene/trait/repeated_harvest";
pub const FUNGAL: &str = ss13_mapload::plants::FUNGAL;
pub const WEED_HARDY: &str = "/datum/plant_gene/trait/plant_type/weed_hardy";
pub const TOXIN_ADAPTATION: &str = "/datum/plant_gene/trait/plant_type/toxin_adaptation";
pub const TOX_RESISTANCE: &str = "/datum/plant_gene/trait/tox_resistance";
pub const CARNIVORY: &str = "/datum/plant_gene/trait/carnivory";
pub const STABLE_STATS: &str = "/datum/plant_gene/trait/stable_stats";
pub const NEVER_MUTATE: &str = "/datum/plant_gene/trait/never_mutate";
pub const SAFE_INSTABILITY: &str = "/datum/plant_gene/trait/safe_instability";
pub const SOIL_LOVER: &str = "/datum/plant_gene/trait/soil_lover";
pub const SEMIAQUATIC: &str = "/datum/plant_gene/trait/semiaquatic";
pub const MAXCHEM: &str = "/datum/plant_gene/trait/maxchem";

/// One gene of a seed (tg keeps them in `genes` in this order).
#[derive(Clone, Debug, PartialEq)]
pub enum Gene {
    /// A `/datum/plant_gene/trait`: its type, `rate`, `mutability_flags` and the type's
    /// `trait_ids` and `trait_flags`.
    Trait {
        path: String,
        rate: f32,
        mutability: u32,
        ids: u32,
        flags: u32,
    },
    /// A `/datum/plant_gene/reagent`: the reagent and its `rate`.
    Reagent {
        path: String,
        rate: f32,
        mutability: u32,
    },
}

impl Gene {
    /// `new trait_type` (`/datum/plant_gene/trait/New` takes the type's vars).
    pub fn new_trait(def: &TraitDef) -> Self {
        Gene::Trait {
            path: def.path.clone(),
            rate: def.rate,
            mutability: def.mutability_flags,
            ids: def.trait_ids,
            flags: def.trait_flags,
        }
    }

    /// `new /datum/plant_gene/reagent(id, rate)`.
    pub fn new_reagent(path: &str, rate: f32) -> Self {
        Gene::Reagent {
            path: path.to_owned(),
            rate,
            mutability: PLANT_GENE_REMOVABLE,
        }
    }

    pub fn path(&self) -> &str {
        match self {
            Gene::Trait { path, .. } | Gene::Reagent { path, .. } => path,
        }
    }

    pub fn mutability(&self) -> u32 {
        match self {
            Gene::Trait { mutability, .. } | Gene::Reagent { mutability, .. } => *mutability,
        }
    }
}

/// `/datum/plant_gene/trait/can_add`: not blacklisted for the seed, no shared `trait_ids`, not
/// already there.
pub fn trait_can_add(def: &TraitDef, seed: &Seed) -> bool {
    if def.seed_blacklist.iter().any(|b| is_under(&seed.path, b)) {
        return false;
    }
    !seed.genes.iter().any(|g| match g {
        Gene::Trait { path, ids, .. } => def.trait_ids & ids != 0 || *path == def.path,
        Gene::Reagent { .. } => false,
    })
}

/// `/datum/plant_gene/reagent/can_add`: not if the seed makes the reagent at this rate or more.
pub fn reagent_can_add(gene: &Gene, seed: &Seed) -> bool {
    let Gene::Reagent { path, rate, .. } = gene else {
        return true;
    };
    !seed
        .genes
        .iter()
        .any(|g| matches!(g, Gene::Reagent { path: p, rate: r, .. } if p == path && r <= rate))
}

/// `/datum/plant_gene/reagent/try_upgrade_gene`: takes the seed's higher rate of the same
/// reagent. True when it did.
pub fn try_upgrade_gene(gene: &mut Gene, seed: &Seed) -> bool {
    let Gene::Reagent { path, rate, .. } = gene else {
        return false;
    };
    for g in &seed.genes {
        if let Gene::Reagent {
            path: p, rate: r, ..
        } = g
            && p == path
            && *r > *rate
        {
            *rate = *r;
            return true;
        }
    }
    false
}

/// `/obj/item/seeds/proc/perform_reagent_pollination(donor)`: the first reagent both make
/// raises the receptor's rate to the donor's (and stops), else one the receptor lacks is copied.
pub fn perform_reagent_pollination(receptor: &mut Seed, donor: &Seed, dice: &mut impl Dice) {
    let mut valid = Vec::new();
    for d in &donor.genes {
        let Gene::Reagent {
            path: dp, rate: dr, ..
        } = d
        else {
            continue;
        };
        let mut repeated = false;
        for r in &mut receptor.genes {
            if let Gene::Reagent { path, rate, .. } = r
                && path == dp
            {
                if *rate < *dr {
                    *rate = *dr;
                    return;
                }
                repeated = true;
                break;
            }
        }
        if !repeated {
            valid.push(d.clone());
        }
    }
    if !valid.is_empty() {
        let i = dice.pick(valid.len());
        receptor.genes.push(valid.swap_remove(i));
    }
}

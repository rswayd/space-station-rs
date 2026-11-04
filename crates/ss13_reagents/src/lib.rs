//! Chemistry after tg `code/modules/reagents`: reagent and reaction data ([`data`], generated
//! from the DM source), reagent holders (`/datum/reagents`: add, remove, transfer, reactions,
//! temperature; [`holder`]), tg's step reactions (`/datum/equilibrium`) and mob metabolism
//! ([`metabolism`]: `metabolize`, the stomach, `on_mob_life` of a starter set of reagents).
//!
//! Pure logic: no Bevy, no entities. `ss13_game::chem` owns the holders and wires them to items
//! and mobs.
//!
//! Numbers are `f32` like DM's, and rounded where tg rounds (`CHEMICAL_QUANTISATION_LEVEL`,
//! `CHEMICAL_VOLUME_ROUNDING`) with DM's `round(x, step)`.

pub mod data;
pub mod holder;
pub mod metabolism;

use std::collections::HashMap;

pub use data::{CHEM_FILE, ChemData, GlassStyleDef, ReactionDef, ReagentDef, SoupDef};
pub use holder::{Equilibrium, Holder, Owner, Reagent};

/// `CHEMICAL_QUANTISATION_LEVEL`: the smallest amount a holder keeps track of.
pub const CHEMICAL_QUANTISATION_LEVEL: f32 = 0.0001;
/// `CHEMICAL_VOLUME_ROUNDING`: totals are rounded to this; smaller reagents are removed.
pub const CHEMICAL_VOLUME_ROUNDING: f32 = 0.01;
pub const CHEMICAL_NORMAL_PH: f32 = 7.0;
pub const CHEMICAL_MIN_PH: f32 = 0.0;
pub const CHEMICAL_MAX_PH: f32 = 14.0;
pub const CHEMICAL_MAXIMUM_TEMPERATURE: f32 = 99999.0;
/// `DEFAULT_REAGENT_TEMPERATURE`, kelvin.
pub const DEFAULT_REAGENT_TEMPERATURE: f32 = 300.0;
/// `SPECIFIC_HEAT_DEFAULT`.
pub const SPECIFIC_HEAT_DEFAULT: f32 = 200.0;
/// `REAGENTS_METABOLISM`: units per second by default.
pub const REAGENTS_METABOLISM: f32 = 0.2;
/// `NO_OVERHEAT`.
pub const NO_OVERHEAT: f32 = 99999.0;

/// Holder flags (`code/__DEFINES/reagents.dm`).
pub mod flags {
    pub const INJECTABLE: u32 = 1 << 0;
    pub const DRAWABLE: u32 = 1 << 1;
    pub const REFILLABLE: u32 = 1 << 2;
    pub const DRAINABLE: u32 = 1 << 3;
    pub const DUNKABLE: u32 = 1 << 4;
    pub const TRANSPARENT: u32 = 1 << 5;
    pub const AMOUNT_VISIBLE: u32 = 1 << 6;
    pub const NO_REACT: u32 = 1 << 7;
    pub const REAGENT_HOLDER_INSTANT_REACT: u32 = 1 << 8;
    pub const REAGENT_HOLDER_ALIVE: u32 = 1 << 9;
    pub const SEALED_CONTAINER: u32 = 1 << 10;
    pub const NO_SPLASH: u32 = 1 << 11;
    pub const OPENCONTAINER: u32 = REFILLABLE | DRAINABLE | TRANSPARENT;
}

/// `chemical_flags` bits.
pub mod reagent_flags {
    pub const REAGENT_DEAD_PROCESS: u32 = 1 << 0;
    pub const REAGENT_CAN_BE_SYNTHESIZED: u32 = 1 << 4;
    pub const REAGENT_REVERSE_METABOLISM: u32 = 1 << 9;
    pub const REAGENT_UNAFFECTED_BY_METABOLISM: u32 = 1 << 10;
}

/// `reaction_flags` bits.
pub mod reaction_flags {
    pub const REACTION_CLEAR_INVERSE: u32 = 1 << 0;
    pub const REACTION_INSTANT: u32 = 1 << 1;
    pub const REACTION_HEAT_ARBITARY: u32 = 1 << 2;
    pub const REACTION_COMPETITIVE: u32 = 1 << 3;
    pub const REACTION_PH_VOL_CONSTANT: u32 = 1 << 4;
}

/// DM `round(x, step)`: `floor(x / step + 0.5) * step`.
pub fn round_to(x: f32, step: f32) -> f32 {
    (x / step + 0.5).floor() * step
}

/// Index into [`Chemistry::reagents`].
pub type ReagentId = u16;

/// A reaction with its reagent paths resolved.
#[derive(Clone, Debug)]
pub struct Reaction {
    pub def: ReactionDef,
    pub required: Vec<(ReagentId, f32)>,
    pub results: Vec<(ReagentId, f32)>,
    pub catalysts: Vec<(ReagentId, f32)>,
}

/// The loaded chemistry: tg `GLOB.chemical_reagents_list` and
/// `GLOB.chemical_reactions_list_reactant_index`.
#[derive(Clone, Debug, Default)]
pub struct Chemistry {
    pub reagents: Vec<ReagentDef>,
    ids: HashMap<String, ReagentId>,
    pub reactions: Vec<Reaction>,
    /// Per reagent: the reactions filed under it (`build_chemical_reactions_lists`: one
    /// required reagent per reaction, the one with the fewest reactions so far).
    by_reactant: Vec<Vec<u32>>,
    /// Reactions dropped because they name a reagent that does not exist.
    pub unresolved_reactions: Vec<String>,
    /// `GLOB.glass_style_singletons`: (container type, reagent) -> style.
    glass_styles: HashMap<(String, ReagentId), GlassStyleDef>,
}

impl Chemistry {
    pub fn new(data: ChemData) -> Self {
        let ids: HashMap<String, ReagentId> = data
            .reagents
            .iter()
            .enumerate()
            .map(|(i, r)| (r.path.clone(), i as ReagentId))
            .collect();
        let mut reactions = Vec::new();
        let mut unresolved_reactions = Vec::new();
        for def in data.reactions {
            let resolve = |list: &[(String, f32)]| {
                list.iter()
                    .map(|(p, n)| ids.get(p).map(|&id| (id, *n)))
                    .collect::<Option<Vec<_>>>()
            };
            match (
                resolve(&def.required_reagents),
                resolve(&def.results),
                resolve(&def.required_catalysts),
            ) {
                (Some(required), Some(results), Some(catalysts)) if !required.is_empty() => {
                    reactions.push(Reaction {
                        def,
                        required,
                        results,
                        catalysts,
                    });
                }
                (Some(required), ..) if required.is_empty() => {}
                _ => unresolved_reactions.push(def.path),
            }
        }
        let mut react_count = vec![0usize; data.reagents.len()];
        for r in &reactions {
            for &(id, _) in &r.required {
                react_count[id as usize] += 1;
            }
        }
        let mut by_reactant: Vec<Vec<u32>> = vec![Vec::new(); data.reagents.len()];
        for (i, r) in reactions.iter().enumerate() {
            let mut preferred: Option<ReagentId> = None;
            for &(id, _) in &r.required {
                let Some(p) = preferred else {
                    preferred = Some(id);
                    continue;
                };
                if by_reactant[id as usize].len() < by_reactant[p as usize].len()
                    || react_count[id as usize] < react_count[p as usize]
                {
                    preferred = Some(id);
                }
            }
            if let Some(p) = preferred {
                by_reactant[p as usize].push(i as u32);
            }
        }
        let glass_styles = data
            .glass_styles
            .into_iter()
            .filter_map(|g| Some(((g.container.clone(), *ids.get(&g.drink)?), g)))
            .collect();
        Self {
            reagents: data.reagents,
            ids,
            reactions,
            by_reactant,
            unresolved_reactions,
            glass_styles,
        }
    }

    /// The glass style of a `container` type whose main reagent is `id`.
    pub fn glass_style(&self, container: &str, id: ReagentId) -> Option<&GlassStyleDef> {
        self.glass_styles.get(&(container.to_owned(), id))
    }

    pub fn id(&self, path: &str) -> Option<ReagentId> {
        self.ids.get(path).copied()
    }

    pub fn def(&self, id: ReagentId) -> &ReagentDef {
        &self.reagents[id as usize]
    }

    /// DM `istype(reagent, path)`.
    pub fn is_type(&self, id: ReagentId, path: &str) -> bool {
        is_under(&self.def(id).path, path)
    }

    fn reactions_of(&self, id: ReagentId) -> &[u32] {
        &self.by_reactant[id as usize]
    }
}

/// DM `ispath(path, root)` on path strings.
pub fn is_under(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// A DM color string as RGBA; `None` for anything else.
pub fn parse_color(color: &str) -> Option<[u8; 4]> {
    let hex = color.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    match hex.len() {
        6 => Some([byte(0)?, byte(2)?, byte(4)?, 255]),
        8 => Some([byte(0)?, byte(2)?, byte(4)?, byte(6)?]),
        _ => None,
    }
}

/// tg `mix_color_from_reagents` (`BlendRGB` = linear `rgb_gradient`): each reagent blends into
/// the running color by its share of the volume so far. A color with alpha only blends into one
/// with alpha the usual way round; tg swaps the arguments (not the weight) otherwise.
pub fn mix_color(chem: &Chemistry, reagents: &[Reagent]) -> Option<[u8; 4]> {
    let mut mix: Option<([f32; 4], bool)> = None;
    let mut vol_counter = 0.0f32;
    for r in reagents {
        let color = &chem.def(r.id).color;
        let Some(rgba) = parse_color(color) else {
            continue;
        };
        let has_alpha = color.len() > 7;
        let c = rgba.map(f32::from);
        vol_counter += r.volume;
        mix = Some(match mix {
            None => (c, has_alpha),
            Some((m, m_alpha)) => {
                let t = if vol_counter > 0.0 {
                    r.volume / vol_counter
                } else {
                    0.0
                };
                let (a, b) = if m_alpha || !has_alpha {
                    (m, c)
                } else {
                    (c, m)
                };
                let blended = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
                (blended, m_alpha || has_alpha)
            }
        });
    }
    mix.map(|(c, _)| c.map(|v| v.round().clamp(0.0, 255.0) as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dm_round_and_paths() {
        assert_eq!(round_to(0.125, 0.01), 0.13);
        assert_eq!(round_to(-0.5, 1.0), 0.0, "floor(x + 0.5)");
        assert!(is_under(
            "/datum/reagent/toxin/amatoxin",
            "/datum/reagent/toxin"
        ));
        assert!(!is_under("/datum/reagent/toxinx", "/datum/reagent/toxin"));
        assert_eq!(parse_color("#AAAAAA77"), Some([0xAA, 0xAA, 0xAA, 0x77]));
        assert_eq!(parse_color("#ECEC8D"), Some([0xEC, 0xEC, 0x8D, 255]));
        assert_eq!(parse_color("red"), None);
    }
}

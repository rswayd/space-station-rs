//! What tg's `Initialize`, `update_icon_state()` and `update_overlays()` draw on common atoms at
//! round start, reproduced from static data: powered machines get their screens, indicator lights
//! and bulbs, with the `emissive_appearance()` masks that keep them visible in the dark
//! ([`EMISSIVE_PLANE`]); random posters, plants and bookcases pick their sprite with the map seed.
//!
//! The converter evaluates one rule per type ([`type_fields`]) into [`DmLook`]; the loader
//! resolves it against the manifest ([`Template::new`](crate::Template::new)) and
//! [`randomize_all`] rolls the random picks.
//!
//! Machines are taken as powered, undamaged, closed and in their idle state (no alarms, no
//! messages, a green security level, charged APCs).
// shortcut: no live state behind these looks; when power, alarms or APC charge are simulated,
// they must rebuild the overlays from the same rules.

use bevy::prelude::*;
use ss13_shared::{Appearance, EMISSIVE_PLANE, IconStateId, Overlay};

use crate::DmAtom;
use crate::convert::DmTypes;
use crate::fidelity::{ConvertContext, IconIndex, ext, is_under, num, truthy};
use crate::spawner::Rng;
use crate::value::{Field, parse_color};

const COMPONENT: &str = "dm_look";

/// One sprite of a look. An empty `icon` is the atom's own `icon`.
#[derive(Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmSprite {
    pub icon: String,
    pub state: String,
    /// `emissive_appearance()`: drawn on [`EMISSIVE_PLANE`].
    pub emissive: bool,
}

/// A type's round-start look (prototype data, see the module docs).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmLook {
    /// `icon_state` after `update_icon_state()`; empty = the atom's.
    pub icon_state: String,
    /// Bottom-up.
    pub overlays: Vec<DmSprite>,
    /// `Initialize` picks one of these at random (`icon` and `icon_state`); empty = no pick.
    pub random: Vec<DmSprite>,
    /// BandaStation window edging (`edge_overlay_file`): `edge-[smoothing_junction]` of this file
    /// drawn over the smoothed window in `edge_color` (`RESET_COLOR`); empty = none. Set by
    /// [`crate::smooth`], which knows the junction.
    pub edge_icon: String,
    pub edge_color: Option<[u8; 4]>,
}

/// Resolved vars of one type.
struct Vars<'a> {
    types: &'a DmTypes,
    path: &'a str,
}

impl Vars<'_> {
    fn get(&self, var: &str) -> Option<&serde_json::Value> {
        self.types.resolved(self.path, var)
    }

    /// Text or `'resource'`; `None` for `null` and the empty string.
    fn text(&self, var: &str) -> Option<&str> {
        let s = match self.get(var)? {
            serde_json::Value::String(s) => s.as_str(),
            serde_json::Value::Object(o) => o.get("resource")?.as_str()?,
            _ => return None,
        };
        (!s.is_empty()).then_some(s)
    }

    fn num(&self, var: &str) -> f64 {
        num(self.get(var)).unwrap_or(0.0)
    }

    fn truthy(&self, var: &str) -> bool {
        truthy(self.get(var))
    }
}

fn own(state: impl Into<String>) -> DmSprite {
    DmSprite {
        icon: String::new(),
        state: state.into(),
        emissive: false,
    }
}

/// `mutable_appearance(icon, state)` + `emissive_appearance(icon, state)`, the usual pair.
fn lit(icon: &str, state: &str) -> [DmSprite; 2] {
    let normal = DmSprite {
        icon: icon.into(),
        state: state.into(),
        emissive: false,
    };
    let emissive = DmSprite {
        emissive: true,
        ..normal.clone()
    };
    [normal, emissive]
}

fn emissive(state: impl Into<String>) -> DmSprite {
    DmSprite {
        emissive: true,
        ..own(state)
    }
}

/// tg `LIGHT_OK`.
const LIGHT_OK: f64 = 0.0;

/// The rule for `path`, `None` when no rule covers it.
pub fn look(ctx: &ConvertContext, path: &str) -> Option<DmLook> {
    let v = Vars {
        types: ctx.types,
        path,
    };
    let under = |root| is_under(path, root);
    let base = || v.text("base_icon_state").unwrap_or_default().to_owned();
    let mut look = DmLook::default();
    if under("/obj/machinery/light") {
        // `update_icon_state()` / `update_overlays()` of a fixture that is on.
        if v.num("status") != LIGHT_OK {
            return Some(look);
        }
        let state = v.text("base_state")?;
        let overlay_icon = v.text("overlay_icon").unwrap_or_default();
        look.icon_state = state.into();
        let [normal, glow] = lit(overlay_icon, state);
        look.overlays = vec![glow, normal];
    } else if under("/obj/machinery/computer") {
        if let Some(keyboard) = v.text("icon_keyboard") {
            look.overlays.push(own(keyboard));
        }
        if let Some(screen) = v.text("icon_screen") {
            look.overlays.extend(lit("", screen));
        }
    } else if under("/obj/machinery/airalarm") {
        look.overlays.extend(lit("", "alarm0"));
    } else if under("/obj/machinery/firealarm") {
        // `fire_[SEC_LEVEL_GREEN]`.
        look.icon_state = "fire0".into();
        look.overlays = vec![own("fire_0"), emissive("fire_level_e")];
    } else if under("/obj/machinery/light_switch") {
        // The area's `lightswitch` starts on.
        look.icon_state = format!("{}-on", base());
        look.overlays
            .push(emissive(format!("{}-emissive-on", base())));
    } else if under("/obj/machinery/button") {
        look.icon_state = format!("{}{}", base(), v.text("skin").unwrap_or_default());
        look.overlays
            .push(emissive(format!("{}-light-mask", base())));
    } else if under("/obj/machinery/newscaster") {
        look.overlays.extend(lit("", &format!("{}_normal", base())));
    } else if under("/obj/machinery/requests_console") {
        look.overlays.extend(lit("", &format!("{}0", base())));
    } else if under("/obj/machinery/power/apc") {
        // Locked, fully charged, every channel auto-on.
        for state in ["apcox-1", "apco3-2", "apco0-3", "apco1-3", "apco2-3"] {
            look.overlays.extend(lit("", state));
        }
    } else if under("/obj/machinery/status_display/evac") {
        // The logo it shows until the shuttle moves or command sets a message.
        look.overlays = vec![own("default"), emissive("outline")];
    } else if under("/obj/machinery/status_display/ai") {
        // `AI_DISPLAY_DONT_GLOW` until an AI picks an emotion.
        look.overlays.push(own("ai_off"));
    } else if under("/obj/machinery/vitals_reader") {
        // No patient: the lit frame only.
        look.overlays.push(emissive("outline"));
    } else if under("/obj/machinery/vending") {
        if let Some(mask) = v.text("light_mask") {
            look.overlays.push(emissive(mask));
        }
    } else if under("/obj/structure/window") {
        if let Some(file) = v.text("edge_overlay_file") {
            look.edge_icon = file.into();
            look.edge_color = v.text("edge_overlay_color").and_then(parse_color);
        }
    } else if under("/obj/structure/sign/poster") {
        look.random = poster_pool(ctx, &v);
    } else if under("/obj/item/kirbyplants/random") {
        if v.truthy("dead") {
            return Some(look);
        }
        let states: Vec<String> = if under("/obj/item/kirbyplants/random/fullysynthetic") {
            (26..=29).map(|n| format!("plant-{n}")).collect()
        } else {
            // `generate_states()`.
            (1..=24)
                .map(|n| format!("plant-{n:02}"))
                .chain(["applebush".to_owned()])
                .collect()
        };
        // `Initialize` also switches from the mapping icon.
        look.random = states
            .into_iter()
            .map(|state| DmSprite {
                icon: "icons/obj/fluff/flora/plants.dmi".into(),
                ..own(state)
            })
            .collect();
    } else if under("/obj/item/reagent_containers/blood/random") {
        look.icon_state = "bloodpack".into();
    } else if under("/obj/structure/bookcase/random") {
        // `books_to_load += pick(-1, -1, 0, 1, 1)`, then `book-[count]`; assumes the library
        // database has books.
        let n = v.num("books_to_load") as i64;
        look.random = [-1, -1, 0, 1, 1]
            .iter()
            .map(|d| own(format!("book-{}", (n + d).clamp(0, 5))))
            .collect();
    } else {
        return None;
    }
    Some(look)
}

/// `/obj/structure/sign/poster/proc/randomise`: subtypes of `random_basetype` minus
/// `blacklisted_types`, with an `icon_state`, not `never_random`, not pixel shifted.
fn poster_pool(ctx: &ConvertContext, v: &Vars) -> Vec<DmSprite> {
    let path_of = |value: &serde_json::Value| value.get("path")?.as_str().map(str::to_owned);
    let Some(base) = v.get("random_basetype").and_then(path_of) else {
        return Vec::new();
    };
    let blacklisted: Vec<String> = v
        .get("blacklisted_types")
        .and_then(|l| l.get("list")?.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(path_of)
        .collect();
    ctx.typesof(&base)
        .into_iter()
        .filter(|&t| t != base && !blacklisted.iter().any(|b| is_under(t, b)))
        .filter_map(|t| {
            let p = Vars {
                types: ctx.types,
                path: t,
            };
            let ok =
                !p.truthy("never_random") && p.num("pixel_x") == 0.0 && p.num("pixel_y") == 0.0;
            Some(DmSprite {
                icon: p.text("icon")?.into(),
                state: p.text("icon_state").filter(|_| ok)?.into(),
                emissive: false,
            })
        })
        .collect()
}

/// Converter fields: the whole look, on every type whose look differs from its parent's (a
/// subtype that loses its parent's look, like a broken light, gets an empty one).
pub fn type_fields(ctx: &ConvertContext, path: &str) -> Vec<Field> {
    let Some(own) = look(ctx, path) else {
        return Vec::new();
    };
    let parent = ctx.types.types.get(path).and_then(|t| t.parent.as_deref());
    if parent.and_then(|p| look(ctx, p)).as_ref() == Some(&own) {
        return Vec::new();
    }
    let sprites = |list: &[DmSprite]| {
        let items: Vec<String> = list
            .iter()
            .map(|s| {
                format!(
                    "(icon: {}, state: {}, emissive: {})",
                    ron_str(&s.icon),
                    ron_str(&s.state),
                    s.emissive
                )
            })
            .collect();
        format!("[{}]", items.join(", "))
    };
    let color = match own.edge_color {
        Some([r, g, b, a]) => format!("Some(({r}, {g}, {b}, {a}))"),
        None => "None".into(),
    };
    vec![
        ext(COMPONENT, "icon_state", ron_str(&own.icon_state)),
        ext(COMPONENT, "overlays", sprites(&own.overlays)),
        ext(COMPONENT, "random", sprites(&own.random)),
        ext(COMPONENT, "edge_icon", ron_str(&own.edge_icon)),
        ext(COMPONENT, "edge_color", color),
    ]
}

fn ron_str(s: &str) -> String {
    ron::to_string(s).expect("strings serialize")
}

/// Resolves a sprite of the atom `atom` against the manifest index.
fn resolve(
    index: &impl Fn(&str, &str) -> Option<IconStateId>,
    atom: &DmAtom,
    s: &DmSprite,
) -> Option<IconStateId> {
    let icon = if s.icon.is_empty() {
        &atom.icon
    } else {
        &s.icon
    };
    index(icon, &s.state)
}

/// Applies the static part of `look` (state and overlays) to an appearance at template build.
/// Returns the sprites the manifest does not have, as `icon:"state"` keys.
pub fn apply(
    index: &impl Fn(&str, &str) -> Option<IconStateId>,
    atom: &DmAtom,
    look: &DmLook,
    appearance: &mut Appearance,
) -> Vec<String> {
    let mut missing = Vec::new();
    let mut key = |s: &DmSprite| {
        let icon = if s.icon.is_empty() {
            &atom.icon
        } else {
            &s.icon
        };
        missing.push(format!("{icon}:{:?}", s.state));
    };
    if !look.icon_state.is_empty() {
        let sprite = own(look.icon_state.clone());
        match resolve(index, atom, &sprite) {
            Some(id) => appearance.icon_state = id,
            None => key(&sprite),
        }
    }
    for s in &look.overlays {
        let Some(id) = resolve(index, atom, s) else {
            key(s);
            continue;
        };
        appearance.overlays.push(Overlay {
            plane: s.emissive.then_some(EMISSIVE_PLANE),
            ..Overlay::new(id)
        });
    }
    missing
}

/// Random picks done (by kind of pick) and sprites missing from the manifest.
#[derive(Debug, Default)]
pub struct RandomStats {
    pub picked: usize,
    pub missing: usize,
}

/// Rolls every entity's random look (posters, plants, bookcases), in spawn order, so a seed
/// gives the same station.
pub fn randomize_all(world: &mut World, icons: &IconIndex, rng: &mut Rng) -> RandomStats {
    let mut stats = RandomStats::default();
    let mut query = world
        .query_filtered::<(Entity, &DmLook, &DmAtom, &mut Appearance), With<ss13_world::Mapped>>();
    let mut rolls: Vec<_> = query
        .iter_mut(world)
        .filter(|(_, look, ..)| !look.random.is_empty())
        .collect();
    rolls.sort_by_key(|(e, ..)| *e);
    let index = |icon: &str, state: &str| icons.get(icon, state);
    for (_, look, atom, mut appearance) in rolls {
        let pick = &look.random[rng.range(0, look.random.len() as i64 - 1) as usize];
        match resolve(&index, atom, pick) {
            Some(id) => {
                appearance.icon_state = id;
                stats.picked += 1;
            }
            None => stats.missing += 1,
        }
    }
    stats
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(json: serde_json::Value) -> DmTypes {
        let mut all = serde_json::json!({ "schema": crate::convert::DM_TYPES_SCHEMA, "types": {} });
        all["types"] = json;
        serde_json::from_value(all).unwrap()
    }

    fn ty(parent: &str, vars: serde_json::Value) -> serde_json::Value {
        serde_json::json!({ "parent": parent, "declared_vars": vars })
    }

    #[test]
    fn machines_get_screens_and_emissive_masks() {
        let t = types(serde_json::json!({
            "/obj": ty("/atom", serde_json::json!({})),
            "/obj/machinery": ty("/obj", serde_json::json!({})),
            "/obj/machinery/computer": ty("/obj/machinery", serde_json::json!({
                "icon_screen": "generic", "icon_keyboard": "generic_key"})),
            "/obj/machinery/computer/crew": ty("/obj/machinery/computer", serde_json::json!({
                "icon_screen": "crew"})),
            "/obj/machinery/light": ty("/obj/machinery", serde_json::json!({
                "base_state": "tube", "status": 0,
                "overlay_icon": {"resource": "icons/obj/lighting_overlay.dmi"}})),
            "/obj/machinery/light/broken": ty("/obj/machinery/light", serde_json::json!({
                "status": 2})),
        }));
        let ctx = ConvertContext::new(&t);
        let crew = look(&ctx, "/obj/machinery/computer/crew").unwrap();
        assert_eq!(
            crew.overlays,
            vec![own("generic_key"), own("crew"), emissive("crew")]
        );
        let light = look(&ctx, "/obj/machinery/light").unwrap();
        assert_eq!(light.icon_state, "tube");
        assert_eq!(light.overlays.len(), 2);
        assert!(light.overlays[0].emissive && !light.overlays[1].emissive);
        assert_eq!(light.overlays[1].icon, "icons/obj/lighting_overlay.dmi");
        // A broken subtype overrides the parent's look with an empty one.
        assert_eq!(
            look(&ctx, "/obj/machinery/light/broken"),
            Some(DmLook::default())
        );
        assert!(!type_fields(&ctx, "/obj/machinery/light/broken").is_empty());
        assert_eq!(type_fields(&ctx, "/obj/machinery/computer/crew").len(), 5);
        assert!(look(&ctx, "/obj/machinery").is_none());
    }

    #[test]
    fn random_posters_follow_randomise() {
        let poster = |vars| ty("/obj/structure/sign/poster", vars);
        let t = types(serde_json::json!({
            "/obj": ty("/atom", serde_json::json!({})),
            "/obj/structure": ty("/obj", serde_json::json!({})),
            "/obj/structure/sign": ty("/obj/structure", serde_json::json!({})),
            "/obj/structure/sign/poster": ty("/obj/structure/sign", serde_json::json!({
                "icon": {"resource": "icons/obj/poster.dmi"}})),
            "/obj/structure/sign/poster/a": poster(serde_json::json!({"icon_state": "a"})),
            "/obj/structure/sign/poster/a/north": ty("/obj/structure/sign/poster/a",
                serde_json::json!({"pixel_y": 32})),
            "/obj/structure/sign/poster/b": poster(serde_json::json!({"icon_state": "b",
                "never_random": 1})),
            "/obj/structure/sign/poster/traitor": poster(serde_json::json!({"icon_state": "t"})),
            "/obj/structure/sign/poster/random": poster(serde_json::json!({
                "icon_state": "random_anything", "never_random": 1,
                "random_basetype": {"path": "/obj/structure/sign/poster"},
                "blacklisted_types": {"list": [{"path": "/obj/structure/sign/poster/traitor"}]}})),
        }));
        let ctx = ConvertContext::new(&t);
        let pool = look(&ctx, "/obj/structure/sign/poster/random")
            .unwrap()
            .random;
        assert_eq!(
            pool.iter().map(|s| s.state.as_str()).collect::<Vec<_>>(),
            ["a"]
        );
        assert_eq!(pool[0].icon, "icons/obj/poster.dmi");
        // Plain posters have no pool.
        assert!(
            look(&ctx, "/obj/structure/sign/poster/a")
                .unwrap()
                .random
                .is_empty()
        );
    }
}

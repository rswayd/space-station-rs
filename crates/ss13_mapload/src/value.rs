//! DM var values -> typed fields of [`DmName`](crate::DmName) / [`DmAtom`](crate::DmAtom), and
//! fields -> the RON patch syntax of `ss13_proto`. Shared by the type converter (values from
//! `dm_types.json`) and map instances (var edits from `.dmm`), so both follow one rule.

use std::fmt::Write as _;

use ss13_assets::dmm::DmValue;

/// A DM value reduced to what the supported vars can hold.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Val<'a> {
    Null,
    Num(f64),
    Str(&'a str),
    /// `'icons/foo.dmi'`.
    Resource(&'a str),
    /// Lists, paths, non-constant expressions, ...
    Other,
}

impl<'a> Val<'a> {
    /// `dm_types.json` encoding (see `ss13_dm_extract`).
    pub fn from_json(v: &'a serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => Val::Null,
            serde_json::Value::Number(n) => n.as_f64().map_or(Val::Other, Val::Num),
            serde_json::Value::String(s) => Val::Str(s),
            serde_json::Value::Object(o) => match o.get("resource") {
                Some(serde_json::Value::String(s)) => Val::Resource(s),
                _ => Val::Other,
            },
            _ => Val::Other,
        }
    }

    pub fn from_dmm(v: &'a DmValue) -> Self {
        match v {
            DmValue::Null => Val::Null,
            DmValue::Number(n) => Val::Num(*n),
            DmValue::Str(s) => Val::Str(s),
            DmValue::Resource(s) => Val::Resource(s),
            DmValue::Path(_) | DmValue::List(_) | DmValue::Raw(_) => Val::Other,
        }
    }
}

/// One supported var with its converted value.
#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    Name(String),
    Desc(String),
    Icon(String),
    IconState(String),
    Layer(f32),
    Plane(i16),
    PixelX(i16),
    PixelY(i16),
    Color(Option<[u8; 4]>),
    Alpha(u8),
    Dir(u8),
    Density(bool),
    Opacity(bool),
    Invisibility(u8),
    MouseOpacity(u8),
    /// A field of another prototype component (see [`crate::fidelity`]): `"component": (key: ron)`.
    Ext {
        component: &'static str,
        key: &'static str,
        ron: String,
    },
}

/// The var names [`field`] understands.
pub const SUPPORTED_VARS: [&str; 15] = [
    "name",
    "desc",
    "icon",
    "icon_state",
    "layer",
    "plane",
    "pixel_x",
    "pixel_y",
    "color",
    "alpha",
    "dir",
    "density",
    "opacity",
    "invisibility",
    "mouse_opacity",
];

/// Why a var could not become a [`Field`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skip {
    /// Not one of [`SUPPORTED_VARS`]: the game has no component for it yet.
    UnknownVar,
    /// A supported var with a value we cannot represent (color matrix, expression, ...).
    Unsupported,
}

/// Converts `var = value`. Numbers are rounded/clamped like BYOND stores them.
pub fn field(var: &str, value: Val) -> Result<Field, Skip> {
    use Val::*;
    let num = |v: Val| match v {
        Num(n) => Ok(n),
        Null => Ok(0.0),
        _ => Err(Skip::Unsupported),
    };
    let text = |v: Val<'_>| match v {
        Str(s) | Resource(s) => Ok(s.to_owned()),
        Null => Ok(String::new()),
        _ => Err(Skip::Unsupported),
    };
    Ok(match var {
        "name" => Field::Name(text(value)?),
        "desc" => Field::Desc(text(value)?),
        "icon" => Field::Icon(text(value)?),
        "icon_state" => Field::IconState(text(value)?),
        "layer" => Field::Layer(num(value)? as f32),
        "plane" => Field::Plane(clamp_i16(num(value)?)),
        "pixel_x" => Field::PixelX(clamp_i16(num(value)?)),
        "pixel_y" => Field::PixelY(clamp_i16(num(value)?)),
        "alpha" => Field::Alpha(num(value)?.round().clamp(0.0, 255.0) as u8),
        "dir" => Field::Dir(num(value)?.round().clamp(0.0, 255.0) as u8),
        "density" => Field::Density(num(value)? != 0.0),
        "opacity" => Field::Opacity(num(value)? != 0.0),
        "invisibility" => Field::Invisibility(num(value)?.round().clamp(0.0, 255.0) as u8),
        "mouse_opacity" => Field::MouseOpacity(num(value)?.round().clamp(0.0, 255.0) as u8),
        "color" => Field::Color(match value {
            Null => None,
            Str(s) => Some(parse_color(s).ok_or(Skip::Unsupported)?),
            _ => return Err(Skip::Unsupported),
        }),
        // Lighting vars do not depend on the type: the converter and `.dmm` edits share them.
        _ => return crate::light::field(var, value),
    })
}

/// [`field`] for `.dmm` var edits, then the vars of the fidelity components (which depend on the
/// type `path`). The converter reads those from `dm_types.json` itself (`fidelity::type_fields`).
pub fn field_for(path: &str, var: &str, value: Val) -> Result<Field, Skip> {
    match field(var, value) {
        Err(Skip::UnknownVar) => crate::fidelity::ext_field(path, var, value),
        other => other,
    }
}

fn clamp_i16(n: f64) -> i16 {
    n.round().clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
}

/// BYOND color text: `#rgb`, `#rrggbb`, `#rrggbbaa` (the `#` is optional) or a color name.
pub fn parse_color(s: &str) -> Option<[u8; 4]> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    let nibble = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let digits: Option<Vec<u8>> = hex.bytes().map(nibble).collect();
    if let Some(d) = digits {
        match d.len() {
            3 => return Some([d[0] * 17, d[1] * 17, d[2] * 17, 255]),
            6 | 8 => {
                let byte = |i: usize| d[i] * 16 + d[i + 1];
                let alpha = if d.len() == 8 { byte(6) } else { 255 };
                return Some([byte(0), byte(2), byte(4), alpha]);
            }
            _ => {}
        }
    }
    // shortcut: the HTML/BYOND basic names only; extend if maps start using others.
    let rgb: u32 = match s.to_ascii_lowercase().as_str() {
        "black" => 0x000000,
        "white" => 0xffffff,
        "gray" | "grey" => 0x808080,
        "silver" => 0xc0c0c0,
        "red" => 0xff0000,
        "maroon" => 0x800000,
        "green" => 0x008000,
        "lime" => 0x00ff00,
        "blue" => 0x0000ff,
        "navy" => 0x000080,
        "yellow" => 0xffff00,
        "olive" => 0x808000,
        "purple" => 0x800080,
        "fuchsia" | "magenta" => 0xff00ff,
        "teal" => 0x008080,
        "aqua" | "cyan" => 0x00ffff,
        "orange" => 0xffa500,
        "violet" => 0xee82ee,
        _ => return None,
    };
    let [_, r, g, b] = rgb.to_be_bytes();
    Some([r, g, b, 255])
}

/// `ss13_proto` component patch for `fields`: `{"dm_name": (...), "dm_atom": (...)}`.
/// Later fields win over earlier ones for the same var, like repeated DM assignments.
pub fn patch_ron(fields: &[Field]) -> String {
    patch_ron_with(fields, &[])
}

/// [`patch_ron`] plus ready `"name": (...)` entries of other components.
pub fn patch_ron_with(fields: &[Field], extra: &[String]) -> String {
    let mut name = String::new();
    let mut atom = String::new();
    let mut ext: Vec<(&str, String)> = Vec::new();
    for f in dedup_last(fields) {
        if let Field::Ext {
            component,
            key,
            ron,
        } = f
        {
            let i = match ext.iter().position(|(c, _)| c == component) {
                Some(i) => i,
                None => {
                    ext.push((component, String::new()));
                    ext.len() - 1
                }
            };
            let out = &mut ext[i].1;
            if !out.is_empty() {
                out.push_str(", ");
            }
            let _ = write!(out, "{key}: {ron}");
            continue;
        }
        let (out, key, value) = match f {
            Field::Name(s) => (&mut name, "name", ron_str(s)),
            Field::Desc(s) => (&mut name, "desc", ron_str(s)),
            Field::Icon(s) => (&mut atom, "icon", ron_str(s)),
            Field::IconState(s) => (&mut atom, "icon_state", ron_str(s)),
            Field::Layer(v) => (&mut atom, "layer", format!("{v:?}")),
            Field::Plane(v) => (&mut atom, "plane", v.to_string()),
            Field::PixelX(v) => (&mut atom, "pixel_x", v.to_string()),
            Field::PixelY(v) => (&mut atom, "pixel_y", v.to_string()),
            Field::Color(None) => (&mut atom, "color", "None".to_owned()),
            Field::Color(Some([r, g, b, a])) => {
                (&mut atom, "color", format!("Some(({r}, {g}, {b}, {a}))"))
            }
            Field::Alpha(v) => (&mut atom, "alpha", v.to_string()),
            Field::Dir(v) => (&mut atom, "dir", v.to_string()),
            Field::Density(v) => (&mut atom, "density", v.to_string()),
            Field::Opacity(v) => (&mut atom, "opacity", v.to_string()),
            Field::Invisibility(v) => (&mut atom, "invisibility", v.to_string()),
            Field::MouseOpacity(v) => (&mut atom, "mouse_opacity", v.to_string()),
            Field::Ext { .. } => unreachable!("handled above"),
        };
        if !out.is_empty() {
            out.push_str(", ");
        }
        let _ = write!(out, "{key}: {value}");
    }
    let mut parts = Vec::new();
    if !name.is_empty() {
        parts.push(format!("\"dm_name\": ({name})"));
    }
    if !atom.is_empty() {
        parts.push(format!("\"dm_atom\": ({atom})"));
    }
    for (component, fields) in ext {
        parts.push(format!("\"{component}\": ({fields})"));
    }
    parts.extend(extra.iter().cloned());
    format!("{{{}}}", parts.join(", "))
}

fn dedup_last(fields: &[Field]) -> impl Iterator<Item = &Field> {
    fields.iter().enumerate().filter_map(move |(i, f)| {
        let same = |g: &Field| match (g, f) {
            (
                Field::Ext {
                    component: c1,
                    key: k1,
                    ..
                },
                Field::Ext {
                    component: c2,
                    key: k2,
                    ..
                },
            ) => c1 == c2 && k1 == k2,
            _ => std::mem::discriminant(g) == std::mem::discriminant(f),
        };
        (!fields[i + 1..].iter().any(same)).then_some(f)
    })
}

fn ron_str(s: &str) -> String {
    ron::to_string(s).expect("strings always serialize")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        assert_eq!(parse_color("#ff8000"), Some([255, 128, 0, 255]));
        assert_eq!(parse_color("#FF800080"), Some([255, 128, 0, 128]));
        assert_eq!(parse_color("#f80"), Some([255, 136, 0, 255]));
        assert_eq!(parse_color("Red"), Some([255, 0, 0, 255]));
        assert_eq!(parse_color("#12"), None);
        assert_eq!(parse_color("error"), None);
    }

    #[test]
    fn fields_and_patch() {
        assert_eq!(field("dir", Val::Num(4.0)), Ok(Field::Dir(4)));
        assert_eq!(field("density", Val::Null), Ok(Field::Density(false)));
        assert_eq!(field("pixel_x", Val::Num(-7.6)), Ok(Field::PixelX(-8)));
        assert_eq!(field("id", Val::Str("x")), Err(Skip::UnknownVar));
        assert_eq!(field("color", Val::Other), Err(Skip::Unsupported));
        let patch = patch_ron(&[
            Field::Dir(4),
            Field::Name("a \"b\"".into()),
            Field::Dir(8),
            Field::Color(Some([1, 2, 3, 4])),
        ]);
        assert_eq!(
            patch,
            r#"{"dm_name": (name: "a \"b\""), "dm_atom": (dir: 8, color: Some((1, 2, 3, 4)))}"#
        );
        assert_eq!(patch_ron(&[]), "{}");
    }
}

//! tg View Variables (`code/modules/admin/view_variables`) over Bevy reflection, and the subtype
//! list of the tg subtype vendor (`code/modules/vending/subtype.dm`).
//!
//! An entity's components are listed with their top-level reflected fields; a component is
//! reflected when it is registered with `ReflectComponent` (every prototype component is, see
//! `ss13_proto::ProtoAppExt`). Booleans, integers, floats and strings are editable; everything
//! else is shown as its debug text. The target is a [`NetId`] the asking client currently sees,
//! resolved on every request, so a stale or forged id never reaches an entity the admin cannot
//! see. Rights are checked by `server::required_rights` before any of this runs.

use bevy::prelude::*;
use bevy::reflect::{PartialReflect, ReflectMut, ReflectRef};
use ss13_net::server::Interest;
use ss13_proto::Prototypes;
use ss13_shared::admin::{AdminResponse, VvComponent, VvEntity, VvField, VvRef, VvValue};
use ss13_shared::protocol::NetId;
use ss13_world::SpatialIndex;

use crate::server::{AnswerResult, admin_action, mob_pos, own_mob, sanitize_text};

/// Debug text of a non-editable value is cut here (bytes).
const MAX_OTHER_LEN: usize = 200;
/// Text typed into a string field is cut here.
const MAX_TEXT_LEN: usize = 1024;
/// At most this many subtypes are listed (tg lists them all in the vendor).
pub const MAX_SUBTYPES: usize = 300;

fn resolve(world: &World, client: Entity, id: NetId) -> Result<Entity, String> {
    world
        .resource::<Interest>()
        .entity(client, id)
        .filter(|&e| world.get_entity(e).is_ok())
        .ok_or_else(|| "Объект не найден или вне вашего обзора.".into())
}

fn entity_name(world: &World, entity: Entity) -> String {
    world
        .get::<Name>(entity)
        .map(|n| n.as_str().to_owned())
        .unwrap_or_else(|| "(без имени)".to_owned())
}

/// The entities on the own tile that the client sees (tg opens VV from the context menu).
pub fn tile(world: &mut World, client: Entity) -> AnswerResult {
    let world: &World = world;
    let own = own_mob(world, client)?;
    let pos = mob_pos(world, own)?;
    let on_tile: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(pos).to_vec();
    let interest = world.resource::<Interest>();
    let refs: Vec<VvRef> = std::iter::once(own)
        .chain(on_tile.into_iter().filter(|&e| e != own))
        .filter_map(|e| {
            interest.net_id(client, e).map(|id| VvRef {
                id,
                name: entity_name(world, e),
            })
        })
        .collect();
    Ok(Some(AdminResponse::VvTile(refs)))
}

/// tg `debug_variables`.
pub fn view(world: &mut World, client: Entity, id: NetId) -> AnswerResult {
    let entity = resolve(world, client, id)?;
    Ok(Some(AdminResponse::Variables(describe(world, entity, id))))
}

pub(crate) fn describe(world: &World, entity: Entity, id: NetId) -> VvEntity {
    let registry = world.resource::<AppTypeRegistry>().read();
    let mut components = Vec::new();
    if let Ok(infos) = world.inspect_entity(entity) {
        let entity_ref = world.entity(entity);
        for (_, info) in infos {
            let registration = info.type_id().and_then(|t| registry.get(t));
            let reflected = registration.and_then(|reg| {
                reg.data::<ReflectComponent>()
                    .and_then(|rc| rc.reflect(entity_ref))
                    .map(|value| (reg.type_info().type_path().to_owned(), value))
            });
            components.push(match reflected {
                Some((name, value)) => VvComponent {
                    name,
                    reflected: true,
                    fields: fields(value.as_partial_reflect()),
                },
                None => VvComponent {
                    name: info.name().to_string(),
                    reflected: false,
                    fields: Vec::new(),
                },
            });
        }
    }
    components.sort_by(|a, b| b.reflected.cmp(&a.reflected).then(a.name.cmp(&b.name)));
    VvEntity {
        id,
        name: entity_name(world, entity),
        components,
    }
}

fn fields(value: &dyn PartialReflect) -> Vec<VvField> {
    match value.reflect_ref() {
        ReflectRef::Struct(s) => (0..s.field_len())
            .filter_map(|i| {
                Some(VvField {
                    name: s.name_at(i)?.to_owned(),
                    value: read(s.field_at(i)?),
                })
            })
            .collect(),
        ReflectRef::TupleStruct(t) => (0..t.field_len())
            .filter_map(|i| {
                Some(VvField {
                    name: i.to_string(),
                    value: read(t.field(i)?),
                })
            })
            .collect(),
        _ => vec![VvField {
            name: String::new(),
            value: read(value),
        }],
    }
}

// `i64::try_from` is infallible for the narrow types: the macro covers all of them alike.
macro_rules! read_ints {
    ($value:ident, $($t:ty),*) => {
        $(if let Some(v) = $value.try_downcast_ref::<$t>() {
            #[allow(irrefutable_let_patterns)]
            if let Ok(v) = i64::try_from(*v) {
                return VvValue::Int(v);
            }
        })*
    };
}

macro_rules! write_ints {
    ($field:ident, $v:ident, $($t:ty),*) => {
        $(if let Some(slot) = $field.try_downcast_mut::<$t>() {
            *slot = <$t>::try_from($v).map_err(|_| format!("{} вне диапазона {}.", $v, stringify!($t)))?;
            return Ok(());
        })*
    };
}

/// A field's value; [`VvValue::Other`] for everything not editable.
pub(crate) fn read(value: &dyn PartialReflect) -> VvValue {
    if let Some(v) = value.try_downcast_ref::<bool>() {
        return VvValue::Bool(*v);
    }
    read_ints!(value, i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);
    if let Some(v) = value.try_downcast_ref::<f32>() {
        return VvValue::Float(f64::from(*v));
    }
    if let Some(v) = value.try_downcast_ref::<f64>() {
        return VvValue::Float(*v);
    }
    if let Some(v) = value.try_downcast_ref::<String>() {
        return VvValue::Text(v.clone());
    }
    let mut text = format!("{value:?}");
    if text.len() > MAX_OTHER_LEN {
        let mut end = MAX_OTHER_LEN;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push('…');
    }
    VvValue::Other(text)
}

/// Writes `value` into a simple field of the same kind (an integer also into a float field).
pub(crate) fn write(field: &mut dyn PartialReflect, value: &VvValue) -> Result<(), String> {
    match value {
        VvValue::Bool(v) => {
            if let Some(slot) = field.try_downcast_mut::<bool>() {
                *slot = *v;
                return Ok(());
            }
        }
        &VvValue::Int(v) => {
            write_ints!(field, v, i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);
            return write(field, &VvValue::Float(v as f64));
        }
        &VvValue::Float(v) => {
            if !v.is_finite() {
                return Err("Не число.".into());
            }
            if let Some(slot) = field.try_downcast_mut::<f32>() {
                if v.abs() > f64::from(f32::MAX) {
                    return Err(format!("{v} вне диапазона f32."));
                }
                *slot = v as f32;
                return Ok(());
            }
            if let Some(slot) = field.try_downcast_mut::<f64>() {
                *slot = v;
                return Ok(());
            }
        }
        VvValue::Text(v) => {
            if let Some(slot) = field.try_downcast_mut::<String>() {
                *slot = sanitize_text(v, MAX_TEXT_LEN);
                return Ok(());
            }
        }
        VvValue::Other(_) => return Err("Это значение не редактируется.".into()),
    }
    Err("Тип значения не совпадает с типом поля.".into())
}

fn field_mut<'a>(
    value: &'a mut dyn PartialReflect,
    name: &str,
) -> Option<&'a mut dyn PartialReflect> {
    let has_fields = matches!(
        value.reflect_ref(),
        ReflectRef::Struct(_) | ReflectRef::TupleStruct(_)
    );
    if !has_fields {
        // A value component (an enum, an opaque type) is its own single unnamed field.
        return name.is_empty().then_some(value);
    }
    match value.reflect_mut() {
        ReflectMut::Struct(s) => s.field_mut(name),
        ReflectMut::TupleStruct(t) => t.field_mut(name.parse().ok()?),
        _ => None,
    }
}

/// tg `modify_variables`: one top-level field of a reflected component.
pub fn edit(
    world: &mut World,
    client: Entity,
    key: &str,
    id: NetId,
    component: &str,
    field: &str,
    value: &VvValue,
) -> AnswerResult {
    let entity = resolve(world, client, id)?;
    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    let registration = registry
        .get_with_type_path(component)
        .ok_or_else(|| format!("{component}: нет такого компонента."))?;
    let reflect_component = registration
        .data::<ReflectComponent>()
        .ok_or_else(|| format!("{component}: не отражается."))?;
    let mutable = world
        .components()
        .get_id(registration.type_id())
        .and_then(|cid| world.components().get_info(cid))
        .is_some_and(|info| info.mutable());
    if !mutable {
        return Err(format!("{component}: неизменяемый компонент."));
    }
    let mut entity_mut = world.entity_mut(entity);
    let mut reflected = reflect_component
        .reflect_mut(&mut entity_mut)
        .ok_or_else(|| format!("{component}: нет у объекта."))?;
    let slot = field_mut(reflected.as_partial_reflect_mut(), field)
        .ok_or_else(|| format!("{component}: нет поля «{field}»."))?;
    write(slot, value)?;
    let shown = match value {
        VvValue::Text(t) => format!("{:?}", sanitize_text(t, MAX_TEXT_LEN)),
        other => format!("{other:?}"),
    };
    let name = entity_name(world, entity);
    admin_action(
        world,
        &format!("{key} modified {name}'s {component}.{field} to {shown}"),
    );
    Ok(Some(AdminResponse::Variables(describe(world, entity, id))))
}

/// The `/obj` prototypes that are `path` or inherit from it (by the DM path or the prototype
/// parent chain), shortest first.
pub fn subtypes(prototypes: &Prototypes, path: &str) -> Vec<String> {
    let path = path.trim_end_matches('/');
    let prefix = format!("{path}/");
    let mut ids: Vec<String> = prototypes
        .ids()
        .filter(|id| id.starts_with("/obj"))
        .filter(|id| {
            if id.starts_with(&prefix) {
                return true;
            }
            let mut current = Some(*id);
            // Bounded: a parent cycle is a load error, but never loop on one anyway.
            for _ in 0..64 {
                match current {
                    Some(c) if c == path => return true,
                    Some(c) => current = prototypes.get(c).and_then(|p| p.parent()),
                    None => return false,
                }
            }
            false
        })
        .map(str::to_owned)
        .collect();
    ids.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    ids.truncate(MAX_SUBTYPES);
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Reflect, Default)]
    struct Sample {
        on: bool,
        count: u8,
        speed: f32,
        label: String,
        list: Vec<u32>,
    }

    #[test]
    fn fields_read_and_write_simple_values_only() {
        let mut sample = Sample {
            label: "a".into(),
            list: vec![1],
            ..default()
        };
        let read_all = fields(sample.as_partial_reflect());
        assert_eq!(read_all.len(), 5);
        assert_eq!(read_all[1].value, VvValue::Int(0));
        assert!(matches!(read_all[4].value, VvValue::Other(_)));

        let s = sample.as_partial_reflect_mut();
        write(field_mut(s, "on").unwrap(), &VvValue::Bool(true)).unwrap();
        write(field_mut(s, "count").unwrap(), &VvValue::Int(200)).unwrap();
        assert!(write(field_mut(s, "count").unwrap(), &VvValue::Int(300)).is_err());
        assert!(write(field_mut(s, "count").unwrap(), &VvValue::Text("x".into())).is_err());
        write(field_mut(s, "speed").unwrap(), &VvValue::Int(2)).unwrap();
        assert!(write(field_mut(s, "speed").unwrap(), &VvValue::Float(f64::NAN)).is_err());
        write(field_mut(s, "label").unwrap(), &VvValue::Text("b".into())).unwrap();
        assert!(write(field_mut(s, "list").unwrap(), &VvValue::Other("[]".into())).is_err());
        assert!(field_mut(s, "missing").is_none());
        assert!(sample.on && sample.count == 200 && sample.speed == 2.0 && sample.label == "b");
    }
}

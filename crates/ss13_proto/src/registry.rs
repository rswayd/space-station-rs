use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::reflect::{
    GetTypeRegistration, PartialReflect, ReflectFromReflect, ReflectRef, TypeRegistry,
};

use crate::ErrorKind;

/// What a component type needs to be carried by prototypes. Derive `Component, Reflect, Default,
/// Clone`. Do not add `#[reflect(Deserialize)]`: it makes reflection deserialize the whole struct
/// through serde, which turns field overrides into whole-component replacement.
pub trait ProtoComponent:
    Component + Reflect + FromReflect + TypePath + GetTypeRegistration + Default + Clone
{
}

impl<T> ProtoComponent for T where
    T: Component + Reflect + FromReflect + TypePath + GetTypeRegistration + Default + Clone
{
}

pub(crate) type MergeFn = fn(
    Option<&dyn Reflect>,
    &dyn PartialReflect,
    &TypeRegistry,
) -> Result<Arc<dyn Reflect>, ErrorKind>;
pub(crate) type InsertFn = fn(&dyn Reflect, &mut EntityWorldMut);

/// Monomorphized per component type, so spawning is a typed clone + insert instead of a
/// reflection walk per field.
#[derive(Clone, Copy)]
pub(crate) struct ComponentVTable {
    pub type_id: TypeId,
    pub merge: MergeFn,
    pub insert: InsertFn,
}

/// Component names prototypes may use. Filled by
/// [`ProtoAppExt::register_proto_component`](crate::ProtoAppExt::register_proto_component).
#[derive(Resource, Default, Clone)]
pub struct ProtoComponents {
    by_name: HashMap<String, ComponentVTable>,
}

impl ProtoComponents {
    /// `T` must also be registered in the type registry the prototypes are loaded with
    /// (`App::register_type`); the app extension does both.
    pub fn register<T: ProtoComponent>(&mut self, name: &str) {
        let type_id = TypeId::of::<T>();
        if let Some((other, _)) = self.by_name.iter().find(|(_, v)| v.type_id == type_id) {
            panic!(
                "{} is already a prototype component as `{other}`",
                T::type_path()
            );
        }
        let previous = self.by_name.insert(
            name.to_owned(),
            ComponentVTable {
                type_id,
                merge: merge::<T>,
                insert: insert::<T>,
            },
        );
        assert!(
            previous.is_none(),
            "prototype component name `{name}` is registered twice"
        );
    }

    pub(crate) fn get(&self, name: &str) -> Option<&ComponentVTable> {
        self.by_name.get(name)
    }
}

fn merge<T: ProtoComponent>(
    base: Option<&dyn Reflect>,
    patch: &dyn PartialReflect,
    registry: &TypeRegistry,
) -> Result<Arc<dyn Reflect>, ErrorKind> {
    let ReflectRef::Struct(fields) = patch.reflect_ref() else {
        return T::from_reflect(patch)
            .map(|v| Arc::new(v) as Arc<dyn Reflect>)
            .ok_or(ErrorKind::Incomplete(None));
    };
    let base = base
        .and_then(|b| b.downcast_ref::<T>())
        .cloned()
        .unwrap_or_default();
    let ReflectRef::Struct(base_fields) = base.reflect_ref() else {
        unreachable!("a struct patch is only deserialized for a struct type");
    };
    let bad = |e: bevy::reflect::ReflectCloneError| ErrorKind::BadValue(e.to_string());
    let mut merged = base_fields.to_dynamic_struct().map_err(bad)?;
    for (name, value) in fields.iter_fields() {
        merged.insert_boxed(name, value.to_dynamic().map_err(bad)?);
    }
    T::from_reflect(&merged)
        .map(|v| Arc::new(v) as Arc<dyn Reflect>)
        .ok_or_else(|| ErrorKind::Incomplete(incomplete_field(fields, registry)))
}

/// Error path only: finds the overridden field that does not convert to its concrete type.
fn incomplete_field(
    fields: &dyn bevy::reflect::structs::Struct,
    registry: &TypeRegistry,
) -> Option<String> {
    fields.iter_fields().find_map(|(name, value)| {
        let from_reflect = registry
            .get_type_data::<ReflectFromReflect>(value.get_represented_type_info()?.type_id())?;
        from_reflect
            .from_reflect(value)
            .is_none()
            .then(|| name.to_owned())
    })
}

fn insert<T: ProtoComponent>(value: &dyn Reflect, entity: &mut EntityWorldMut) {
    let value = value
        .downcast_ref::<T>()
        .expect("resolved by the vtable of the same type");
    entity.insert(value.clone());
}

//! Entity prototypes: data-driven entity templates, the replacement for DM type paths with
//! var overrides.
//!
//! # Format
//!
//! Every `*.ron` file under the prototypes directory (recursively) holds a list of prototypes:
//!
//! ```ron
//! [
//!     (
//!         id: "door",
//!         components: {
//!             "density": (true),
//!             "door": (open: false, open_time: 1.0),
//!         },
//!     ),
//!     (
//!         id: "airlock",
//!         parent: "door",
//!         components: {
//!             "door": (open_time: 0.5),
//!         },
//!     ),
//! ]
//! ```
//!
//! Ids are opaque, globally unique strings; the parent is named explicitly instead of being
//! derived from the id, so renaming or moving a prototype never re-parents others.
//! Hand-written prototypes use `snake_case` ids; generated ones may use DM paths verbatim
//! (`"/obj/machinery/door/airlock"`), which keeps the `.dmm` type -> prototype mapping trivial.
//! Component names are the names given to [`ProtoAppExt::register_proto_component`].
//!
//! # Merge rule
//!
//! One rule for inheritance and for per-instance overrides:
//! - a child starts with every component of its parent;
//! - for a component with named fields, a child sets only the fields it lists; every other field
//!   keeps the parent's value (or `Default` if the parent does not have the component);
//! - a listed field is replaced as a whole: lists, maps, options and nested structs are not merged
//!   (DM var overrides replace the value too), so a nested struct must be written in full;
//! - a component without named fields (tuple struct, enum, value) is replaced as a whole.
//!
//! Inheritance is resolved once at load; spawning clones the already resolved values.

mod error;
mod load;
mod registry;

use std::path::PathBuf;
use std::sync::Arc;

use bevy::prelude::*;

pub use error::{ErrorKind, LoadError, ProtoError};
pub use load::{Prototype, Prototypes};
pub use registry::{ProtoComponent, ProtoComponents};

/// Loads every prototype from `dir` when the app finishes building, so plugins register their
/// components in `build` and systems (including `Startup`) see a complete [`Prototypes`].
pub struct ProtoPlugin {
    pub dir: PathBuf,
}

impl Default for ProtoPlugin {
    fn default() -> Self {
        Self {
            dir: PathBuf::from("prototypes"),
        }
    }
}

impl Plugin for ProtoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ProtoComponents>();
    }

    fn finish(&self, app: &mut App) {
        let world = app.world();
        // A server without valid prototypes cannot build a station: fail at startup with every
        // error listed instead of spawning half-configured entities later.
        let prototypes = Prototypes::load_dir(
            &self.dir,
            world.resource::<ProtoComponents>(),
            world.resource::<AppTypeRegistry>(),
        )
        .unwrap_or_else(|e| panic!("prototypes in {} are invalid:\n{e}", self.dir.display()));
        app.insert_resource(prototypes);
    }
}

pub trait ProtoAppExt {
    /// Makes `T` available to prototypes under `name`. Call from `Plugin::build`.
    fn register_proto_component<T: ProtoComponent>(&mut self, name: &str) -> &mut Self;
}

impl ProtoAppExt for App {
    fn register_proto_component<T: ProtoComponent>(&mut self, name: &str) -> &mut Self {
        // `ReflectComponent` lets the admin View Variables and the debug inspector read and edit
        // the component through the type registry.
        self.register_type::<T>()
            .register_type_data::<T, ReflectComponent>();
        self.world_mut()
            .get_resource_or_init::<ProtoComponents>()
            .register::<T>(name);
        self
    }
}

pub trait ProtoWorldExt {
    /// Spawns `id` with optional per-instance `overrides` (same syntax as a prototype's
    /// `components` map). `extra` is inserted after the prototype's components, so observers on
    /// it (e.g. `TilePos` indexing) see a complete entity.
    fn spawn_proto(
        &mut self,
        id: &str,
        overrides: Option<&str>,
        extra: impl Bundle,
    ) -> Result<Entity, ProtoError>;
}

impl ProtoWorldExt for World {
    fn spawn_proto(
        &mut self,
        id: &str,
        overrides: Option<&str>,
        extra: impl Bundle,
    ) -> Result<Entity, ProtoError> {
        let proto = self.resource::<Prototypes>().instance(id, overrides)?;
        Ok(proto.spawn(self, extra).id())
    }
}

pub trait ProtoCommandsExt {
    /// Deferred [`ProtoWorldExt::spawn_proto`]. On an unknown id or bad overrides the entity is
    /// despawned and the error goes to the command error handler.
    fn spawn_proto(
        &mut self,
        id: impl Into<String>,
        overrides: Option<String>,
        extra: impl Bundle,
    ) -> Entity;
}

impl ProtoCommandsExt for Commands<'_, '_> {
    fn spawn_proto(
        &mut self,
        id: impl Into<String>,
        overrides: Option<String>,
        extra: impl Bundle,
    ) -> Entity {
        let entity = self.spawn_empty().id();
        let id = id.into();
        self.queue(move |world: &mut World| -> Result<(), ProtoError> {
            let proto: Arc<Prototype> = match world
                .resource::<Prototypes>()
                .instance(&id, overrides.as_deref())
            {
                Ok(proto) => proto,
                Err(e) => {
                    world.despawn(entity);
                    return Err(e);
                }
            };
            if let Ok(mut e) = world.get_entity_mut(entity) {
                proto.insert_into(&mut e);
                e.insert(extra);
            }
            Ok(())
        });
        entity
    }
}

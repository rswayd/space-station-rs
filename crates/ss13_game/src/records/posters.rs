//! Wanted and missing posters (server) after tg `code/game/objects/effects/wanted_poster.dm`:
//! the security console prints a rolled `/obj/item/poster/wanted` (or `.../missing`) holding a
//! `/obj/structure/sign/poster/wanted` with the record's front photo, the name and the text; it
//! hangs on a wall and comes off like any poster (`crate::decor`).
// shortcut: the sprite is the plain `wanted_background` (tg blends the photo and the header
// letters into the icon); the photo shows in the photo window when the poster is examined up
// close or used in hand (`crate::service::Photo` on the poster), the header leads the
// description.

use std::sync::Arc;

use bevy::prelude::*;
use ss13_mapload::{DmName, IconIndex};
use ss13_shared::TilePos;

use crate::decor::RolledPoster;
use crate::service::{Photo, Picture};

pub const WANTED_ITEM: &str = "/obj/item/poster/wanted";
pub const MISSING_ITEM: &str = "/obj/item/poster/wanted/missing";
pub const WANTED_STRUCTURE: &str = "/obj/structure/sign/poster/wanted";
const POSTER_WANTED_DMI: &str = "icons/obj/poster_wanted.dmi";
/// `background` of both kinds.
const BACKGROUND: &str = "wanted_background";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PosterKind {
    Wanted,
    Missing,
}

impl PosterKind {
    /// `postName`, `postDesc`.
    fn names(self) -> (&'static str, &'static str) {
        match self {
            Self::Wanted => ("плакат розыска", "Плакат розыска:"),
            Self::Missing => ("плакат о пропаже", "Плакат о пропаже:"),
        }
    }
}

/// The poster's names: the rolled item's (name, desc) and the hung structure's (name, desc).
pub fn poster_names(
    kind: PosterKind,
    wanted_name: &str,
    description: &str,
    header: &str,
) -> (DmName, DmName) {
    let (post_name, post_desc) = kind.names();
    let name = format!("{post_name} ({wanted_name})");
    (
        DmName {
            name: name.clone(),
            desc: format!("{post_desc} {wanted_name}."),
        },
        DmName {
            name,
            desc: format!("{header}\n{description}"),
        },
    )
}

/// `new /obj/item/poster/wanted(null, null, mugshot, alias, description, header)` at `pos`.
pub fn print_poster(
    world: &mut World,
    pos: TilePos,
    kind: PosterKind,
    photo: Option<Arc<Picture>>,
    wanted_name: &str,
    description: &str,
    header: &str,
) -> Option<Entity> {
    let path = match kind {
        PosterKind::Wanted => WANTED_ITEM,
        PosterKind::Missing => MISSING_ITEM,
    };
    let item = crate::construction::spawn_obj(world, path, pos)?;
    let (item_name, structure_name) = poster_names(kind, wanted_name, description, header);
    // The hung sprite; a resource pack without the dmi keeps the item's own.
    let icon_state = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(POSTER_WANTED_DMI, BACKGROUND))
        .or_else(|| {
            world
                .get::<ss13_shared::Appearance>(item)
                .map(|a| a.icon_state)
        })?;
    world.entity_mut(item).insert((
        Name::new(item_name.name.clone()),
        item_name,
        RolledPoster {
            structure: WANTED_STRUCTURE.to_owned(),
            icon_state,
            name: structure_name,
        },
        Photo {
            picture: photo,
            scribble: String::new(),
        },
    ));
    Some(item)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_kind() {
        let (item, structure) = poster_names(
            PosterKind::Missing,
            "Боб",
            "Последний раз видели в баре.",
            "ПРОПАЛ",
        );
        assert_eq!(item.name, "плакат о пропаже (Боб)");
        assert_eq!(item.desc, "Плакат о пропаже: Боб.");
        assert_eq!(structure.name, item.name);
        assert_eq!(structure.desc, "ПРОПАЛ\nПоследний раз видели в баре.");
        let (item, _) = poster_names(PosterKind::Wanted, "Ева", "", "РОЗЫСК");
        assert_eq!(item.name, "плакат розыска (Ева)");
    }
}

//! Contract between the importer, the server and the client renderer.
//! `IconStateId(n)` is the index `n` in [`AssetManifest::states`].

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ss13_shared::IconStateId;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AssetManifest {
    /// Atlas image files, relative to the generated assets root.
    pub atlases: Vec<String>,
    pub states: Vec<IconStateInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IconStateInfo {
    /// Source `.dmi`, relative to the source repository root (e.g. `icons/obj/doors.dmi`).
    pub dmi: String,
    pub state: String,
    pub atlas: u32,
    /// Frame size in pixels; `.dmi` files are not all 32x32.
    pub width: u32,
    pub height: u32,
    /// 1, 4 or 8, in `Dir::DMI_ORDER`.
    pub dirs: u8,
    /// Rect `[x, y, w, h]` in the atlas for frame `f`, direction index `d`: `rects[f * dirs + d]`.
    /// Pixels, origin at the atlas image's top-left corner, y down (the renderer passes them to
    /// `Sprite::rect` as is).
    pub rects: Vec<[u32; 4]>,
    /// Per-frame delay in deciseconds, as in `.dmi`; empty for a single frame.
    pub delays: Vec<f32>,
    /// 0 = loop forever.
    pub loop_count: u32,
    pub rewind: bool,
    /// BYOND movement state (`movement = 1`), drawn while gliding.
    pub movement: bool,
}

impl AssetManifest {
    pub fn get(&self, id: IconStateId) -> Option<&IconStateInfo> {
        self.states.get(id.0 as usize)
    }

    /// `(dmi, state) -> id` lookup for the importer and prototype loading; not for hot paths.
    /// Like BYOND, the first state with a name wins, and movement states are not addressable by
    /// name (they are the gliding twins of the plain state with the same name).
    pub fn index(&self) -> HashMap<(&str, &str), IconStateId> {
        let mut index = HashMap::new();
        for (i, s) in self.states.iter().enumerate().filter(|(_, s)| !s.movement) {
            index
                .entry((s.dmi.as_str(), s.state.as_str()))
                .or_insert(IconStateId(i as u32));
        }
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(name: &str, movement: bool) -> IconStateInfo {
        IconStateInfo {
            dmi: "a.dmi".into(),
            state: name.into(),
            atlas: 0,
            width: 32,
            height: 32,
            dirs: 1,
            rects: vec![[0, 0, 32, 32]],
            delays: vec![],
            loop_count: 0,
            rewind: false,
            movement,
        }
    }

    #[test]
    fn index_prefers_first_plain_state() {
        let m = AssetManifest {
            atlases: vec![],
            states: vec![
                state("walk", true),
                state("walk", false),
                state("walk", false),
            ],
        };
        assert_eq!(m.index()[&("a.dmi", "walk")], IconStateId(1));
    }
}

//! Client: atlas sprites inside tgui windows and chat emojis. `ss13_tgui` places [`UiSprite`]
//! nodes and `ss13_chat` [`ChatEmoji`] ones (neither knows the atlas); this gives each its
//! `ImageNode` from the icon atlas once, when it appears. The atlas page loads on demand and the
//! image shows when it arrives; the node's handle keeps the page resident while the window shows
//! it. The nanomap images of the camera console and crew monitor are resolved here too
//! (`crate::nanomap`).

use bevy::prelude::*;
use ss13_chat::client::panel::ChatEmoji;
use ss13_render::IconAtlas;
use ss13_render::atlas::frame_rect;
use ss13_shared::Dir;
use ss13_tgui::client::UiSprite;

pub struct UiSpritesPlugin;

impl Plugin for UiSpritesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::nanomap::NanomapImages>()
            .add_observer(crate::nanomap::wheel_zoom)
            .add_observer(crate::nanomap::drag_pan)
            .add_systems(
                PostUpdate,
                (
                    resolve_sprites,
                    resolve_emojis,
                    crate::nanomap::resolve_images,
                    crate::nanomap::fit_maps.before(bevy::ui::UiSystems::Layout),
                ),
            );
    }
}

// shortcut: the base icon's first frame; no overlays or animation in windows.
fn resolve_sprites(
    sprites: Query<(Entity, &UiSprite), Changed<UiSprite>>,
    mut atlas: Option<ResMut<IconAtlas>>,
    mut commands: Commands,
) {
    let Some(atlas) = atlas.as_deref_mut() else {
        return;
    };
    for (entity, sprite) in &sprites {
        let Some((info, image)) = sprite.icon_state.and_then(|id| atlas.state(id)) else {
            continue;
        };
        let color = sprite
            .color
            .map_or(Color::WHITE, |[r, g, b, a]| Color::srgba_u8(r, g, b, a));
        commands.entity(entity).insert(ImageNode {
            image,
            rect: frame_rect(info, sprite.dir.unwrap_or(Dir::South), 0),
            color,
            ..default()
        });
    }
}

/// A chat emoji's first frame.
fn resolve_emojis(
    emojis: Query<(Entity, &ChatEmoji), Changed<ChatEmoji>>,
    mut atlas: Option<ResMut<IconAtlas>>,
    mut commands: Commands,
) {
    let Some(atlas) = atlas.as_deref_mut() else {
        return;
    };
    for (entity, emoji) in &emojis {
        let Some((info, image)) = emoji.icon_state.and_then(|id| atlas.state(id)) else {
            continue;
        };
        commands.entity(entity).insert(ImageNode {
            image,
            rect: frame_rect(info, Dir::South, 0),
            ..default()
        });
    }
}

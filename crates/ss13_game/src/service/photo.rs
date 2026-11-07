//! Cameras, film, photos and picture frames (server), after tg `code/modules/photography`.
//!
//! - A camera (`/obj/item/camera`) clicked on anything it sees, next to the user or far
//!   (`interact_with_atom` -> `ranged_interact_with_atom`), takes a picture of the
//!   `(2 * size - 1)`-tile square around it (`capture_image`): the tiles the user sees, with
//!   what stands on them and a description of the mobs in it (`get_photo_description`). Then the
//!   flash (a short light on the user's tile, `on_flash`), the polaroid sound (only the user
//!   hears a silent camera), 6.4 s until the next one; the photo prints into the hands when film
//!   is left, after the optional name, description and caption (`printpicture`). Alt-click sets
//!   the half width and height (1-4, `adjust_zoom`); a film cartridge reloads an empty camera.
//! - The detective's camera is silent, has no flash and holds 30 pictures; the camera obscura
//!   (`spooky`) also sees ghosts.
//! - A photo used in hand or examined up close opens the photo window; a pen writes on its back.
//!   The photocopier copies photos (`crate::service::office`).
//! - A picture frame takes a photo and shows it (as an item, in hand; on the wall, by hand or
//!   examine). Wirecutters cut the photo out of a wall frame, a screwdriver or wrench takes the
//!   frame off the wall (3 s) with its photo; the frame item hangs like the other wall frames
//!   (`crate::wallmount::frames`). Portraits show one of four pictures until a photo goes in.
//!
//! A picture keeps no pixels (tg flattens the turfs into an icon): it is the list of sprites the
//! camera saw, each base sprite and overlay with its facing, colour and pixel position, in draw
//! order; the client draws them in the photo window like the map. That is the replicated
//! `Appearance` as is, so a photo costs a few hundred small entries and no image pipeline.
// shortcut: no multi-z photos (the level below open space), no lighting or darkness, no turned
// (lying) sprites, monochrome prints or greyscale copies, no photo icon on the item itself (the
// plain `photo` sprite), no blueprints flag, holodisks, circuit shells, renaming verb or soul
// stealing of spirits (no mob biotypes for basic mobs); camera apps, the traitor's flash camera
// and the AI's and cyborgs' cameras (silicon group) are not cameras here; frames keep no photos
// between rounds and give no art mood (persistence, done last).

use std::sync::Arc;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmItem, DmName, Opaque};
use ss13_net::server::{Invisibility, Networked};
use ss13_shared::chat::Span;
use ss13_shared::line_of_sight::ViewGrid;
use ss13_shared::{
    Appearance, Dir, EMISSIVE_PLANE, Facing, IconStateId, LightSource, Optics, Overlay, TILE_SIZE,
    TilePos,
};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptKind, ask_world};
use ss13_tgui::server::{AlwaysState, OpenUi, ServerUi};
use ss13_world::SpatialIndex;

use super::server::{
    PendingAsk, TimerJob, after, client_of, item_path, now, refresh_windows, set_icon_state,
    set_name,
};
use super::{PhotoData, PhotoSprite, PhotoUi};
use crate::chem::{map_pos, name_of};
use crate::health::Health;
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::storage::{put_inside, take_out};
use crate::tools::{Actor, ToolKind, tell, tool_of, use_tool};

const CAMERA: &str = "/obj/item/camera";
const DETECTIVE: &str = "/obj/item/camera/detective";
const SPOOKY: &str = "/obj/item/camera/spooky";
/// Subtypes that are not hand cameras here (see the shortcut).
const NOT_CAMERAS: [&str; 3] = [
    "/obj/item/camera/siliconcam",
    "/obj/item/camera/app",
    "/obj/item/camera/flash",
];
const FILM: &str = "/obj/item/camera_film";
pub(super) const PHOTO: &str = "/obj/item/photo";
const FRAME_ITEM: &str = "/obj/item/wallframe/picture";
const FRAME: &str = "/obj/structure/sign/picture_frame";
const SHOWROOM: &str = "/obj/structure/sign/picture_frame/showroom";
const PORTRAIT: &str = "/obj/structure/sign/picture_frame/portrait";
/// `cooldown`.
const COOLDOWN_SECS: f64 = 6.4;
/// `FLASH_LIGHT_DURATION` (2 deciseconds), `light_range`, `FLASH_LIGHT_POWER`.
const FLASH_SECS: f64 = 0.2;
const FLASH_RANGE: f32 = 6.0;
const FLASH_POWER: f32 = 2.0;
/// `CAMERA_PICTURE_SIZE_HARD_LIMIT`.
const MAX_HALF_SIZE: u8 = 4;
/// `picture_size_x`, `picture_size_y`.
const DEFAULT_HALF_SIZE: u8 = 2;
/// tg `world.view` 9 (the widest side of a client's view): how far a picture is taken.
const VIEW_RADIUS: i32 = 9;
/// tg `TRANSPARENT_FLOOR_PLANE`: open space, never drawn.
const TRANSPARENT_FLOOR_PLANE: i16 = -10;
const CLICK_SOUND: &str = "sound/machines/click.ogg";
const POLAROID: &str = "polaroid";
const DECONSTRUCT_SOUND: &str = "sound/items/deconstruct.ogg";
/// `try_deconstruct`: `use_tool(src, user, 3 SECONDS)`.
const UNSECURE_DS: f32 = 30.0;
/// `/mob/dead/observer` `photo_description`.
const GHOST_LINE: &str = "Также на фото виден п-п-п-п-призра-а-ак!";
/// `/obj/structure/sign/picture_frame` `desc`, which a portrait takes with a photo in it.
const FRAME_DESC: &str = "Каждый раз, когда смотришь на неё, становится смешно.";
/// `portrait/Initialize`: name, `icon_state`, desc.
const PORTRAITS: [(&str, &str, &str); 4] = [
    (
        "портрет мистера Димписи",
        "frame-monkey",
        "Под портретом табличка: «Пусть мясорубка тебя и не пощадила, не бойся. Ни одна твоя часть не пропала даром... Ты был восхитителен.»",
    ),
    (
        "картина с фруктом",
        "frame-fruit",
        "Ceci n'est pas une orange.",
    ),
    (
        "портрет Тома",
        "frame-rat",
        "Кот Джерри до сих пор не в восторге.",
    ),
    (
        "портрет заточённого бога",
        "frame-ratvar",
        "Под портретом табличка: «В светлую память о Ратваре, древней могущественной сущности и сопернике Нар'Си, в конце концов сражённом блюспейс-артиллерией NT руками экипажа Аванпоста 17. Покойся в ржавчине.»",
    ),
];

/// `/obj/item/camera`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Camera {
    pub pictures_left: u32,
    pub pictures_max: u32,
    /// `picture_size_x`, `picture_size_y`: half the side, 1..=4.
    pub size_x: u8,
    pub size_y: u8,
    /// `flash_enabled`.
    pub flash: bool,
    pub silent: bool,
    /// `see_ghosts`.
    pub see_ghosts: bool,
    /// `on`: game seconds when it may take the next picture.
    pub ready_at: f64,
}

impl Camera {
    fn of(path: &str) -> Self {
        let detective = is_under(path, DETECTIVE);
        let pictures = if detective { 30 } else { 10 };
        Self {
            pictures_left: pictures,
            pictures_max: pictures,
            size_x: DEFAULT_HALF_SIZE,
            size_y: DEFAULT_HALF_SIZE,
            flash: !detective,
            silent: detective,
            see_ghosts: is_under(path, SPOOKY),
            ready_at: 0.0,
        }
    }
}

/// `/datum/picture`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Picture {
    /// `picture_name`, empty when not set.
    pub name: String,
    pub desc: String,
    /// Tiles.
    pub width: u8,
    pub height: u8,
    pub sprites: Vec<PhotoSprite>,
}

/// `/obj/item/photo`: its picture (none on a blank photo) and `scribble`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Photo {
    pub picture: Option<Arc<Picture>>,
    pub scribble: String,
}

/// `/obj/item/wallframe/picture` (`displayed`) and `/obj/structure/sign/picture_frame`
/// (`framed`, `can_decon`); a portrait's own look while it holds no photo.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct PictureFrame {
    pub photo: Option<Entity>,
    pub can_decon: bool,
    /// The frame's name and desc, and the portrait's name, icon state and desc.
    portrait: Option<Portrait>,
    /// The photo sprite overlaid on the frame.
    shown: Option<IconStateId>,
}

impl Default for PictureFrame {
    fn default() -> Self {
        Self {
            photo: None,
            can_decon: true,
            portrait: None,
            shown: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Portrait {
    frame_name: String,
    name: String,
    icon_state: String,
    desc: String,
}

/// The questions of cameras and photos; a picture waiting for its print rides along.
#[derive(Clone, Debug)]
pub enum PhotoAsk {
    /// "Do you want to customize the photo?"
    Customise {
        camera: Entity,
        picture: Box<Picture>,
    },
    Name {
        camera: Entity,
        picture: Box<Picture>,
    },
    Desc {
        camera: Entity,
        picture: Box<Picture>,
    },
    Caption {
        camera: Entity,
        picture: Box<Picture>,
    },
    ZoomX {
        camera: Entity,
    },
    ZoomY {
        camera: Entity,
        x: u8,
    },
    /// A pen on a photo.
    Scribble {
        photo: Entity,
        pen: Entity,
    },
}

/// A timed step on a picture frame (`crate::construction::Job::Photo`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhotoJob {
    /// `try_deconstruct`: the frame off the wall.
    Unsecure,
}

pub(super) fn add_parts(world: &mut World, e: Entity, path: &str) {
    if is_under(path, CAMERA) && !NOT_CAMERAS.iter().any(|p| is_under(path, p)) {
        if world.get::<Camera>(e).is_none() {
            world.entity_mut(e).insert(Camera::of(path));
        }
    } else if is_under(path, PHOTO) {
        // A camera or copier may have printed on it before this deferred `Initialize` ran.
        if world.get::<Photo>(e).is_none() {
            world.entity_mut(e).insert(Photo::default());
        }
        world.entity_mut(e).insert(AlwaysState);
    } else if is_under(path, FRAME_ITEM) && world.get::<PictureFrame>(e).is_none() {
        world.entity_mut(e).insert(PictureFrame::default());
    }
}

pub(super) fn add_obj_parts(world: &mut World, e: Entity, path: &str) {
    if !is_under(path, FRAME) || world.get::<PictureFrame>(e).is_some() {
        return;
    }
    let portrait = is_under(path, PORTRAIT).then(|| {
        let pick = world
            .get_resource_mut::<crate::combat::CombatRng>()
            .map_or(0, |mut rng| {
                rng.range(0, PORTRAITS.len() as u32 - 1) as usize
            });
        let (name, icon_state, desc) = PORTRAITS[pick];
        Portrait {
            frame_name: name_of(world, e),
            name: name.to_owned(),
            icon_state: icon_state.to_owned(),
            desc: desc.to_owned(),
        }
    });
    world.entity_mut(e).insert(PictureFrame {
        can_decon: portrait.is_none() && !is_under(path, SHOWROOM),
        portrait,
        ..PictureFrame::default()
    });
    refresh_frame(world, e);
}

// --- Taking pictures ----------------------------------------------------------------------

/// `capture_image` from `eye` at `target`: the `(2 * half - 1)`-tile square around it, of the
/// tiles `eye` sees (`view()` through opacity, as far as the client view plus the picture).
/// `None` when the target tile is out of sight.
pub fn capture(
    world: &World,
    eye: TilePos,
    target: TilePos,
    half: (u8, u8),
    see_ghosts: bool,
) -> Option<Picture> {
    let (hx, hy) = (
        i32::from(half.0.clamp(1, MAX_HALF_SIZE)) - 1,
        i32::from(half.1.clamp(1, MAX_HALF_SIZE)) - 1,
    );
    if eye.chebyshev(target).is_none_or(|d| d > VIEW_RADIUS as u32) {
        return None;
    }
    let index = world.resource::<SpatialIndex>();
    let mut grid = ViewGrid::default();
    grid.compute(VIEW_RADIUS + hx.max(hy), |dx, dy| {
        index
            .on_tile(TilePos::new(eye.x + dx, eye.y + dy, eye.z))
            .iter()
            .any(|&e| world.get::<Opaque>(e).is_some())
    });
    let seen = |p: TilePos| grid.visible(p.x - eye.x, p.y - eye.y);
    if !seen(target) {
        return None;
    }
    let (width, height) = (2 * hx + 1, 2 * hy + 1);
    let mut drawn: Vec<(DrawKey, PhotoSprite)> = Vec::new();
    let mut mobs = Vec::new();
    for ty in 0..height {
        for tx in 0..width {
            let pos = TilePos::new(target.x - hx + tx, target.y - hy + ty, target.z);
            if !seen(pos) {
                continue;
            }
            for &e in index.on_tile(pos) {
                let ghost = world.get::<crate::ghost::Ghost>(e).is_some();
                let hidden = world.get::<Invisibility>(e).is_some_and(|i| i.0 > 0);
                if world.get::<Networked>(e).is_none() || (hidden && !(see_ghosts && ghost)) {
                    continue;
                }
                let Some(look) = world.get::<Appearance>(e) else {
                    continue;
                };
                if ghost || world.get::<Health>(e).is_some() {
                    mobs.push(e);
                }
                let facing = world.get::<Facing>(e).map_or(Dir::South, |f| f.0);
                let at = [
                    (tx * TILE_SIZE as i32) as i16,
                    (ty * TILE_SIZE as i32) as i16,
                ];
                sprites_of(look, facing, at, drawn.len(), &mut drawn);
            }
        }
    }
    drawn.sort_by(|(a, _), (b, _)| a.cmp(b));
    let mut desc = vec![area_line(width, height)];
    desc.extend(mobs.iter().map(|&mob| mob_line(world, mob)));
    Some(Picture {
        name: String::new(),
        desc: desc.join(" "),
        width: width as u8,
        height: height as u8,
        sprites: drawn.into_iter().map(|(_, s)| s).collect(),
    })
}

/// Draw order: plane, layer, then the atom's place in the scan and the overlay's in its list.
#[derive(Clone, Copy, Debug, PartialEq)]
struct DrawKey {
    plane: i16,
    layer: f32,
    atom: usize,
    overlay: usize,
}

impl Eq for DrawKey {}

impl PartialOrd for DrawKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DrawKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.plane
            .cmp(&other.plane)
            .then(self.layer.total_cmp(&other.layer))
            .then(self.atom.cmp(&other.atom))
            .then(self.overlay.cmp(&other.overlay))
    }
}

/// One atom's base sprite and overlays as the map draws them (`ss13_render::sprites`): an
/// overlay takes the parent's facing and colour unless it has its own (the parent's alpha
/// multiplied in), is shifted by the parent's offset, and is left out on facings it is not drawn
/// for; emissive masks are not pictures. `atom` keeps one atom's sprites together.
fn sprites_of(
    look: &Appearance,
    facing: Dir,
    at: [i16; 2],
    atom: usize,
    out: &mut Vec<(DrawKey, PhotoSprite)>,
) {
    if look.plane == TRANSPARENT_FLOOR_PLANE {
        return;
    }
    let [x, y] = [at[0] + look.pixel_offset[0], at[1] + look.pixel_offset[1]];
    let key = |plane: i16, overlay: usize| DrawKey {
        plane,
        layer: look.layer.0,
        atom,
        overlay,
    };
    out.push((
        key(look.plane, 0),
        PhotoSprite {
            icon_state: look.icon_state,
            dir: facing,
            color: look.color,
            x,
            y,
        },
    ));
    for (i, overlay) in look.overlays.iter().enumerate() {
        if overlay.plane == Some(EMISSIVE_PLANE) || !overlay.shown_for(facing) {
            continue;
        }
        let color = match (overlay.color, look.color) {
            (Some([r, g, b, a]), Some([.., pa])) => {
                Some([r, g, b, (u16::from(a) * u16::from(pa) / 255) as u8])
            }
            (own, parent) => own.or(parent),
        };
        out.push((
            key(overlay.plane.unwrap_or(look.plane), i + 1),
            PhotoSprite {
                icon_state: overlay.icon_state,
                dir: overlay.dir.unwrap_or(facing),
                color,
                x: x + overlay.pixel_offset[0],
                y: y + overlay.pixel_offset[1],
            },
        ));
    }
}

/// "This is a photo of an area of [width] meters by [height] meters."
fn area_line(width: i32, height: i32) -> String {
    let meters = match height {
        1 => "метр",
        2..=4 => "метра",
        _ => "метров",
    };
    format!("Это фотография области размером {width} на {height} {meters}.")
}

/// `get_photo_description` of a mob in the picture: a ghost's line, or `/mob/living`'s name,
/// hurt look and held items.
fn mob_line(world: &World, mob: Entity) -> String {
    if world.get::<crate::ghost::Ghost>(mob).is_some() {
        return GHOST_LINE.to_owned();
    }
    let name =
        crate::clothing::cover::hidden_face_name(world, mob).unwrap_or_else(|| name_of(world, mob));
    let hurt = world
        .get::<Health>(mob)
        .is_some_and(|h| h.health() < h.max * 0.75);
    let held: Vec<String> = world
        .get::<Inventory>(mob)
        .map(|inv| {
            inv.hands
                .iter()
                .flatten()
                .map(|&item| name_of(world, item))
                .collect()
        })
        .unwrap_or_default();
    let mut line = format!(
        "На фотографии также имеется {name}{}",
        if hurt {
            ", и выглядит немного ранено"
        } else {
            ""
        }
    );
    if held.is_empty() {
        line.push('.');
    } else {
        // `ru_p_they(TRUE)` of a male mob, as the examine text.
        line.push_str(&format!(". Он держит {}.", held.join(", и ")));
    }
    line
}

/// `attempt_picture` + `capture_image` + `after_picture`: a held camera used on `target`.
pub(super) fn snap(
    world: &mut World,
    client: Entity,
    mob: Entity,
    camera: Entity,
    target: Entity,
) -> bool {
    let Some(state) = world.get::<Camera>(camera).cloned() else {
        return false;
    };
    let t = now(world);
    if t < state.ready_at {
        crate::balloon::balloon_alert(world, mob, mob, "вспышка заряжается!");
        return true;
    }
    let (Some(eye), Some(at)) = (world.get::<TilePos>(mob).copied(), map_pos(world, target)) else {
        return true;
    };
    let Some(picture) = capture(
        world,
        eye,
        at,
        (state.size_x, state.size_y),
        state.see_ghosts,
    ) else {
        return true;
    };
    // `on_flash`.
    if let Some(mut cam) = world.get_mut::<Camera>(camera) {
        cam.ready_at = t + COOLDOWN_SECS;
    }
    set_camera_icon(world, camera, "camera_off");
    after(world, COOLDOWN_SECS, TimerJob::CameraReady { camera });
    if state.flash {
        let light = world
            .spawn((
                Name::new("camera flash"),
                eye,
                Optics {
                    light: Some(LightSource {
                        range: FLASH_RANGE,
                        power: FLASH_POWER,
                        color: [255, 255, 255],
                        height: 1.0,
                        angle: 360,
                        dir: Dir::South,
                        offset: [0, 0],
                    }),
                    ..default()
                },
                Networked,
            ))
            .id();
        after(world, FLASH_SECS, TimerJob::FlashEnd { light });
    }
    if state.silent {
        play_to(world, client, POLAROID, 35.0);
    } else {
        let sound = crate::sound::PlaySound::new(eye, POLAROID, 75.0, true).extrarange(-3.0);
        crate::sound::playsound(world, sound);
    }
    // `printpicture`.
    if state.pictures_left == 0 {
        tell(world, client, Span::Warning, "Плёнка закончилась.");
        return true;
    }
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Настройка",
            "Хотите настроить фотографию?",
            PromptKind::Alert {
                buttons: vec!["Да".to_owned(), "Нет".to_owned()],
                autofocus: true,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(PendingAsk::Photo(PhotoAsk::Customise {
            camera,
            picture: Box::new(picture),
        }));
    true
}

/// `playsound_local` for one client; an SFX key picks its file first.
fn play_to(world: &mut World, client: Entity, sound: &str, volume: f32) {
    if !world.contains_resource::<Messages<ss13_net::sound::server::SendSound>>() {
        return;
    }
    let can_pick = world.contains_resource::<crate::sound::Sfx>()
        && world.contains_resource::<crate::combat::CombatRng>();
    let file = if can_pick {
        world.resource_scope(|world, mut rng: Mut<crate::combat::CombatRng>| {
            crate::sound::pick_sfx(world.resource::<crate::sound::Sfx>(), &mut rng, sound)
        })
    } else {
        sound.to_owned()
    };
    world.write_message(ss13_net::sound::server::SendSound {
        client,
        sound: ss13_shared::protocol::SoundEvent {
            file,
            volume,
            speed: 1.0,
            offset: Some([0, 0]),
        },
    });
}

fn set_camera_icon(world: &mut World, camera: Entity, state: &str) {
    if let Some(icon) = world.get::<DmAtom>(camera).map(|a| a.icon.clone()) {
        set_icon_state(world, camera, &icon, state);
    }
}

/// `cooldown`: ready again.
pub(super) fn camera_ready(world: &mut World, camera: Entity) {
    if world.get::<Camera>(camera).is_some() {
        set_camera_icon(world, camera, "camera");
    }
}

/// `flash_end`.
pub(super) fn flash_end(world: &mut World, light: Entity) {
    if let Ok(e) = world.get_entity_mut(light) {
        e.despawn();
    }
}

fn ask_text(
    world: &mut World,
    client: Entity,
    actor: Entity,
    (title, message, max_length): (&str, &str, u32),
    ask: PhotoAsk,
) {
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            actor,
            title,
            message,
            PromptKind::Text {
                default: String::new(),
                max_length,
                multiline: false,
            },
        ),
    );
    world.entity_mut(prompt).insert(PendingAsk::Photo(ask));
}

fn answer_text(answer: Option<&PromptAnswer>) -> Option<String> {
    match answer {
        Some(PromptAnswer::Text(text)) if !text.trim().is_empty() => Some(text.trim().to_owned()),
        _ => None,
    }
}

/// The answers of [`PhotoAsk`]; a closed prompt (`None`) skips its step, as tg's null input.
pub(super) fn answer(
    world: &mut World,
    actor: Entity,
    ask: PhotoAsk,
    answer: Option<PromptAnswer>,
) {
    let Some(client) = client_of(world, actor) else {
        return;
    };
    match ask {
        PhotoAsk::Customise { camera, picture } => {
            if answer == Some(PromptAnswer::Button(0)) {
                ask_text(
                    world,
                    client,
                    actor,
                    ("Название", "Задайте название фото или оставьте пустым.", 32),
                    PhotoAsk::Name { camera, picture },
                );
            } else {
                print(world, actor, camera, *picture, String::new());
            }
        }
        PhotoAsk::Name {
            camera,
            mut picture,
        } => {
            if let Some(name) = answer_text(answer.as_ref()) {
                picture.name = name;
            }
            ask_text(
                world,
                client,
                actor,
                (
                    "Описание",
                    "Добавьте описание к фото или оставьте пустым.",
                    128,
                ),
                PhotoAsk::Desc { camera, picture },
            );
        }
        PhotoAsk::Desc {
            camera,
            mut picture,
        } => {
            if let Some(desc) = answer_text(answer.as_ref()) {
                picture.desc = format!("{desc} - {}", picture.desc);
            }
            ask_text(
                world,
                client,
                actor,
                (
                    "Подпись",
                    "Задайте подпись к фото или оставьте пустым.",
                    256,
                ),
                PhotoAsk::Caption { camera, picture },
            );
        }
        PhotoAsk::Caption { camera, picture } => {
            let caption = answer_text(answer.as_ref()).unwrap_or_default();
            print(world, actor, camera, *picture, caption);
        }
        PhotoAsk::ZoomX { camera } => {
            let Some(PromptAnswer::Number(x)) = answer else {
                return;
            };
            if !holds(world, actor, camera) {
                return;
            }
            let x = (x as u8).clamp(1, MAX_HALF_SIZE);
            let current = world.get::<Camera>(camera).map_or(1, |c| c.size_y);
            ask_zoom(
                world,
                client,
                actor,
                "Полувысота кадра",
                current,
                PhotoAsk::ZoomY { camera, x },
            );
        }
        PhotoAsk::ZoomY { camera, x } => {
            let Some(PromptAnswer::Number(y)) = answer else {
                return;
            };
            if !holds(world, actor, camera) {
                return;
            }
            let y = (y as u8).clamp(1, MAX_HALF_SIZE);
            let Some(mut cam) = world.get_mut::<Camera>(camera) else {
                return;
            };
            cam.size_x = x;
            cam.size_y = y;
            let silent = cam.silent;
            let (w, h) = (2 * x - 1, 2 * y - 1);
            tell(
                world,
                client,
                Span::Notice,
                format!("Размер снимка будет {w}x{h}."),
            );
            click_sound(world, client, camera, silent);
        }
        PhotoAsk::Scribble { photo, pen } => {
            let Some(text) = answer_text(answer.as_ref()) else {
                return;
            };
            if !super::paper::pen_at_hand(world, actor, pen)
                || !super::paper::reachable(world, actor, photo)
            {
                return;
            }
            let Some(mut state) = world.get_mut::<Photo>(photo) else {
                return;
            };
            state.scribble = text;
            if let Some(pos) = map_pos(world, photo) {
                let sound = crate::sound::PlaySound::new(pos, "writing_pen", 50.0, true)
                    .extrarange(crate::sound::SHORT_RANGE_SOUND_EXTRARANGE)
                    .falloff_exponent(crate::sound::SOUND_FALLOFF_EXPONENT + 3.0)
                    .ignore_walls(false);
                crate::sound::playsound(world, sound);
            }
            refresh_windows(world, photo);
        }
    }
}

fn holds(world: &World, mob: Entity, item: Entity) -> bool {
    world.get::<Inventory>(mob).is_some_and(|i| i.holds(item))
}

/// `printpicture`: a photo of `picture` into the user's hands, film left checked again (a
/// second print waiting on its questions may have used the last one).
fn print(world: &mut World, mob: Entity, camera: Entity, picture: Picture, caption: String) {
    let Some(client) = client_of(world, mob) else {
        return;
    };
    let Some(left) = world.get::<Camera>(camera).map(|c| c.pictures_left) else {
        return;
    };
    if left == 0 {
        tell(world, client, Span::Warning, "Плёнка закончилась.");
        return;
    }
    let Some(pos) = map_pos(world, mob) else {
        return;
    };
    let Some(photo) = make_photo(world, pos, Arc::new(picture), caption) else {
        return;
    };
    if let Some(mut cam) = world.get_mut::<Camera>(camera) {
        cam.pictures_left -= 1;
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("Осталось снимков: {}.", left - 1),
    );
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, photo);
    }
}

/// A new photo of `picture` on `pos` (`photo/Initialize` -> `set_picture`): named after the
/// picture when it has a name, described by it.
pub(super) fn make_photo(
    world: &mut World,
    pos: TilePos,
    picture: Arc<Picture>,
    scribble: String,
) -> Option<Entity> {
    let photo = crate::construction::spawn_obj(world, PHOTO, pos)?;
    if !picture.name.is_empty() {
        let name = format!("{} - {}", name_of(world, photo), picture.name);
        set_name(world, photo, &name);
    }
    if let Some(mut dm) = world.get_mut::<DmName>(photo) {
        dm.desc.clone_from(&picture.desc);
    }
    world.entity_mut(photo).insert((
        Photo {
            picture: Some(picture),
            scribble,
        },
        AlwaysState,
    ));
    Some(photo)
}

/// The photocopier's copy of `original` (`make_photo_copy`: the picture without the writing).
pub(super) fn copy_photo(world: &mut World, original: Entity, pos: TilePos) -> Option<Entity> {
    let picture = world.get::<Photo>(original)?.picture.clone()?;
    make_photo(world, pos, picture, String::new())
}

/// Whether the copier can copy `item` (a photo with a picture).
pub(super) fn copyable(world: &World, item: Entity) -> bool {
    world
        .get::<Photo>(item)
        .is_some_and(|p| p.picture.is_some())
}

fn ask_zoom(
    world: &mut World,
    client: Entity,
    mob: Entity,
    message: &str,
    current: u8,
    ask: PhotoAsk,
) {
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Зум",
            message,
            PromptKind::Number {
                default: f64::from(current),
                min: 1.0,
                max: f64::from(MAX_HALF_SIZE),
                round: true,
            },
        ),
    );
    world.entity_mut(prompt).insert(PendingAsk::Photo(ask));
}

fn click_sound(world: &mut World, client: Entity, camera: Entity, silent: bool) {
    if silent {
        play_to(world, client, CLICK_SOUND, 50.0);
    } else if let Some(pos) = map_pos(world, camera) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, CLICK_SOUND, 50.0, true),
        );
    }
}

/// `camera/click_alt` -> `adjust_zoom`: the half width, then the half height.
pub(super) fn click_alt(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(size_x) = world.get::<Camera>(target).map(|c| c.size_x) else {
        return false;
    };
    if !holds(world, mob, target) {
        tell(
            world,
            client,
            Span::Warning,
            "Нужно держать фотоаппарат в руках!",
        );
        return true;
    }
    ask_zoom(
        world,
        client,
        mob,
        "Полуширина кадра",
        size_x,
        PhotoAsk::ZoomX { camera: target },
    );
    true
}

/// Clicks of a held camera past reach (`ranged_interact_with_atom`).
pub(super) fn ranged_snaps(
    world: &mut World,
    mut cursor: Local<MessageCursor<crate::click::RangedClick>>,
) {
    let clicks: Vec<crate::click::RangedClick> = cursor
        .read(world.resource::<Messages<crate::click::RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        if click.modifiers.right {
            continue;
        }
        let held = world
            .get::<Inventory>(click.mob)
            .and_then(Inventory::active_item);
        if let Some(camera) = held.filter(|&h| world.get::<Camera>(h).is_some()) {
            snap(world, click.client, click.mob, camera, click.target);
        }
    }
}

// --- Photos and frames --------------------------------------------------------------------

/// `photo/show`: the window, or "blank" without a picture.
fn show(world: &mut World, client: Entity, mob: Entity, photo: Entity) {
    let name = name_of(world, photo);
    if world
        .get::<Photo>(photo)
        .is_none_or(|p| p.picture.is_none())
    {
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} кажется пустой..."),
        );
        return;
    }
    world.write_message(OpenUi::new::<PhotoUi>(client, mob, photo, name));
}

/// `attack_self` of photos (`examinate` -> `show`) and of a held picture frame with a photo.
pub(super) fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<Photo>(item).is_some() {
        show(world, client, mob, item);
        return true;
    }
    if let Some(photo) = world.get::<PictureFrame>(item).and_then(|f| f.photo) {
        show(world, client, mob, photo);
        return true;
    }
    false
}

/// `examine` of a photo (shown up close, "get closer" from afar) and of a wall frame (its photo
/// up close).
pub fn examined(world: &mut World, client: Entity, examiner: Entity, target: Entity) {
    let near = match (map_pos(world, examiner), map_pos(world, target)) {
        (Some(a), Some(b)) => a.chebyshev(b).is_some_and(|d| d <= 1),
        _ => false,
    };
    let ghost = world.get::<crate::ghost::Ghost>(examiner).is_some();
    if world.get::<Photo>(target).is_some() {
        if near || ghost {
            show(world, client, examiner, target);
        } else {
            tell(
                world,
                client,
                Span::Warning,
                "Нужно подойти ближе, чтобы рассмотреть фотографию!",
            );
        }
    } else if near && let Some(photo) = world.get::<PictureFrame>(target).and_then(|f| f.photo) {
        show(world, client, examiner, photo);
    }
}

/// `picture_frame/attack_hand`: shows the framed photo.
pub(super) fn frame_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<DmItem>(target).is_some() {
        return false;
    }
    let Some(photo) = world.get::<PictureFrame>(target).and_then(|f| f.photo) else {
        return false;
    };
    show(world, client, mob, photo);
    true
}

/// `wallframe/picture/attack_hand` from the other hand: the photo out into the empty hand.
pub(super) fn frame_take_photo(
    world: &mut World,
    client: Entity,
    mob: Entity,
    frame: Entity,
) -> bool {
    if world.get::<DmItem>(frame).is_none() {
        return false;
    }
    let Some(photo) = world.get::<PictureFrame>(frame).and_then(|f| f.photo) else {
        return false;
    };
    set_framed(world, frame, None);
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, photo);
    }
    let name = name_of(world, frame);
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы аккуратно вынимаете фотографию из {name}."),
    );
    true
}

/// `item_interaction` of cameras (film), photos (pens) and picture frames (photos, tools). True
/// when the click was theirs.
pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if let Some(left) = world.get::<Camera>(target).map(|c| c.pictures_left)
        && item_path(world, held) == FILM
    {
        if left > 0 {
            crate::balloon::balloon_alert(world, mob, target, "не пуст!");
            return true;
        }
        world.despawn(held);
        if let Some(mut cam) = world.get_mut::<Camera>(target) {
            cam.pictures_left = cam.pictures_max;
        }
        if let Some(pos) = map_pos(world, target) {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(pos, CLICK_SOUND, 50.0, true),
            );
        }
        return true;
    }
    if world.get::<Photo>(target).is_some()
        && matches!(
            super::paper::writing(world, held),
            Some(super::paper::Writing::Pen(_))
        )
    {
        // `can_write`.
        if crate::vision::is_blind(world, mob) {
            tell(
                world,
                client,
                Span::Warning,
                "Вы ничего не видите, чтобы писать!",
            );
            return true;
        }
        if crate::medical::traumas::illiterate(world, mob) {
            tell(
                world,
                client,
                Span::Warning,
                "Вы пытаетесь писать, но вы же не умеете писать!",
            );
            return true;
        }
        let title = name_of(world, target);
        ask_text(
            world,
            client,
            mob,
            (&title, "Что вы хотите написать на обороте?", 128),
            PhotoAsk::Scribble {
                photo: target,
                pen: held,
            },
        );
        return true;
    }
    let Some(frame) = world.get::<PictureFrame>(target).cloned() else {
        return false;
    };
    if world.get::<Photo>(held).is_some() {
        if frame.photo.is_some() {
            let name = name_of(world, target);
            tell(
                world,
                client,
                Span::Warning,
                format!("{name} уже содержит фотографию."),
            );
            return true;
        }
        put_inside(world, held, target);
        set_framed(world, target, Some(held));
        return true;
    }
    // The tool acts are the wall frame's; the frame item's are `crate::wallmount::frames`.
    if world.get::<DmItem>(target).is_some() {
        return false;
    }
    match tool_of(world, held).map(|(k, _)| k) {
        Some(ToolKind::Wirecutters) => {
            let Some(photo) = frame.photo else {
                return false;
            };
            let Some(pos) = world.get::<TilePos>(target).copied() else {
                return true;
            };
            set_framed(world, target, None);
            take_out(world, photo, pos);
            let (user, photo_name, frame_name) = (
                name_of(world, mob),
                name_of(world, photo),
                name_of(world, target),
            );
            crate::medical::visible(
                world,
                mob,
                &format!("Вы вырезаете {photo_name} из {frame_name}!"),
                &format!("{user} вырезает {photo_name} из {frame_name}!"),
                Span::Warning,
            );
            true
        }
        Some(ToolKind::Screwdriver | ToolKind::Wrench) => {
            if !frame.can_decon {
                return false;
            }
            let name = name_of(world, target);
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы начинаете откреплять {name}..."),
            );
            use_tool(
                world,
                Actor {
                    client,
                    mob,
                    item: Some(held),
                },
                target,
                UNSECURE_DS,
                0,
                crate::construction::Job::Photo(PhotoJob::Unsecure),
            );
            true
        }
        _ => false,
    }
}

/// End of a [`PhotoJob`]: `deconstruct` -> `atom_deconstruct`, the frame item with the photo.
pub fn complete(world: &mut World, _mob: Entity, action: &crate::tools::DoAfter, job: PhotoJob) {
    let PhotoJob::Unsecure = job;
    let frame = action.target;
    let Some(state) = world.get::<PictureFrame>(frame).cloned() else {
        return;
    };
    let Some(pos) = world.get::<TilePos>(frame).copied() else {
        return;
    };
    let name = name_of(world, frame);
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, DECONSTRUCT_SOUND, 50.0, true),
    );
    tell(
        world,
        action.client,
        Span::Notice,
        format!("Вы открепляете {name}."),
    );
    let Some(item) = crate::construction::spawn_obj(world, FRAME_ITEM, pos) else {
        return;
    };
    world.entity_mut(item).insert(PictureFrame::default());
    if let Some(photo) = state.photo {
        put_inside(world, photo, item);
        set_framed(world, item, Some(photo));
    }
    world.despawn(frame);
    crate::construction::tile_changed(world, pos);
}

/// A picture frame item leaving as `built` (hung on a wall) or taken apart on `pos`
/// (`crate::wallmount::frames`): its photo goes into the wall frame or onto the floor.
pub fn frame_moved(world: &mut World, frame: Entity, built: Option<Entity>, pos: TilePos) {
    let Some(photo) = world.get::<PictureFrame>(frame).and_then(|f| f.photo) else {
        return;
    };
    match built {
        Some(built) => {
            // The wall frame's own `Initialize` may not have run yet.
            if world.get::<PictureFrame>(built).is_none() {
                world.entity_mut(built).insert(PictureFrame::default());
            }
            put_inside(world, photo, built);
            set_framed(world, built, Some(photo));
        }
        None => take_out(world, photo, pos),
    }
}

/// `set_and_save_framed` / `displayed`, then `update_appearance`.
fn set_framed(world: &mut World, frame: Entity, photo: Option<Entity>) {
    if let Some(mut state) = world.get_mut::<PictureFrame>(frame) {
        state.photo = photo;
    }
    refresh_frame(world, frame);
}

/// `update_overlays` (the photo over the frame) and a portrait's `update_name`,
/// `update_icon_state`, `update_desc`.
fn refresh_frame(world: &mut World, frame: Entity) {
    let Some(state) = world.get::<PictureFrame>(frame).cloned() else {
        return;
    };
    let wanted = state
        .photo
        .and_then(|p| world.get::<Appearance>(p))
        .map(|a| a.icon_state);
    if wanted != state.shown {
        if let Some(mut look) = world.get_mut::<Appearance>(frame) {
            if let Some(old) = state.shown
                && let Some(i) = look.overlays.iter().position(|o| o.icon_state == old)
            {
                look.overlays.remove(i);
            }
            if let Some(id) = wanted {
                look.overlays.push(Overlay::new(id));
            }
        }
        if let Some(mut s) = world.get_mut::<PictureFrame>(frame) {
            s.shown = wanted;
        }
    }
    let Some(portrait) = state.portrait else {
        return;
    };
    let framed = state.photo.is_some();
    let (name, icon_state, desc) = if framed {
        (portrait.frame_name.as_str(), "frame-overlay", FRAME_DESC)
    } else {
        (
            portrait.name.as_str(),
            portrait.icon_state.as_str(),
            portrait.desc.as_str(),
        )
    };
    set_name(world, frame, name);
    if let Some(mut dm) = world.get_mut::<DmName>(frame) {
        desc.clone_into(&mut dm.desc);
    }
    if let Some(icon) = world.get::<DmAtom>(frame).map(|a| a.icon.clone()) {
        set_icon_state(world, frame, &icon, icon_state);
    }
}

impl ServerUi for PhotoUi {
    type Source = (&'static Photo, Option<&'static DmName>);
    type UserParam = ();
    type Act = ();

    fn static_data((photo, name): (&Photo, Option<&DmName>)) -> PhotoData {
        let picture = photo.picture.as_deref().cloned().unwrap_or_default();
        PhotoData {
            name: name.map(|n| n.name.clone()).unwrap_or_default(),
            width: picture.width,
            height: picture.height,
            sprites: picture.sprites,
            scribble: photo.scribble.clone(),
        }
    }

    fn data(_: (&Photo, Option<&DmName>), _: Entity, _: &()) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_shared::Layer;

    fn look(icon: u32, plane: i16, layer: f32) -> Appearance {
        Appearance {
            icon_state: IconStateId(icon),
            layer: Layer(layer),
            plane,
            color: None,
            pixel_offset: [0, 0],
            rotation: 0,
            overlays: Vec::new(),
            mouse_opacity: ss13_shared::MouseOpacity::default(),
        }
    }

    #[test]
    fn sprites_follow_plane_layer_and_keep_overlays_with_their_atom() {
        let mut mob = look(1, 0, 4.0);
        mob.pixel_offset = [2, 3];
        mob.color = Some([255, 255, 255, 128]);
        let mut hat = Overlay::new(IconStateId(2));
        hat.pixel_offset = [0, 5];
        hat.color = Some([255, 0, 0, 255]);
        let mut glow = Overlay::new(IconStateId(3));
        glow.plane = Some(EMISSIVE_PLANE);
        let mut north_only = Overlay::new(IconStateId(4));
        north_only.only_dirs = Dir::North as u8;
        mob.overlays = vec![hat, glow, north_only];
        let floor = look(9, -4, 2.0);
        let mut out = Vec::new();
        sprites_of(&mob, Dir::East, [32, 0], 0, &mut out);
        sprites_of(&floor, Dir::South, [32, 0], 1, &mut out);
        out.sort_by(|(a, _), (b, _)| a.cmp(b));
        let ids: Vec<u32> = out.iter().map(|(_, s)| s.icon_state.0).collect();
        assert_eq!(
            ids,
            [9, 1, 2],
            "floor first, emissive and other-facing overlays left out"
        );
        let hat = out[2].1;
        assert_eq!((hat.x, hat.y), (34, 8));
        assert_eq!(hat.dir, Dir::East);
        assert_eq!(hat.color, Some([255, 0, 0, 128]), "the parent's alpha");
    }

    #[test]
    fn area_line_counts_meters() {
        assert_eq!(
            area_line(3, 3),
            "Это фотография области размером 3 на 3 метра."
        );
        assert!(area_line(7, 5).ends_with("5 метров."));
        assert!(area_line(1, 1).ends_with("1 метр."));
    }
}

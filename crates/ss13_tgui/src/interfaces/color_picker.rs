//! `ColorPickerModal` (tg `tgui_color_picker`): `tgui/packages/tgui/interfaces/ColorPickerModal`
//! and `/datum/tgui_color_picker` (`code/modules/tgui_input/color.dm`). The server half is
//! [`crate::prompt`].
//!
//! Acts: `submit` with `entry` (`#rrggbb`), `cancel`.

use serde::{Deserialize, Serialize};

use super::alert::PromptTimer;
use crate::UiInterface;

pub struct ColorPickerUi;

/// `ui_static_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ColorPickerStatic {
    pub message: String,
    /// `#rrggbb`.
    pub default_color: String,
    pub autofocus: bool,
    pub large_buttons: bool,
    pub swapped_buttons: bool,
}

impl UiInterface for ColorPickerUi {
    const NAME: &'static str = "ColorPickerModal";
    type Data = PromptTimer;
    type StaticData = ColorPickerStatic;
}

/// tg `sanitize_hexcolor(desired_format = 6, include_crunch = TRUE)` on a lowercased entry:
/// `#abc`, `abc`, `#aabbcc` and `aabbcc` become `#aabbcc`; anything else is `None`.
pub fn sanitize_hex(text: &str) -> Option<String> {
    let hex = text.trim().strip_prefix('#').unwrap_or(text.trim());
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let hex = hex.to_ascii_lowercase();
    match hex.len() {
        6 => Some(format!("#{hex}")),
        3 => Some(format!(
            "#{}",
            hex.chars().flat_map(|c| [c, c]).collect::<String>()
        )),
        _ => None,
    }
}

/// Hue 0..360, saturation and value 0..100, as tgui's `HsvaColor`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}

impl Hsv {
    pub fn to_rgb(self) -> [u8; 3] {
        let (h, s, v) = (
            self.h.rem_euclid(360.0) / 60.0,
            self.s / 100.0,
            self.v / 100.0,
        );
        let c = v * s;
        let x = c * (1.0 - (h % 2.0 - 1.0).abs());
        let (r, g, b) = match h as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = v - c;
        [r, g, b].map(|ch| ((ch + m) * 255.0).round().clamp(0.0, 255.0) as u8)
    }

    pub fn from_rgb([r, g, b]: [u8; 3]) -> Self {
        let [r, g, b] = [r, g, b].map(|c| c as f32 / 255.0);
        let max = r.max(g).max(b);
        let delta = max - r.min(g).min(b);
        let h = if delta == 0.0 {
            0.0
        } else if max == r {
            60.0 * ((g - b) / delta).rem_euclid(6.0)
        } else if max == g {
            60.0 * ((b - r) / delta + 2.0)
        } else {
            60.0 * ((r - g) / delta + 4.0)
        };
        let s = if max == 0.0 { 0.0 } else { delta / max * 100.0 };
        Self {
            h,
            s,
            v: max * 100.0,
        }
    }

    pub fn to_hex(self) -> String {
        let [r, g, b] = self.to_rgb();
        format!("#{r:02x}{g:02x}{b:02x}")
    }

    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = sanitize_hex(hex)?;
        let n = u32::from_str_radix(&hex[1..], 16).ok()?;
        Some(Self::from_rgb([(n >> 16) as u8, (n >> 8) as u8, n as u8]))
    }
}

/// tg `ColorPresets.tsx` `colorList` (tgui, MIT, itsmeow): rows of the preset grid.
pub const PRESETS: &[&[&str]] = &[
    &[
        "003366", "336699", "3366cc", "003399", "000099", "0000cc", "000066",
    ],
    &[
        "006666", "006699", "0099cc", "0066cc", "0033cc", "0000ff", "3333ff", "333399",
    ],
    &[
        "008080", "009999", "33cccc", "00ccff", "0099ff", "0066ff", "3366ff", "3333cc", "666699",
    ],
    &[
        "339966", "00cc99", "00ffcc", "00ffff", "33ccff", "3399ff", "6699ff", "6666ff", "6600ff",
        "6600cc",
    ],
    &[
        "339933", "00cc66", "00ff99", "66ffcc", "66ffff", "66ccff", "99ccff", "9999ff", "9966ff",
        "9933ff", "9900ff",
    ],
    &[
        "006600", "00cc00", "00ff00", "66ff99", "99ffcc", "ccffff", "ccecff", "ccccff", "cc99ff",
        "cc66ff", "cc00ff", "9900cc",
    ],
    &[
        "003300", "008000", "33cc33", "66ff66", "99ff99", "ccffcc", "ffffff", "ffccff", "ff99ff",
        "ff66ff", "ff00ff", "cc00cc", "660066",
    ],
    &[
        "336600", "009900", "66ff33", "99ff66", "ccff99", "ffffcc", "ffcccc", "ff99cc", "ff66cc",
        "ff33cc", "cc0099", "800080",
    ],
    &[
        "333300", "669900", "99ff33", "ccff66", "ffff99", "ffcc99", "ff9999", "ff6699", "ff3399",
        "cc3399", "990099",
    ],
    &[
        "666633", "99cc00", "ccff33", "ffff66", "ffcc66", "ff9966", "ff7c80", "ff0066", "d60093",
        "993366",
    ],
    &[
        "808000", "cccc00", "ffff00", "ffcc00", "ff9933", "ff6600", "ff5050", "cc0066", "660033",
    ],
    &[
        "996633", "cc9900", "ff9900", "cc6600", "ff3300", "ff0000", "cc0000", "990033",
    ],
    &[
        "663300", "996600", "cc3300", "993300", "990000", "800000", "a50021",
    ],
    &[
        "f8f8f8", "dddddd", "b2b2b2", "808080", "5f5f5f", "333333", "1c1c1c", "080808",
    ],
    &[
        "ffffff", "eaeaea", "c0c0c0", "969696", "777777", "4d4d4d", "292929", "111111", "000000",
    ],
    &[
        "f0197d", "ff7f50", "ffe45e", "21fa90", "7df9ff", "2ccaff", "21649f", "7b55dd", "b81fff",
        "ff5cad",
    ],
    &[
        "ffe2db", "ffd5cc", "feb8a0", "eaa78d", "e7a992", "d49487", "facca4", "e8b782", "ffc905",
    ],
    &[
        "c48c5e", "bd7740", "ab7968", "8c553f", "755246", "86655b", "754223", "471b18",
    ],
];

/// `height={message ? 465 : 430}`.
pub fn window_height(data: &ColorPickerStatic) -> f32 {
    if data.message.is_empty() {
        430.0
    } else {
        465.0
    }
}

#[cfg(feature = "client")]
pub use client_side::{ColorArea, ColorChannel, ColorHexField, ColorPreset};

#[cfg(feature = "client")]
pub(crate) mod client_side {
    use bevy::input::keyboard::KeyCode;
    use bevy::picking::Pickable;
    use bevy::picking::events::{PointerDrag, PointerPress};
    use bevy::scene::{EntityScene, SceneList};
    use bevy::ui::{ComputedNode, UiGlobalTransform};
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::theme::UiColor;
    use ss13_ui_kit::{InputChanged, NumberInputChanged};

    use super::*;
    use crate::client::{
        ActOnKey, AutoFocusWindow, ClientUi, ClientWindow, SetLocal, UiLocalState,
    };
    use crate::interfaces::alert::{CANCEL, loader};
    use crate::interfaces::input_buttons::{self, ENTRY};

    const SHOW_PRESETS: &str = "presets";
    /// react-colorful's default size.
    const AREA: f32 = 200.0;
    const HUE_HEIGHT: f32 = 24.0;

    /// A drag target: the saturation/value square or the hue strip.
    #[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub enum ColorArea {
        #[default]
        SaturationValue,
        Hue,
    }

    /// On a `NumberInput`: which channel of the color it edits.
    #[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub enum ColorChannel {
        #[default]
        H,
        S,
        V,
        R,
        G,
        B,
    }

    /// On the hex `Input`.
    #[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct ColorHexField;

    /// On a preset swatch: pressing it picks this `#rrggbb`.
    #[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
    pub struct ColorPreset(pub String);

    /// The color shown: the edited one, else the default.
    fn current(default: &str, local: &UiLocalState) -> Hsv {
        let get = |k: &str| local.value(k).and_then(|v| v.parse::<f32>().ok());
        match (get("h"), get("s"), get("v")) {
            (Some(h), Some(s), Some(v)) => Hsv { h, s, v },
            _ => Hsv::from_hex(default).unwrap_or(Hsv {
                h: 0.0,
                s: 0.0,
                v: 0.0,
            }),
        }
    }

    /// Keeps hue and saturation of grays and blacks, which the hex alone loses.
    fn store(local: &mut UiLocalState, hsv: Hsv) {
        let hsv = Hsv {
            h: hsv.h.clamp(0.0, 360.0),
            s: hsv.s.clamp(0.0, 100.0),
            v: hsv.v.clamp(0.0, 100.0),
        };
        local.set("h", &hsv.h.to_string());
        local.set("s", &hsv.s.to_string());
        local.set("v", &hsv.v.to_string());
        local.set(ENTRY, &hsv.to_hex());
    }

    type Windows<'w, 's> = Query<'w, 's, (&'static mut ClientWindow, &'static mut UiLocalState)>;

    /// Applies `edit` to the color of the window `entity` is in, and re-renders it.
    fn edit_color(
        entity: Entity,
        parents: &Query<&ChildOf>,
        windows: &mut Windows,
        edit: impl FnOnce(Hsv) -> Option<Hsv>,
    ) {
        let Some(root) = parents
            .iter_ancestors(entity)
            .find(|e| windows.contains(*e))
        else {
            return;
        };
        let Ok((mut window, mut local)) = windows.get_mut(root) else {
            return;
        };
        let default = local.value("default").unwrap_or("#000000").to_owned();
        if let Some(hsv) = edit(current(&default, &local)) {
            store(&mut local, hsv);
            window.mark_dirty();
        }
    }

    /// Where `pointer` (window logical px) is inside `node`, 0..1 on both axes.
    fn fraction(
        pointer: Vec2,
        scale: f32,
        node: &ComputedNode,
        transform: &UiGlobalTransform,
    ) -> Vec2 {
        let size = node.size.max(Vec2::ONE);
        let min = transform.translation - size / 2.0;
        ((pointer * scale - min) / size).clamp(Vec2::ZERO, Vec2::ONE)
    }

    fn area_edit(kind: ColorArea, at: Vec2) -> impl FnOnce(Hsv) -> Option<Hsv> {
        move |hsv| {
            Some(match kind {
                ColorArea::SaturationValue => Hsv {
                    s: at.x * 100.0,
                    v: (1.0 - at.y) * 100.0,
                    ..hsv
                },
                ColorArea::Hue => Hsv {
                    h: at.x * 360.0,
                    ..hsv
                },
            })
        }
    }

    fn window_scale(windows: &Query<&bevy::window::Window>) -> f32 {
        windows.iter().next().map_or(1.0, |w| w.scale_factor())
    }

    #[allow(clippy::too_many_arguments)]
    fn area_press(
        press: On<PointerPress>,
        areas: Query<(&ColorArea, &ComputedNode, &UiGlobalTransform)>,
        screens: Query<&bevy::window::Window>,
        parents: Query<&ChildOf>,
        mut windows: Windows,
    ) {
        let entity = press.event_target();
        if let Ok((kind, node, transform)) = areas.get(entity) {
            let at = fraction(
                press.pointer.position,
                window_scale(&screens),
                node,
                transform,
            );
            edit_color(entity, &parents, &mut windows, area_edit(*kind, at));
        }
    }

    fn area_drag(
        drag: On<PointerDrag>,
        areas: Query<(&ColorArea, &ComputedNode, &UiGlobalTransform)>,
        screens: Query<&bevy::window::Window>,
        parents: Query<&ChildOf>,
        mut windows: Windows,
    ) {
        let entity = drag.event_target();
        if let Ok((kind, node, transform)) = areas.get(entity) {
            let at = fraction(
                drag.pointer.position,
                window_scale(&screens),
                node,
                transform,
            );
            edit_color(entity, &parents, &mut windows, area_edit(*kind, at));
        }
    }

    fn channel_changed(
        changed: On<NumberInputChanged>,
        channels: Query<&ColorChannel>,
        parents: Query<&ChildOf>,
        mut windows: Windows,
    ) {
        let Ok(&channel) = channels.get(changed.entity) else {
            return;
        };
        let value = changed.value as f32;
        edit_color(changed.entity, &parents, &mut windows, |hsv| {
            let mut rgb = hsv.to_rgb();
            let c = value.round().clamp(0.0, 255.0) as u8;
            Some(match channel {
                ColorChannel::H => Hsv { h: value, ..hsv },
                ColorChannel::S => Hsv { s: value, ..hsv },
                ColorChannel::V => Hsv { v: value, ..hsv },
                ColorChannel::R | ColorChannel::G | ColorChannel::B => {
                    let i = match channel {
                        ColorChannel::R => 0,
                        ColorChannel::G => 1,
                        _ => 2,
                    };
                    rgb[i] = c;
                    Hsv::from_rgb(rgb)
                }
            })
        });
    }

    fn hex_changed(
        changed: On<InputChanged>,
        fields: Query<(), With<ColorHexField>>,
        parents: Query<&ChildOf>,
        mut windows: Windows,
    ) {
        if fields.contains(changed.entity) {
            let hex = changed.value.clone();
            edit_color(changed.entity, &parents, &mut windows, move |_| {
                Hsv::from_hex(&hex)
            });
        }
    }

    fn preset_press(
        press: On<PointerPress>,
        presets: Query<&ColorPreset>,
        parents: Query<&ChildOf>,
        mut windows: Windows,
    ) {
        let entity = press.event_target();
        if let Ok(ColorPreset(hex)) = presets.get(entity) {
            let hex = hex.clone();
            edit_color(entity, &parents, &mut windows, move |_| Hsv::from_hex(&hex));
        }
    }

    /// Remembers the default per window, so edits start from it.
    fn remember_default(
        added: Query<(Entity, &DefaultColor), Added<DefaultColor>>,
        parents: Query<&ChildOf>,
        mut windows: Windows,
    ) {
        for (entity, DefaultColor(hex)) in &added {
            if let Some(root) = parents
                .iter_ancestors(entity)
                .find(|e| windows.contains(*e))
                && let Ok((_, mut local)) = windows.get_mut(root)
                && local.value("default").is_none()
            {
                local.set("default", hex);
            }
        }
    }

    /// On the view root: the window's `default_color`.
    #[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
    pub struct DefaultColor(pub String);

    pub(crate) fn plugin(app: &mut App) {
        app.add_observer(area_press)
            .add_observer(area_drag)
            .add_observer(channel_changed)
            .add_observer(hex_changed)
            .add_observer(preset_press)
            .add_systems(Update, remember_default);
    }

    fn swatch(hex: &str, width: f32, height: f32) -> EntityScene<impl Scene + use<>> {
        let color = Hsv::from_hex(hex).map_or(Color::BLACK, |c| {
            let [r, g, b] = c.to_rgb();
            Color::srgb_u8(r, g, b)
        });
        EntityScene(bsn! {
            Node { width: px(width), height: px(height) }
            BackgroundColor(color)
            Pickable::IGNORE
        })
    }

    fn picker(hsv: Hsv) -> EntityScene<impl Scene + use<>> {
        let hue = {
            let [r, g, b] = Hsv {
                h: hsv.h,
                s: 100.0,
                v: 100.0,
            }
            .to_rgb();
            Color::srgb_u8(r, g, b)
        };
        let white = BackgroundGradient::from(LinearGradient::to_right(vec![
            ColorStop::auto(Color::WHITE),
            ColorStop::auto(Color::WHITE.with_alpha(0.0)),
        ]));
        let black = BackgroundGradient::from(LinearGradient::to_bottom(vec![
            ColorStop::auto(Color::BLACK.with_alpha(0.0)),
            ColorStop::auto(Color::BLACK),
        ]));
        let rainbow = BackgroundGradient::from(LinearGradient::to_right(
            (0..=6)
                .map(|i| {
                    let [r, g, b] = Hsv {
                        h: i as f32 * 60.0,
                        s: 100.0,
                        v: 100.0,
                    }
                    .to_rgb();
                    ColorStop::auto(Color::srgb_u8(r, g, b))
                })
                .collect(),
        ));
        let (sx, sy) = (hsv.s / 100.0 * AREA, (1.0 - hsv.v / 100.0) * AREA);
        let hx = hsv.h / 360.0 * AREA;
        let (sv_area, hue_area) = (ColorArea::SaturationValue, ColorArea::Hue);
        EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Column, row_gap: px(12) }
            Children [
                Node { width: px(AREA), height: px(AREA) }
                BackgroundColor(hue)
                sv_area
                Name("color: saturation value")
                Children [
                    Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100) }
                    white
                    Pickable::IGNORE
                    --
                    Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100) }
                    black
                    Pickable::IGNORE
                    --
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(sx - 7.0),
                        top: px(sy - 7.0),
                        width: px(14),
                        height: px(14),
                        border: UiRect::all(px(2)),
                        border_radius: BorderRadius::MAX,
                    }
                    BorderColor::all(Color::WHITE)
                    Pickable::IGNORE
                ]
                --
                Node { width: px(AREA), height: px(HUE_HEIGHT) }
                rainbow
                hue_area
                Name("color: hue")
                Children [
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(hx - 4.0),
                        width: px(8),
                        height: percent(100),
                        border: UiRect::all(px(2)),
                    }
                    BorderColor::all(Color::WHITE)
                    Pickable::IGNORE
                ]
            ]
        })
    }

    fn channel_row(label: &str, channel: ColorChannel, value: f32, max: f32) -> impl Scene + use<> {
        let label = label.to_owned();
        bsn! {
            @StackItem
            Children [
                @Stack { @align: {Some(AlignItems::Center)} }
                Children [
                    @StackItem
                    Node { width: px(28) }
                    Children [ @UiBox { @color: UiColor::Label } Children [ Text(label) ] ]
                    --
                    @StackItem { @grow: 1.0 }
                    Children [
                        NumberInput {
                            value: {value.round() as f64},
                            min_value: 0.0,
                            max_value: {max as f64},
                            step: 1.0,
                            step_pixel_size: 2.0,
                            fluid: true,
                        }
                        channel
                    ]
                ]
            ]
        }
    }

    fn presets() -> Box<dyn SceneList> {
        let rows: Vec<_> = PRESETS
            .iter()
            .map(|row| {
                let cells: Vec<_> = row
                    .iter()
                    .map(|hex| {
                        let hex = format!("#{hex}");
                        let pick = ColorPreset(hex.clone());
                        let swatch = swatch(&hex, 24.0, 16.0);
                        bsn! {
                            Node { padding: {UiRect::all(px(1))} }
                            BackgroundColor(Color::BLACK)
                            pick
                            Children [ {swatch} ]
                        }
                    })
                    .collect();
                bsn! {
                    Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::Center }
                    Children [ {cells} ]
                }
            })
            .collect();
        let hide = SetLocal {
            key: SHOW_PRESETS.into(),
            value: String::new(),
            rerender: true,
        };
        Box::new(EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Column }
            Children [
                @ss13_ui_kit::Button { @content: "Hide", @icon: "chevron-up" }
                Node { align_self: AlignSelf::FlexEnd }
                hide
                --
                {rows}
            ]
        }))
    }

    fn channels(hsv: Hsv) -> Box<dyn SceneList> {
        let [r, g, b] = hsv.to_rgb();
        let hex = hsv.to_hex()[1..].to_owned();
        let show = SetLocal {
            key: SHOW_PRESETS.into(),
            value: "1".into(),
            rerender: true,
        };
        let rows = vec![
            channel_row("H:", ColorChannel::H, hsv.h, 360.0),
            channel_row("S:", ColorChannel::S, hsv.s, 100.0),
            channel_row("V:", ColorChannel::V, hsv.v, 100.0),
            channel_row("R:", ColorChannel::R, r as f32, 255.0),
            channel_row("G:", ColorChannel::G, g as f32, 255.0),
            channel_row("B:", ColorChannel::B, b as f32, 255.0),
        ];
        Box::new(EntityScene(bsn! {
            @Stack { @vertical: true }
            Children [
                @StackItem
                Children [
                    @Stack { @align: {Some(AlignItems::Center)} }
                    Children [
                        @StackItem
                        Children [ @UiBox { @color: UiColor::Label } Children [ Text("Hex:") ] ]
                        --
                        @StackItem { @grow: 1.0 }
                        Children [
                            Input { value: hex, max_length: {Some(7)}, fluid: true }
                            ColorHexField
                            Name("color: hex")
                        ]
                        --
                        @StackItem
                        Children [
                            @ss13_ui_kit::Button { @content: "Skin Tones and Presets", @icon: "chevron-down" }
                            show
                        ]
                    ]
                ]
                --
                @StackDivider
                --
                {rows}
            ]
        }))
    }

    impl ClientUi for ColorPickerUi {
        const WIDTH: f32 = 700.0;
        const HEIGHT: f32 = 430.0;

        fn size(st: &ColorPickerStatic, _: &PromptTimer) -> (f32, f32) {
            (Self::WIDTH, window_height(st))
        }

        fn content(st: &ColorPickerStatic, data: &PromptTimer) -> impl Scene {
            view(st, data, &UiLocalState::default())
        }

        fn content_local(
            st: &ColorPickerStatic,
            data: &PromptTimer,
            local: &UiLocalState,
        ) -> impl Scene {
            view(st, data, local)
        }
    }

    fn view(
        st: &ColorPickerStatic,
        data: &PromptTimer,
        local: &UiLocalState,
    ) -> impl Scene + use<> {
        let hsv = current(&st.default_color, local);
        let hex = hsv.to_hex();
        let message = (!st.message.is_empty()).then(|| {
            let text = st.message.clone();
            EntityScene(bsn! {
                @StackItem
                Children [
                    @Section { @fill: true }
                    Children [ @UiBox { @color: UiColor::Label } Children [ Text(text) ] ]
                ]
            })
        });
        let right = if local.value(SHOW_PRESETS).is_some_and(|v| !v.is_empty()) {
            presets()
        } else {
            channels(hsv)
        };
        let new_swatch = swatch(&hex, 100.0, 30.0);
        let old_swatch = swatch(&st.default_color, 100.0, 30.0);
        let buttons = input_buttons::buttons(
            st.large_buttons,
            st.swapped_buttons,
            String::new(),
            &hex,
            false,
        );
        let default_color = DefaultColor(st.default_color.clone());
        let autofocus = AutoFocusWindow(st.autofocus);
        let loader = loader(data);
        let picker = picker(hsv);
        bsn! {
            @WindowContent
            ActOnKey(vec![(KeyCode::Escape, CANCEL.into(), Vec::new())])
            default_color
            autofocus
            Children [
                {loader}
                --
                @Stack { @vertical: true, @fill: true }
                Children [
                    {message}
                    --
                    @StackItem { @grow: 1.0 }
                    Children [
                        @Section { @fill: true }
                        Children [
                            @Stack
                            Children [
                                @StackItem
                                Node { margin: {UiRect { right: px(24), top: px(30), ..default() }} }
                                Children [
                                    @Stack { @vertical: true }
                                    Children [
                                        @StackItem Children [ {picker} ]
                                        --
                                        @StackItem
                                        Node { margin: {UiRect::top(px(24))}, flex_direction: FlexDirection::Column }
                                        Children [
                                            Node { flex_direction: FlexDirection::Row }
                                            Children [
                                                Node { width: px(100), justify_content: JustifyContent::Center }
                                                Children [ Text("New") ]
                                                --
                                                Node { width: px(100), justify_content: JustifyContent::Center }
                                                Children [ Text("Current") ]
                                            ]
                                            --
                                            Node { flex_direction: FlexDirection::Row }
                                            Children [ {new_swatch} -- {old_swatch} ]
                                        ]
                                        --
                                        @StackItem Children [ {buttons} ]
                                    ]
                                ]
                                --
                                @StackItem { @grow: 1.0 }
                                Node { margin: {UiRect::top(px(44))} }
                                Children [ {right} ]
                            ]
                        ]
                    ]
                ]
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_hsv_round_trip_like_tg() {
        assert_eq!(sanitize_hex("#ABC").as_deref(), Some("#aabbcc"));
        assert_eq!(sanitize_hex("ff0000").as_deref(), Some("#ff0000"));
        assert_eq!(sanitize_hex("#12345"), None);
        assert_eq!(sanitize_hex("#gg0000"), None);
        for hex in [
            "#ff0000", "#00ff00", "#123456", "#000000", "#ffffff", "#c0ffee",
        ] {
            assert_eq!(Hsv::from_hex(hex).unwrap().to_hex(), hex);
        }
        let red = Hsv::from_hex("#ff0000").unwrap();
        assert_eq!((red.h, red.s, red.v), (0.0, 100.0, 100.0));
        assert!(
            PRESETS
                .iter()
                .flat_map(|r| r.iter())
                .all(|c| sanitize_hex(c).is_some())
        );
    }
}

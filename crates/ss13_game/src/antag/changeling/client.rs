//! Client half of the changeling: the chemical counter (`/atom/movable/screen/ling/chems`:
//! `icons/hud/screen_changeling.dmi` `power_display` at `WEST,CENTER-1:15`, the charges in
//! `#dd66dd`, the storage in `#dd2828` while hovered), the `AntagInfoChangeling` panel and the
//! `CellularEmporium` window.
// shortcut: no search field or compact mode in the emporium, no memory dropdown in the panel
// (no memories are stolen yet).

use bevy::app::Propagate;
use bevy::picking::events::{PointerOut, PointerOver};
use bevy::scene::EntityScene;
use ss13_net::ui::client::UiMessageReceived;
use ss13_render::atlas::frame_rect;
use ss13_render::{IconAtlas, MapInsets};
use ss13_shared::Dir;
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::client::{AppTguiClientExt, ClientUi, SetLocal, UiLocalState, UiSprite};
use ss13_tgui::{UiInterface, decode};
use ss13_ui_kit::ActParams;
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::{Bold, FontPx};

use super::{
    Ability, AntagInfoChangelingUi, CellularEmporiumUi, ChangelingHudData, ChangelingHudUi,
    ChangelingInfo, EVOLVE, EmporiumData, EmporiumStatic, READAPT,
};
use crate::inventory_client::{ScreenAxis, screen_loc};

const LING_DMI: &str = "icons/hud/screen_changeling.dmi";
/// `useState` key of the emporium's category.
const CATEGORY: &str = "category";
/// `CATEGORY_ORDER`.
const CATEGORY_ORDER: [&str; 4] = ["combat", "stealth", "utility", "stings"];
const PINK: Color = Color::srgb(0.867, 0.4, 0.867);

pub struct ChangelingClientPlugin;

impl Plugin for ChangelingClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<AntagInfoChangelingUi>()
            .register_ui_view::<CellularEmporiumUi>()
            .register_ui_headless::<ChangelingHudUi>()
            .init_resource::<ChangelingHud>()
            .add_systems(Startup, spawn_counter)
            .add_systems(
                Update,
                (
                    receive,
                    draw_counter.run_if(resource_changed::<ChangelingHud>),
                    follow_map_insets.run_if(resource_changed::<MapInsets>),
                ),
            );
    }
}

/// The server's chemical counter window, its last data and the hover.
#[derive(Resource, Default, Debug)]
pub struct ChangelingHud {
    pub window: Option<u32>,
    pub data: ChangelingHudData,
    pub hovering: bool,
}

#[derive(Component)]
struct CounterRoot;

#[derive(Component)]
struct Counter;

#[derive(Component)]
struct CounterText;

fn spawn_counter(mut commands: Commands) {
    let root = commands
        .spawn((
            CounterRoot,
            Name::new("changeling chemicals"),
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(0),
                bottom: px(0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();
    let slot = commands
        .spawn((
            Counter,
            Name::new("chemical storage"),
            screen_loc(ScreenAxis(-1, 0, 0), ScreenAxis(0, -1, 15)),
            ImageNode::default(),
            Visibility::Hidden,
            ChildOf(root),
        ))
        .observe(|_: On<PointerOver>, mut hud: ResMut<ChangelingHud>| hud.hovering = true)
        .observe(|_: On<PointerOut>, mut hud: ResMut<ChangelingHud>| hud.hovering = false)
        .id();
    // `FORMAT_CHEM_CHARGES_TEXT`: centered, 6 px right.
    commands.spawn((
        CounterText,
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            right: px(0),
            top: px(0),
            bottom: px(0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Pickable::IGNORE,
        ChildOf(slot),
        children![(Text::default(), TextColor(PINK), FontPx(14.0), Bold(true))],
    ));
}

fn receive(mut received: MessageReader<UiMessageReceived>, mut hud: ResMut<ChangelingHud>) {
    for UiMessageReceived(message) in received.read() {
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == ChangelingHudUi::NAME => {
                hud.window = Some(*window);
                Some(data)
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if hud.window == Some(*window) => Some(data),
            UiServerMessage::Close { window } if hud.window == Some(*window) => {
                *hud = ChangelingHud::default();
                None
            }
            _ => None,
        };
        if let Some(data) = data.filter(|d| !d.is_empty()) {
            match decode::<ChangelingHudData>(data) {
                Ok(data) => hud.data = data,
                Err(e) => warn!("undecodable changeling HUD data: {e}"),
            }
        }
    }
}

fn draw_counter(
    hud: Res<ChangelingHud>,
    mut atlas: Option<ResMut<IconAtlas>>,
    mut slot: Single<(&mut ImageNode, &mut Visibility), With<Counter>>,
    holders: Query<&Children, With<CounterText>>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
) {
    let (image, visibility) = &mut *slot;
    **visibility = if hud.window.is_some() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    let sprite = atlas.as_deref_mut().and_then(|atlas| {
        let id = atlas
            .manifest
            .index()
            .get(&(LING_DMI, "power_display"))
            .copied()?;
        let (info, handle) = atlas.state(id)?;
        Some(ImageNode {
            image: handle,
            rect: frame_rect(info, Dir::South, 0),
            ..default()
        })
    });
    if let Some(sprite) = sprite {
        **image = sprite;
    }
    // `update_chemical_hud`: the storage while hovered (`FORMAT_CHEM_MAX_TEXT`).
    let (value, color) = if hud.hovering {
        (hud.data.chem_max, Color::srgb_u8(0xdd, 0x28, 0x28))
    } else {
        (hud.data.chem_charges, PINK)
    };
    for children in &holders {
        for &child in children {
            if let Ok((mut text, mut text_color)) = texts.get_mut(child) {
                text.0 = value.to_string();
                text_color.0 = color;
            }
        }
    }
}

fn follow_map_insets(insets: Res<MapInsets>, mut roots: Query<&mut Node, With<CounterRoot>>) {
    for mut node in &mut roots {
        node.left = px(insets.left);
        node.right = px(insets.right);
    }
}

// ---------------------------------------------------------------------------------------------
// AntagInfoChangeling

impl ClientUi for AntagInfoChangelingUi {
    const WIDTH: f32 = 720.0;
    const HEIGHT: f32 = 750.0;

    fn content(_: &(), data: &ChangelingInfo) -> impl Scene {
        let yellow = Color::srgb(1.0, 1.0, 0.0);
        let intro = format!("Вы {} из", data.true_name);
        let hive = format!(" {}", data.hive_name);
        let dna = data.absorbed_dna.to_string();
        let mut lines: Vec<String> = data
            .objectives
            .iter()
            .map(|o| format!("#{}: {}", o.count, o.explanation))
            .collect();
        if lines.is_empty() {
            lines.push("Ни одной!".to_owned());
        }
        let objectives: Vec<_> = lines
            .into_iter()
            .map(|line| bsn! { Node Children [ Text(line) ] })
            .collect();
        let stolen = if data.stolen_antag_info.is_empty() {
            "Сначала поглотите жертву!".to_owned()
        } else {
            data.stolen_antag_info.clone()
        };
        let memories = if data.memories.is_empty() {
            "Сначала поглотите жертву!".to_owned()
        } else {
            data.memories
                .iter()
                .map(|m| format!("{}: {}", m.name, m.story))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let abilities = EntityScene(abilities_section(&data.true_name));
        bsn! {
            @WindowContent
            Children [
                @Stack { @vertical: true, @fill: true }
                Children [
                    @StackItem
                    Children [
                        @Section {
                            @title: "Intro",
                            @buttons: bsn! {
                                @ss13_ui_kit::Button { @content: dna, @icon: "dna", @tooltip: "Absorbed DNA" }
                                ss13_ui_kit::Button { color: UiColor::Purple }
                                Name("changeling info: absorbed")
                            },
                        }
                        Children [
                            Node { flex_direction: FlexDirection::Column, row_gap: px(6) }
                            Children [
                                Node Propagate::<FontPx>(FontPx(25.0)) Name("changeling info: name")
                                Children [
                                    Node Children [ Text(intro) ]
                                    --
                                    Node Propagate::<Bold>(Bold(true)) Propagate::<TextColor>(TextColor(yellow))
                                    Children [ Text(hive) ]
                                ]
                                --
                                Node Propagate::<Bold>(Bold(true))
                                Children [ Text("Ваши текущие задачи:") ]
                                --
                                Node { flex_direction: FlexDirection::Column }
                                Name("changeling info: objectives")
                                Children [ {objectives} ]
                            ]
                        ]
                    ]
                    --
                    @StackItem { @grow: 4.0 }
                    Node { min_height: px(0) }
                    Children [ {abilities} ]
                    --
                    @StackItem
                    Children [
                        @NoticeBox { @danger: true }
                        Children [ Text("Другие генокрады — сильные союзники, но некоторые могут вас предать. Генокрады становятся намного сильнее, поглощая себе подобных, а поглощённый генокрад становится Падшим генокрадом. Нет большего унижения.") ]
                    ]
                    --
                    @StackItem { @grow: 3.0 }
                    Node { min_height: px(0) }
                    Children [
                        @Stack { @fill: true }
                        Children [
                            @StackItem { @grow: 1.0 }
                            Node { min_width: px(0) }
                            Children [
                                @Section { @title: "Украденные воспоминания", @fill: true, @scrollable: true }
                                Children [ Node Children [ Text(memories) ] ]
                            ]
                            --
                            @StackItem { @grow: 1.0 }
                            Node { min_width: px(0) }
                            Children [
                                @Section { @title: "Дополнительная украденная информация", @fill: true, @scrollable: true }
                                Children [ Node Name("changeling info: stolen") Children [ Text(stolen) ] ]
                            ]
                        ]
                    ]
                ]
            ]
        }
    }
}

/// `AbilitiesSection`.
fn abilities_section(true_name: &str) -> impl Scene + use<> {
    let texts = [
        "Ваша способность Absorb DNA позволяет вам украсть ДНК и воспоминания жертвы. Способность \
         Extract DNA Sting также крадет ДНК жертвы и не поддается обнаружению, но не дает вам их \
         воспоминаний или образцов речи."
            .to_owned(),
        "Ваша способность Reviving Stasis позволяет вам возродиться. Это значит, что ничто, кроме \
         полного разрушения тела, не сможет остановить вас! Разумеется, это громко, поэтому не \
         стоит делать это в присутствии людей, которых вы не собираетесь заткнуть."
            .to_owned(),
        "Ваша способность Transform позволяет вам превратиться в форму тех, у кого вы собрали \
         ДНК, летально и нелетально. Он также будет имитировать (НЕ НАСТОЯЩУЮ ОДЕЖДУ) одежду, в \
         которую они были одеты, для каждого вашего свободного слота."
            .to_owned(),
        "Cellular Emporium это место, где вы приобретаете новые способности, помимо стартового \
         набора. У вас есть 10 генетических очков, которые вы можете потратить на способности, и \
         вы можете реадаптироваться после поглощения тела, возвращая свои очки для разных \
         наборов."
            .to_owned(),
        "All abilities require using chemicals, you can see how much you have with the HUD on the \
         left side of the screen. You may also hover your cursor over it to see the maximum amount \
         of chemicals you can hold. This number can increase by absorbing other Changelings."
            .to_owned(),
        format!(
            "All Changelings, regardless of origin, are linked together by the hivemind. You may \
             communicate to other Changelings under your mental alias, {true_name}, by starting a \
             message with :g. Work together, and you will bring the station to new heights of \
             terror."
        ),
    ];
    let label = Color::srgba(1.0, 1.0, 1.0, 0.7);
    let columns: Vec<_> = texts
        .chunks(2)
        .map(|pair| {
            let (a, b) = (pair[0].clone(), pair[1].clone());
            bsn! {
                Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, flex_basis: px(0), row_gap: px(8), padding: UiRect::horizontal(px(4)) }
                Propagate::<TextColor>(TextColor(label))
                Children [ Node Children [ Text(a) ] -- Node Children [ Text(b) ] ]
            }
        })
        .collect();
    bsn! {
        @Section { @title: "Способности", @fill: true, @scrollable: true }
        Children [
            Node { flex_direction: FlexDirection::Row }
            Children [ {columns} ]
        ]
    }
}

// ---------------------------------------------------------------------------------------------
// CellularEmporium

impl ClientUi for CellularEmporiumUi {
    const WIDTH: f32 = 900.0;
    const HEIGHT: f32 = 520.0;

    fn content(st: &EmporiumStatic, data: &EmporiumData) -> impl Scene {
        emporium_view(st, data, &UiLocalState::default())
    }

    fn content_local(st: &EmporiumStatic, data: &EmporiumData, local: &UiLocalState) -> impl Scene {
        emporium_view(st, data, local)
    }
}

/// `sortedCategories`: tg's order first, then the rest as they come.
pub fn sorted_categories(abilities: &[Ability]) -> Vec<String> {
    let mut all: Vec<String> = Vec::new();
    for a in abilities {
        if !all.contains(&a.category) {
            all.push(a.category.clone());
        }
    }
    let mut out: Vec<String> = CATEGORY_ORDER
        .iter()
        .filter(|c| all.iter().any(|a| a == *c))
        .map(|c| (*c).to_owned())
        .collect();
    out.extend(
        all.into_iter()
            .filter(|c| !CATEGORY_ORDER.contains(&c.as_str())),
    );
    out
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn emporium_view(
    st: &EmporiumStatic,
    data: &EmporiumData,
    local: &UiLocalState,
) -> impl Scene + use<> {
    let categories = sorted_categories(&st.abilities);
    let selected = match local.get(CATEGORY) {
        "" => categories
            .first()
            .cloned()
            .unwrap_or_else(|| "utility".to_owned()),
        s => s.to_owned(),
    };
    let tabs: Vec<_> = categories
        .iter()
        .map(|category| {
            let is_selected = *category == selected;
            let label = capitalized(category);
            let category = category.clone();
            bsn! {
                @TabsTab { @content: label }
                TabsTab { selected: is_selected }
                SetLocal { key: {CATEGORY.to_owned()}, value: category, rerender: true }
            }
        })
        .collect();
    let mut shown: Vec<&Ability> = st
        .abilities
        .iter()
        .filter(|a| a.category == selected)
        .collect();
    shown.sort_by(|a, b| a.name.cmp(&b.name));
    let items: Vec<_> = shown.into_iter().map(|a| ability_row(a, data)).collect();
    let empty = items.is_empty().then(|| {
        EntityScene(bsn! { @NoticeBox Children [ Text("No abilities in this category.") ] })
    });
    let points = format!("{} DNA", data.genetic_points_count);
    let readapt = format!("Readapt ({})", data.can_readapt);
    let readapt_tip = if data.can_readapt > 0 {
        "Un-evolve all abilities and refund genetic points."
    } else {
        "Need more DNA to readapt."
    };
    bsn! {
        @WindowContent
        Children [
            @Section {
                @fill: true,
                @buttons: bsn! {
                    @ss13_ui_kit::Button { @content: readapt, @icon: "undo", @tooltip: readapt_tip }
                    ss13_ui_kit::Button { color: UiColor::Good, disabled: {data.can_readapt == 0} }
                    Act({READAPT.to_owned()})
                    Name("emporium: readapt")
                },
            }
            Children [
                @Stack { @vertical: true, @fill: true }
                Children [
                    @StackItem
                    Node { margin: UiRect::bottom(px(6)), column_gap: px(6) }
                    Propagate::<FontPx>(FontPx(16.0))
                    Propagate::<TextColor>(TextColor(PINK))
                    Name("emporium: points")
                    Children [ @Icon { @name: "dna" } -- Node Children [ Text(points) ] ]
                    --
                    @StackItem { @grow: 1.0 }
                    Node { min_height: px(0) }
                    Children [
                        @Stack { @fill: true }
                        Children [
                            @StackItem
                            Node { width: px(180), flex_shrink: 0.0 }
                            Children [
                                @Tabs { @vertical: true, @fill: true }
                                Children [ {tabs} ]
                            ]
                            --
                            @StackItem { @grow: 1.0 }
                            Node { min_width: px(0) }
                            Children [
                                @Section { @fill: true, @scrollable: true }
                                Children [ {empty} -- {items} ]
                            ]
                        ]
                    ]
                ]
            ]
        ]
    }
}

/// `ItemList` entry, detailed mode: the icon on its background, the name as the section title
/// with the cost button, the description and the help text in green.
fn ability_row(ability: &Ability, data: &EmporiumData) -> impl Scene + use<> {
    let owned = data.owned_abilities.contains(&ability.path);
    let can_afford = data.can_afford(ability);
    let cost = if ability.dna_required > 0 {
        format!("Cost: {} DNA", ability.dna_required)
    } else {
        format!("Cost: {} DNA", ability.genetic_point_required)
    };
    let mut tooltip = vec![format!("{} DNA", ability.genetic_point_required)];
    if ability.absorbs_required > 0 {
        tooltip.push(format!("{} absorptions", ability.absorbs_required));
    }
    if ability.dna_required > 0 {
        tooltip.push(format!("{} DNA", ability.dna_required));
    }
    let tooltip = tooltip.join(", ");
    let color = if owned {
        UiColor::Good
    } else if can_afford {
        UiColor::Average
    } else {
        UiColor::Bad
    };
    let title = if owned {
        format!("{} (Owned)", ability.name)
    } else {
        ability.name.clone()
    };
    let desc = ability.desc.clone();
    let help = (!ability.helptext.is_empty()).then(|| {
        let help = ability.helptext.clone();
        let green = Color::srgb_u8(0x44, 0xbd, 0x46);
        EntityScene(bsn! {
            Node { margin: UiRect::top(px(4)) }
            Propagate::<TextColor>(TextColor(green))
            Children [ Text(help) ]
        })
    });
    let params = ActParams(vec![("path".to_owned(), ability.path.clone())]);
    let button_name = Name::new(format!("evolve: {}", ability.name));
    let (background, icon) = (ability.background, ability.icon_state);
    bsn! {
        Node { flex_direction: FlexDirection::Row, margin: UiRect::bottom(px(8)) }
        Children [
            Node { width: px(64), height: px(64), margin: UiRect::new(px(16), px(8), px(0), px(0)), flex_shrink: 0.0 }
            Children [
                Node { position_type: PositionType::Absolute, width: px(64), height: px(64) }
                UiSprite { icon_state: background, color: None }
                --
                Node { position_type: PositionType::Absolute, width: px(64), height: px(64) }
                UiSprite { icon_state: icon, color: None }
            ]
            --
            Node { flex_grow: 1.0, min_width: px(0) }
            Children [
                @Section {
                    @title: title,
                    @buttons: bsn! {
                        @ss13_ui_kit::Button { @content: cost, @tooltip: tooltip }
                        ss13_ui_kit::Button { color: color, disabled: {owned || !can_afford} }
                        Act({EVOLVE.to_owned()})
                        params
                        button_name
                    },
                }
                Children [
                    Node
                    Propagate::<TextColor>(TextColor(Color::srgba(1.0, 1.0, 1.0, 0.8)))
                    Children [ Text(desc) ]
                    --
                    {help}
                ]
            ]
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_follow_tg_order() {
        let ability = |category: &str| Ability {
            category: category.to_owned(),
            ..default()
        };
        let abilities = [ability("utility"), ability("other"), ability("combat")];
        assert_eq!(
            sorted_categories(&abilities),
            ["combat", "utility", "other"]
        );
    }
}

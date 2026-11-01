//! Client half of the heretic: the `AntagInfoHeretic` window (`AntagInfoHeretic.tsx`): the
//! introduction with the objectives, the paths and the research tree with the shop.
// shortcut: no per-path themes or the ascended gradient, no "reject ascension" button (tg's
// `can_assign_self_objectives` is off here), knowledge icons are their first south frame.

use bevy::app::Propagate;
use bevy::scene::{EntityScene, SceneList};
use ss13_tgui::client::{AppTguiClientExt, ClientUi, SetLocal, UiLocalState, UiSprite};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::{Bold, FontPx};

use super::{AntagInfoHereticUi, HereticInfo, HereticPath, HereticStatic, KnowledgeNode, RESEARCH};

/// `useState` keys: the top tab and the previewed path.
const TAB: &str = "tab";
const PATH_TAB: &str = "path";

const FLAVOR: &str = "Еще один день на бессмысленной работе. Вы чувствуете мерцание вокруг себя, \
                      когда что-то странное в воздухе озаряет вас. Вы смотрите внутрь себя и \
                      находите то, что изменит вашу жизнь.";

const RED: Color = Color::srgb(0.878, 0.235, 0.235);
const BLUE: Color = Color::srgb(0.129, 0.522, 0.816);
const PURPLE: Color = Color::srgb(0.741, 0.329, 0.878);
const GREEN: Color = Color::srgb(0.125, 0.694, 0.259);
const YELLOW: Color = Color::srgb(1.0, 1.0, 0.0);

pub struct HereticClientPlugin;

impl Plugin for HereticClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<AntagInfoHereticUi>();
    }
}

impl ClientUi for AntagInfoHereticUi {
    const WIDTH: f32 = 750.0;
    const HEIGHT: f32 = 635.0;

    fn content(st: &HereticStatic, data: &HereticInfo) -> impl Scene {
        view(st, data, &UiLocalState::default())
    }

    fn content_local(st: &HereticStatic, data: &HereticInfo, local: &UiLocalState) -> impl Scene {
        view(st, data, local)
    }
}

fn view(st: &HereticStatic, data: &HereticInfo, local: &UiLocalState) -> impl Scene + use<> {
    let tab: usize = local.get(TAB).parse().unwrap_or(1);
    let labels = ["Информация", "Информация пути", "Исследования"];
    let tabs: Vec<_> = labels
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let name = Name::new(format!("heretic tab {i}"));
            let label = (*label).to_owned();
            bsn! {
                @TabsTab { @content: label, @icon: {if i == 2 { "book" } else { "info" }} }
                TabsTab { selected: {tab == i} }
                SetLocal { key: {TAB.to_owned()}, value: {i.to_string()}, rerender: true }
                name
            }
        })
        .collect();
    let body = match tab {
        0 => Box::new(EntityScene(introduction(data))) as Box<dyn SceneList>,
        2 => Box::new(EntityScene(research(st, data))) as Box<dyn SceneList>,
        _ => Box::new(EntityScene(path_info(st, data, local))) as Box<dyn SceneList>,
    };
    bsn! {
        @WindowContent
        Children [
            @Stack { @vertical: true, @fill: true }
            Children [
                @StackItem
                Children [ @Tabs { @fluid: true } Children [ {tabs} ] ]
                --
                @StackItem { @grow: 1.0 }
                Node { min_height: px(0) }
                Children [ {body} ]
            ]
        ]
    }
}

/// A line of text in `color` (`<span style={...}>`), bold where tg makes it bold.
fn colored(text: impl Into<String>, color: Color, bold: bool) -> impl Scene {
    let text = text.into();
    bsn! {
        Node
        Propagate::<TextColor>(TextColor(color))
        Propagate::<Bold>(Bold(bold))
        Children [ Text(text) ]
    }
}

fn plain(text: impl Into<String>) -> impl Scene {
    let text = text.into();
    bsn! { Node Children [ Text(text) ] }
}

/// `IntroductionSection`: flavor, the guide, the points and sacrifices, the objectives.
fn introduction(data: &HereticInfo) -> impl Scene + use<> {
    let guide = [
        "- Ищите на станции рушащие реальность влияния. Они не видны обычному глазу. Нажмите \
         правой кнопкой мыши по ним чтобы получить очки знаний. После добычи, они вскоре \
         становятся видимыми для всех. Сноведения о Мансусе помогут найти их.",
        "- Используйте ваше живое сердце, чтобы найти цели для жертвоприношения, но будьте \
         аккуратны: пульсируя, оно будет издавать звук сердцебиения на коротком расстоянии. Эта \
         способность связана с вашим сердцем - если вы его потеряете, совершите ритуал, чтобы \
         вернуть её.",
        "- Нарисуйте руну трансмутации, используя инструмент для рисования (ручка или карандаш) \
         на полу. Необходимо иметь хватку Мансуса в вашей другой руке. Эта руна позволяет \
         совершать ритуалы и жертвоприношения.",
        "- Следуйте за зовом живого сердца, чтобы найти свои цели. Принесите их на руну \
         трансмутации в критическом, или хуже, состоянии для жертвоприношения, которое даст очки \
         знаний. Мансус примет ТОЛЬКО цели, указанные вашим живым сердцем.",
        "- Сделайте себе фокусировку, чтобы читать более продвинутые заклинания, которые помогут \
         вам с более сложными жертвоприношениями.",
        "- Выполните все свои задачи, чтобы узнать финальный ритуал. Завершите его, чтобы стать \
         всемогущим!",
    ];
    let guide: Vec<_> = guide.iter().map(|g| plain(*g)).collect();
    let aura = format!(
        "ВНИМАНИЕ! При накоплении в общем {} очков знаний, вас окружит энергия Мансуса. Для \
         получения ауры необходимо потратить очки знаний, простое получение не раскроет вас.",
        data.points_to_aura
    );
    let points = format!("Доступно очков знаний: {}", data.charges);
    let sacrifices = format!("Жертвоприношений сделано: {}", data.total_sacrifices);
    let ascended = data
        .ascended
        .then(|| EntityScene(colored("Вы ВОЗВЫСИЛИСЬ!", YELLOW, true)));
    let objectives = (!data.ascended).then(|| {
        let mut lines: Vec<String> = data
            .objectives
            .iter()
            .map(|o| format!("#{}: {}", o.count, o.explanation))
            .collect();
        if lines.is_empty() {
            lines.push("Ни одной!".to_owned());
        }
        let lines: Vec<_> = lines.into_iter().map(plain).collect();
        EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Column, row_gap: px(4) }
            Name("heretic info: objectives")
            Children [
                Node Propagate::<Bold>(Bold(true))
                Children [ Text("Для вознесения вам нужно выполнить следующие задачи") ]
                --
                {lines}
            ]
        })
    });
    bsn! {
        @Section { @title: "Вы Еретик!", @fill: true, @scrollable: true }
        Children [
            Node { flex_direction: FlexDirection::Column, row_gap: px(8) }
            Propagate::<FontPx>(FontPx(14.0))
            Children [
                Node { flex_direction: FlexDirection::Column }
                Children [
                    {EntityScene(plain(FLAVOR))}
                    --
                    {EntityScene(colored("Врата Мансуса открылись для вашего разума.", PURPLE, true))}
                ]
                --
                Node { flex_direction: FlexDirection::Column, row_gap: px(4) }
                Propagate::<FontPx>(FontPx(12.0))
                Children [ {guide} -- {EntityScene(colored(aura, RED, false))} ]
                --
                Node { flex_direction: FlexDirection::Column }
                Name("heretic info: points")
                Children [ {ascended} -- {EntityScene(colored(points, BLUE, true))} -- {EntityScene(plain(sacrifices))} ]
                --
                {objectives}
            ]
        ]
    }
}

/// `formatTooltipText`: `<br>` lines, `&bull;` bullets.
fn tooltip_text(text: &str) -> String {
    text.replace("<br>", "\n").replace("&bull;", "•")
}

/// `KnowledgeNode`: the background, the icon, the cost; clicking researches it.
fn knowledge_node(
    st: &HereticStatic,
    node: &KnowledgeNode,
    charges: u32,
    can_buy: bool,
) -> impl Scene + use<> {
    let buyable = can_buy && !node.done && !node.disabled;
    let background = if !can_buy {
        node.bgr_icon
    } else if node.done {
        st.node_finished
    } else if charges < node.cost || node.disabled {
        st.node_locked
    } else {
        node.bgr_icon
    };
    let mut tip = format!("{}\nРезультат: {}", node.name, tooltip_text(&node.desc));
    if !node.notice.is_empty() {
        tip.push_str(&format!("\n{}", tooltip_text(&node.notice)));
    }
    if !node.info.is_empty() {
        tip.push_str(&format!("\nРецепт: {}", tooltip_text(&node.info)));
    }
    let (size, icon_size, inset) = if node.ascension {
        (192.0, 152.0, 20.0)
    } else {
        (64.0, 64.0, 0.0)
    };
    let cost = buyable.then(|| {
        let text = if node.cost > 0 {
            node.cost.to_string()
        } else {
            "ДАР".to_owned()
        };
        EntityScene(bsn! {
            Node {
                position_type: PositionType::Absolute,
                left: px(2),
                top: px(2),
                padding: UiRect::horizontal(px(3)),
                border_radius: BorderRadius::MAX,
            }
            BackgroundColor(Color::BLACK)
            Propagate::<Bold>(Bold(true))
            Propagate::<TextColor>(TextColor(Color::WHITE))
            Children [ Text(text) ]
        })
    });
    let sunset = node
        .ascension
        .then(|| EntityScene(colored("ЗАКАТ", PURPLE, true)));
    let params = ActParams(vec![
        ("path".to_owned(), node.path.clone()),
        ("category".to_owned(), node.category.clone()),
    ]);
    let name = Name::new(format!("knowledge: {}", node.name));
    let icon = node.icon;
    bsn! {
        Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, margin: UiRect::all(px(8)) }
        Children [
            @ss13_ui_kit::Button { @tooltip: tip }
            ss13_ui_kit::Button { color: UiColor::Transparent, disabled: {!buyable} }
            Node { width: px(size), height: px(size), padding: UiRect::ZERO, border_radius: BorderRadius::MAX }
            Act({RESEARCH.to_owned()})
            params
            name
            Children [
                Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: px(size), height: px(size) }
                UiSprite { icon_state: background, color: None }
                --
                Node { position_type: PositionType::Absolute, left: px(inset), top: px(inset), width: px(icon_size), height: px(icon_size) }
                UiSprite { icon_state: icon, color: None }
                --
                {cost}
            ]
            --
            {sunset}
        ]
    }
}

/// `ResearchInfo`: the points, the knowledge tree and the shop by tier.
fn research(st: &HereticStatic, data: &HereticInfo) -> impl Scene + use<> {
    let points = format!("Доступные очки знаний : {}.", data.charges);
    let tiers: Vec<_> = data
        .knowledge_tiers
        .iter()
        .filter(|t| !t.is_empty())
        .map(|tier| {
            let nodes: Vec<_> = tier
                .iter()
                .map(|n| knowledge_node(st, n, data.charges, true))
                .collect();
            bsn! {
                Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::bottom(px(1)),
                }
                BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.2))
                Children [ {nodes} ]
            }
        })
        .collect();
    let empty = tiers.is_empty().then(|| EntityScene(plain("Нет!")));
    let mut shop_tiers: Vec<Vec<&KnowledgeNode>> = Vec::new();
    for n in &data.knowledge_shop {
        let i = n.depth.max(1) as usize - 1;
        while shop_tiers.len() <= i {
            shop_tiers.push(Vec::new());
        }
        shop_tiers[i].push(n);
    }
    let shop = (!data.knowledge_shop.is_empty()).then(|| {
        let rows: Vec<_> = shop_tiers
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.is_empty())
            .map(|(i, tier)| {
                let title = format!("Уровень {}", i + 1);
                let nodes: Vec<_> = tier
                    .iter()
                    .map(|n| knowledge_node(st, n, data.charges, true))
                    .collect();
                bsn! {
                    Node { flex_direction: FlexDirection::Column, border: UiRect::bottom(px(1)) }
                    BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.2))
                    Children [
                        {EntityScene(plain(title))}
                        --
                        Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap }
                        Children [ {nodes} ]
                    ]
                }
            })
            .collect();
        EntityScene(bsn! {
            @StackItem { @grow: 1.0 }
            Node { min_width: px(0) }
            Children [
                @Section { @title: "Магазин знаний", @fill: true, @scrollable: true }
                Children [ Node { flex_direction: FlexDirection::Column } Children [ {rows} ] ]
            ]
        })
    });
    bsn! {
        @Stack { @vertical: true, @fill: true }
        Children [
            @StackItem
            Node { justify_content: JustifyContent::Center, margin: UiRect::bottom(px(6)) }
            Propagate::<FontPx>(FontPx(20.0))
            Name("heretic research: points")
            Children [ Text(points) ]
            --
            @StackItem { @grow: 1.0 }
            Node { min_height: px(0) }
            Children [
                @Stack { @fill: true }
                Children [
                    @StackItem { @grow: 1.0 }
                    Node { min_width: px(0) }
                    Children [
                        @Section { @title: "Древо знаний", @fill: true, @scrollable: true }
                        Children [
                            Node { flex_direction: FlexDirection::Column }
                            Children [
                                Node { justify_content: JustifyContent::Center }
                                Propagate::<FontPx>(FontPx(32.0))
                                Children [ {EntityScene(colored("РАССВЕТ", YELLOW, true))} ]
                                --
                                {empty}
                                --
                                {tiers}
                            ]
                        ]
                    ]
                    --
                    {shop}
                ]
            ]
        ]
    }
}

/// `PathInfo`: the paths' tabs and the previewed one, or the chosen path.
fn path_info(st: &HereticStatic, data: &HereticInfo, local: &UiLocalState) -> impl Scene + use<> {
    let current = data.current_path(&st.paths);
    let selected = match current {
        Some(p) => st
            .paths
            .iter()
            .position(|x| x.route == p.route)
            .unwrap_or(0),
        None => local.get(PATH_TAB).parse().unwrap_or(0),
    };
    let tabs = current.is_none().then(|| {
        let tabs: Vec<_> = st
            .paths
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let route = p.route.clone();
                bsn! {
                    @TabsTab { @content: route, @icon: "info" }
                    TabsTab { selected: {selected == i} }
                    SetLocal { key: {PATH_TAB.to_owned()}, value: {i.to_string()}, rerender: true }
                }
            })
            .collect();
        EntityScene(bsn! {
            @StackItem
            Node { width: px(180), flex_shrink: 0.0 }
            Children [ @Tabs { @vertical: true, @fill: true } Children [ {tabs} ] ]
        })
    });
    let content = st.paths.get(selected).map(|p| {
        let inner = if current.is_some() {
            Box::new(EntityScene(path_selected(p, data.passive_level))) as Box<dyn SceneList>
        } else {
            Box::new(EntityScene(path_unselected(st, p, data.charges))) as Box<dyn SceneList>
        };
        let title = p.route.clone();
        EntityScene(bsn! {
            @Section { @title: title, @fill: true, @scrollable: true }
            Children [ Node { flex_direction: FlexDirection::Column, row_gap: px(8) } Children [ {inner} ] ]
        })
    });
    bsn! {
        @Stack { @fill: true }
        Children [
            {tabs}
            --
            @StackItem { @grow: 1.0 }
            Node { min_width: px(0) }
            Children [ {content} ]
        ]
    }
}

fn bullets(title: &str, items: &[String]) -> impl Scene + use<> {
    let title = format!("{title}:");
    let items: Vec<_> = items.iter().map(|i| plain(format!("• {i}"))).collect();
    bsn! {
        Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, flex_basis: px(0), row_gap: px(4) }
        Children [ Node Propagate::<Bold>(Bold(true)) Children [ Text(title) ] -- {items} ]
    }
}

/// `PathContentUnselected`: the starting knowledge to buy, complexity, description, passive,
/// the path's abilities, pros and cons.
fn path_unselected(st: &HereticStatic, path: &HereticPath, charges: u32) -> impl Scene + use<> {
    let start = EntityScene(knowledge_node(st, &path.starting_knowledge, charges, true));
    let complexity_color = parse_hex(&path.complexity_color).unwrap_or(Color::WHITE);
    let complexity = EntityScene(colored(path.complexity.clone(), complexity_color, true));
    let description: Vec<_> = path.description.iter().map(|l| plain(l.clone())).collect();
    let passive = format!("Пассивный навык: {}", path.passive_name);
    let recharge = EntityScene(plain(path.passive_recharge.clone()));
    let first = EntityScene(plain(
        path.passive_description
            .first()
            .cloned()
            .unwrap_or_default(),
    ));
    let abilities: Vec<_> = path
        .preview_abilities
        .iter()
        .map(|a| knowledge_node(st, a, charges, false))
        .collect();
    let pros = EntityScene(bullets("Сильные стороны", &path.pros));
    let cons = EntityScene(bullets("Слабые стороны", &path.cons));
    bsn! {
        Node { flex_direction: FlexDirection::Column, row_gap: px(8) }
        Children [
            Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center }
            Children [
                {start}
                --
                Node { flex_direction: FlexDirection::Row, column_gap: px(6) }
                Propagate::<Bold>(Bold(true))
                Children [ Node Children [ Text("Сложность:") ] -- {complexity} ]
            ]
            --
            Node { flex_direction: FlexDirection::Column } Children [ {description} ]
            --
            Node { flex_direction: FlexDirection::Row }
            Children [
                Node { flex_direction: FlexDirection::Column, width: percent(50) }
                Children [
                    Node Propagate::<Bold>(Bold(true)) Children [ Text(passive) ]
                    --
                    {recharge}
                    --
                    {first}
                ]
                --
                Node { flex_direction: FlexDirection::Column, width: percent(50) }
                Children [
                    Node Propagate::<Bold>(Bold(true)) Children [ Text("Способности пути:") ]
                    --
                    Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, justify_content: JustifyContent::Center }
                    Children [ {abilities} ]
                ]
            ]
            --
            Node { flex_direction: FlexDirection::Row, column_gap: px(8) }
            Children [ {pros} -- {cons} ]
        ]
    }
}

/// `PathContentSelected`: description, the passive's levels, the tips.
fn path_selected(path: &HereticPath, level: u32) -> impl Scene + use<> {
    let description: Vec<_> = path.description.iter().map(|l| plain(l.clone())).collect();
    let passive = format!("Пассивный навык: {}, уровень: {level}", path.passive_name);
    let recharge = EntityScene(plain(path.passive_recharge.clone()));
    let levels: Vec<_> = path
        .passive_description
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let active = level as usize > i;
            let text = format!("Уровень {}\n{line}", i + 1);
            let color = if active {
                GREEN
            } else {
                Color::srgba(1.0, 1.0, 1.0, 0.6)
            };
            colored(text, color, false)
        })
        .collect();
    let tips: Vec<_> = path.tips.iter().map(|t| plain(format!("• {t}"))).collect();
    bsn! {
        Node { flex_direction: FlexDirection::Column, row_gap: px(8) }
        Children [
            Node { flex_direction: FlexDirection::Column } Children [ {description} ]
            --
            Node { flex_direction: FlexDirection::Column, row_gap: px(4) }
            Children [
                Node Propagate::<Bold>(Bold(true)) Children [ Text(passive) ]
                --
                {recharge}
                --
                {levels}
            ]
            --
            Node { flex_direction: FlexDirection::Column, row_gap: px(4) }
            Children [ Node Propagate::<Bold>(Bold(true)) Children [ Text("Советы:") ] -- {tips} ]
        ]
    }
}

fn parse_hex(s: &str) -> Option<Color> {
    let s = s.strip_prefix('#')?;
    let v = u32::from_str_radix(s, 16).ok()?;
    Some(Color::srgb_u8((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltips_and_colors_parse() {
        assert_eq!(tooltip_text("a<br>&bull; b"), "a\n• b");
        assert_eq!(parse_hex("#008000"), Some(Color::srgb_u8(0, 128, 0)));
        assert_eq!(parse_hex("nope"), None);
    }
}

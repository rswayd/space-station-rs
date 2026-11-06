//! The `Cargo` window (client): tgui `Cargo/index.tsx` with `CargoStatus`, `CargoCatalog`,
//! `CargoRequests`, `CargoCart`, `CargoCartButtons` and `CargoHelp`. The data is
//! [`super::CargoData`], the server half [`super::console`].
//!
//! Client-only state (`useSharedState`/`useState`): the tab, the catalogue group, the pack whose
//! contents are shown and the pack a request is being written for ([`UiLocalState`] keys).
// shortcut: no search bar and no amount input in the cart (the -/+ buttons stay); the request
// reason is asked in a modal here instead of tg's server-side `tgui_input_text`, and the budget
// (tg's `tgui_alert` "Cargo Budget" or the department) is the button that sends it.

use bevy::app::Propagate;
use bevy::scene::EntityScene;
use bevy::ui_widgets::Activate;
use ss13_net::client::SendIntent;
use ss13_shared::protocol::ClientIntent;
use ss13_tgui::client::{
    AppTguiClientExt, ClientUi, ClientWindow, LocalOnInput, SetLocal, UiLocalState, UiSprite,
};
use ss13_tgui::interfaces::vending::ProductIcon;
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::{Bold, FontPx, Italic};

use super::*;

/// `useState` keys.
const TAB: &str = "cargotab";
const GROUP: &str = "supply";
const INFO: &str = "info";
const REQUESTING: &str = "requesting";
const INPUT: &str = "reason";

/// Characters of a reason: act params are capped at 256 bytes, two per Cyrillic letter.
const REASON_MAX: usize = 120;

const TAB_CATALOG: &str = "catalog";
const TAB_REQUESTS: &str = "active requests";
const TAB_CART: &str = "cart";
const TAB_HELP: &str = "help";

pub struct CargoClientPlugin;

impl Plugin for CargoClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<CargoUi>()
            .register_ui_view::<CargoExpressUi>()
            .register_ui_view::<MatMarketUi>()
            .add_observer(submit_request);
    }
}

/// On the request modal's submit button: `act('add', { id, reason })`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct SubmitRequest(pub String);

/// With [`SubmitRequest`]: the requester's department pays (`department: 1`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct FromDepartment;

fn submit_request(
    activate: On<Activate>,
    submits: Query<(&SubmitRequest, Has<FromDepartment>)>,
    parents: Query<&ChildOf>,
    mut windows: Query<(&ClientWindow, &mut UiLocalState)>,
    mut intents: MessageWriter<SendIntent>,
) {
    let Ok((submit, department)) = submits.get(activate.entity) else {
        return;
    };
    let Some(root) = parents
        .iter_ancestors(activate.entity)
        .find(|e| windows.contains(*e))
    else {
        return;
    };
    let Ok((window, mut local)) = windows.get_mut(root) else {
        return;
    };
    let reason = local.get(INPUT).to_owned();
    let mut params: Vec<(String, String)> =
        vec![(ID.into(), submit.0.clone()), (REASON.into(), reason)];
    if department {
        params.push((DEPARTMENT.into(), "1".into()));
    }
    intents.write(SendIntent(ClientIntent::UiAct {
        window: window.id,
        action: ADD.into(),
        params,
    }));
    local.set(INPUT, "");
}

/// tgui-core `formatMoney`: thousands separated by a thin space.
pub fn format_money(value: i64) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('\u{2009}');
        }
        out.push(c);
    }
    if value < 0 {
        out.insert(0, '-');
    }
    out
}

/// tgui-core `toTitleCase` as `Cargo/index.tsx` uses it on the tab name.
fn title_case(text: &str) -> String {
    text.split(' ')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

impl ClientUi for CargoUi {
    const WIDTH: f32 = 800.0;
    const HEIGHT: f32 = 750.0;

    fn content(st: &CargoStatic, data: &CargoData) -> impl Scene {
        view(st, data, &UiLocalState::default())
    }

    fn content_local(st: &CargoStatic, data: &CargoData, local: &UiLocalState) -> impl Scene {
        view(st, data, local)
    }
}

fn view(st: &CargoStatic, data: &CargoData, local: &UiLocalState) -> impl Scene + use<> {
    let tab = match local.get(TAB) {
        "" => TAB_CATALOG,
        t => t,
    };
    let tab = if data.requestonly && matches!(tab, TAB_CART | TAB_HELP) {
        TAB_CATALOG
    } else {
        tab
    };
    let (amount, _) = data.cart_totals();
    let tab_button = |key: &'static str, icon: &'static str, label: String, notify: bool| {
        let color = if tab != key && notify {
            UiColor::Yellow
        } else {
            UiColor::Default
        };
        EntityScene(bsn! {
            @TabsTab { @content: label, @icon: icon }
            TabsTab { selected: {tab == key}, color: color }
            SetLocal { key: {TAB.to_owned()}, value: {key.to_owned()}, rerender: true }
        })
    };
    let requests_tab = tab_button(
        TAB_REQUESTS,
        "envelope",
        format!("Requests ({})", data.requests.len()),
        !data.requests.is_empty(),
    );
    let more_tabs = (!data.requestonly).then(|| {
        let cart = tab_button(
            TAB_CART,
            "shopping-cart",
            format!("Checkout ({amount})"),
            amount > 0,
        );
        let help = tab_button(TAB_HELP, "question", "Help".into(), false);
        EntityScene(bsn! { Node Children [ {cart} -- {help} ] })
    });
    let catalog_tab = tab_button(TAB_CATALOG, "list", "Catalog".into(), false);
    let header_buttons: Box<dyn SceneList> = if tab == TAB_REQUESTS && !data.requestonly {
        Box::new(EntityScene(bsn! {
            @ss13_ui_kit::Button { @icon: "times", @content: "Clear" }
            ss13_ui_kit::Button { color: UiColor::Transparent }
            Act({DENY_ALL.to_owned()})
        }))
    } else if tab == TAB_CATALOG || tab == TAB_CART {
        Box::new(EntityScene(cart_buttons(data, st)))
    } else {
        Box::new(EntityScene(bsn! { Node }))
    };
    let body: Box<dyn SceneList> = match tab {
        TAB_REQUESTS => Box::new(EntityScene(requests_view(data, st))),
        TAB_CART => Box::new(EntityScene(cart_view(data, st))),
        TAB_HELP => Box::new(EntityScene(help_view())),
        _ => Box::new(EntityScene(catalog_view(st, data, local, false))),
    };
    let modal = modal(st, data, local);
    let title = title_case(tab);
    bsn! {
        @WindowContent
        Children [
            @Stack { @vertical: true, @fill: true }
            Children [
                @StackItem
                Children [ {EntityScene(status_view(data, st))} ]
                --
                @StackItem
                Children [
                    @Section { @title: title, @buttons: header_buttons }
                    Children [
                        @Tabs { @fluid: true }
                        Children [ {catalog_tab} -- {requests_tab} -- {more_tabs} ]
                    ]
                ]
                --
                @StackItem { @grow: 1.0 }
                Node { min_height: px(0) }
                Children [ {body} ]
            ]
            --
            {modal}
        ]
    }
}

/// `CargoStatus`.
fn status_view(data: &CargoData, st: &CargoStatic) -> impl Scene + use<> {
    let points = format!(
        "{}{}",
        format_money(data.points),
        st.displayed_currency_name
    );
    let location = data.location.clone();
    let shuttle: Box<dyn SceneList> = if data.docked && !data.requestonly && data.can_send {
        Box::new(EntityScene(bsn! {
            @ss13_ui_kit::Button { @content: location }
            ss13_ui_kit::Button { color: UiColor::Green }
            Act({SEND.to_owned()})
            Name("cargo: send")
        }))
    } else {
        Box::new(EntityScene(bsn! { Node Children [ Text(location) ] }))
    };
    let message = data.message.clone();
    let department = data.department.clone();
    bsn! {
        @Section {
            @title: department,
            @buttons: bsn! {
                Node
                Propagate::<Bold>(Bold(true))
                Children [ Text(points) Name("cargo: points") ]
            },
        }
        Children [
            @LabeledList
            Children [
                @LabeledListItem { @label: "Shuttle" }
                Children [ {shuttle} ]
                --
                @LabeledListItem { @label: "CentCom Message", @content: message }
            ]
        ]
    }
}

/// `CargoCartButtons`.
fn cart_buttons(data: &CargoData, st: &CargoStatic) -> impl Scene + use<> {
    let (amount, total) = data.cart_totals();
    let mut text = match amount {
        0 => "Cart is empty".to_owned(),
        1 => "1 item".to_owned(),
        n => format!("{n} items"),
    };
    if total > 0 {
        text.push_str(&format!(
            " ({}{})",
            format_money(i64::from(total)),
            st.displayed_currency_name
        ));
    }
    let can_clear =
        !data.requestonly && data.can_send && data.can_approve_requests && !data.cart.is_empty();
    bsn! {
        Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
        Children [
            Node { margin: UiRect::horizontal(px(6)) }
            Children [ Text(text) ]
            --
            @ss13_ui_kit::Button { @icon: "times", @content: "Clear" }
            ss13_ui_kit::Button { color: UiColor::Transparent, disabled: {!can_clear} }
            Act({CLEAR.to_owned()})
        ]
    }
}

fn icon_scene(icon: Option<ProductIcon>) -> Box<dyn SceneList> {
    match icon {
        Some(icon) => Box::new(EntityScene(bsn! {
            Node { width: percent(100), height: percent(100) }
            UiSprite { icon_state: {Some(icon.icon_state)}, color: {icon.color} }
        })),
        None => Box::new(EntityScene(bsn! {
            Node {
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
            }
            Children [ @Icon { @name: "question", @size: 2.0 } ]
        })),
    }
}

/// `CargoCatalog`: the group tabs and the selected group's packs; `express` (the express
/// console's) without "Buy Privately".
pub(super) fn catalog_view(
    st: &CargoStatic,
    data: &CargoData,
    local: &UiLocalState,
    express: bool,
) -> impl Scene + use<> {
    let selected = match local.get(GROUP) {
        "" => st
            .groups
            .first()
            .map(|g| g.name.clone())
            .unwrap_or_default(),
        g => g.to_owned(),
    };
    let groups: Vec<_> = st
        .groups
        .iter()
        .map(|group| {
            let is_selected = group.name == selected;
            let name = group.name.clone();
            let count = group.packs.len().to_string();
            let color = if is_selected {
                UiColor::Green
            } else {
                UiColor::Default
            };
            bsn! {
                @TabsTab
                TabsTab { selected: is_selected, color: color }
                SetLocal { key: {GROUP.to_owned()}, value: {name.clone()}, rerender: true }
                Children [
                    Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::SpaceBetween,
                        flex_grow: 1.0,
                    }
                    Children [ Text(name) -- Text(count) ]
                ]
            }
        })
        .collect();
    let (private_color, private_icon) = if data.self_paid {
        (UiColor::Yellow, "check-square-o")
    } else {
        (UiColor::Transparent, "square-o")
    };
    let private = (!express).then(|| {
        EntityScene(bsn! {
            @StackItem
            Children [
                @ss13_ui_kit::Button {
                    @icon: private_icon,
                    @content: "Buy Privately",
                    @fluid: true,
                    @tooltip: "Use your own funds to purchase items.",
                    @tooltip_position: Placement::Top,
                }
                ss13_ui_kit::Button { color: private_color }
                Act({TOGGLE_PRIVATE.to_owned()})
            ]
        })
    });
    let grow = if express { 2.0 } else { 3.0 };
    let packs: Vec<_> = st
        .groups
        .iter()
        .find(|g| g.name == selected)
        .map(|g| g.packs.iter().map(|p| pack_row(p, data, st)).collect())
        .unwrap_or_default();
    bsn! {
        @Stack { @fill: true }
        Children [
            @StackItem { @grow: 1.0 }
            Node { min_width: px(0), max_width: px(260) }
            Children [
                @Section { @fill: true }
                Children [
                    @Stack { @vertical: true, @fill: true }
                    Children [
                        @StackItem { @grow: 1.0 }
                        Node { min_height: px(0), overflow: Overflow::scroll_y() }
                        Children [
                            @Tabs { @vertical: true }
                            Children [ {groups} ]
                        ]
                        --
                        {private}
                    ]
                ]
            ]
            --
            @StackItem { @grow: grow }
            Node { min_width: px(0) }
            Children [
                @Section { @fill: true, @scrollable: true }
                Children [ {packs} ]
            ]
        ]
    }
}

/// `CatalogList` entry.
fn pack_row(pack: &SupplyRecord, data: &CargoData, st: &CargoStatic) -> impl Scene + use<> {
    let digits = (pack.cost.max(1) as f64).log10().floor() as u32 + 1;
    let color = if data.self_paid {
        UiColor::Yellow
    } else if (5..=6).contains(&digits) {
        UiColor::Orange
    } else if digits > 6 {
        UiColor::Bad
    } else {
        UiColor::Default
    };
    let private_buy = data.self_paid && !pack.goody;
    let amount = data
        .cart
        .iter()
        .find(|e| e.object == pack.name)
        .map_or(0, |e| e.amount);
    let mut cost = format!(
        "{}{}",
        format_money(i64::from(pack.cost)),
        st.displayed_currency_name
    );
    if private_buy {
        cost = format!(
            "{}{}",
            format_money((f64::from(pack.cost) * 1.1).round() as i64),
            st.displayed_currency_name
        );
    }
    let lock = pack.access.then(|| {
        EntityScene(bsn! {
            Node { margin: UiRect::horizontal(px(4)) }
            Children [ @Icon { @name: "lock", @color: UiColor::Average } ]
        })
    });
    let name = pack.name.clone();
    let info = pack.id.clone();
    let gold = UiColor::Gold.text().unwrap_or_default();
    let small = ss13_ui_kit::theme::FONT_SIZE * 0.85;
    let image = icon_scene(pack.icon);
    // `add` orders at once; the request console's `add` without a reason adds nothing, the
    // reason modal (`requesting`) sends it with one.
    let params = ActParams(vec![(ID.to_owned(), pack.id.clone())]);
    let requesting = SetLocal {
        key: REQUESTING.to_owned(),
        value: pack.id.clone(),
        rerender: data.requestonly && !data.self_paid,
    };
    let button_name = Name::new(format!("pack: {}", pack.name));
    bsn! {
        Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
        Children [
            Node { flex_grow: 1.0, min_width: px(0), flex_direction: FlexDirection::Column }
            Children [
                @ImageButton { @image: image, @fluid: true, @image_size: 32.0 }
                ImageButton { color: color, disabled: {amount >= st.max_order} }
                button_name
                Act({ADD.to_owned()})
                params
                requesting
                Children [
                    Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                    Children [
                        Node { flex_grow: 1.0 } Children [ Text(name) ]
                        --
                        {lock}
                        --
                        Node { width: px(80), justify_content: JustifyContent::FlexEnd }
                        Propagate::<TextColor>(TextColor(gold))
                        Propagate::<FontPx>(FontPx(small))
                        Children [ Text(cost) ]
                    ]
                ]
            ]
            --
            @ss13_ui_kit::Button { @icon: "info" }
            ss13_ui_kit::Button { color: UiColor::Transparent }
            SetLocal { key: {INFO.to_owned()}, value: info, rerender: true }
        ]
    }
}

/// `CatalogPackInfo` or the request reason modal.
pub(super) fn modal(
    st: &CargoStatic,
    data: &CargoData,
    local: &UiLocalState,
) -> Option<EntityScene<impl Scene + use<>>> {
    let find = |id: &str| {
        st.groups
            .iter()
            .flat_map(|g| &g.packs)
            .find(|p| p.id == id)
            .cloned()
    };
    if data.requestonly
        && !data.self_paid
        && let Some(pack) = find(local.get(REQUESTING))
    {
        let title = format!("Request: {}", pack.name);
        let body: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Column, width: px(360) }
            Children [
                Node { margin: UiRect::bottom(px(6)) }
                Children [ Text("Reason") ]
                --
                Node { margin: UiRect::bottom(px(6)) }
                Input {
                    placeholder: {"Reason".to_owned()},
                    max_length: {Some(REASON_MAX)},
                    fluid: true,
                    auto_focus: true,
                }
                LocalOnInput({INPUT.to_owned()})
                --
                Node
                Children [
                    @ss13_ui_kit::Button { @icon: "envelope", @content: "Cargo Budget" }
                    ss13_ui_kit::Button { color: UiColor::Good }
                    SubmitRequest({pack.id.clone()})
                    SetLocal { key: {REQUESTING.to_owned()}, value: {String::new()}, rerender: true }
                    --
                    @ss13_ui_kit::Button { @icon: "building", @content: "Department Budget" }
                    ss13_ui_kit::Button { color: UiColor::Good }
                    SubmitRequest({pack.id.clone()})
                    FromDepartment
                    SetLocal { key: {REQUESTING.to_owned()}, value: {String::new()}, rerender: true }
                    --
                    @ss13_ui_kit::Button { @icon: "times", @content: "Cancel" }
                    ss13_ui_kit::Button { color: UiColor::Bad }
                    SetLocal { key: {REQUESTING.to_owned()}, value: {String::new()}, rerender: true }
                ]
            ]
        }));
        return Some(EntityScene(modal_frame(title, body)));
    }
    let pack = find(local.get(INFO))?;
    let desc = if pack.desc.is_empty() {
        "No description available.".to_owned()
    } else {
        pack.desc.clone()
    };
    let items: Vec<_> = pack
        .contains
        .iter()
        .map(|item| {
            let image = icon_scene(item.icon);
            let name = item.name.clone();
            let amount = format!("x{}", item.amount);
            bsn! {
                @ImageButton {
                    @image: image,
                    @fluid: true,
                    @image_size: 32.0,
                    @buttons_alt: bsn! { Node Children [ Text(amount) ] },
                }
                Children [ Text(name) ]
            }
        })
        .collect();
    let body: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Column, width: px(400) }
        Children [
            Node { margin: UiRect::bottom(px(6)) }
            Propagate::<Italic>(Italic(true))
            Children [ Text(desc) ]
            --
            Node { flex_direction: FlexDirection::Column, max_height: px(300), overflow: Overflow::scroll_y() }
            Children [ {items} ]
        ]
    }));
    Some(EntityScene(modal_frame(pack.name.clone(), body)))
}

fn modal_frame(title: String, body: Box<dyn SceneList>) -> impl Scene + use<> {
    bsn! {
        @Modal
        Children [
            @Section {
                @title: title,
                @buttons: bsn! {
                    @ss13_ui_kit::Button { @icon: "close" }
                    ss13_ui_kit::Button { color: UiColor::Bad }
                    SetLocal { key: {INFO.to_owned()}, value: {String::new()}, rerender: true }
                },
            }
            Children [ {body} ]
        ]
    }
}

/// `CargoRequests`.
fn requests_view(data: &CargoData, st: &CargoStatic) -> impl Scene + use<> {
    let actions = (!data.requestonly || data.can_send) && data.can_approve_requests;
    let empty = data.requests.is_empty().then(|| {
        EntityScene(bsn! {
            @NoticeBox { @success: true }
            Children [ Text("No Requests") ]
        })
    });
    let rows: Vec<_> = data
        .requests
        .iter()
        .map(|r| {
            let id = format!("#{}", r.id);
            let cost = format!(
                "{}{}",
                format_money(i64::from(r.cost)),
                st.displayed_currency_name
            );
            let buttons = actions.then(|| {
                let params = ActParams(vec![(ID.to_owned(), r.id.to_string())]);
                let deny = params.clone();
                EntityScene(bsn! {
                    @TableCell { @collapsing: true }
                    Children [
                        @ss13_ui_kit::Button { @icon: "check" }
                        ss13_ui_kit::Button { color: UiColor::Good }
                        Act({APPROVE.to_owned()})
                        params
                        Name("cargo: approve")
                        --
                        @ss13_ui_kit::Button { @icon: "times" }
                        ss13_ui_kit::Button { color: UiColor::Bad }
                        Act({DENY.to_owned()})
                        deny
                    ]
                })
            });
            bsn! {
                @TableRow
                Children [
                    @TableCell { @collapsing: true, @content: id }
                    --
                    @TableCell { @content: {r.object.clone()} }
                    --
                    @TableCell { @bold: true, @content: {r.orderer.clone()} }
                    --
                    @TableCell { @content: {r.reason.clone()} }
                    --
                    @TableCell { @collapsing: true, @content: {r.account.clone()} }
                    --
                    @TableCell { @collapsing: true, @color: UiColor::Gold, @content: cost }
                    --
                    {buttons}
                ]
            }
        })
        .collect();
    let header = (!data.requests.is_empty()).then(|| {
        let actions = actions.then(|| EntityScene(bsn! { @TableCell { @content: "Actions" } }));
        EntityScene(bsn! {
            @TableRow { @header: true }
            Children [
                @TableCell { @content: "ID" } -- @TableCell { @content: "Object" } --
                @TableCell { @content: "Orderer" } -- @TableCell { @content: "Reason" } --
                @TableCell { @content: "Account" } -- @TableCell { @content: "Cost" } --
                {actions}
            ]
        })
    });
    bsn! {
        @Section { @fill: true, @scrollable: true }
        Children [
            {empty}
            --
            @Table
            Children [ {header} -- {rows} ]
        ]
    }
}

/// `CargoCart`.
fn cart_view(data: &CargoData, st: &CargoStatic) -> impl Scene + use<> {
    let empty = data
        .cart
        .is_empty()
        .then(|| EntityScene(bsn! { @NoticeBox Children [ Text("Nothing in cart") ] }));
    let rows: Vec<_> = data
        .cart
        .iter()
        .map(|entry| {
            let id = format!("#{}", entry.id);
            let cost = format!("{} {}", format_money(i64::from(entry.cost)), MONEY_SYMBOL);
            let amount = entry.amount.to_string();
            let controls: Box<dyn SceneList> = if data.can_send && entry.can_be_cancelled {
                let name = vec![(ORDER_NAME.to_owned(), entry.object.clone())];
                Box::new(EntityScene(bsn! {
                    Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                    Children [
                        @ss13_ui_kit::Button { @icon: "minus" }
                        Act({REMOVE.to_owned()})
                        ActParams({name.clone()})
                        --
                        Node { width: px(40), justify_content: JustifyContent::Center }
                        Children [ Text(amount) ]
                        --
                        @ss13_ui_kit::Button { @icon: "plus" }
                        ss13_ui_kit::Button { disabled: {entry.amount >= st.max_order} }
                        Act({ADD_BY_NAME.to_owned()})
                        ActParams(name)
                    ]
                }))
            } else {
                Box::new(EntityScene(bsn! { Node Children [ Text(amount) ] }))
            };
            let private = (entry.paid > 0).then(|| format!("[Private x {}]", entry.amount));
            bsn! {
                @TableRow
                Children [
                    @TableCell { @collapsing: true, @color: UiColor::Label, @content: id }
                    --
                    @TableCell { @content: {entry.object.clone()} }
                    --
                    @TableCell Children [ {controls} ]
                    --
                    @TableCell {
                        @collapsing: true,
                        @color: UiColor::Average,
                        @bold: true,
                        @content: {private.unwrap_or_default()},
                    }
                    --
                    @TableCell {
                        @collapsing: true,
                        @color: UiColor::Gold,
                        @text_align: {Some(Justify::Right)},
                        @content: cost,
                    }
                ]
            }
        })
        .collect();
    let sendable = data.away && data.docked;
    let confirm = (!data.cart.is_empty() && data.can_send).then(|| {
        let tooltip = if sendable {
            String::new()
        } else {
            format!("Shuttle is at {}", data.location)
        };
        EntityScene(bsn! {
            @StackItem
            Children [
                @Section
                Children [
                    Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::FlexEnd }
                    Children [
                        @ss13_ui_kit::Button { @content: "Confirm the order", @tooltip: tooltip }
                        ss13_ui_kit::Button { color: UiColor::Green, disabled: {!sendable} }
                        Act({SEND.to_owned()})
                    ]
                ]
            ]
        })
    });
    bsn! {
        @Stack { @vertical: true, @fill: true }
        Children [
            @StackItem { @grow: 1.0 }
            Node { min_height: px(0) }
            Children [
                @Section { @fill: true, @scrollable: true }
                Children [ {empty} -- @Table Children [ {rows} ] ]
            ]
            --
            {confirm}
        ]
    }
}

/// `CargoHelp`, abridged.
fn help_view() -> impl Scene {
    let lines = [
        "Departmental Orders: requests from the request consoles land in Requests; approve them \
         to move them into the cart, deny them to drop them.",
        "Ordering: the cart is paid from the cargo budget when the shuttle leaves CentCom. \
         Private purchases are charged to your bank account (+10%) on delivery.",
        "Restricted crates (lock icon) open only with the access their pack needs.",
        "Selling: everything loose on the shuttle is sold when it docks at CentCom; empty \
         crates return 200 credits each. Living things are not allowed on the shuttle.",
    ];
    let rows: Vec<_> = lines
        .into_iter()
        .map(|l| bsn! { Node { margin: UiRect::bottom(px(8)) } Children [ Text(l) ] })
        .collect();
    bsn! {
        @Section { @fill: true, @scrollable: true }
        Children [ {rows} ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_and_titles() {
        assert_eq!(format_money(0), "0");
        assert_eq!(format_money(999), "999");
        assert_eq!(format_money(4999), "4\u{2009}999");
        assert_eq!(format_money(-1234567), "-1\u{2009}234\u{2009}567");
        assert_eq!(title_case("active requests"), "Active Requests");
    }
}

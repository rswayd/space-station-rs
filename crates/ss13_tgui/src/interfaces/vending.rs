//! `Vending`: `tgui/packages/tgui/interfaces/Vending.tsx` and
//! `code/modules/vending/vendor/ui_data.dm`. The server half (records, `vend`) lives with the
//! game (`ss13_game::vending`); this module has the data both halves share and the client view.
//!
//! Client-only state, as `useState` in the JSX: the search text, the selected category and the
//! grid/list layout ([`UiLocalState`](crate::client::UiLocalState) keys [`LOCAL_SEARCH`],
//! [`LOCAL_CATEGORY`], [`LOCAL_LAYOUT`]). Search and category filter the rendered rows in place
//! (no re-render while typing); the layout toggle re-renders.

use serde::{Deserialize, Serialize};
use ss13_shared::IconStateId;

use crate::UiInterface;

pub struct VendingUi;

/// A product's picture: the item type's icon state and color (tgui `dmIcon`/`dmIconState`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProductIcon {
    pub icon_state: IconStateId,
    pub color: Option<[u8; 4]>,
}

/// `collect_records_for_static_data` entry.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProductRecord {
    /// `SANITIZED_PATH`: the type path without `/obj/item/`, `/` as `-`.
    pub path: String,
    pub name: String,
    pub price: u32,
    /// tg `REF(record)`: the record's index in the vendor; `vend` sends it back.
    pub reference: u32,
    pub category: String,
    /// GAGS color selection; always false here (no greyscale menu yet).
    pub colorable: bool,
    pub premium: bool,
    /// `None`: no drawable icon (tgui shows its fallback).
    pub icon: Option<ProductIcon>,
}

/// `ui_static_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct VendingStatic {
    /// One of `product_ads`, picked on open; empty: none.
    pub ad: String,
    pub all_products_free: bool,
    pub department: String,
    /// `DEPARTMENT_DISCOUNT`.
    pub job_discount: f32,
    pub displayed_currency_icon: String,
    /// With tg's leading space: `" ¢"`.
    pub displayed_currency_name: String,
    pub product_records: Vec<ProductRecord>,
    pub coin_records: Vec<ProductRecord>,
    pub hidden_records: Vec<ProductRecord>,
    /// `categories`, in the order the DM assoc list builds them: (name, Font Awesome icon).
    pub categories: Vec<(String, String)>,
}

/// `ui_data` `user`: the account of the user's ID.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UserData {
    pub name: String,
    /// Account balance plus held cash. `None`: there is no economy, products cost nothing.
    // shortcut: no bank accounts yet; `Some` once the economy exists.
    pub cash: Option<u32>,
    pub job: String,
    pub department: String,
    pub is_captain_card: bool,
}

/// `ui_data` `stock` entry.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct StockItem {
    pub amount: u32,
    /// Returned items wait in the vendor and are free.
    pub free: bool,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct VendingData {
    /// `None`: no ID card with an account.
    pub user: Option<UserData>,
    /// Indexed by [`ProductRecord::reference`] (tg keys it by path, which collides when one
    /// type is in two lists).
    pub stock: Vec<StockItem>,
    pub extended_inventory: bool,
}

impl UiInterface for VendingUi {
    const NAME: &'static str = "Vending";
    type Data = VendingData;
    type StaticData = VendingStatic;
}

/// `SANITIZED_PATH(path)`.
pub fn sanitized_path(path: &str) -> String {
    path.replace("/obj/item/", "").replace('/', "-")
}

pub const LOCAL_SEARCH: &str = "search";
pub const LOCAL_CATEGORY: &str = "category";
/// `"grid"` (tg's default `tgui_layout`) or `"list"`.
pub const LOCAL_LAYOUT: &str = "layout";

pub use super::ru_jobs::job_ru;

/// tgui-core `capitalizeAll`: every word's first letter upper-cased.
pub fn capitalize_all(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start = true;
    for c in text.chars() {
        if start {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
        start = c.is_whitespace();
    }
    out
}

/// Which rows and categories the window shows, as `Vending.tsx` derives them.
pub mod filter {
    /// `stockSearch.length >= 2` filters by name; any non-empty search turns the category
    /// filter off (a one-letter search shows everything, as in tgui).
    pub fn row_visible(search: &str, selected: &str, name: &str, category: &str) -> bool {
        if search.is_empty() {
            return category == selected;
        }
        !name_filtered(search) || name_matches(search, name)
    }

    /// The name filter applies (the JSX counts UTF-16 units; Cyrillic is one unit per letter).
    pub fn name_filtered(search: &str) -> bool {
        search.encode_utf16().count() >= 2
    }

    /// tgui-core `createSearch`: case-insensitive, trimmed substring match.
    pub fn name_matches(search: &str, name: &str) -> bool {
        name.to_lowercase()
            .contains(search.trim().to_lowercase().as_str())
    }
}

#[cfg(feature = "client")]
pub(crate) use client_side::filter_rows;
#[cfg(feature = "client")]
pub use client_side::{VendingCategoryBar, VendingCategoryButton, VendingRow};

#[cfg(feature = "client")]
mod client_side {
    use bevy::app::Propagate;
    use bevy::prelude::*;
    use bevy::scene::EntityScene;
    use bevy::text::TextColor;
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::theme::FontPx;

    use super::{
        LOCAL_CATEGORY, LOCAL_LAYOUT, LOCAL_SEARCH, ProductRecord, StockItem, VendingData,
        VendingStatic, VendingUi, capitalize_all, filter, job_ru,
    };
    use crate::client::{
        ClientUi, ClientWindow, LocalOnInput, SetLocal, UiLocalState, UiSprite, memo_of,
    };

    /// A product row: what the in-place filter needs.
    #[derive(Component, Clone, Debug, Default)]
    pub struct VendingRow {
        pub name: String,
        pub category: String,
    }

    /// The `CategorySelector` stack item: shown only with more than one category to pick.
    #[derive(Component, Clone, Debug, Default)]
    pub struct VendingCategoryBar {
        /// Every category, in order; the first is the default selection.
        pub categories: Vec<String>,
    }

    #[derive(Component, Clone, Debug, Default)]
    pub struct VendingCategoryButton(pub String);

    /// `CATEGORY_COLORS`. Its keys are Russian while BandaStation's DM sends English names
    /// ("Contraband", "Premium"), so in the reference no button gets a color; kept as is.
    fn category_color(name: &str) -> UiColor {
        match name {
            "Контрабанда" => UiColor::Red,
            "Премиум" => UiColor::Yellow,
            _ => UiColor::Default,
        }
    }

    /// CSS `lightgray`.
    const LIGHTGRAY: Color = Color::srgb(211.0 / 255.0, 211.0 / 255.0, 211.0 / 255.0);
    /// `rgba(255, 255, 255, 0.5)`.
    const WHITE_HALF: Color = Color::srgba(1.0, 1.0, 1.0, 0.5);

    impl ClientUi for VendingUi {
        const WIDTH: f32 = 431.0;
        const HEIGHT: f32 = 635.0;

        fn content(static_data: &VendingStatic, data: &VendingData) -> impl Scene {
            view(static_data, data, &UiLocalState::default())
        }

        fn content_local(
            static_data: &VendingStatic,
            data: &VendingData,
            local: &UiLocalState,
        ) -> impl Scene {
            view(static_data, data, local)
        }
    }

    fn view(st: &VendingStatic, data: &VendingData, local: &UiLocalState) -> impl Scene + use<> {
        let search = local.get(LOCAL_SEARCH).to_owned();
        let list = local.get(LOCAL_LAYOUT) == "list";
        let user_details = (!st.all_products_free).then(|| {
            let user = user_details(data);
            EntityScene(bsn! { @StackItem Children [ {EntityScene(user)} ] })
        });
        let ad = (!st.ad.is_empty()).then(|| {
            let ad = st.ad.clone();
            EntityScene(bsn! {
                @StackItem
                Children [
                    @NoticeBox { @color: UiColor::Yellow }
                    Node { margin: UiRect::ZERO }
                    Children [
                        @Stack { @align: {Some(AlignItems::Center)} }
                        Children [ @StackItem Children [ Text(ad) ] ]
                    ]
                ]
            })
        });
        let products = product_display(st, data, &search, list);
        let categories: Vec<String> = st.categories.iter().map(|(n, _)| n.clone()).collect();
        let buttons: Vec<_> = st
            .categories
            .iter()
            .map(|(name, icon)| {
                let (name, icon) = (name.clone(), icon.clone());
                let color = category_color(&name);
                bsn! {
                    @ss13_ui_kit::Button { @icon: icon, @content: {name.clone()} }
                    ss13_ui_kit::Button { color: color }
                    VendingCategoryButton({name.clone()})
                    SetLocal { key: {LOCAL_CATEGORY.to_owned()}, value: name, rerender: false }
                }
            })
            .collect();
        bsn! {
            @WindowContent
            Children [
                @Stack { @vertical: true, @fill: true }
                Children [
                    {user_details}
                    --
                    {ad}
                    --
                    @StackItem { @grow: 1.0 }
                    // CSS lets the absolutely positioned scroll content not size the item;
                    // here the item must not grow to its content either.
                    Node { min_height: px(0) }
                    Children [ {EntityScene(products)} ]
                    --
                    @StackItem
                    VendingCategoryBar { categories: categories }
                    Node { display: Display::None }
                    Children [
                        @Section
                        Children [ {buttons} ]
                    ]
                ]
            ]
        }
    }

    /// `UserDetails`.
    fn user_details(data: &VendingData) -> impl Scene + use<> {
        let (color, text) = match &data.user {
            Some(user) => {
                let name = if user.name.is_empty() {
                    "Unknown"
                } else {
                    &user.name
                };
                let job = job_ru(&user.job).map_or_else(
                    || {
                        if user.job.is_empty() {
                            "Без работы".to_owned()
                        } else {
                            user.job.clone()
                        }
                    },
                    str::to_owned,
                );
                (UiColor::Blue, format!("{name} | {job}"))
            }
            None => (
                UiColor::Default,
                "ID-карта не обнаружена! Обратитесь к главе персонала.".to_owned(),
            ),
        };
        bsn! {
            @NoticeBox { @color: color }
            Node { margin: UiRect::ZERO }
            Children [
                @Stack { @align: {Some(AlignItems::Center)} }
                Children [
                    @StackItem Children [ @Icon { @name: "id-card", @size: 1.5 } ]
                    --
                    @StackItem Children [ Text(text) ]
                ]
            ]
        }
    }

    /// `ProductDisplay`: every product of the inventory; the filter hides rows in place.
    fn product_display(
        st: &VendingStatic,
        data: &VendingData,
        search: &str,
        list: bool,
    ) -> impl Scene + use<> {
        let mut inventory: Vec<&ProductRecord> =
            st.product_records.iter().chain(&st.coin_records).collect();
        if data.extended_inventory {
            inventory.extend(&st.hidden_records);
        }
        let balance = match (&data.user, st.all_products_free) {
            (Some(user), false) => user.cash.map(|cash| {
                let green = UiColor::Green.text().unwrap_or_default();
                let text = format!("{cash}{}", st.displayed_currency_name);
                let icon = st.displayed_currency_icon.clone();
                EntityScene(bsn! {
                    @StackItem
                    Propagate::<FontPx>(FontPx(16.0))
                    Propagate::<TextColor>(TextColor(green))
                    Children [
                        Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                        Children [
                            Text(text)
                            --
                            @Icon { @name: icon, @color: UiColor::Gold }
                        ]
                    ]
                })
            }),
            _ => None,
        };
        let (layout_icon, layout_tooltip, next) = if list {
            ("border-all", "View as Grid", "grid")
        } else {
            ("list", "View as List", "list")
        };
        let rows: Vec<_> = inventory
            .into_iter()
            .map(|product| {
                let stock = data
                    .stock
                    .get(product.reference as usize)
                    .copied()
                    .unwrap_or(StockItem {
                        amount: 0,
                        free: false,
                    });
                memo_of(&row_view(st, data, product, stock, list), product_row)
            })
            .collect();
        let search = search.to_owned();
        bsn! {
            @Section {
                @title: "Товары",
                @fill: true,
                @scrollable: true,
                @buttons: bsn! {
                    @Stack
                    Children [
                        {balance}
                        --
                        @StackItem
                        Children [
                            Input { value: search, placeholder: {"Поиск...".to_owned()} }
                            LocalOnInput({LOCAL_SEARCH.to_owned()})
                        ]
                        --
                        @StackItem
                        Children [
                            @ss13_ui_kit::Button {
                                @icon: layout_icon,
                                @tooltip: layout_tooltip,
                                @tooltip_position: Placement::BottomEnd,
                            }
                            SetLocal { key: {LOCAL_LAYOUT.to_owned()}, value: {next.to_owned()}, rerender: true }
                            Name::new("layout toggle")
                        ]
                    ]
                },
            }
            Children [ {rows} ]
        }
    }

    /// Everything a product row shows, so that a re-render reuses the rows that did not change
    /// (a purchase changes one row's stock, and the balance only those it makes unaffordable).
    #[derive(Hash)]
    struct RowView {
        name: String,
        category: String,
        price_text: String,
        remaining: u32,
        disabled: bool,
        icon: Option<super::ProductIcon>,
        reference: u32,
        premium: bool,
        list: bool,
    }

    /// `Product`: the derived state of one row.
    fn row_view(
        st: &VendingStatic,
        data: &VendingData,
        product: &ProductRecord,
        stock: StockItem,
        list: bool,
    ) -> RowView {
        let free = st.all_products_free || stock.free || product.price == 0;
        let discount = !product.premium
            && data
                .user
                .as_ref()
                .is_some_and(|u| u.is_captain_card || st.department == u.department);
        let red_price = (product.price as f32 * st.job_discount).round() as u32;
        let price = if discount { red_price } else { product.price };
        let remaining = stock.amount;
        // Without an economy (`cash: None`) nothing is too expensive.
        let too_expensive = !free
            && data
                .user
                .as_ref()
                .is_some_and(|u| u.cash.is_some_and(|cash| price > cash));
        let disabled =
            remaining == 0 || (!st.all_products_free && data.user.is_none()) || too_expensive;
        let price_text = if free {
            "Бесплатно".to_owned()
        } else {
            format!("{price}{}", st.displayed_currency_name)
        };
        RowView {
            name: product.name.clone(),
            category: product.category.clone(),
            price_text,
            remaining,
            disabled,
            icon: product.icon,
            reference: product.reference,
            premium: product.premium,
            list,
        }
    }

    /// `ProductGrid` / `ProductList`.
    fn product_row(view: &RowView) -> Box<dyn Scene> {
        let (disabled, remaining, list) = (view.disabled, view.remaining, view.list);
        let price_text = view.price_text.clone();
        let name = capitalize_all(&view.name);
        let image = product_image(view.icon);
        let params = ActParams(vec![
            ("ref".into(), view.reference.to_string()),
            ("premium".into(), u8::from(view.premium).to_string()),
        ]);
        let row = VendingRow {
            name: view.name.clone(),
            category: view.category.clone(),
        };
        // `ProductPrice`: `fontSize={0.85}` of the surrounding text, gold.
        let gold = UiColor::Gold.text().unwrap_or_default();
        if list {
            let size = theme_font() * 0.85;
            let small = theme_font() * 0.8;
            let remaining = format!("{remaining} ост.");
            Box::new(bsn! {
                @ImageButton { @image: image, @fluid: true, @image_size: 32.0 }
                ImageButton { disabled: disabled }
                Act("vend")
                params
                row
                Children [
                    @Stack { @align: {Some(AlignItems::Center)} }
                    Children [
                        @StackItem { @grow: 1.0 }
                        Children [ Text(name) ]
                        --
                        @StackItem
                        Node { width: px(42.0) }
                        Propagate::<FontPx>(FontPx(small))
                        Propagate::<TextColor>(TextColor(WHITE_HALF))
                        Children [ Text(remaining) TextLayout { justify: Justify::Right } ]
                        --
                        @StackItem
                        Node { width: px(42.0), margin: UiRect::right(px(32.0)) }
                        Propagate::<FontPx>(FontPx(size))
                        Propagate::<TextColor>(TextColor(gold))
                        Children [ Text(price_text) TextLayout { justify: Justify::Right } ]
                    ]
                ]
            }) as Box<dyn Scene>
        } else {
            let small = theme_font() * 0.8;
            let size = small * 0.85;
            let remaining = format!("x{remaining}");
            let tooltip = name.clone();
            Box::new(bsn! {
                @ImageButton {
                    @image: image,
                    @tooltip: tooltip,
                    @tooltip_position: Placement::Bottom,
                    @buttons_alt: bsn! {
                        Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                        Propagate::<FontPx>(FontPx(small))
                        Children [
                            Node { flex_grow: 1.0 }
                            Propagate::<FontPx>(FontPx(size))
                            Propagate::<TextColor>(TextColor(gold))
                            Children [ Text(price_text) ]
                            --
                            Node
                            Propagate::<TextColor>(TextColor(LIGHTGRAY))
                            Children [ Text(remaining) ]
                        ]
                    },
                }
                ImageButton { disabled: disabled }
                Act("vend")
                params
                row
                Children [ Text(name) TextLayout { linebreak: LineBreak::NoWrap } ]
            }) as Box<dyn Scene>
        }
    }

    fn theme_font() -> f32 {
        ss13_ui_kit::theme::FONT_SIZE
    }

    /// The picture, or tgui's `question` fallback glyph without an icon.
    fn product_image(icon: Option<super::ProductIcon>) -> Box<dyn SceneList> {
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
                Propagate::<TextColor>(TextColor(ss13_ui_kit::theme::COLOR_TEXT_TRANSLUCENT))
                Children [ @Icon { @name: "question", @size: 4.0 } ]
            })),
        }
    }

    /// The in-place half of the JSX's derived state: rows, category buttons and the category
    /// bar follow the window's search and category. Runs when either changes or a render
    /// spawned new rows; cost: one walk over that window's entities (a few hundred).
    pub(crate) fn filter_rows(
        windows: Query<(Entity, &ClientWindow, Ref<UiLocalState>)>,
        new_bars: Query<Entity, Added<VendingCategoryBar>>,
        parents: Query<&ChildOf>,
        children: Query<&Children>,
        rows: Query<&VendingRow>,
        bars: Query<&VendingCategoryBar>,
        mut buttons: Query<(&VendingCategoryButton, &mut ss13_ui_kit::Button)>,
        mut nodes: Query<&mut Node>,
    ) {
        use crate::UiInterface;
        let rendered: Vec<Entity> = new_bars
            .iter()
            .filter_map(|bar| parents.iter_ancestors(bar).find(|e| windows.contains(*e)))
            .collect();
        for (root, window, local) in &windows {
            if window.interface != VendingUi::NAME
                || !(local.is_changed() || rendered.contains(&root))
            {
                continue;
            }
            let search = local.get(LOCAL_SEARCH);
            let entities: Vec<Entity> = children.iter_descendants(root).collect();
            // The shown render and a pending one (laid out hidden) each have a bar.
            let window_bars: Vec<(Entity, &VendingCategoryBar)> = entities
                .iter()
                .filter_map(|e| bars.get(*e).ok().map(|b| (*e, b)))
                .collect();
            let Some(&(_, bar)) = window_bars.last() else {
                continue;
            };
            let first = bar.categories.first().cloned().unwrap_or_default();
            let selected = match local.get(LOCAL_CATEGORY) {
                "" => first.as_str(),
                s => s,
            };
            let mut with_rows: Vec<&str> = Vec::new();
            let set_display = |e: Entity, shown: bool, nodes: &mut Query<&mut Node>| {
                if let Ok(mut node) = nodes.get_mut(e) {
                    let display = if shown { Display::Flex } else { Display::None };
                    if node.display != display {
                        node.display = display;
                    }
                }
            };
            for &e in &entities {
                if let Ok(row) = rows.get(e) {
                    let named =
                        !filter::name_filtered(search) || filter::name_matches(search, &row.name);
                    if named && !with_rows.contains(&row.category.as_str()) {
                        with_rows.push(row.category.as_str());
                    }
                    let shown = filter::row_visible(search, selected, &row.name, &row.category);
                    set_display(e, shown, &mut nodes);
                }
            }
            let with_rows: Vec<String> = with_rows.into_iter().map(str::to_owned).collect();
            for &e in &entities {
                if let Ok((button, mut b)) = buttons.get_mut(e) {
                    let is_selected = button.0 == selected;
                    if b.selected != is_selected {
                        b.selected = is_selected;
                    }
                    set_display(e, with_rows.contains(&button.0), &mut nodes);
                }
            }
            let show_bar = !filter::name_filtered(search) && with_rows.len() > 1;
            for &(e, _) in &window_bars {
                set_display(e, show_bar, &mut nodes);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_follows_the_jsx() {
        use filter::*;
        // Empty search: the selected category only.
        assert!(row_visible("", "Products", "Cola", "Products"));
        assert!(!row_visible("", "Premium", "Cola", "Products"));
        // One letter: everything (category filter off, name filter not yet on).
        assert!(row_visible("x", "Premium", "Cola", "Products"));
        // Two letters: case-insensitive substring, any category.
        assert!(row_visible("CO", "Premium", "Space Cola", "Products"));
        assert!(!row_visible("ab", "Products", "Cola", "Products"));
        assert!(row_visible("ко", "", "Кола", "Products"));
    }

    #[test]
    fn helpers_match_tg() {
        assert_eq!(
            sanitized_path("/obj/item/food/chips/shrimp"),
            "food-chips-shrimp"
        );
        assert_eq!(capitalize_all("space cola can"), "Space Cola Can");
    }
}

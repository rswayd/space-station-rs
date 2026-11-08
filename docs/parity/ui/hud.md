# HUD
Экранный интерфейс моба: слоты инвентаря, индикаторы здоровья, кнопки действий, алерты, полноэкранные эффекты, радиальные меню.

### HUD и стили
`/datum/hud` по типу моба (человек, призрак, силиконы, ксеноморфы, блоб, ...): набор экранных объектов, скрытие (F12), стили UI (Midnight, Retro, Plasmafire, Slimecore, Operative, Clockwork, Glass...).

- **Код:** [`/datum/hud`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/hud.dm#L23), [`/datum/hud/human`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/human.dm#L1), [`code/_onclick/hud`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud), [`ui_style.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/ui_style.dm#L1)
- **BandaStation:** [`human.dm#L74`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/human.dm#L74), [`hud.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/hud/code/hud.dm#L1) — своя иконка кнопки DNR призрака
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/hud.rs`, `crates/ss13_game/src/inventory_client.rs`, `crates/ss13_game/src/health_client.rs`: HUD человека в стиле Midnight; нет других мобов, стилей, скрытия HUD

### Слоты инвентаря на экране
Нижний ряд (руки, пояс, ID, спина, карманы, хранилище костюма) и выпадающие слоты одежды, переключатель, кнопки «выбросить/бросить/сменить руку».

- **Код:** [`inventory_slot.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/inventory_slot.dm#L1), [`screen human.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/screen_objects/human.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/inventory_client.rs` (слоты, toggle-кнопка, X/Q); оверлеи предметов на иконках не рисуются

### Индикаторы здоровья, зона, режим боя
Индикатор здоровья, кукла повреждений, выбор зоны (с нумпадом), переключатель боевого режима, бег/ходьба, тяга, отдых, индикаторы дыхания/температуры/голода.

- **Код:** [`living.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/living.dm#L1), [`screen_objects.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/screen_objects/screen_objects.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/health_client.rs` (индикатор, кукла, зона, боевой режим, F): нет кнопок бега/ходьбы, тяги, отдыха, пользовательских иконок голода/дыхания на HUD

### Кнопки действий
`/datum/action`: кнопки способностей и предметов на экране, перетаскивание и группы, перезарядка с наложением, маленькие/скрытые кнопки, палитра.

- **Код:** [`/datum/action`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/actions/action.dm#L6), [`/atom/movable/screen/movable/action_button`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/screen_objects/action_button.dm#L1), [`action_group.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/action_group.dm#L1), [`cooldown_action.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/actions/cooldown_action.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/actions.rs`, `crates/ss13_game/src/actions_client.rs` (headless `ActionButtons`: ряд действий tg сверху слева после кнопки «Antag Info», фон/иконка/рамка, красный цвет недоступной, секунды кулдауна или своя надпись, `trigger`; наведение указательных заклинаний — `click::ClickIntercept`); пользуются только способности вампира; нет перетаскивания, палитры, хоткеев и подсказок

### Алерты
`throw_alert`: иконки состояний справа (нет воздуха, токсины, холод/жара, голод, пристёгнут, наручники, огонь, мудлеты...), клик — действие (выбраться, встать), подсказка при наведении.

- **Код:** [`/mob/proc/throw_alert()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/screen_objects/alert.dm#L15), [`/atom/movable/screen/alert`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/screen_objects/alert.dm#L98)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/hud.rs` (`AlertHudUi`: иконка, имя, описание): только алерты реализованных систем (дыхание, голод, ...), без кликов-действий

### Полноэкранные эффекты
Оверлеи: крит, урон (красная рамка), нехватка кислорода, слепота, размытие, вспышка, галлюцинации, ночное зрение.

- **Код:** [`/atom/movable/screen/fullscreen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/screen_objects/fullscreen.dm#L75)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/vision.rs` (сервер: `hud_data`, `damage_screens` — `update_damage_hud`, `flash_act`, `Vision`), `crates/ss13_game/src/hud.rs` (`FullscreenHudUi`/`FullscreenHudData`: категория, severity, время затухания; клиентские цвета), `crates/ss13_game/src/fullscreen_client.rs` (оверлеи `screen_full.dmi`/`screen_gen.dmi` растянуты на вид карты под HUD, по слоям tg, анимация `.dmi`, затухание `clear_fullscreen`), `crates/ss13_render/src/filter.rs` (`ScreenFilter`: цветовая матрица `client_colours` с затуханием 2 с и гауссово размытие `eye_blur` на мире): крит (`passage`, `crit/vision`), урон (brute), кислород, слепота (мерцающая/статичная + приглушённый цвет), близорукость и затемнение (`impaired`), вспышка, размытие, ахроматопсия; данные только владельцу (headless tgui). Галлюцинации, ночное зрение (`see_through_darkness`), `high`/наркотики, `curse`, помехи киборгов — у своих систем; нет `damageoverlaytemp`, `TRAIT_NOCRITOVERLAY`, предпочтения тёмной вспышки. Состояния `curse1` и `kinesis0` (19 кадров 480×480) импортёр пропускает (не влезают в страницу 2048) — они не нужны ни одной перенесённой системе

### Радиальные меню
Круговые меню выбора (инструменты, крафт, режимы машин, смена формы).

- **Код:** [`/atom/movable/screen/radial`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/screen_objects/radial.dm#L3), [`radial_persistent.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/screen_objects/radial_persistent.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_tgui/src/prompt.rs` (`PromptKind::Radial`, проверка ответа), `crates/ss13_tgui/src/interfaces/radial.rs`, `crates/ss13_game/src/radial.rs` (анкор, `require_near`, повторный клик, `can_still_use`), `crates/ss13_game/src/radial_client.rs` (рисование и страницы). Подключено: микроволновка и выбор облика ножен сабли (`crates/ss13_game/src/kitchen.rs`, `crates/ss13_game/src/clothing/reskin.rs`); остальные ещё используют списки или фиксированные клики (гриндер, инструменты RHD, крафт) — переводятся заменой `PromptKind::List` на `radial::show`

### Экраны карт и картинка-в-картинке
Встроенные виды карты (камеры, превью), окна-попапы карты.

- **Код:** [`map_view.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/screen_objects/map_view.dm#L1), [`picture_in_picture.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/screen_objects/picture_in_picture.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Миникарта
Миникарта уровня станции (действие и стол-голограмма) с отметками.

- **Код:** [`/datum/minimap`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/minimap/minimap.dm#L5), [`code/modules/minimap`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/minimap)
- **Статус в ss13-rs:** ❌ нет

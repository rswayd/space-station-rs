# Прочий интерфейс
Подсказки, баллоны, статпанель, меню Escape, «Who», индикатор SSD, указание, панель лута, сны, эмодзи, навигатор.

### Экранные подсказки (screentips)
Строка над экраном с именем объекта под курсором и подсказками действий мыши (ЛКМ/ПКМ/Ctrl/Alt) по контексту предмета.

- **Код:** [`screentips.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/screentips/screentips.dm#L1), [`item_context.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/screentips/item_context.dm#L1), [`screentip.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/screen_objects/screentip.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/screentips.rs`, клиент — `crates/ss13_game/src/screentips_client.rs` (`ClientIntent::Hover`): строка над картой с именем (`get_tip_name`, видимое имя в маске) и контекстами носителя `add_context` людей/карбонов (обнять/погладить/потрясти/СЛР/атаковать, толкнуть, потащить/схватить/за горло, предложить предмет) и дверей/шлюзов (Open/Постучать, отвёртка/лом/сварка); нет остальных ~250 `add_context`, контекстов предмета в руке, элементов `contextual_screentip_*`, настройки и иконок мыши, подсказок над HUD

### Всплывающие подсказки (tooltips)
Тултипы при наведении на предметы в инвентаре и кнопки (настраиваемая задержка).

- **Код:** [`/datum/tooltip`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tooltip/tooltip.dm#L22), [`tooltips.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/tooltips.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_ui_kit/src/tooltip.rs`: тултипы в окнах интерфейса; на предметах карты/HUD нет

### Баллоны (balloon alerts)
Короткие всплывающие надписи над объектом для отказов и результатов действий (видны только себе или окружающим).

- **Код:** [`/atom/proc/balloon_alert()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/balloon_alert/balloon_alert.dm#L18), [`/atom/proc/balloon_alert_to_viewers()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/balloon_alert/balloon_alert.dm#L25)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/balloon.rs` (`balloon_alert`, `balloon_alert_to_viewers`: в радиусе 7, видящим тайл и не слепым, своя строка источнику), клиент — `crates/ss13_game/src/balloon_client.rs`: текст над объектом (предмет в руке/хранилище — над держателем) поднимается на 1.2 тайла и гаснет по таймингам tg (0.2 / 0.7×длина / 0.1 с); доставка — `Animation::Balloon` только клиенту зрителя (`PlayAnimationFor`). Переведены отказы хранилищ (`storage::refusal_text`), карты, ввод/значение портов схем. Отличия: шрифт Bevy с тенью вместо обводки; нет запасной строки в чат при выключенном runechat и `balloon_alert_to_hearers`; остальные места tg с баллонами по-прежнему пишут в чат

### Полоска прогресса и шестерёнка (do_after)
Пока идёт действие с задержкой (`do_after`), его исполнитель видит над целью полоску, которая заполняется и краснеет при прерывании. Остальные видят шестерёнку над занятым персонажем.

- **Код:** [`/datum/progressbar`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/progressbar.dm#L4), [`/datum/cogbar`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/cogbar.dm#L7), [`/datum/timed_action`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/timed_actions.dm#L24)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/progressbar.rs` (наблюдатели за `DoAfter`: `Animation::Progress`/`ProgressEnd` только клиенту исполнителя над целью, переносимая цель — над держателем; `Animation::Cog` всем видящим моба при действии от 1 с), клиент — `crates/ss13_game/src/progressbar_client.rs`: спрайты `progressbar.dmi`, шаг 5 %, `_fail` при прерывании, подъём и появление/угасание за 0.5 с (`SINE_EASING`), стопка полосок по 6 px со сдвигом вниз; свою шестерёнку игрок не видит. Скрытые действия как в tg (`progressbar::visuals`): лепка — без полоски и шестерёнки (`show_progress = FALSE`); поглощение генокрада, руна и влияние еретика, кража шпиона, укус вампира, стена войдволкера, выбирание из пут, чужие шнурки, сканеры хамелеона, руна синдикатского баллончика, побег из гнезда, полёт вверх/вниз — без шестерёнки (`cog_icon = null`). Отличия: полоска заполняется за реальное время действия (в tg цель — задержка до замедления действий); нет своих иконок шестерёнки; опоздавший зритель шестерёнку не получает; нет смещения для больших иконок

### Статпанель
Панель вкладок (Status, вербы по категориям, MC для админов, тикеты, объекты на тайле), обновляется подсистемой `SSstatpanels`.

- **Код:** [`SUBSYSTEM_DEF(statpanels)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/statpanel.dm#L1), [`stat_panel.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/verbs/stat_panel.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/statpanel.rs`, `crates/ss13_game/src/statpanel_client.rs`: окно с вкладками (Status: станция, время раунда, игроки; вкладки вербов моба, верб — кнопка, сервер проверяет, что верб у моба есть), обновление раз в секунду. Нет вкладок MC/админов, тикетов, предметов на тайле и других категорий вербов (в порте нет объектов-вербов); окно закрывается до переподключения

### Меню Escape
Меню по Esc: продолжить, настройки, админ-помощь (тикеты), выход в лобби/дисконнект, время сервера.

- **Код:** [`/datum/escape_menu`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/escape_menu/escape_menu.dm#L31), [`subsystem.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/escape_menu/subsystem.dm#L1)
- **BandaStation:** [`escape_menu.dm#L111`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/escape_menu/escape_menu.dm#L111) — время сервера (IC-время), ручная отправка обновлений
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/escape_menu.rs`, клиент — `crates/ss13_game/src/escape_menu_client.rs`: Esc открывает/закрывает окно (станция, время смены или «Pre-Game», игроков онлайн, кнопка «Кто онлайн», подсказка F1 для админ-помощи), данные раз в секунду; нет настроек, «покинуть тело», суицида, выхода, списка админов, IC-времени сервера

### Окно «Who» (BandaStation)
TGUI-окно списка игроков онлайн с пингом (для админов — статусы и роли).

- **Код:** [`/datum/tgui_who`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tgui_who/code/tgui_who.dm#L1); tgui: [`Who.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Who/index.tsx)
- **BandaStation:** [`_tgui_who.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tgui_who/_tgui_who.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/escape_menu.rs` (`WhoUi`, из меню Escape): ключи клиентов онлайн по алфавиту, число, свой ключ, кнопка «Обновить»; нет пинга (транспорт его не меряет), поиска/сортировок и админ-режима (статус, роли, follow, smite — в админ-панели)

### Индикатор SSD (BandaStation)
Оверлей «zzz» над вышедшими из игры персонажами и время отсутствия в осмотре.

- **Код:** [`/datum/element/ssd`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/ssd_indicator/code/ssd_indicator.dm#L3)
- **BandaStation:** [`_ssd_indicator.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/ssd_indicator/_ssd_indicator.dm#L1)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/ssd.rs`: живое тело, покинутое игроком (выход, призрак, другое тело), получает оверлей `zzz_glow` (снимается при смерти, возвращается при оживлении) и каждый `Life()` засыпает на 7 с (`handle_ssd`); возвращение игрока снимает оба; строка осмотра — `crates/ss13_game/src/examine.rs`. Отличие: тело при админ-призраке тоже помечается (форк снимает элемент для ключа `@`), ядро ИИ не помечается

### Указание (point)
Shift+СКМ/верб: стрелка над объектом и сообщение «указывает на».

- **Код:** [`/atom/movable/proc/point_at()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/point/point.dm#L10)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/point.rs`: shift+СКМ, стрелка `arrow` над целью на 2.5 с, сообщения «указывает на», кулдаун 1 с форка; стрелка появляется сразу (без анимации полёта), моб не поворачивается к цели, нет эмоции у связанных, указания призраками и «пузыря» изнутри шкафа

### Панель лута
Alt-клик по тайлу открывает окно со списком объектов на нём (поиск предметов в куче).

- **Код:** [`/datum/lootpanel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/lootpanel/_lootpanel.dm#L6), [`code/modules/lootpanel`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/lootpanel); tgui: [`LootPanel.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/LootPanel/index.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/loot.rs`, окно — `crates/ss13_game/src/panels_client.rs`: alt-клик (если у цели нет `click_alt`) и alt+shift открывают окно тайла (турф и всё кликабельное на нём, сгруппировано, обновляется), клик по записи кликает предмет, одно окно на клиента; нет поиска и переключения группировки, модификаторов и правого клика по записи, окно закрывается на 6 тайлах вместо границы обзора

### Сны
Во сне игрок видит обрывки «снов» (фрагменты из окружения и строк).

- **Код:** [`dreaming.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/flufftext/dreaming.dm#L1)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/dreams.rs`: спящий карбон (пульс сна `status_effects`) с шансом 10% начинает сон, если ещё не видит его; обрывки «... x ...» приходят в чат через 1–3 с, сон прерывается при пробуждении или тяжёлом крите. Сны `random` (вес 1000: «you see», субъект — 90% из `dream_messages` простыни на тайле, иначе `strings/dreamstrings.txt`, прилагательные, глагол/наречие, 75% — объект) и `hear_something` (500: звук и строки). Отличия: звук «hear something» — из короткого списка вместо всех файлов `sound/`, не обрывается через 5 с; нет пулов снов `COMSIG_PRE_DREAMING`

### Эмодзи
`:emoji:` в OOC/чате заменяется иконкой; BandaStation добавляет свой набор.

- **Код:** [`emoji_parse()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/emoji/emoji_parse.dm#L1)
- **BandaStation:** [`modular_bandastation/emojis220/icons`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/emojis220/icons) — эмодзи BandaStation
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_chat/src/emoji.rs` (`emoji_parse` по алгоритму tg, `EMOJI_SET` форка `emoji220.dmi`), набор — `crates/ss13_game/src/emojis.rs` (состояния иконки из манифеста), сегмент `Span::Emoji(IconStateId)` с текстом `:name:`; клиент рисует его иконкой в строке чата (`ChatEmoji` в `crates/ss13_chat/src/client/panel.rs`, спрайт — `crates/ss13_game/src/ui_sprites.rs`). Разбираются мёртвый чат и asay; OOC/LOOC, админ-PM и мессенджер КПК в порте пока нет — их отправка должна вызывать `emoji_parse`. Отличие: нет флага конфига `emojis` (пустой набор = выключено)

### Навигатор (BandaStation)
Программа КПК: построение маршрута к выбранной точке станции.

- **Код:** [`/datum/computer_file/program/navigator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/navigator/code/navigator.dm#L1)
- **BandaStation:** [`_navigator.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/navigator/_navigator.dm#L1)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/modpc/programs/navigator.rs` (`ProgramKind::Navigator`, `ntos_navigator`, 5 ГК, только КПК, в каталоге NT Software Hub), экран — `NtosScreen::Navigator` в `crates/ss13_game/src/modpc/client.rs`: зона, координаты и направление пользователя по сигналу NTNet (слабый — позиция округляется до 5 тайлов на сервере, без направления; нет сигнала — уведомление), лестницы и стремянки уровня с расстоянием и направлением, ближайшие первыми. Отличие: вместо карты список (10 ближайших) — изображения nanomap в порте есть (`crates/ss13_game/src/nanomap.rs`), но лестниц в данных карты и зума пока нет; сама программа в оригинале маршрут не строит

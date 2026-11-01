# Прочая инфраструктура

Мелкие модули `code/modules/*`, не вошедшие в другие файлы: ввод с клавиатуры, отчёты об ошибках, запросы к администрации, вспомогательные визуальные шаги, «болтовня» и т. п. Сетка `SSspatial_grid` описана в [master-controller.md](master-controller.md); её трекер клеток (`/datum/cell_tracker`, [`cell_tracker.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spatial_grid/cell_tracker.dm#L12)) входит туда же.

### Обработка нажатий клавиш
Клиент шлёт нажатия и отпускания клавиш, сервер раз в тик вызывает `keyLoop` у мобов (движение с зажатой клавишей, удерживаемые действия); макросы BYOND стираются и строятся из привязок игрока. Настройка привязок — см. ../ui/keybindings.md.

- **Код:** [`/datum/proc/key_down`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/keybindings/setup.dm#L3), [`keyLoop`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/keybindings/setup.dm#L8), [`bindings_client.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/keybindings/bindings_client.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_client`, `crates/ss13_game/src/prediction.rs`: клавиши обрабатываются на клиенте и уходят на сервер намерениями (движение, действия); привязки зашиты, переназначения нет

### «Болтовня» (chatter)
Озвучка речи некоторых говорящих (например, дроны и роботы) короткими звуками-фонемами по числу букв в словах.

- **Код:** [`/proc/chatter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/chatter/chatter.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Панель эмоций (апстрим)
Окно tgui со списком всех доступных мобу эмоций с поиском и кнопкой исполнения. Панель форка — см. ../ui/emotes.md.

- **Код:** [`/datum/emote_panel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/emote_panel/emote_panel.dm#L1); tgui: [`EmotePanel.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/EmotePanel.tsx)
- **Статус в ss13-rs:** ❌ нет — есть только `*help` в строке речи (`crates/ss13_chat/src/emote.rs`)

### Обработчик и просмотрщик ошибок
`world/Error` считает и логирует рантаймы, подавляет повторы; админы смотрят сгруппированные ошибки во встроенном просмотрщике.

- **Код:** [`/world/Error`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/error_handler/error_handler.dm#L7), [`/datum/error_viewer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/error_handler/error_viewer.dm#L24)
- **Статус в ss13-rs:** ❌ нет — паники и ошибки идут только в лог сервера

### Отладчик и профилировщик (Tracy)
Подключение внешнего отладчика DM (auxtools) и профилировщика Tracy к живому серверу.

- **Код:** [`/datum/debugger`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/debugging/debugger.dm#L7), [`/datum/tracy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/debugging/tracy.dm#L5)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_debug`: панели Perf/Server (время тика, p50/p99) в отладочном оверлее; внешнего профилировщика нет

### Менеджер запросов администрации
Молитвы, сообщения ЦК и Синдикату, запросы кодов нюки, факсы и запросы песен собираются в общий список, который админы разбирают в окне tgui.

- **Код:** [`/datum/request`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/requests/request.dm#L6), [`/datum/request_manager`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/requests/request_manager.dm#L26); tgui: [`RequestManager.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/RequestManager.tsx)
- **Статус в ss13-rs:** ❌ нет

### Редактор спрайтов (рабочее пространство)
Холст с заливкой и слоями, на котором работает программа NtOS Nanopaint (рисование на планшетах, правка фото и картин). Сама программа — см. ../comms/.

- **Код:** [`/datum/sprite_editor_workspace`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/sprite_editing/workspace.dm#L1), [`/proc/flood_fill`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/sprite_editing/flood_fill.dm#L12)
- **Статус в ss13-rs:** ❌ нет

### Шаги рендера (render steps)
Служебные визуальные объекты, которые копируют облик атома в другой план: эмиссив (свечение в темноте) и блокировщик эмиссива, цветовые шаги.

- **Код:** [`/atom/movable/render_step`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/visuals/render_steps.dm#L5), [`render_step/emissive_blocker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/visuals/render_steps.dm#L71)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_render/src/sprites.rs`, `crates/ss13_render/src/lighting/mod.rs`: эмиссивные оверлеи светятся в темноте; общего механизма render steps и блокировщиков эмиссива нет

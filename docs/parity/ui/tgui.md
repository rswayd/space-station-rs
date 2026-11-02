# TGUI: фреймворк и окна ввода
TGUI — система окон на React: сервер владеет данными и проверяет действия, клиент рисует. Сюда же стандартные окна ввода (`tgui_input_*`).

### Ядро TGUI
`/datum/tgui`: окно привязано к пользователю и источнику, `ui_data`/`ui_static_data`, `ui_act` с проверкой, автообновление, `SStgui` отслеживает открытые окна, закрытие при потере доступа.

- **Код:** [`/datum/tgui`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui/tgui.dm#L9), [`/datum/tgui_window`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui/tgui_window.dm#L6), [`SUBSYSTEM_DEF(tgui)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/tgui.dm#L13), [`/datum/proc/ui_interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui/external.dm#L17)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_tgui/src/server.rs`, `crates/ss13_tgui/src/client.rs` (серверные данные, типизированные `ui_data`, проверка каждого act, обновления по изменению и таймеру; окна рисуются нативно)

### Состояния UI (ui_state)
Правила доступа к окну: default (рядом/в руках/телекинез/силикон), always, admin, conscious, hands, inventory, physical, observer, not_incapacitated, z-level и др.; статусы interactive/update/disabled/close.

- **Код:** [`code/modules/tgui/states`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui/states), [`default_ui_state()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui/status_composers.dm#L2), [`ui_status_user_is_adjacent()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui/status_composers.dm#L18)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_tgui/src/server.rs` (`UiStatus`, статус по расстоянию, «always»-источники): нет проверки `view()`, «в контейнере», силиконов, телекинеза, большинства состояний

### Интерфейсы TGUI
Сотни интерфейсов в `tgui/packages/tgui/interfaces`; у каждой машины/предмета своё окно.

- **Код:** [`tgui/packages/tgui/interfaces`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_tgui/src/interfaces/`: `JobSelection`, `Vending`, `Wires`, `ThermoMachine`; в `ss13_game`: `CommunicationsConsole`, `Lobby`, `JobPreferences`, `AntagInfoTraitor`, `Uplink`, `NtosMain` (`modpc`), `Cargo` (`cargo`) и HUD-интерфейсы; остальных сотен окон нет

### Темы окон и fancy-режим
Темы оформления (ntos, syndicate, cardtable, hackerman, ...), «fancy» окна без рамок BYOND, предпочтение.

- **Код:** [`tgui.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/tgui.dm#L1)
- **Статус в ss13-rs:** ❌ нет — одна тема `ss13_ui_kit::theme`

### Окно оповещения (tgui_alert)
Модальное окно с вопросом и кнопками, с таймаутом; используется повсеместно (подтверждения).

- **Код:** [`tgui_alert()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui_input/alert.dm#L13), [`/datum/tgui_alert`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui_input/alert.dm#L49); tgui: [`AlertModal.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AlertModal.tsx)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_tgui/src/prompt.rs` (`ask`/`PromptAnswered`: ответ, отмена, закрытие окна, отключение и тайм-аут дают `None`; больше трёх кнопок превращаются в список, как в tg), окно — `crates/ss13_tgui/src/interfaces/alert.rs` (полоса тайм-аута, автофокус, Enter/пробел/стрелки/Escape); кнопки «крупные»/«поменять местами» берут умолчания tg, пока нет игровых настроек (см. preferences.md)

### Ввод текста и числа
`tgui_input_text` (с лимитом, многострочный, шифрование ввода) и `tgui_input_number` (мин/макс, округление).

- **Код:** [`tgui_input_text()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui_input/text.dm#L18), [`tgui_input_number()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui_input/number.dm#L18); tgui: [`TextInputModal.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/TextInputModal.tsx), [`NumberInputModal.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/NumberInputModal.tsx)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_tgui/src/prompt.rs` (проверка длины в символах, диапазона и округления на сервере), окна — `crates/ss13_tgui/src/interfaces/{text_input,number_input}.rs` (многострочный ввод, счётчик символов, кнопки мин/−1/+1/макс/сброс); длинный текст идёт по частям (`UiParams::text`); `html_encode` не нужен — текст чата не HTML. Сообщение культа и ключ руны идут через него

### Выбор из списка, чекбоксы, цвет, комбинация клавиш
`tgui_input_list` (с поиском), `tgui_input_checkboxes` (мин/макс выбранных), `tgui_color_picker`, `tgui_input_keycombo`.

- **Код:** [`tgui_input_list()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui_input/list.dm#L13), [`tgui_input_checkboxes()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui_input/checkboxes.dm#L13), [`tgui_color_picker()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui_input/color.dm#L11), [`tgui_input_keycombo()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui_input/keycombo.dm#L13); tgui: [`ListInputWindow.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ListInputWindow/index.tsx), [`CheckboxInput.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/CheckboxInput.tsx), [`ColorPickerModal.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ColorPickerModal/index.tsx)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_tgui/src/prompt.rs` (повторы в списке различаются как `avoid_assoc_duplicate_keys`, число отмеченных проверяется как в tg, цвет — `sanitize_hexcolor`), окна — `crates/ss13_tgui/src/interfaces/{list_input,checkbox_input,color_picker,key_combo}.rs`: список с режимами поиска и «буква — к первому совпадению» (поиск сам включается для списков длиннее 9), выбором щелчком, двойным щелчком, стрелками по отфильтрованным пунктам и Enter; чекбоксы с поиском и лимитами; палитра с квадратом насыщенности/яркости, полосой оттенка, полями H/S/V/R/G/B, hex и пресетами tg; клавиши в названиях BYOND (`CtrlShiftA`, `North`, `Numpad8`)

### Панель TGUI (чат-окно) и обслуживание
`tgui_panel` — браузерная панель чата: инициализация, аудио-плеер (музыка админов), телеметрия; вербы «Fix chat» и «Refresh TGUI».

- **Код:** [`/datum/tgui_panel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui_panel/tgui_panel.dm#L10), [`audio.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui_panel/audio.dm#L1), [`telemetry.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui_panel/telemetry.dm#L1)
- **BandaStation:** [`external.dm#L11`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/tgui_panel/external.dm#L11) — вербы в категории «Special»
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_chat/src/client/panel.rs`: нативная панель чата; нет аудио-плеера, телеметрии, вербов починки

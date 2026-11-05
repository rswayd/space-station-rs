# ИИ станции
Мобы `/mob/living/silicon/ai`: ядро, камеры, взаимодействие с машинами и экипажем.

### ИИ как моб и роль
Неподвижный синтетик в ядре: видит станцию через камеры, говорит по всем каналам радио, управляет дверями и машинами, подчиняется законам. Работа «ИИ» берётся из лобби (спавн в `/obj/structure/ai_core/latejoin_inactive` или на месте landmark).

- **Код:** [`/mob/living/silicon/ai`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai_defines.dm#L1), [`/mob/living/silicon/ai/Initialize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai.dm#L12)
- **BandaStation:** [`ai_defines.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai_defines.dm#L178) — поле запроса смены законов (модуль `ai_law_change`, см. laws.md)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/silicon/ai.rs`: работа «AI» спавнит ядро на основном `/obj/effect/landmark/start/ai` (иначе на запасном), законы раунда, встроенное радио со всеми каналами ИИ, клиент управляет глазом; нет имён ИИ из префов, латджойна в неактивное ядро, объявления о загрузке

### Камеры и свободный обзор (AI eye, freelook)
ИИ двигает «глаз» по станции; видно только то, что покрывают работающие камеры (чанки 16×16 с маской видимости). Трекинг мобов, список камер, подсветка камер.

- **Код:** [`/mob/eye/camera/ai`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/freelook/eye.dm#L1), [`/datum/camerachunk`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/freelook/chunk.dm#L6), [`proc/show_camera_list()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai.dm#L1112), [`proc/toggle_camera_light()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai.dm#L738)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/silicon/ai.rs` + `cameras.rs`: глаз ходит по уровню сквозь стены, вид следует за ним, HUD `CameraView` даёт маску видимых камерами тайлов (статика — тёмные тайлы), сервер реплицирует клиенту ИИ только сущности и свет на этих тайлах (`ss13_net` `ViewMask`); `silicon/ai_hud.rs`: кнопки «AI core» (глаз на ядро) и «Show Camera List» (список камер сети `ss13` по тегам, `(Deactivated)` у выключенных, прыжок `switchCamera`); `silicon/ai_track.rs`: кнопка «Track With Camera» — список видимых камерами живых мобов (люди первыми, повторы «имя (2)»), глаз следует за целью, вне камер «Attempting to reacquire...» и раз в секунду 10 попыток, ручной шаг глаза/ядро/прыжок, смерть цели или ИИ снимают слежение; нет ускорения, подсветки камер, двойного клика, верба `track` по имени

### Мультикамера
Апгрейд, открывающий несколько дополнительных окон-камер на экране ИИ.

- **Код:** [`multicam.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/multicam.dm#L3), [`proc/toggle_multicam()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/multicam.dm#L230)
- **Статус в ss13-rs:** ❌ нет — упирается в клиент: у него один вид карты (`ss13_render`), окна-врезки с отдельной камерой и своей репликацией тайлов (tg `pic_in_pic`) нужно сначала сделать там; серверная часть (маска камеры на окно, `cameras.rs` `camera_mask`) уже есть

### Питание ядра и потеря энергии
Ядро питается от APC зоны; без питания ИИ слепнет и запускает резервную процедуру восстановления (перезапуск APC через провода). Действие «удалённое питание» включает/выключает APC машин.

- **Код:** [`proc/ai_lose_power()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/life.dm#L153), [`proc/ai_restore_power()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/life.dm#L140)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/silicon/ai_power.rs`: `Life` раз в 2 с; без питания оборудования в зоне ядра (или в космосе) ИИ обесточен — без кликов и кнопок, вид без тайлов камер (пустая маска репликации), окна кроме взломанного ЛКП выключены; таймированная процедура восстановления с текстами tg, поиск ЛКП зоны, его окно открывается и работает вслепую (`apc_override`); резервная батарея 200 тиков, при нуле 200 урона кислородом и смерть; питание вернулось — восстановление. Сокращения в модуле: ядро не тянет свои 500 Вт с терминала ЛКП, вместо чёрного экрана статика. Действие «удалённое питание» (`remote_power.dm`) — модуль малфа, см. «Апгрейды ИИ»

### Управление машинами и дверями
ИИ кликает по дверям, APC, машинам на расстоянии (если не перерезан провод ИИ); окно управления шлюзом (`AiAirlock`), сигналы шлюзов. Форк добавляет открытие ближайшей к цели двери.

- **Код:** [`/obj/machinery/door/airlock/attack_ai()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/airlock.dm#L810); tgui: [`AiAirlock.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AiAirlock.jsx)
- **BandaStation:** [`ai.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai.dm#L1313) — `open_nearest_door()`
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/silicon/ai.rs` (tg `ClickOn` ИИ), `crates/ss13_game/src/silicon/interact.rs` (`attack_ai`, статус окон по камерам): только на видимом камерами; shift/ctrl/alt по шлюзам и ЛКП как в tg; обычный клик открывает окна шлюза (`AiAirlock`), ЛКП (`Apc`), воздушных тревог и атмос-машин, солнечной консоли, консоли мех-отсека; окна закрываются вне обзора камер; нет взлома шлюза, консолей R&D/карго/записей/камер, торгоматов

### Объявления ИИ и VOX
ИИ делает текстовые объявления и голосовые VOX-объявления из набора слов-сэмплов. Форк добавляет наборы голосов (HL1, BMS, военный) и громкость.

- **Код:** [`proc/announcement()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai_say.dm#L119), [`ai_say.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai_say.dm#L119), [`vox_sounds.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/vox_sounds.dm#L10)
- **BandaStation:** [`modular_bandastation/ai_vox/code/vox_procs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/ai_vox/code/vox_procs.dm#L5), [`ai_say.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai_say.dm#L108) — типы VOX, выбор по ИИ, громкость
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/silicon/ai_hud.rs` (кнопка «Make Vox Announcement»): текстовый ввод, до 30 слов, каждое — слово VOX, откат 60 с на всех ИИ, малое объявление «<имя> объявляет» игрокам на уровне ИИ, затем слова звучат одно за другим (длительности слов пишет `ss13-mapload convert` в `sfx.ron`, файлы `sound/announcer/vox_fem` копируются); без слов VOX в ассетах — только текст. Нет голосов BandaStation (HL, BMS, военный: ассеты Valve/Crowbar Collective без свободной лицензии), выбора голоса и громкости VOX в настройках, окна «Announcement Help»

### Голографический разговор через голопады
ИИ говорит через голопад, проецируя голограмму (выбор облика из настроек), и слышит речь рядом с ним.

- **Код:** [`proc/holopad_talk()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai_say.dm#L68), [`proc/apply_pref_hologram_display()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai.dm#L225)
- **Статус в ss13-rs:** ❌ нет — голопады см. ../comms/

### Внешний вид ядра, статус-дисплеи и портрет
Выбор картинки ядра, эмоций на статус-дисплеях, портрета (окна выбора).

- **Код:** [`ai_core_display_picker.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai_core_display_picker.dm#L5), [`ai_status_display_picker.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai_status_display_picker.dm#L6), [`ai_portrait_picker.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai_portrait_picker.dm#L8); tgui: [`AiCoreDisplayPicker.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AiCoreDisplayPicker.tsx), [`AiStatusDisplayPicker.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AiStatusDisplayPicker.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/silicon/ai_hud.rs`: кнопка «Change Core Display» — список дисплеев tg (`GLOB.ai_core_display_screens`, `Random`), ядро показывает `ai-<имя>`, мёртвое — `<состояние>_dead` или `ai_dead`; выбор списком без превью. Нет статус-дисплеев ИИ (`/obj/machinery/status_display/ai` и их окна), портрета, дисплея из префов

### Вызов шаттла ИИ, вызов ботов, путевые точки
ИИ может вызвать эвакуационный шаттл, отправить бота в точку и ставить путевые точки ботам.

- **Код:** [`proc/ai_call_shuttle()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai.dm#L305), [`proc/call_bot()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai.dm#L555), [`proc/set_waypoint()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai.dm#L545)
- **Статус в ss13-rs:** ✅ реализовано (вызов шаттла) — `crates/ss13_game/src/silicon/ai_hud.rs` (кнопка «Call Emergency Shuttle»): отключённое управление и `canEvac` проверяются до ввода причины, после — `requestEvac` (`crates/ss13_game/src/comms_server.rs`: снова `canEvac`, причина короче 12 символов отклоняется выше зелёного кода). Вызов ботов и путевые точки — пульт ботов ИИ, см. bots.md

### Тревоги станции у синтетиков
ИИ и борги получают тревоги (пожар, атмос, питание, камеры, двигатели) и видят их список; консоль тревог для экипажа.

- **Код:** [`proc/alarm_triggered()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai.dm#L565), [`proc/show_alarms()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/silicon.dm#L165), [`/obj/machinery/computer/station_alert`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/station_alert.dm#L1); tgui: [`NtosStationAlertConsole.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/NtosStationAlertConsole.tsx)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/silicon/alarms.rs`: тревоги пожара (активная пожарная тревога зоны: извещатели и пожарные шлюзы), атмосферы (воздушная тревога в опасности) и питания (`ALARM_POWER` ЛКП с гистерезисом 30/75 %, `power/model.rs`) по зонам и источникам; ИИ и киборги получают строки в чат с задержкой 3 с (с камерой зоны у ИИ, сводка по типам при многих); окно `StationAlertConsole` по кнопке ИИ «Show Alerts» и киборга «Alert Panel» (у ИИ — камеры и источники зоны, кнопка прыгает глазом на камеру или спрашивает какую); консоль тревог станции (`/obj/machinery/computer/station_alert`: пожар, атмос, питание станционных зон или своего уровня вне станции, экран `alert:2`). Тревоги камер, движения и витрин не шлются: камеры здесь не ломаются и не видят движение, витрин нет (см. их записи); NTOS-приложение — см. ../comms/ntos-apps.md
ИИ может «вселяться» в пустые оболочки киборгов (shell) и управлять ими удалённо.

- **Код:** [`proc/select_shell()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai.dm#L1008), [`/mob/living/silicon/robot/shell`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/robot/robot_defines.dm#L139), [`/obj/item/borg/upgrade/ai`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/robot/robot_upgrades.dm#L579)
- **Статус в ss13-rs:** ❌ нет

### Управление киборгами
ИИ видит привязанных боргов и может блокировать/детонировать их (как консоль робототехники).

- **Код:** [`get_status_tab_items()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai.dm#L276) (список привязанных боргов во вкладке статуса); `/datum/robot_control` с `RemoteRobotControl` — пульт ботов, см. bots.md
- **Статус в ss13-rs:** ❌ нет — привязка борга к ИИ есть (`crates/ss13_game/src/silicon/cyborg.rs`, `AiCore::connected_robots`), но нет вкладки статуса, где tg показывает список; блокировка и подрыв — консоль робототехники (см. cyborgs.md)

### Ядро ИИ: сборка и разборка
Каркас ядра собирается поэтапно (плата, кабели, стекло, MMI/позитронный мозг, законы); готовое ядро с ИИ можно открутить и перенести (`ai_mob_to_structure`).

- **Код:** [`/obj/structure/ai_core`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/ai_core/_core.dm#L3), [`core_construction.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/ai_core/core_construction.dm#L5), [`proc/ai_mob_to_structure()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai.dm#L425)
- **Статус в ss13-rs:** ❌ нет

### ИнтеллКарта (intelliCard)
Переносной носитель ИИ: забирает ИИ из ядра, позволяет переносить его и отключать беспроводной доступ.

- **Код:** [`/obj/item/aicard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/aicard.dm#L1), [`aicard_evil.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/aicard_evil.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Восстановление ИИ (AI fixer)
Консоль и NTOS-приложение для ремонта и перезапуска ИИ из интеллКарты.

- **Код:** [`/obj/machinery/computer/aifixer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/aifixer.dm#L1); tgui: [`AiRestorer.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AiRestorer.tsx), [`NtosAiRestorer.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/NtosAiRestorer.tsx)
- **Статус в ss13-rs:** ❌ нет

### Защита ядра: AI slipper и турели
Раздатчик пены у ядра, которым управляет ИИ, и турели аплоада/ядра.

- **Код:** [`/obj/machinery/ai_slipper`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/ai_slipper.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Урон, смерть и EMP ИИ
Ядро получает урон, EMP отключает ИИ, смерть оставляет корпус; возможна перекарта в MMI.

- **Код:** [`ai_defense.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai_defense.dm#L1), [`ai/death.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/death.dm#L1), [`proc/make_mmi()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai.dm#L409)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/silicon/mod.rs`: brute/burn и кислород (только ИИ, от севшей батареи), без крита, смерть на −100, мёртвый ИИ не двигает глаз и не кликает, глаз возвращается на ядро (`ai.rs`), ядро показывает мёртвый дисплей (`ai_hud.rs`); ЭМИ (`crates/ss13_game/src/emp/mobs.rs`): ожог 20/10, «*БЗЗЗТ*», статика, с 30% глаз на ядро или вызов эвакуации с текстом tg (нет оболочек для отключения); нет перегрузки, взрыва, сообщения смерти ИИ (текст deathgasp как у карбона)

### Апгрейды ИИ
Модули-апгрейды: наблюдение (слух через камеры), передача энергии, боевые (малф).

- **Код:** [`/obj/item/ai_module/upgrade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/AI_modules/ai_upgrades.dm#L4), [`/obj/item/ai_module/upgrade/surveillance`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/AI_modules/ai_upgrades.dm#L104), [`/obj/item/ai_module/upgrade/power_transfer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/AI_modules/ai_upgrades.dm#L121), [`remote_power.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/ai_actions/remote_power.dm#L8) (удалённое питание ЛКП)
- **Статус в ss13-rs:** ❌ нет

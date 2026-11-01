# Master Controller и служебные подсистемы
Планировщик BYOND-сервера (MC) и подсистемы, чья работа заметна игрокам (лаг, таймеры, очистка, AFK). В ss13-rs вместо MC — расписание Bevy (`FixedUpdate`/системы плагинов).

### Master Controller
`/datum/controller/master`: инициализация подсистем по зависимостям (с прогрессом в лобби BandaStation), цикл тиков с бюджетом по приоритетам, tick dilation, перезапуск при падении; failsafe следит за MC.

- **Код:** [`/datum/controller/master`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/master.dm#L12), [`/datum/controller/master/proc/Loop()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/master.dm#L592), [`/datum/controller/failsafe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/failsafe.dm#L10)
- **BandaStation:** [`master.dm#L436`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/master.dm#L436) — счётчик загрузки подсистем для HTML-лобби
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/server.rs` (Bevy-приложение с фиксированным шагом): нет бюджетов тика, tick dilation, failsafe; индикатора загрузки нет

### Таймеры
`SStimer`: `addtimer` с флагами (уникальный, перезаписываемый, останавливаемый), бакеты по тикам — основа отложенных эффектов.

- **Код:** [`SUBSYSTEM_DEF(timer)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/timer.dm#L17)
- **Статус в ss13-rs:** 🟡 частично — отложенные эффекты реализованы по системам (таймеры действий в `crates/ss13_game/src/tools.rs` и др.), общего планировщика нет

### Сборщик мусора
`SSgarbage`: очередь `qdel`, проверка висящих ссылок, hard delete, логи для отладки утечек.

- **Код:** [`SUBSYSTEM_DEF(garbage)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/garbage.dm#L24)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game`: сущности ECS удаляются через `despawn` (Bevy), ссылок-утечек BYOND нет

### Обслуживание сервера и AFK-кик
`SSserver_maint`: кик неактивных игроков (`kick_inactive`, `afk_period`), очистка списков клиентов, перезапуск TGS-соединения.

- **Код:** [`SUBSYSTEM_DEF(server_maint)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/server_maint.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Лаг-свитчи
`SSlag_switch`: меры против лага (отключить runechat, зум призраков, медленный режим вербов, телепорт призраков...), авто-включение при большом онлайне.

- **Код:** [`SUBSYSTEM_DEF(lag_switch)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/lag_switch.dm#L2), [`/datum/controller/subsystem/lag_switch/proc/set_measure()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/lag_switch.dm#L76)
- **Статус в ss13-rs:** ❌ нет

### Ввод и очередь вербов
`SSinput`/`SSverb_manager`/`SSspeech_controller`: обработка нажатий и вербов в начале тика, вынос речи из потока ввода.

- **Код:** [`VERB_MANAGER_SUBSYSTEM_DEF(input)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/input.dm#L1), [`SUBSYSTEM_DEF(verb_manager)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/verb_manager.dm#L22), [`SUBSYSTEM_DEF(speech_controller)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/speech_controller.dm#L2)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_net` (намерения клиента `ClientIntent` с лимитами и проверкой), `crates/ss13_chat/src/server.rs` (лимит речи)

### Пинг
`SSping`: измерение пинга клиентов (в статпанели и «Who»).

- **Код:** [`SUBSYSTEM_DEF(ping)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ping.dm#L6)
- **Статус в ss13-rs:** ❌ нет — пинг не показывается игроку

### Учёт времени и профилирование
`SStime_track` (tick dilation, время для логов), `SSprofiler`/`SSinit_profiler` (профили тиков и инициализации).

- **Код:** [`SUBSYSTEM_DEF(time_track)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/time_track.dm#L1), [`SUBSYSTEM_DEF(profiler)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/profiler.dm#L1), [`SUBSYSTEM_DEF(init_profiler)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/init_profiler.dm#L5)
- **Статус в ss13-rs:** ❌ нет

### Пространственная сетка
`SSspatial_grid`: сетка клеток для быстрого поиска слушателей и атомов в радиусе (речь, радио).

- **Код:** [`SUBSYSTEM_DEF(spatial_grid)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/spatial_gridmap.dm#L86)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_chat/src/server.rs` (поиск слушателей через пространственный индекс в боксе 15x15)

### Система модпаков (BandaStation)
`SSmodpacks`: инициализация модпаков форка (`pre_initialize`/`initialize`/`post_initialize`), верб «Modpacks List» со списком модпаков.

- **Код:** [`SUBSYSTEM_DEF(modpacks)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/_modpacks.dm#L1), [`/datum/modpack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/_modpack.dm#L1)
- **BandaStation:** [`modular_bandastation.dme`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/modular_bandastation.dme#L1)
- **Статус в ss13-rs:** ❌ нет — модпаки учитываются только как DM-данные в `crates/ss13_dm_extract`

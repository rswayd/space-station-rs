# Система случайных событий

Планировщик случайных событий станции, админские инструменты запуска и общие правила, по которым события выбираются.

### Подсистема событий SSevents
Раз в 2,5–7 минут (конфиг `events_frequency_lower/upper`) выбирает случайное событие по весам среди доступных: учитываются минимум игроков, `earliest_start`, `max_occurrences`, карта и праздники. Перед стартом админы получают уведомление и могут отменить событие.

- **Код:** [`SUBSYSTEM_DEF(events)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/events.dm#L1), [`proc/spawnEvent()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/events.dm#L105), [`proc/reschedule()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/events.dm#L96)
- **BandaStation:** [`code/controllers/subsystem/events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/events.dm#L214) — праздничное переименование станции пропускается, если у карты задан `fluff_name` (Station Fluff)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/controller.rs`: раз в 2 с (раунд идёт) по `scheduled` выбор по весам (`pick_weight`) среди событий, прошедших `can_spawn_event` (`earliest_start`, `min_players`, `max_occurrences`, эвакуация), перенос на `rand(1500, 4200)` дс (конфиг BandaStation), 20 с админского окна с повторной проверкой и перевыбором; в пуле только портированные события (`crates/ss13_game/src/events/mod.rs` `controls()`). Нет: учёта AFK, `events.json` (у BandaStation `EVENTS_CONFIG_ENABLED` выключен), админской отмены/перевыбора в окне

### Контроллер и жизненный цикл события
`/datum/round_event_control` описывает событие (вес, категория, ограничения, описание для админов), `/datum/round_event` — запущенный экземпляр: `setup()`, `announce()`, `start()`, `tick()` каждую секунду до `end_when`, `end()`. Объявление может приходить с задержкой `announce_when` и с шансом `announce_chance`, призраки получают уведомление с точкой интереса.

- **Код:** [`/datum/round_event_control`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/_event.dm#L4), [`/datum/round_event`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/_event.dm#L198), [`proc/run_event()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/_event.dm#L155), [`proc/tick()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/_event.dm#L267)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/controller.rs`: `EventControl` (вес, ограничения, `occurrences`, `fakeable`) и `RoundEvent` с `setup/announce/start/tick/end`, `process()` по tg (`start_when`, `announce_when` + `announce_chance`, `end_when`, `kill`); без оповещения призраков (deadchat, точка интереса)

### Принудительный запуск события (Force Event)
Админское окно со списком всех событий по категориям, поиском и описаниями; запускает событие немедленно, с объявлением или без. Часть событий перед запуском спрашивает параметры (`/datum/event_admin_setup`: выбор из списка, локация, число, текст, подтверждение, минимум кандидатов).

- **Код:** [`force_event.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/force_event.dm#L14), [`/datum/event_admin_setup`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/_event_admin_setup.dm#L3), [`/datum/event_admin_setup/set_location`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/_event_admin_setup.dm#L91); tgui: [`ForceEvent.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ForceEvent.tsx)
- **Статус в ss13-rs:** 🟡 частично — `events::force_event` (имя tg, опционально с объявлением/без): кнопки событий во вкладке «Игра» админ-панели (`crates/ss13_admin`, `+FUN`, в админ-лог), тест-хук `{"op":"event"}` (`crates/ss13_debug/src/test_hooks.rs`, шаг ботов `event "…"`) и `SS13_FORCE_EVENT` (debug-сборки); нет окна `ForceEvent` с поиском и `event_admin_setup`

### Переключение случайных событий
Админский глагол сервера включает и выключает автоматический запуск случайных событий на раунд; конфиг `allow_random_events` задаёт значение по умолчанию.

- **Код:** [`ADMIN_VERB(toggle_random_events)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/server.dm#L3)
- **Статус в ss13-rs:** 🟡 частично — `EventsConfig::allow_random_events` (`crates/ss13_game/src/events/controller.rs`, `SS13_RANDOM_EVENTS=0` выключает, по умолчанию включено как в конфиге BandaStation); админского глагола нет

### Режим «Summon Events» (wizard mode)
Заклинание мага переключает SSevents на пул волшебных событий (`EVENT_CATEGORY_WIZARD`) с повышенной частотой; повторный вызов увеличивает частоту.

- **Код:** [`proc/toggleWizardmode()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/events.dm#L143), [`/datum/round_event_control/wizard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/_event.dm#L67)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/controller.rs` `summon_events` (ритуал Summon Events в книге мага, `crates/ss13_game/src/antag/wizard/server.rs`): первый вызов — частота 1–5 минут и `wizardmode`, повторный — верхняя граница −1 минута (не ниже нижней), перенос следующего события; `can_spawn` пропускает только события своего режима

### Праздничные события
События с `holidayID` выпадают только в соответствующий праздник (`check_holidays`, включается конфигом `ALLOW_HOLIDAYS`); праздники заданы датумами `/datum/holiday`.

- **Код:** [`proc/check_holidays()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/events.dm#L175), [`/datum/holiday`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holiday/holidays.dm#L4)
- **BandaStation:** [`modular_bandastation/balance/code/events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/events.dm#L18) — Pride Week никогда не наступает
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/holidays.rs` (календарь, `check_holidays`, `HolidaysConfig`: `SS13_HOLIDAYS=0`, `SS13_HOLIDAY_DATE` в debug), `crates/ss13_game/src/events/controller.rs`: `EventControl::holiday` исключает событие вне праздника (`refresh_availability`), отрицательный вес запускает событие при выборе в обход ростера (`spawn_event`), в День Чернобыля вес Radiation Storm ×2 и +2 к лимиту; Pride Week не наступает (BandaStation)

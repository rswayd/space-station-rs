# Загрузка карт, уровни и маппинг-хелперы

Как сервер собирает мир: конфиг карты, чтение `.dmm`, z-уровни и их трейты, резервации, хелперы маппинга, голосование за карту, мелкий рандом при загрузке.

### Конфиг карты (`/datum/map_config`)
JSON карты: имя, путь к `.dmm`, шаттлы, трейты уровней (`Linkage`, `Baseturf`, `Up`/`Down`, `Mining`...), `minetype`, число космических руин, `job_changes`, `planetary`, `give_players_hooks`, `bonus_weakpoints`. Форк добавляет `main_floor`, `fluff_name` (имя станции, перекрывающее случайное и праздничное) и `welcome_sound_override`.

- **Код:** [`/datum/map_config`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/map_config.dm#L6), [`LoadConfig()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/map_config.dm#L132)
- **BandaStation:** [`BANDASTATION ADDITION - Underfloor Blobs / Station Fluff`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/map_config.dm#L75), [`station_name()` с `fluff_name`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/names.dm#L90)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/dock.rs` (`map_shuttle`): из json читается только `"shuttles"`; `crates/ss13_mapload/src/jobs.rs` прямо игнорирует `job_changes`; трейты, `minetype`, `fluff_name`, звук приветствия не читаются; `station_name()` в `crates/ss13_game/src/round.rs` — просто имя карты

### Чтение `.dmm` / TGM
Парсер карт (оба формата: классический dmm и TGM), префабы с правками переменных, площадь — последний путь клетки, турф — предпоследний, остальное — объекты по порядку; кроп и смещение для шаблонов.

- **Код:** [`/datum/parsed_map`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/reader.dm#L72), [`/proc/load_map`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/reader.dm#L151), [`_tgm_load()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/reader.dm#L374), [`/datum/map_report`/`check_for_errors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/verify.dm#L66)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_assets/src/dmm.rs`, `crates/ss13_mapload/src/load.rs` (те же правила клетки; правки переменных превращаются в компоненты прототипов, неизвестные считаются в отчёте)

### Загрузка мира и групп уровней (SSmapping)
`loadWorld()`: станция (один или несколько уровней), CentCom, уровень шахты (Лаваленд при `minetype = lavaland`), космические уровни для руин, пустые уровни; список станционных зон, генерация рельефа.

- **Код:** [`SUBSYSTEM_DEF(mapping)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/mapping.dm#L1), [`loadWorld()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/mapping.dm#L471), [`LoadGroup()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/mapping.dm#L404), [`generate_station_area_list()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/mapping.dm#L521)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/load.rs` (`load_maps`): карты из `SS13_MAP` грузятся на последовательные z, затем шаблон эвакошаттла на свой z как замена CentCom (`crates/ss13_game/src/shuttle.rs`); нет CentCom.dmm, Лаваленда, космических/пустых уровней

### Z-уровни и трейты
`/datum/space_level` с трейтами (`Station`, `CentCom`, `Mining`, `Space Ruins`, `Gravity`, `Baseturf`, `No Parallax`...), которые определяют гравитацию, радио-досягаемость, правила телепортации, антагонистов и т. д.

- **Код:** [`/datum/space_level`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/space_management/space_level.dm#L1), [`add_new_zlevel()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/space_management/zlevel_manager.dm#L24), [`traits.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/space_management/traits.dm#L1), [`ZTRAITS_*`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__DEFINES/maps.dm#L132)
- **Статус в ss13-rs:** ❌ нет — уровни в `ss13_world::SpatialIndex` (`crates/ss13_world/src/index.rs`) без трейтов; радио ограничено «тем же z» (`crates/ss13_game/src/radio.rs`)

### Мульти-Z (связь уровней по вертикали)
Уровни с `Up`/`Down` образуют один многоэтажный объект: открытое пространство (`openspace`) с падением, лестницы, лестницы-стремянки, вид вниз, газ и свет через перекрытия. Нужна для Tramstation, Cyberiad, Nebula, Wawa, IceBox. Сами лестницы и стремянки — см. ../engineering/.

- **Код:** [`multiz_helpers.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/space_management/multiz_helpers.dm#L1), [`ZTRAIT_UP/DOWN`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__DEFINES/maps.dm#L77)
- **Статус в ss13-rs:** 🟡 частично — какой уровень над каким (`crates/ss13_mapload/src/load.rs`, `MapLoadReport::stacked_levels`, как tg `height_autosetup`): по ней соединяются межэтажные трубы атмоса; все уровни станционной карты — станционные (`station_levels`). переход между этажами (`crates/ss13_game/src/multiz.rs`: ступени, стремянки, падение, PgUp/PgDn) и вид вниз сквозь открытое пространство и стекло — в записях ../engineering/ и ../mobs/movement.md. Нет газа и света через перекрытия, звука с соседних уровней; `crates/ss13_game/src/pull.rs` запрещает действия между уровнями; `crates/ss13_game/src/power/mod.rs`: «add with multi-z maps»

### Переходы через край карты
Космические уровни с `Linkage = Cross` склеены краями: улетевший за край карты попадает на соседний космический уровень.

- **Код:** [`setup_map_transitions()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/space_management/space_transition.dm#L60)
- **Статус в ss13-rs:** ❌ нет

### Резервации турфов и транзит
Выделение свободных прямоугольников на служебных уровнях для шаблонов, транзита шаттлов и ленивых шаблонов; кордон вокруг, выброс в космос из транзита.

- **Код:** [`/datum/turf_reservation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/space_management/space_reservation.dm#L3), [`initialize_reserved_level()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/mapping.dm#L687)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/shuttle.rs` (`make_transit`): отдельный космический уровень под транзит эвакошаттла; общей системы резерваций нет

### Шаблоны карт и ленивые шаблоны
`/datum/map_template` — загрузка `.dmm` в заданное место (шаттлы, руины, админские шаблоны); `/datum/lazy_template` грузит большие локации по требованию (база нюкеров, CentCom синдиката и др.); предзагрузка из `_maps/templates/`.

- **Код:** [`/datum/map_template`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/map_template.dm#L1), [`/proc/load_new_z_level`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/map_template.dm#L233), [`preloadTemplates()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/mapping.dm#L541), [`/datum/lazy_template`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/lazy_template.dm#L6), [`lazy_load_template()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/mapping.dm#L879)
- **BandaStation:** [`modular_bandastation/mapping/code/templates.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mapping/code/templates.dm#L2) — ленивые шаблоны `nukie_base`, `syndie_cc`
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/load.rs`, `crates/ss13_mapload/src/dock.rs`: только загрузка шаблона эвакошаттла на новый уровень (`ShuttleTemplates`); `template_noop` учитывается; админских, ленивых и прочих шаблонов нет

### Модульный загрузчик карт
`/obj/modular_map_root` на карте грузит случайный вариант кусочка из набора (`modular_map_connector`) — варианты комнат на одной станции.

- **Код:** [`/obj/modular_map_root`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/modular_map_loader/modular_map_loader.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Хелперы доступа и шлюзов
Объекты-хелперы на карте, которые при загрузке задают доступ шлюзам (`access/any`, `access/all` по отделам), болты, сварку, `unres`, циклинг шлюзов, заметки на дверях.

- **Код:** [`/obj/effect/mapping_helpers/airlock`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/mapping_helpers.dm#L130), [`airlock/access`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/access_helpers.dm#L1), [`airlock_note_placer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/mapping_helpers.dm#L1144)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/door.rs` (`DmAirlockHelper`: доступ `any`/`all`, `locked`, `unres`, `welded`); `// shortcut:` не применяются `cyclelink_helper` (циклинг), `abandoned`, `red_alert_access` и редкие хелперы, заметки на дверях

### Хелперы машин и окружения
Хелперы воздушных тревог, консолей запросов, выключателей света, сортировки почты (`mail_sorting`), повреждённых полов/окон/машин, вмятин на стенах, гравировок, наклеек, трупов, мобов на кроватях, инжектор атомов и трейтов уровня, праздничные (день рождения Иана, новый год). Хелперы APC — см. [../power/apc.md](../power/apc.md).

- **Код:** [`airalarm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/mapping_helpers.dm#L363), [`turn_off_lights_with_lightswitch`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/mapping_helpers.dm#L678), [`atom_injector`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/mapping_helpers.dm#L720), [`broken_floor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/mapping_helpers.dm#L1272), [`dead_body_placer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/mapping_helpers.dm#L974), [`ianbirthday`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/mapping_helpers.dm#L1028), [`mail_sorting`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mapping/mail_sorting_helpers.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/power.rs`: `turn_off_lights_with_lightswitch`; `crates/ss13_game/src/atmos_machinery/alarm.rs`: хелперы воздушных тревог (доступ, unlocked, TLV cold_room/kitchen/no_checks; не `link` и `surgery`); остальные хелперы (повреждения, трупы, почта, праздники, инжекторы) удаляются без эффекта

### Случайные спаунеры при загрузке
`/obj/effect/spawner/random/*` — лут в техтоннелях, мусор, случайные машины/предметы/декор; часть зависит от трейтов станции и праздников.

- **Код:** [`code/game/objects/effects/spawners`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/spawners)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/spawner.rs`: броски по весам при загрузке (`SS13_MAP_SEED`); спаунеры, зависящие от глобальных таблиц лута, трейтов станции и праздников, удаляются без лута; «визуальные» варианты не меняют облик (`// shortcut:`)

### Мелкий маппинг (SSminor_mapping)
После загрузки: миграция мышей и тараканов на открытые провода, тайники-сумки контрабандистов под полом, слабые точки стен (`weakpoint`, их число растёт на `bonus_weakpoints`).

- **Код:** [`SUBSYSTEM_DEF(minor_mapping)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/minor_mapping.dm#L3), [`trigger_migration()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/minor_mapping.dm#L37), [`place_satchels()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/minor_mapping.dm#L63), [`place_weakpoints()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/minor_mapping.dm#L108)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_mapload/src/undertile.rs` только знает тип `/obj/effect/weakpoint`

### Голосование за карту
В конце раунда (или по запросу) игроки голосуют за следующую карту из `config/maps.txt` с учётом онлайна; результат кешируется, админ может сменить или откатить следующую карту.

- **Код:** [`SUBSYSTEM_DEF(map_vote)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/map_vote.dm#L3), [`finalize_map_vote()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/map_vote.dm#L64), [`get_valid_map_vote_choices()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/map_vote.dm#L112)
- **Статус в ss13-rs:** ❌ нет — после перезагрузки мира грузится та же карта

# Тайловая атмосфера (LINDA и SSair)
Как воздух живёт на открытых тайлах станции: подсистема `SSair`, обмен газом между тайлами, группы, вакуум космоса, планетарная атмосфера и ветер давления.

### Подсистема SSair
Обрабатывает атмосферу каждые 0.5 с в фиксированном порядке фаз: перестройка смежности, трубные сети, машины, активные тайлы, группы, хотспоты, сверхпроводимость, перепады давления, атомы. Пишет в лог тайлы, которые были активны на старте раунда.

- **Код:** [`SSair`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/air.dm#L1), [`fire()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/air.dm#L108), [`proc/setup_allturfs()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/air.dm#L532), [`proc/log_active_turfs()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/air.dm#L619)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_atmos/src/lib.rs`, `crates/ss13_atmos/src/level.rs`: тайлы, группы, хотспоты и перепады давления каждые 0.5 с, уровни параллельно, лог активных тайлов при загрузке; нет фаз трубных сетей, атмос-машин, сверхпроводимости и `process_atoms`

### Обмен газом между тайлами (process_cell)
Активный тайл сравнивает воздух с соседями, делится газом и теплом (`share`), при малой разнице засыпает; изменения стен/дверей будят тайл через `air_update_turf`.

- **Код:** [`/turf/open/process_cell()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_turf_tile.dm#L253), [`proc/share()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/gas_mixture.dm#L353), [`proc/add_to_active()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/air.dm#L501), [`proc/sleep_active_turf()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/air.dm#L490)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/level.rs` (`process_cell`, share/compare по числам DM, сон и пробуждение)

### Группы возбуждённых тайлов (excited groups)
Соседние активные тайлы объединяются в группу, которая периодически усредняет воздух целиком и распадается, когда разница исчезает. Есть админ-отладка показа групп.

- **Код:** [`/datum/excited_group/proc/self_breakdown()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_turf_tile.dm#L469), [`proc/merge_groups()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_turf_tile.dm#L441), [`proc/process_excited_groups()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/air.dm#L385), [`proc/display_turfs()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_turf_tile.dm#L545)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_atmos/src/level.rs`: группы, слияние, breakdown/dismantle, вентиляция в космос; нет админского показа групп (`display_turfs`)

### Проницаемость для воздуха (can_atmos_pass)
Стены (`blocks_air`), закрытые двери, окна и направленные объекты (границы тайла) перекрывают обмен; смежность пересчитывается при изменении плотности.

- **Код:** [`/atom/proc/can_atmos_pass()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_system.dm#L5), [`proc/immediate_calculate_adjacent_turfs()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_system.dm#L111), [`proc/air_update_turf()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_system.dm#L174)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/lib.rs` (`AtmosBlocker`, `AtmosPass`), `crates/ss13_mapload/src/atmos.rs`

### Космос и неизменяемые смеси
Космос — общий неизменяемый вакуум (2.7 K), который поглощает газ; планетарные смеси тоже неизменяемы и возвращают тайл к исходному воздуху.

- **Код:** [`/datum/gas_mixture/immutable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/immutable_mixtures.dm#L4), [`/datum/gas_mixture/immutable/space`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/immutable_mixtures.dm#L45), [`/datum/gas_mixture/immutable/planetary`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/gasmixtures/immutable_mixtures.dm#L58)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/level.rs` (`TileKind::Space`, планетарные тайлы релаксируют к `initial_gas_mix`)

### Генераторы планетарной атмосферы (/datum/atmosphere)
Случайный состав воздуха Лаваленда и Ледяной луны на раунд (базовые и редкие газы, диапазоны давления и температуры), подставляется по id в `initial_gas_mix`.

- **Код:** [`/datum/atmosphere`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/atmosphere/_atmosphere.dm#L1), [`proc/generate_gas_string()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/atmosphere/_atmosphere.dm#L19), [`/datum/atmosphere/lavaland`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/atmosphere/planetary.dm#L2), [`/datum/atmosphere/icemoon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/atmosphere/planetary.dm#L28)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_atmos/src/lib.rs`: shortcut, планеты получают станционный воздух

### Сверхпроводимость (теплопередача через стены)
Горячие или холодные тайлы проводят тепло через твёрдые тайлы и стены по `thermal_conductivity`, излучают в космос (`radiate_to_spess`).

- **Код:** [`/turf/proc/super_conduct()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_turf_tile.dm#L618), [`proc/radiate_to_spess()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_turf_tile.dm#L674), [`proc/process_super_conductivity()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/air.dm#L334)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_atmos/src/level.rs`: shortcut «no superconduction»

### Ветер давления (space wind)
Перепад давления толкает незакреплённые предметы и мобов по потоку с вероятностью от `pressure_resistance`; разгерметизация выдувает всё в космос.

- **Код:** [`/turf/open/proc/high_pressure_movements()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_turf_tile.dm#L383), [`proc/experience_pressure_difference()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_turf_tile.dm#L398), [`proc/process_high_pressure_delta()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/air.dm#L361)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/fire.rs`, `crates/ss13_atmos/src/lib.rs` (`PressureDifference`): нет магбутсов (`TRAIT_NEGATES_GRAVITY`) и пристёгивания

### Оверлеи видимых газов
Плазма, N2O, пар, трития и др. рисуются полупрозрачными оверлеями с прозрачностью по количеству молей.

- **Код:** [`/turf/open/proc/update_visuals()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_turf_tile.dm#L166), [`proc/typecache_of_gases_with_no_overlays()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_turf_tile.dm#L193)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/lib.rs` (`GasOverlay`), `crates/ss13_game/src/atmos.rs`

### Выпуск газа в тайл (atmos_spawn_air, assume_air)
Общие процедуры, которыми машины, взрывы, предметы и админы добавляют или забирают газ из тайла.

- **Код:** [`/turf/open/atmos_spawn_air()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_system.dm#L212), [`/turf/open/assume_air()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_turf_tile.dm#L91), [`/turf/open/remove_air()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_turf_tile.dm#L99)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_atmos/src/lib.rs`: API тайлового воздуха для дыхания, баков и разбитых окон; нет вызовов из машин и админ-команд

### Обжиг пола и экспозиция атомов
Сильный жар обжигает пол (`burn_tile`), атомы на тайле получают `atmos_expose`/`temperature_expose` (плавление, повреждение).

- **Код:** [`/turf/proc/burn_turf()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_turf_tile.dm#L137), [`/turf/atmos_expose()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_turf_tile.dm#L129), [`proc/process_atoms()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/air.dm#L304)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_atmos/src/level/hotspot.rs`: shortcut, нет `burn_tile`; урон от огня получают только горючие объекты через `FireAct`

# Пожары (хотспоты)
Атмосферный огонь: объект `/obj/effect/hotspot`, который появляется при поджоге горючей смеси, сжигает воздух тайла и распространяется.

### Поджог тайла (hotspot_expose)
Источник жара (сварка, зажигалка, искра, горящий моб) «экспонирует» тайл; если в воздухе есть топливо и окислитель и температура достаточна, появляется хотспот.

- **Код:** [`/turf/open/hotspot_expose()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_fire.dm#L23), [`proc/init_hotspot_reactions()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_fire.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_atmos/src/level/hotspot.rs`, `crates/ss13_game/src/fire.rs`: поджигают только сварка и горящие мобы; нет зажигалок, спичек, сигарет, игнайтеров (см. shortcut в `fire.rs`)

### Жизненный цикл хотспота
Хотспот отбирает порцию воздуха, прогоняет в ней реакции, растёт до полного тайла, распространяется на соседей лучистым теплом и гаснет без топлива или кислорода; поджигает (`fire_act`) всё на тайле.

- **Код:** [`/obj/effect/hotspot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_fire.dm#L64), [`process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_fire.dm#L290), [`proc/perform_exposure()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_fire.dm#L171), [`SSair.process_hotspots()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/air.dm#L346)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_atmos/src/level/hotspot.rs` (`process_hotspot`, `perform_exposure`, `FireAct`)

### Вид огня: стадии, цвет, свет
Спрайт огня по стадии (маленький, средний, большой), цвет по температуре, свечение. BandaStation заменяет иконку огня, меняет прозрачность и добавляет «поднимающийся» огонь.

- **Код:** [`proc/update_color()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_fire.dm#L223), [`proc/set_fire_stage()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_fire.dm#L359)
- **BandaStation:** [`LINDA_fire.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_fire.dm#L229) — новая иконка (`modular_bandastation/aesthetics/fire`), `heat_a` 180 вместо 255, блоки «NEW FIRE RISES»
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_atmos/src/lib.rs` (`HotspotEffect`): стадии `fire.dmi` и оверлей BandaStation, свет; цветовая матрица вне 1000–3000 K упрощена (shortcut)

### Группы огня и звук пожара (/datum/hot_group)
Соседние хотспоты объединяются в группу, у которой один зацикленный звук горения с громкостью по размеру.

- **Код:** [`/datum/hot_group`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_fire.dm#L395), [`proc/update_sound()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_fire.dm#L463)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_atmos/src/level/hotspot.rs`: shortcut «no /datum/hot_group»

### Мобы и предметы в огне
Вошедший в хотспот моб получает огненные стаки; горючие объекты горят (`/datum/component/burning`). Механика горения мобов — см. ../mobs/.

- **Код:** [`proc/on_entered()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/environmental/LINDA_fire.dm#L377)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/fire.rs`: мобы и горючие структуры горят; не горят предметы и одежда (`burn_clothing`, пепел)

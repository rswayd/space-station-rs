# Освещение и темнота

Движок освещения (`code/modules/lighting`, `SSlighting`): источники света, тени, базовое освещение зон, звёздный свет, свечение (emissive), поедатели света. Светильники станции, выключатели и питание ламп — в ../power/.

### Статическое освещение: источники, углы, объекты освещения
Каждый атом со `light_range`/`light_power`/`light_color` — источник `/datum/light_source`; свет считается по углам тайлов (`/datum/lighting_corner`) с затенением непрозрачными турфами и рисуется объектом освещения тайла (`/atom/movable/lighting_object`) на плоскости освещения с умножением.

- **Код:** [`/datum/light_source`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/lighting/lighting_source.dm#L7), [`/datum/lighting_corner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/lighting/lighting_corner.dm#L5), [`/atom/movable/lighting_object`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/lighting/lighting_object.dm#L1), [`/atom/proc/set_light()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/lighting/lighting_atom.dm#L3)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_render/src/lighting/` (углы по формуле tg, карта освещения с умножением), `crates/ss13_mapload/src/light.rs` (источники из DM-данных), `crates/ss13_net/src/light_server.rs` (репликация источников и непрозрачности по блокам)

### Подсистема SSlighting: инкрементальное обновление
Изменение источника или непрозрачности тайла (дверь открылась, лампа погасла) ставит в очередь пересчёт только затронутых источников, углов и объектов.

- **Код:** [`SUBSYSTEM_DEF(lighting)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/lighting.dm#L1), [`SSlighting/fire()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/lighting.dm#L48)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_render/src/lighting/mod.rs`: кэш вкладов источника, пересчёт при изменении источника или непрозрачности в радиусе

### Направленный свет (конусы)
`light_angle` и `light_dir` дают конус (светильники светят от стены на 170°), `light_height` и смещение источника к стене.

- **Код:** [`/atom/proc/set_light_angle()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/lighting/lighting_atom.dm#L180), [`/atom/proc/set_light_dir()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/lighting/lighting_atom.dm#L191)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_render/src/lighting/math.rs` (конусы), `crates/ss13_mapload/src/light.rs` (`light_dir`, `light_angle = 170`, смещение к стене)

### Базовое освещение зон и зоны без освещения
Зоны с `static_lighting = FALSE` полностью освещены, `base_lighting_color`/`base_lighting_alpha` добавляют фоновый свет (шаттлы, ЦК, особые зоны).

- **Код:** [`/area/proc/set_base_lighting()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/lighting/lighting_area.dm#L13), [`code/modules/lighting/static_lighting_area.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/lighting/static_lighting_area.dm)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_mapload/src/light.rs` (зоны без `static_lighting`, базовое освещение как ambient), `crates/ss13_shared/src/light.rs`

### Звёздный свет космоса
Космос светится звёздным светом (`starlight_color/range/power`) на тайлах рядом с освещаемыми; `set_starlight()` меняет его в раунде (события, станционные трейты, Aurora Caelus).

- **Код:** [`proc/set_starlight()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/space/space.dm#L25), [`space.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/space/space.dm#L4)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/light.rs`, `crates/ss13_shared/src/light.rs`: звёздный свет только стартовый, `set_starlight` в раунде нет

### Свет, следующий за предметом (overlay lighting)
Компонент `overlay_lighting`: свет фонарика, ПДА, сварки или огня следует за владельцем, направленный конус по взгляду моба, включается и выключается.

- **Код:** [`/datum/component/overlay_lighting`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/overlay_lighting.dm#L17)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/light.rs`: `OVERLAY_LIGHT` рисуется как обычный источник того же радиуса; свет горящего моба и горячих точек есть (`crates/ss13_game/src/fire.rs`); фонарики не включаются, конуса по взгляду нет

### Свечение (emissive) и блокировщики
Оверлеи на плоскости emissive (экраны, лампы, индикаторы) видны в темноте; `emissive_blocker` — тела и предметы перед ними закрывают свечение.

- **Код:** [`proc/emissive_appearance()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/lighting.dm#L2), [`proc/emissive_blocker()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/lighting.dm#L59), [`/atom/movable/screen/plane_master/emissive`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/rendering/plane_masters/plane_master_subtypes.dm#L440)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_render/src/lighting/mod.rs`: маска emissive вторым проходом; блокировщиков нет, моб перед светящимся пикселем тоже освещён

### Световые пустышки мобов
`/obj/effect/dummy/lighting_obj` (и `moblight`): временный свет вокруг моба (огонь, видовые способности, заклинания).

- **Код:** [`/obj/effect/dummy/lighting_obj`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/lighting.dm#L7), [`/obj/effect/dummy/lighting_obj/moblight/fire`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/lighting.dm#L36)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/fire.rs`: горящий моб светится; прочих источников (виды, заклинания) нет

### Поедатели света (light eater)
Оружие и способности (клинок теней, ночное зрение тенеходов) «съедают» свет: источник навсегда гаснет и не может быть включён (`light_eaten`).

- **Код:** [`/datum/element/light_eater`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/light_eater.dm#L6), [`/datum/element/light_eaten`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/light_eaten.dm#L4)
- **Статус в ss13-rs:** ❌ нет — нет механики

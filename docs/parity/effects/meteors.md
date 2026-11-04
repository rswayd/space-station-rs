# Метеоры

Метеоры как объекты (`code/modules/meteors`): полёт, удар, типы, защита. События, которые их бросают (волны, пыль, песчаная буря), — в ../events/space-events.md.

### Метеор: полёт и удар
`/obj/effect/meteor` появляется у края карты и летит к точке на станции (`spaceDebrisStartLoc`/`FinishLoc`); каждое столкновение `ram_turf` бьёт турф `ex_act`, у метеора есть `hits` — сколько стен он пробьёт; в конце `meteor_effect` (взрыв по типу) и обломки (руда, мясо).

- **Код:** [`/obj/effect/meteor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_types.dm#L3), [`proc/ram_turf()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_types.dm#L104), [`proc/meteor_effect()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_types.dm#L144), [`proc/spawn_meteor()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_spawning.dm#L7)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/meteors.rs`: старт и цель по `spaceDebrisStartLoc`/`FinishLoc` (13 клеток от края уровня станции), прямая `move_towards` 10 клеток/с, `ram_turf` (`ex_act` с `hitpwr`) на входе в клетку и при ударе о плотное, `hits`, 10% удара вне космоса, `meteor_effect`, обломки-руда (если есть прототип); нет вращения, тряски, звука

### Обычные метеоры: dust, sand, medium, big, flaming, irradiated, cluster
Пыль и песок царапают обшивку; средний и большой — взрывы разной силы; огненный — поджигает; облучённый — радиация и уран; кластерный распадается на осколки.

- **Код:** [`/obj/effect/meteor/dust`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_types.dm#L219), [`/obj/effect/meteor/sand`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_types.dm#L192), [`/obj/effect/meteor/medium`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_types.dm#L230), [`/obj/effect/meteor/big`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_types.dm#L240), [`/obj/effect/meteor/flaming`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_types.dm#L253), [`/obj/effect/meteor/irradiated`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_types.dm#L269), [`/obj/effect/meteor/cluster`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_types.dm#L287)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/meteors.rs`: dust, medium, big, flaming (горячие точки в `flame_range`), irradiated, cluster (8 осколков) с параметрами tg, их взрывы с `flash_range` (`crates/ss13_game/src/explosions/`); нет sand (песчаной бури), гринглоу и радиации облучённого

### Особые метеоры: тунгуска, bluespace, EMP, banana, carp, pumpkin
Тунгуска — огромный пробивающий станцию метеор; bluespace телепортирует; EMP даёт ЭМИ; банановый скользкий; карп выпускает карпа; тыквенный — хэллоуинский.

- **Код:** [`/obj/effect/meteor/tunguska`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_types.dm#L437), [`/obj/effect/meteor/bluespace`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_types.dm#L335), [`/obj/effect/meteor/emp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_types.dm#L375), [`/obj/effect/meteor/banana`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_types.dm#L350), [`/obj/effect/meteor/carp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_types.dm#L318), [`/obj/effect/meteor/pumpkin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_types.dm#L463)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/meteors.rs`: тунгуска (`EXPLODE_DEVASTATE`, 20% взрыва при ударе), bluespace (35% телепорта на 6 клеток), banana (проходит сквозь всё, валит мобов на 10 с); EMP — `empulse(3, 8)` (`crate::emp`, без звука `zapbang`), carp без карпа (нет простых мобов), pumpkin нет (праздники)

### Мясные метеоры и метеор генокрада
Мясной метеор разбрасывает мясо и кровь (ксено-вариант — ксено-останки); генокрад-метеор — способ появления антагониста-генокрада из космоса.

- **Код:** [`/obj/effect/meteor/meaty`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_types.dm#L394), [`/obj/effect/meteor/meaty/xeno`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_types.dm#L423), [`/obj/effect/meteor/meaty/changeling`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_changeling.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/meteors.rs`: мясной и ксено-метеор (`ex_act` только по тому, во что врезался); нет крови, гибов, мяса/органов и метеора генокрада

### Метеор тёмной материи
Притягивается, только если взломать (emag) 10 спутников метеорного щита; летит с кольцом искажения пространства и оставляет сингулярность тёмной материи.

- **Код:** [`/obj/effect/meteor/dark_matteor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_dark_matteor.dm#L2)
- **Статус в ss13-rs:** ❌ нет — нет метеоров и сингулярности

### Таблицы волн
Глобальные списки весов типов метеоров для каждой волны (нормальная, угрожающая, катастрофическая, мясная, пыль, одиночный, песчаная, хэллоуинская).

- **Код:** [`meteor_waves.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_waves.dm#L7)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/meteors.rs`: `meteors_normal/threatening/catastrophic`, `meateors`, `meteors_dust`, `meteors_stray` с весами tg; нет `meteors_sandstorm` и `meteorsSPOOKY`

### Отражение и уничтожение метеоров руками
Компонент `meteor_combat`: метеор можно ударить кулаком (с ачивкой) и отразить, или разбить кирка/инструментом; осмотр приближающегося метеора тоже даёт ачивку.

- **Код:** [`/datum/component/meteor_combat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_deflection.dm#L5)
- **Статус в ss13-rs:** ❌ нет — нет метеоров

### Метеорный щит (станционная цель)
Спутники `meteor_shield`, развёрнутые карго и включённые с консоли спутников, сбивают метеоры в радиусе; станционная цель — покрыть станцию.

- **Код:** [`/obj/machinery/satellite/meteor_shield`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/station_goals/meteor_shield.dm#L60), [`/obj/machinery/computer/sat_control`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/satellite/satellite_control.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет метеоров и спутников

### Контроллер режима метеоров
`/datum/meteor_mode_controller`: постоянный метеорный дождь с нарастающей силой (используется режимами/админами).

- **Код:** [`/datum/meteor_mode_controller`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_mode_controller.dm#L5)
- **Статус в ss13-rs:** ❌ нет — нет метеоров

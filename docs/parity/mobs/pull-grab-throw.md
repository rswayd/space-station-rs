# Волочение, захваты, переноска и броски
Как мобы тянут предметы и друг друга, захватывают, носят на плечах и бросают предметы. Урон от брошенных предметов — см. ../combat/.

### Волочение (pull)
Ctrl+клик по соседнему незакреплённому объекту начинает тянуть его; после шага тянущего объект следует за ним; волочение лежачих и тяжёлых объектов замедляет. Прерывается расстоянием, оглушением, клавишей «отпустить».

- **Код:** [`proc/start_pulling()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/atoms_movable.dm#L525), [`living/start_pulling()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living.dm#L391), [`proc/stop_pulling()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/atoms_movable.dm#L563), [`proc/update_pull_movespeed()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_movement.dm#L92), [`verb/pulled`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living.dm#L531)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/pull.rs`

### Захваты: пассивный и агрессивный
Повторный ctrl+клик по тянутому мобу усиливает захват до агрессивного: жертва не может уйти, должна вырываться (шанс зависит от состояния захвата), захваченного можно бросить на стол.

- **Код:** [`proc/grabbedby()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_defense.dm#L385), [`proc/grippedby()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_defense.dm#L405), [`living/resist_grab()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living.dm#L1182)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/pull.rs`: пассивный и агрессивный захват, вырывание; нет броска на стол и таблслэма

### Захват за шею и удушение
Дальнейшее усиление (с `do_after`) даёт захват за шею (жертва обездвижена) и удушающий (oxy-урон, нельзя дышать).

- **Код:** [`proc/grippedby()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_defense.dm#L405), [`/datum/movespeed_modifier/grab_slowdown`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/movespeed/modifiers/mobs.dm#L38)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/pull.rs` (shortcut: захваты останавливаются на агрессивном)

### Смещение спрайтов при захвате
Захваченный моб сдвигается на тайл к тянущему в зависимости от силы захвата и направления.

- **Код:** [`proc/set_pull_offsets()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living.dm#L486)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/pull.rs` (shortcut: нет pull offsets)

### Переноска на плечах и «на закорках»
Агрессивно захваченного лежачего можно взвалить на плечо (fireman carry, `do_after`), стоящий может забраться на спину (piggyback); используется компонент `riding`.

- **Код:** [`proc/fireman_carry()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/human.dm#L891), [`proc/piggyback()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/human.dm#L935)
- **Статус в ss13-rs:** ❌ нет

### Режим броска
Клавиша R (или кнопка HUD) включает режим броска; следующий клик бросает предмет из активной руки в цель с `CLICK_CD_THROW`. Агрессивно захваченного моба тоже можно бросить.

- **Код:** [`proc/toggle_throw_mode()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_item_handling.dm#L2), [`living/throw_item()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_item_handling.dm#L38)
- **BandaStation:** [`code/modules/mob/living/living_item_handling.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_item_handling.dm#L112) — глаголы броска из `ru_verbs`
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/throw.rs`: режим броска, бросок предмета; нет броска захваченного моба

### Полёт брошенного предмета
Подсистема `throwing` ведёт `thrownthing` по линии со скоростью `throw_speed` до `throw_range`; предмет бьёт первое плотное препятствие, вращается, в невесомости даёт инерцию бросающему.

- **Код:** [`SUBSYSTEM_DEF(throwing)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/throwing.dm#L4), [`/datum/thrownthing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/throwing.dm#L43), [`proc/throw_at()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/atoms_movable.dm#L1379), [`proc/throw_impact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/atoms_movable.dm#L1345)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/throw.rs`: траектория, скорость, дальность, удар о препятствие; нет вращения, инерции бросающего, дрейфа после приземления

### Ловля брошенного
Моб в режиме броска с пустой активной рукой ловит летящий в него предмет.

- **Код:** [`carbon/hitby()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon_defense.dm#L77)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/throw.rs`

### Брошенный моб
Брошенный или отлетевший моб, врезаясь в стену или другого моба, сбивается с ног и получает урон (`throw_impact` карбона).

- **Код:** [`carbon/throw_impact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L33)
- **Статус в ss13-rs:** ❌ нет

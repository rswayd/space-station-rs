# Радиация

Радиация tg: импульсы с вероятностью облучения, облучённое состояние, излучатели, защита и приборы. Источники (суперматерия, уран, событие Radiation Leak, радиационный шторм) — в своих разделах.

### Радиационный импульс и SSradiation
`radiation_pulse(source, max_range, threshold, chance)`: подсистема проходит от источника к каждому человеку и предмету в радиусе, умножая силу на `rad_insulation` стен, дверей и объектов на пути; пока сила выше порога, цель облучается с шансом, убывающим с расстоянием. `minimum_exposure_time` требует постоять рядом с источником.

- **Код:** [`proc/radiation_pulse()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/radiation.dm#L25), [`SUBSYSTEM_DEF(radiation)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/radiation.dm#L1), [`/datum/radiation_pulse_information`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/radiation.dm#L47)
- **Статус в ss13-rs:** ❌ нет — в `crates/` нет радиации (`crates/ss13_atmos/src/reactions.rs`: shortcut no radiation pulse у реакций тритиевого огня и протонитрата)

### Облучённое состояние (irradiated)
Облучённый человек или предмет светится зелёным; человек сразу получает токсины и затем по 2 токсина раз в 25 с, пока токсины не вылечены, плюс периодические радиационные ожоги; душ смывает облучение, иммунитет даёт радиационная защита.

- **Код:** [`/datum/component/irradiated`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/irradiated.dm#L15), [`/datum/component/radiation_countdown`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/radiation_countdown.dm#L7)
- **Статус в ss13-rs:** ❌ нет — нет радиации

### Радиоактивные объекты
Элемент `radioactive` (раз в 3 с импульс) и компонент `radioactive_emitter` (настраиваемый период и радиус) делают объект постоянным источником: уран, облучённые предметы, протекающие машины.

- **Код:** [`/datum/element/radioactive`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/radioactive.dm#L4), [`/datum/component/radioactive_emitter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/radioactive_emitter.dm#L10)
- **Статус в ss13-rs:** ❌ нет — нет радиации

### Защита от радиации
Радиационный костюм и капюшон, скафандры, MOD-модули дают `TRAIT_RADIMMUNE`/защиту через элемент `radiation_protected_clothing`; статус-эффект временного иммунитета (препараты, туманность).

- **Код:** [`/datum/element/radiation_protected_clothing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/radiation_protected_clothing.dm#L4), [`/obj/item/clothing/suit/utility/radiation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/utility.dm#L214), [`/obj/item/clothing/head/utility/radiation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/utility.dm#L189), [`/datum/status_effect/radiation_immunity`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/buffs.dm#L610)
- **Статус в ss13-rs:** ❌ нет — нет радиации

### Счётчик Гейгера
Ручной прибор: щёлкает при импульсах рядом, показывает уровень, сканирует облучённость человека или предмета.

- **Код:** [`/obj/item/geiger_counter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/geiger_counter.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет радиации

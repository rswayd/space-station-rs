# Абдукторы
Команда пришельцев (агент + учёный) похищает экипаж, вживляет им железы и возвращает на станцию.

### Абдукторы и команда
Агент (боевой жилет, дубинка) и учёный (консоль, хирургия), телепатия команды, цель — провести N экспериментов; одиночный учёный.

- **Код:** [`/datum/antagonist/abductor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/abductor.dm#L1), [`/datum/antagonist/abductor/agent`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/abductor.dm#L40), [`/datum/antagonist/abductor/scientist`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/abductor.dm#L48), [`/datum/team/abductor_team`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/abductor.dm#L197), [`/datum/objective/experiment`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/abductor.dm#L244), [`/datum/outfit/abductor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/equipment/abduction_outfits.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Корабль и машины
Консоль (камера наблюдения, телепорт на станцию и обратно, точка сброса, покупка снаряжения за очки), телепад, раздатчик желёз, экспериментальная капсула.

- **Код:** [`/obj/machinery/abductor/console`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/machinery/console.dm#L15), [`/obj/machinery/computer/camera_advanced/abductor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/machinery/camera.dm#L1), [`/obj/machinery/abductor/pad`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/machinery/pad.dm#L1), [`/obj/machinery/abductor/gland_dispenser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/machinery/dispenser.dm#L1), [`/obj/machinery/abductor/experiment`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/machinery/experiment.dm#L1); tgui: [`AbductorConsole.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AbductorConsole.tsx)
- **Статус в ss13-rs:** ❌ нет

### Снаряжение абдукторов
Жилет (стелс/боевой режим, маскировка), шлем, продвинутая дубинка (оглушение/сон/наручники/зонд), энергонаручники, гизмо-сканер, глушитель радио, ментальное устройство, инопланетный пистолет и луч сжатия, инопланетные хирургические инструменты и омнитул.

- **Код:** [`/obj/item/clothing/suit/armor/abductor/vest`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/equipment/gear/abductor_clothing.dm#L15), [`/obj/item/melee/baton/abductor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/equipment/gear/abductor_items.dm#L323), [`/obj/item/abductor/gizmo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/equipment/gear/abductor_items.dm#L27), [`/obj/item/abductor/silencer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/equipment/gear/abductor_items.dm#L104), [`/obj/item/abductor/mind_device`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/equipment/gear/abductor_items.dm#L144), [`/obj/item/gun/energy/alien`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/equipment/gear/abductor_items.dm#L248), [`/obj/item/abductor/alien_omnitool`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/equipment/gear/abductor_items.dm#L652), [`/datum/abductor_gear`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/equipment/orderable_gear.dm#L7)
- **BandaStation:** [`abductor_items.dm#L242`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/equipment/gear/abductor_items.dm#L242) — несъёмный боёк инопланетного оружия
- **Статус в ss13-rs:** ❌ нет

### Железы
Сердце-железа вместо органа: доступ, кровь, химикаты, яйца, электричество, лечение, ментальный шок, плазма, квантовый обмен, слизь, пауки, трансформация, травмы, вентиляция, вирусы.

- **Код:** [`/obj/item/organ/heart/gland`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/equipment/gland.dm#L1), [`code/modules/antagonists/abductor/equipment/glands`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/equipment/glands)
- **Статус в ss13-rs:** ❌ нет

### Похищенные
Похищенный получает странную навязчивую цель (украсть, нарисовать, говорить определённым образом, поймать, «запретное число»).

- **Код:** [`/datum/objective/abductee`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/abductee/abductee_objectives.dm#L1), [`/datum/objective/abductee/random`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/abductee/abductee_objectives.dm#L4)
- **Статус в ss13-rs:** ❌ нет

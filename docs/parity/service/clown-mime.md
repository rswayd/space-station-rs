# Клоун и мим
Клоунские предметы (гудки, утята, консервированный смех) и способности мима. Клоунская одежда и маски — см. ../items/, банановая кожура — см. ../botany/, кремовый пирог — см. ../kitchen/.

### Велосипедный гудок
Клик или удар гудком издаёт «хонк» (компонент писка, тоже срабатывает при наступании); воздушный гудок громче и крафтится, золотой заставляет всех вокруг сделать сальто.

- **Код:** [`/obj/item/bikehorn`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/clown_items.dm#L196), [`/obj/item/bikehorn/airhorn`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/clown_items.dm#L235), [`/obj/item/bikehorn/golden/proc/flip_mobs()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/clown_items.dm#L270), [`/datum/component/squeak`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/squeak.dm#L1)
- **Статус в ss13-rs:** ❌ нет — гудок есть только как цель кражи в `crates/ss13_mapload/src/antag.rs`

### Резиновые уточки
Пищащие утята (резиновый, пластиковый) — вариант гудка.

- **Код:** [`/obj/item/bikehorn/rubberducky`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/clown_items.dm#L284)
- **Статус в ss13-rs:** ❌ нет

### Консервированный смех и надувной молот
Банка «смеха» (напиток, вызывающий смех), надувной молот без урона с забавными звуками.

- **Код:** [`/obj/item/reagent_containers/cup/soda_cans/canned_laughter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/clown_items.dm#L296), [`/obj/item/balloon_mallet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/clown_items.dm#L304)
- **Статус в ss13-rs:** ❌ нет

### Клоунская печать
Печать клоуна «HONK» для бумаг (механика печатей — paperwork.md).

- **Код:** [`/obj/item/stamp/clown`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/stamps.dm#L93)
- **Статус в ss13-rs:** ❌ нет

### Клоунская машина
Машина клоуна вмещает экипаж, похищает людей при наезде, имеет кнопки с гэгами; угнанная emag — хаос-режимы. Транспорт как система вне этой папки.

- **Код:** [`/obj/vehicle/sealed/car/clowncar`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/cars/clowncar.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Обет молчания мима
Мим начинает с обетом молчания (`TRAIT_MIMING`): не может говорить; кнопка «Break Vow» навсегда нарушает обет, даёт плохое настроение и открывает ещё один слот мима.

- **Код:** [`/datum/action/cooldown/spell/vow_of_silence`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/self/mime_vow.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Пантомима: невидимая стена, ящик, стул
Способности мима (пока соблюдён обет): невидимый ящик-предмет, невидимый стул; книги «Guide to Advanced Mimery» дают невидимую стену и «пальцевые пистолеты».

- **Код:** [`/datum/action/cooldown/spell/forcewall/mime`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/self/forcewall.dm#L45), [`/datum/action/cooldown/spell/conjure_item/invisible_box`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/conjure_item/invisible_box.dm#L3), [`/datum/action/cooldown/spell/conjure/invisible_chair`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/conjure/invisible_chair.dm#L1), [`/obj/item/book/granter/action/spell/mime`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/granters/magic/mime.dm#L1)
- **Статус в ss13-rs:** ❌ нет

# Клоун и мим
Клоунские предметы (гудки, утята, консервированный смех) и способности мима. Клоунская одежда и маски — см. ../items/, банановая кожура — см. ../botany/, кремовый пирог — см. ../kitchen/.

### Велосипедный гудок
Клик или удар гудком издаёт «хонк» (компонент писка, тоже срабатывает при наступании); воздушный гудок громче и крафтится, золотой заставляет всех вокруг сделать сальто.

- **Код:** [`/obj/item/bikehorn`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/clown_items.dm#L196), [`/obj/item/bikehorn/airhorn`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/clown_items.dm#L235), [`/obj/item/bikehorn/golden/proc/flip_mobs()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/clown_items.dm#L270), [`/datum/component/squeak`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/squeak.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/service/clown.rs`: компонент писка — в руке (раз в 2 с), удар по живому мобу, наступание стоящего человека на тайл; воздушный гудок со своим звуком; клоун, хонкнувший другого, даёт ему mood `honk`; клоунские ботинки пищат каждый третий шаг, костюм клоуна — тоже писклявый; нет сальто от золотого гудка, писка при броске/столкновении, крафта

### Резиновые уточки
Пищащие утята (резиновый, пластиковый) — вариант гудка.

- **Код:** [`/obj/item/bikehorn/rubberducky`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/clown_items.dm#L284)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/service/clown.rs`: писк `quack.ogg` у резиновой и пластиковой уточки (те же триггеры, что у гудка)

### Консервированный смех и надувной молот
Банка «смеха» (напиток, вызывающий смех), надувной молот без урона с забавными звуками.

- **Код:** [`/obj/item/reagent_containers/cup/soda_cans/canned_laughter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/clown_items.dm#L296), [`/obj/item/balloon_mallet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/clown_items.dm#L304)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/service/clown.rs` (`laughter`, `mallet_hit`, `mallet_effects`), `crates/ss13_game/src/combat.rs`: реагент смеха один раз заставляет выпившего смеяться и даёт moodlet `chemical_laughter`; молот хихикает при каждом взмахе по живому, сила по рассудку цели (8 / 4 / 1), унижение (`mallet_humiliation`), бьёт только в боевом режиме; нет текста осмотра для `TRAIT_BALLOON_SUTRA` (трейт не портирован, см. toys.md)

### Клоунская печать
Печать клоуна «HONK» для бумаг (механика печатей — paperwork.md).

- **Код:** [`/obj/item/stamp/clown`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/paperwork/stamps.dm#L93)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/service/paper.rs`: печать клоуна ставится как любая печать (`paper_stamp-clown`)

### Клоунская машина
Машина клоуна вмещает экипаж, похищает людей при наезде, имеет кнопки с гэгами; угнанная emag — хаос-режимы. Транспорт как система вне этой папки.

- **Код:** [`/obj/vehicle/sealed/car/clowncar`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vehicles/cars/clowncar.dm#L1)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/cars.rs`: экипаж внутри (водит только Клоун), похищение наездом и перетаскиванием, кнопки клаксона/фар/выгрузки/благодарности, после емага — разноцветные кнопки и пушка из похищенных; подробности и недоделки — [../vehicles/vehicles.md](../vehicles/vehicles.md) («Закрытые машины»)

### Обет молчания мима
Мим начинает с обетом молчания (`TRAIT_MIMING`): не может говорить; кнопка «Break Vow» навсегда нарушает обет, даёт плохое настроение и открывает ещё один слот мима.

- **Код:** [`/datum/action/cooldown/spell/vow_of_silence`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/self/mime_vow.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/service/clown.rs`: мим со спавна под обетом (`MimeVow`), `crates/ss13_game/src/medical/traumas.rs` складывает его в немоту чата (нельзя говорить, шептать, в рацию); нет кнопки «Break Vow» (система спеллов — wizard/spells), mood за нарушение, немых эмоций мима

### Пантомима: невидимая стена, ящик, стул
Способности мима (пока соблюдён обет): невидимый ящик-предмет, невидимый стул; книги «Guide to Advanced Mimery» дают невидимую стену и «пальцевые пистолеты».

- **Код:** [`/datum/action/cooldown/spell/forcewall/mime`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/self/forcewall.dm#L45), [`/datum/action/cooldown/spell/conjure_item/invisible_box`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/conjure_item/invisible_box.dm#L3), [`/datum/action/cooldown/spell/conjure/invisible_chair`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/conjure/invisible_chair.dm#L1), [`/obj/item/book/granter/action/spell/mime`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/granters/magic/mime.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/service/clown.rs` (`mimery_attack_self`, `mimery_answer`), `crates/ss13_game/src/spells/mod.rs` (`SPELL_REQUIRES_MIME_VOW`): книга «Guide to Dank Mimery» даёт радиал из ещё не изученных невидимой стены, стула и ящика, выбранное изучается, книга исчезает; три заклинания требуют обета (`MimeVow`, «You must dedicate yourself to silence first!»); сами заклинания — `crates/ss13_game/src/spells/conjure.rs`; нет книг Advanced Mimery (Invisible Blockade, Finger Guns), нет действия обета для не-мима, прочитавшего книгу

> Из `antagonists/spells.md`: заклинания мима (`InvisibleWall`, `InvisibleChair`, `InvisibleBox`) реализованы в `crates/ss13_game/src/spells/conjure.rs`, нужна их выдача мимам и обет (`SPELL_REQUIRES_MIME_VOW`).

# Одежда: головные уборы и маски
Слоты `head` и `mask`: шлемы, шапки, сварочные щитки, противогазы, дыхательные маски и маски-костюмы.

### Шлемы СБ и защитные шлемы
Шлемы СБ (с креплением фонарика), противоударные, морпеховские, старые; дают броню головы. Переключаемые шлемы опускают визор.

- **Код:** [`/obj/item/clothing/head/helmet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/head/helmet.dm#L1), [`/obj/item/clothing/head/helmet/sec`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/head/helmet.dm#L35), [`/obj/item/clothing/head/helmet/toggleable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/head/helmet.dm#L221)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: надевается; нет брони, крепления фонарика, визора

### Каски с фонарём
Строительные каски и сварочные каски: включаемый фонарь на голове, у сварочных — щиток от вспышек; торт-шляпа как каска-светильник.

- **Код:** [`/obj/item/clothing/head/utility/hardhat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/head/hardhat.dm#L9), [`toggle_helmet_light()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/head/hardhat.dm#L49), [`/obj/item/clothing/head/utility/hardhat/welding`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/head/hardhat.dm#L134), [`/obj/item/clothing/head/utility/hardhat/cakehat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/head/cakehat.dm#L1)
- **Статус в ss13-rs:** ❌ нет — фонарь не включается (свет предметов только из карты, `crates/ss13_mapload/src/light.rs`)

### Сварочная маска
Опускаемый щиток: полная защита от вспышек и сварки ценой затемнения обзора.

- **Код:** [`/obj/item/clothing/head/utility/welding`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/head/welding.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/security.rs`: `flash_protect` защищает от вспышки; щиток не поднимается, нет затемнения (`tint`), защиты глаз от сварки

### Шапки, береты и шляпы
Береты и фуражки командования, шляпы, кепки (переворачиваются козырьком назад), шапки-бини, федоры, ковбойские, религиозные, колпаки повара (крыса-повар управляет носителем), коллекционные шляпы, короны, конусы, бумажные пакеты, парики (меняют цвет), шапочка из фольги (защита от телепатии, паранойя).

- **Код:** [`/obj/item/clothing/head`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/head/_head.dm#L1), [`/obj/item/clothing/head/hats`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/head/hat.dm#L1), [`/obj/item/clothing/head/hats/caphat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/head/jobs.dm#L100), [`/obj/item/clothing/head/soft`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/head/soft_caps.dm#L1), [`/obj/item/clothing/head/utility/chefhat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/head/jobs.dm#L8), [`/obj/item/clothing/head/collectable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/head/collectable.dm#L4), [`/obj/item/clothing/head/wig`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/head/wig.dm#L1), [`/obj/item/clothing/head/costume/foilhat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/head/tinfoilhat.dm#L1), [`/obj/item/clothing/head/costume`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/head/costume.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: надеваются, worn-спрайт; нет переворота кепки, перекраски парика, эффектов фольги и колпака повара

### Особые шлемы
Перцептоматрица (аномальный шлем-щит от галлюцинаций), шлем разума обезьяны (даёт обезьяне разум призрака).

- **Код:** [`/obj/item/clothing/head/helmet/perceptomatrix`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/head/perceptomatrix.dm#L2), [`/obj/item/clothing/head/helmet/monkey_sentience`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/head/mind_monkey_helmet.dm#L4)
- **Статус в ss13-rs:** ❌ нет

### Противогазы и фильтры
Противогаз работает с баллоном (`MASKINTERNALS`), а без баллона фильтрует вредные газы сменным фильтром, который изнашивается; варианты: атмосферный, сварочный, синдикатовский, клоунский, чумного доктора.

- **Код:** [`/obj/item/clothing/mask/gas`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/masks/gasmask.dm#L10), [`/obj/item/gas_filter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/masks/gas_filter.dm#L14), [`/obj/item/clothing/mask/gas/welding`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/masks/gasmask.dm#L218), [`/obj/item/clothing/mask/gas/clown_hat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/masks/gasmask.dm#L303)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/internals.rs`: маска позволяет дышать из баллона; нет фильтрации газов фильтром и его износа, клоунской смены облика

### Дыхательные маски
Дыхательная и медицинская маска для баллона; сдвигается с лица.

- **Код:** [`/obj/item/clothing/mask/breath`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/masks/breath.dm#L1), [`/obj/item/clothing/mask`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/masks/_masks.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/internals.rs`: `MASKINTERNALS`, баллон закрывается при снятии маски; маска не сдвигается

### Маска СБ с громкоговорителем
Маска-хейлер СБ: кнопка проигрывает фразу «Стоять!» и т.п. (злоупотребление ломает), SWAT-вариант; свисток, дудка для вечеринок.

- **Код:** [`/obj/item/clothing/mask/gas/sechailer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/masks/hailer.dm#L45), [`play_phrase()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/masks/hailer.dm#L182), [`/obj/item/clothing/mask/whistle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/masks/hailer.dm#L193)
- **BandaStation:** [`hailer.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/masks/hailer.dm#L66) — хейлер не защищает от перцового спрея (`pepper_tint`)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/internals.rs`: работает как противогаз для баллона; нет фраз (и звуков вообще)

### Кляпы и скотч
Намордник и полоски скотча на рот не дают говорить и есть; снимаются с задержкой.

- **Код:** [`/obj/item/clothing/mask/muzzle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/masks/muzzle.dm#L1), [`/obj/item/clothing/mask/muzzle/tape`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/masks/muzzle.dm#L22)
- **Статус в ss13-rs:** ❌ нет

### Маски-костюмы
Банданы (носятся на лице или шее), балаклавы, маски луча́доров, хирургические маски, усы, маски животных (проклятые прирастают и меняют речь), гондола (меняет речь).

- **Код:** [`/obj/item/clothing/mask/bandana`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/masks/bandana.dm#L1), [`/obj/item/clothing/mask/balaclava`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/masks/boxing.dm#L1), [`/obj/item/clothing/mask/surgical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/masks/surgical.dm#L1), [`/obj/item/clothing/mask/fakemoustache`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/masks/moustache.dm#L1), [`/obj/item/clothing/mask/animal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/masks/animal_masks.dm#L16), [`/obj/item/clothing/mask/gondola`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/masks/gondola.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: надеваются; нет изменения речи, проклятия, сдвига банданы

# Пчеловодство
Ульи, рамки, соты, дымарь и защитный костюм пчеловода. Сами пчёлы и пчеломатка — мобы, см. раздел мобов.

### Улей
Улей с пчеломаткой (вставляется предметом `/obj/item/queen_bee`) и рамками; пчёлы вылетают опылять лотки, набирают реагент последнего опылённого растения и производят соты в рамках; число пчёл и сотов ограничено числом рамок. Тревожащий улей злит пчёл (без костюма пчеловода).

- **Код:** [`/obj/structure/beebox`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/beebox.dm#L8), [`process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/beebox.dm#L69), [`item_interaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/beebox.dm#L128), [`interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/beebox.dm#L174), [`beebox/premade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/beebox.dm#L35), [`/obj/item/queen_bee`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/bee/_bee.dm#L242)
- **Статус в ss13-rs:** ❌ нет

### Рамки и соты
Рамки вставляются в улей; извлечённые соты — еда с мёдом и реагентом пчёл, перерабатываются в мёд и воск.

- **Код:** [`/obj/item/honey_frame`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/honey_frame.dm#L2), [`/obj/item/food/honeycomb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/honeycomb.dm#L1), [`set_reagent()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/honeycomb.dm#L26)
- **Статус в ss13-rs:** ❌ нет

### Дымарь
Включается в руке, тратит топливо (заправляется растениями/бумагой); дым по улью или пчёлам успокаивает их, позволяя забрать соты без нападения.

- **Код:** [`/obj/item/bee_smoker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/bee_smoker.dm#L4), [`interact_with_atom()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/bee_smoker.dm#L43), [`process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/bee_smoker.dm#L96)
- **Статус в ss13-rs:** ❌ нет

### Костюм пчеловода
Шляпа и костюм пчеловода защищают от укусов пчёл (толстая ткань, `THICKMATERIAL`).

- **Код:** [`/obj/item/clothing/head/utility/beekeeper_head`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/beekeeper_suit.dm#L2), [`/obj/item/clothing/suit/utility/beekeeper_suit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hydroponics/beekeeping/beekeeper_suit.dm#L16)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload` (`dm_clothing`), `crates/ss13_game/src/inventory.rs`: надевается как одежда; защиты от пчёл нет (пчёл нет)

# Наклейки на ID-карту (BandaStation)
Модпак `id_stickers`: декоративные наклейки на ID-карты. Сами ID-карты — см. ../items/.

### Наклейки на ID-карту
Наклейка (~20 видов: пончик, серебро, золото, бизнес, цветные с выбором цвета в руке и др.) наносится на продвинутую ID-карту кликом, меняет её вид и описание; Ctrl+Shift+клик снимает наклейку.

- **Код:** [`/obj/item/id_sticker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/id_stickers/code/base.dm#L1), [`/obj/item/id_sticker/colored`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/id_stickers/code/base.dm#L9), [`/obj/item/card/id/advanced/item_interaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/id_stickers/code/card_interactions.dm#L9), [`click_ctrl_shift()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/id_stickers/code/card_interactions.dm#L29)
- **BandaStation:** [`modular_bandastation/id_stickers/code/stickers.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/id_stickers/code/stickers.dm#L1) — модпак целиком форковый
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/service/id_stickers.rs`: наклейка на продвинутую карту за 2 с (оверлей, строки осмотра с `id_card_desc`), одна на карту; Ctrl+Shift — в боевом режиме срыв со звуком, иначе снятие за 5 с в руки; нет выбора цвета голографических наклеек (GAGS рендерится при импорте, цвет — как у типа), покраска в PDA-painter не запрещена

### Заказ наклеек через карго
Ящик наборов наклеек в каталоге снабжения.

- **Код:** [`/datum/supply_pack/misc/id_stickers`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/id_stickers/code/supply_packs.dm#L1)
- **BandaStation:** [`modular_bandastation/id_stickers/code/supply_packs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/id_stickers/code/supply_packs.dm#L1) — форковый пак
- **Статус в ss13-rs:** ❌ нет

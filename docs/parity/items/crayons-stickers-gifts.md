# Мелки, баллончики, наклейки и подарки
Рисование на полу и стенах, покраска, наклейки и подарочные коробки.

### Мелки и рисунки
Мелок рисует на полу руны, символы, буквы, граффити и цифры (выбор в tgui-окне), заряды мелка тратятся; рисунок — смываемая декаль; мелки можно съесть. Коробка мелков всех цветов, радужный и мимский мелок.

- **Код:** [`/obj/item/toy/crayon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/crayons.dm#L24), [`use_on()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/crayons.dm#L451), [`ui_interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/crayons.dm#L320), [`/obj/effect/decal/cleanable/crayon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/crayon.dm#L1), [`/obj/item/storage/crayons`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/crayons.dm#L741); tgui: [`Crayon.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Crayon.tsx)
- **BandaStation:** [`crayons.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/crayons.dm#L434) — надписи мелком кириллицей; [`crayons.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/crayons.dm#L193) — убран один рисунок
- **Статус в ss13-rs:** ❌ нет

### Баллончики с краской
Спрей рисует крупнее мелка и на стенах, красит предметы, конечности (окраска роботизированных частей) и лицо (ослепляет брызгом); вариации: адский баллон, баллон смазки, мимский, бесконечный.

- **Код:** [`/obj/item/toy/crayon/spraycan`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/crayons.dm#L780), [`color_limb()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/crayons.dm#L1016), [`/obj/item/toy/crayon/spraycan/lubecan`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/crayons.dm#L1095)
- **Статус в ss13-rs:** ❌ нет

### Баллончик предателя
Шпионский баллон для рисования рун-граффити (цель-объектив), вызывающих эффекты у смотрящих.

- **Код:** [`/obj/item/traitor_spraycan`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/syndie_spraycan.dm#L4)
- **Статус в ss13-rs:** ❌ нет

_«Банки краски» описано в [../service/art.md](../service/art.md)._

### Наклейки
Наклейки (смайлики, стрелки, звёзды, синдикатовские) приклеиваются на предметы и стены в место клика; отклеиваются.

- **Код:** [`/obj/item/sticker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stickers.dm#L14), [`attempt_attach()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stickers.dm#L42), [`/datum/component/sticker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/sticker.dm#L7)
- **BandaStation:** [`stickers.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stickers.dm#L43) — наклейки не клеятся на мобов
- **Статус в ss13-rs:** ❌ нет

### Подарки
Подарок открывается в руке и даёт случайный предмет (на Рождество — из всего списка предметов, именной подарок — для конкретного игрока); упаковка предметов в подарочную бумагу.

- **Код:** [`/obj/item/gift`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/gift.dm#L7), [`get_gift_type()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/gift.dm#L56), [`/obj/item/gift/anything`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/gift.dm#L107), [`/obj/item/stack/wrapping_paper`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/wrap.dm#L7)
- **BandaStation:** [`gift.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/gift.dm#L118) — подарки не выдают отладочные и ЦК-предметы
- **Статус в ss13-rs:** ❌ нет

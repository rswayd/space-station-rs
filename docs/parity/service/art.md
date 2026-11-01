# Искусство
Живопись на холстах с сохранением картин между раундами, скульптура и статуи (`code/modules/art`).

### Мольберт и холст
Холст (несколько размеров) ставится на мольберт или держится в руке и рисуется попиксельно в tgui инструментом в руке (мелок, ручка, кисть с палитрой, баллончик); после «завершения» картину можно назвать и подписать.

- **Код:** [`/obj/structure/easel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/paintings.dm#L14), [`/obj/item/canvas`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/paintings.dm#L65), [`ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/paintings.dm#L199), [`finalize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/paintings.dm#L255); tgui: [`Canvas.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Canvas.tsx)
- **Статус в ss13-rs:** ❌ нет

### Палитра художника
Кисть-палитра с набором цветов, переключаемых при рисовании на холсте.

- **Код:** [`/obj/item/paint_palette`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/paintings.dm#L936), [`get_paint_tool_palette()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/paintings.dm#L423)
- **Статус в ss13-rs:** ❌ нет

### Рамы картин и галерея
Законченный холст вешается в раму на стене; рамы-галереи (библиотека, бар и т.д.) при старте раунда загружают случайную сохранённую картину своей категории; меценаты платят кредитами за картину и выбирают раму.

- **Код:** [`/obj/structure/sign/painting`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/paintings.dm#L575), [`frame_canvas()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/paintings.dm#L658), [`load_persistent()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/paintings.dm#L713), [`/obj/item/canvas/proc/patron()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/paintings.dm#L276), [`/obj/item/wallframe/painting`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/paintings.dm#L564)
- **Статус в ss13-rs:** ❌ нет

### Персистентные картины
Подсистема хранит картины игроков между раундами (PNG + JSON с автором, названием, ценой меценатов, тегами); админ-панель для удаления/правки.

- **Код:** [`SUBSYSTEM_DEF(persistent_paintings)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistent_paintings.dm#L121), [`/datum/painting`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistent_paintings.dm#L27); tgui: [`PaintingAdminPanel.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PaintingAdminPanel.tsx)
- **Статус в ss13-rs:** ❌ нет

### Банки с краской
Банка краски окрашивает пол/стены в свой цвет; «любой цвет» выбирается при использовании.

- **Код:** [`/obj/item/paint`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/paint.dm#L4), [`/obj/item/paint/anycolor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/paint.dm#L58)
- **Статус в ss13-rs:** ❌ нет

### Статуи
Статуи из минералов (уран светится и излучает, плазма горит, золото, серебро, алмаз, бананиум и др.), сносятся сваркой/разбираются; мраморные и прочие декоративные.

- **Код:** [`/obj/structure/statue`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/statues.dm#L5), [`/obj/structure/statue/uranium`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/statues.dm#L98), [`/obj/structure/statue/plasma`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/statues.dm#L120)
- **BandaStation:** [`modular_bandastation/objects/code/structures/statues.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/statues.dm#L2) — статуи Фемиды, статуя-карта станции и др.
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/spawner.rs`: статуи появляются с карты (в т.ч. из случайного спавнера декора) как статичные объекты; свойств материалов (радиация, горение) и разборки нет

### Скульптура (резец и блок)
Резцом по каменному блоку выбирают цель (моб или предмет в поле зрения) и за время вырезают статую с её обликом в материале блока.

- **Код:** [`/obj/item/chisel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/statues.dm#L301), [`start_sculpting()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/statues.dm#L392), [`/obj/structure/carving_block`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/statues.dm#L471), [`create_statue()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/statues.dm#L534)
- **Статус в ss13-rs:** ❌ нет

### Бюсты и фигурки
Настольные бюсты и фигурки: декор, у бюста Гиппократа можно дать клятву врача (Solemn Vow).

- **Код:** [`/obj/item/statuebust`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/busts_and_figurines.dm#L1)
- **Статус в ss13-rs:** ❌ нет

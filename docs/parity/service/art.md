# Искусство
Живопись на холстах с сохранением картин между раундами, скульптура и статуи (`code/modules/art`).

### Мольберт и холст
Холст (несколько размеров) ставится на мольберт или держится в руке и рисуется попиксельно в tgui инструментом в руке (мелок, ручка, кисть с палитрой, баллончик); после «завершения» картину можно назвать и подписать.

- **Код:** [`/obj/structure/easel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/paintings.dm#L14), [`/obj/item/canvas`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/paintings.dm#L65), [`ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/paintings.dm#L199), [`finalize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/paintings.dm#L255); tgui: [`Canvas.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Canvas.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/service/art.rs`: холст ставится на мольберт (на его тайл); мелок, ручка, баллончик, палитра на незаконченном холсте «рисуют» — оверлей `<размер>wip` и техника (`get_paint_tool_medium`, «Mixed medium»); холст в руке завершается: название, затем подпись или «Anonymous» (`try_rename`/`finalize`), законченный сообщает название, автора и технику; нет попиксельного рисования (tgui `Canvas`, спрайт-редактор) и сгенерированной картинки — нужен клиентский редактор; нет очистки мылом, меценатов

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
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/service/art.rs`: банка красит пол/стену (не космос) в свой цвет, 200 использований, затем `paint_empty`; адаптивная краска выбирает цвет списком в руке (проверка, что банка ещё в руке); растворитель снимает краску (у tg условие `!isturf || !isobj` никогда не проходит — портировано задуманное поведение). Отклонения: простое умножение цвета вместо фильтра насыщенности (правый клик-override), нет падающей опасности; смывание уборкой — за группой уборки (janitor)

### Статуи
Статуи из минералов (уран светится и излучает, плазма горит, золото, серебро, алмаз, бананиум и др.), сносятся сваркой/разбираются; мраморные и прочие декоративные.

- **Код:** [`/obj/structure/statue`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/statues.dm#L5), [`/obj/structure/statue/uranium`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/statues.dm#L98), [`/obj/structure/statue/plasma`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/statues.dm#L120)
- **BandaStation:** [`modular_bandastation/objects/code/structures/statues.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/statues.dm#L2) — статуи Фемиды, статуя-карта станции и др.
- **Статус в ss13-rs:** ✅ — статуи с карты (`crates/ss13_mapload/src/spawner.rs`), закрепление гаечным ключом и распил сваркой на листы (`crates/ss13_game/src/decor.rs`); `crates/ss13_game/src/service/statues.rs`: урановые излучают раз в 3 с (элемент `radioactive`: радиус 3, лёгкая изоляция, 3 с экспозиции), плазменные (`combustible_flooder`) от зажжённой сварки или огня выпускают 25 моль плазмы при этой температуре и разрушаются (листов на 2 меньше). Нет: зажигалок/спичек/искр/лазеров как поджигателей (нет температуры предметов, кроме сварки), элементов красоты/искусства, вращения

### Скульптура (резец и блок)
Резцом по каменному блоку выбирают цель (моб или предмет в поле зрения) и за время вырезают статую с её обликом в материале блока.

- **Код:** [`/obj/item/chisel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/statues.dm#L301), [`start_sculpting()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/statues.dm#L392), [`/obj/structure/carving_block`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/statues.dm#L471), [`create_statue()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/art/statues.dm#L534)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/service/statues.rs`: резец по блоку готовит его, по объекту на тайле выбирает модель, повторно по блоку — резьба 30 с (прерванная сохраняет прогресс, сход с тайла сбрасывает подготовку); получается `/obj/structure/statue/custom` с обликом модели в цвете блока, «statue of X». Нет: меню готовых статуй по материалам, выбора цели издалека, серого фильтра и построчного проявления, материалов блока (кастомная статуя не даёт листов)

### Бюсты и фигурки
Настольные бюсты и фигурки: декор, у бюста Гиппократа можно дать клятву врача (Solemn Vow).

- **Код:** [`/obj/item/statuebust`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/busts_and_figurines.dm#L1)
- **Статус в ss13-rs:** ✅ — бюсты и фигурки — предметы из прототипов; `crates/ss13_game/src/service/statues.rs`: бюст Гиппократа в руке — клятва (4+2+3+3 с фраз, затем пацифизм через `traumas::refresh`), отказ от клятвы (5 с; tg после отказа сразу клянётся заново — баг, не портирован), откат 5 мин, прерванная клятва — 2 урона мозгу и сброс отката; «Solemn Vow» (1%) отказывает. Нет: медицинского HUD и медленной атаки у «Solemn Vow», элементов красоты/искусства

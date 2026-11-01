# Экспорт, сканеры цен и гражданские заказы
Продажа вещей ЦК, оценка стоимости, упаковка посылок и персональные заказы (bounties) экипажа.

### Система экспорта
`export_item_and_contents()` перебирает проданный предмет и его содержимое, для каждого находит `/datum/export` и начисляет кредиты в карго-бюджет; отчёт о продаже приходит в консоль. Цена у части экспортов «эластична»: падает с каждой проданной единицей и восстанавливается со временем.

- **Код:** [`/datum/export`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/exports.dm#L95), [`export_item_and_contents()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/exports.dm#L43), [`sell_object()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/exports.dm#L204), [`/datum/export_report`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/exports.dm#L23)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/cargo/export.rs` (`export_item_and_contents`, эластичность `k_elasticity`): продажа предмета со всем содержимым, отчёт ЦК в консоли; `// shortcut:` нет обработчиков `COMSIG_ITEM_EXPORTED` (баунти, ценники), проверки голограмм, движения биржи

### Категории экспорта
Что покупает ЦК: материалы и листы, руду, оборудование и скафандры, инструменты, оружие, органы, еду и напитки, рыбу, семена, аномальные ядра, мехов и детали, крупные объекты (канистры, генераторы), образцы ксенобиологии, антиквариат, лаваленд-лут, проштампованные документы и манифесты.

- **Код:** [`exports/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/exports), напр. [`/datum/export/food`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/exports/food_and_drink.dm#L5), [`/datum/export/weapon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/exports/weapons.dm#L3), [`/datum/export/paperwork`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/exports/stamped_paperwork.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/cargo.rs`: все `/datum/export`, чья цена считается базовыми, стековыми или материальными процедурами (ящики, листы, плазма, инструменты...); `// shortcut:` предметы с `custom_materials`, не являющиеся стеками (монеты, биржевые блоки), материалом не оцениваются

### Повышенный спрос на экспорт (boosted exports)
`SSeconomy` периодически выбирает несколько категорий экспорта с повышенной ценой.

- **Код:** [`update_boosted_exports()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/economy.dm#L135)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/cargo/export.rs`: `// shortcut:` `boosted_exports` нет

### Универсальный сканер (export scanner / price tagger)
Ручной сканер карго: режим оценки показывает экспортную стоимость предмета, режим ценника вешает на предмет цену для продажи, режим штрихкода печатает наклейки `barcode`, которыми помечают товар в пользу владельца.

- **Код:** [`/obj/item/universal_scanner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/universal_scanner.dm#L3), [`export_scan()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/universal_scanner.dm#L177), [`/obj/item/barcode`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/universal_scanner.dm#L272)
- **Статус в ss13-rs:** ❌ нет

_«Упаковка посылок, теггер назначения и теггер продаж» описано в [../engineering/recycling.md](../engineering/recycling.md)._

### Гражданские заказы: консоль и площадка
Терминал `piratepad_control/civilian` с площадкой: игрок вставляет ID, выбирает один из трёх заказов своей должности, кладёт на площадку нужное и отправляет — получается куб заказа. Перезарядка выбора заказов зависит от деталей площадки. Печать листа с заказами.

- **Код:** [`/obj/machinery/computer/piratepad_control/civilian`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/civilian_bounty/bounty_machinery.dm#L29), [`pick_bounty()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/civilian_bounty/bounty_machinery.dm#L209), [`/obj/machinery/piratepad/civilian`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/civilian_bounty/bounty_machinery.dm#L6); tgui: [`CivCargoHoldTerminal.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/CivCargoHoldTerminal.tsx)
- **Статус в ss13-rs:** ❌ нет

### Куб заказа и маяк доставки
`bounty_cube` — результат выполненного заказа: его нужно продать шаттлом снабжения, награда делится между исполнителем, карго и тем, кто доставил; невыполненный куб со временем объявляется в радио и дорожает. `civ_bounty_beacon` вызывает площадку капсулой.

- **Код:** [`/obj/item/bounty_cube`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/civilian_bounty/bounty_cube.dm#L6), [`set_up()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/civilian_bounty/bounty_cube.dm#L77), [`/obj/item/civ_bounty_beacon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/civilian_bounty/bounty_machinery.dm#L422)
- **Статус в ss13-rs:** ❌ нет

### Типы заказов (bounties)
`/datum/bounty`: заказы на предметы, реагенты, таблетки, вирусы, патрулирование (СБ), а также по отделам — ассистент, атмос, ботаника, повар, инженерия, мехи, медицина, шахта, наука, слаймы, битраннинг, особые. Набор для ID определяется тримом должности (`generate_bounty_list`).

- **Код:** [`/datum/bounty`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/bounty.dm#L4), [`random_bounty()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/bounty.dm#L76), [`bounties/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/bounties), [`/datum/id_trim/proc/generate_bounty_list()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/civilian_bounty/civilian_bounty_trims.dm#L9)
- **Статус в ss13-rs:** ❌ нет

Доска объявлений/заказов NTOS (`bounty_board`, запросы экипажа через ньюскастер) — см. ../service/ или раздел ПДА.

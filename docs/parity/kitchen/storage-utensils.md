# Хранение, посуда и кухонный инвентарь
Умные холодильники, сушилки, тарелки, столовые приборы, коробки пиццы и приправы.

### Умный холодильник (SmartFridge)
Хранит до сотни предметов допустимого типа, выдаёт их через окно SmartVend (по одному или пачкой); загрузка из рук и из сумок. Варианты: для еды, для напитков, а также ксенобиологический, чашек Петри, органов (замедляет гниение), химический, вирусологический, для дисков — последние относятся к своим отделам.

- **Код:** [`/obj/machinery/smartfridge`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/smartfridge.dm#L4), [`proc/accept_check()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/smartfridge.dm#L333), [`/obj/machinery/smartfridge/drinks`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/smartfridge.dm#L619), [`/obj/machinery/smartfridge/food`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/smartfridge.dm#L640), [`/obj/machinery/smartfridge/organ`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/smartfridge.dm#L687); tgui: [`SmartVend.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SmartVend.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/smartfridge.rs`, окно `crates/ss13_tgui/src/interfaces/smart_vend.rs`, данные `crates/ss13_mapload/src/smartfridge.rs`: `initial_contents` (в т.ч. химия и вирусология), приём по виду (растения, напитки, еда, экстракты, чашки Петри, органы, медикаменты с пузырьками таблеток, диски), загрузка из сумки, вместимость × тир бункера, окно с группами «тип-имя» и выдачей по одному или всех (в руки, иначе на пол, с расходом энергии), спрайт заполнения, стекла и экрана; сборка из платы; нет сушки (сушилки ничего не принимают), консервации органов, приварки, взлома, поломки, поиска и плитки в окне

### Сушилка и сушильная стойка
Разновидность умного холодильника: включённая сушит предметы с `dryable` (мясо в вяленое, фрукты в сухофрукты, табак, кожу); деревянная стойка работает без питания.

- **Код:** [`/obj/machinery/smartfridge/drying`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/smartfridge.dm#L449), [`/obj/machinery/smartfridge/drying/rack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/smartfridge.dm#L556), [`/datum/element/dryable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/dryable.dm#L2), [`proc/make_dryable()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/_food.dm#L211)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/smartfridge.rs` (`toggle_drying`, `drying_tick`, `dry_item`), `dryable` из `AddElement` (`ss13_dm_extract` `SCANNED_ELEMENTS`, `crates/ss13_mapload/src/food.rs` `dry_result`, `type` = сам тип): принимают сушимое и не высушенное, кнопка «Dry» в окне SmartVend, каждый тик машин всё внутри сохнет (свой тип — `Dried` и загар, иначе `dry_result` с реагентами), энергия, без питания выключается; оверлеи стойки и дегидратора; нет `TRAIT_HANDMADE` от конкретного игрока (результат просто `Handmade`), ЭМИ дегидратора (жар 1000 K на тайл), сушки кожи (не еда)

### Тарелки
Тарелка держит одно (большая — несколько) блюдо, отображая его на себе; бьётся при падении/броске в осколки. Малая, большая, железная.

- **Код:** [`/obj/item/plate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/equipment/plate.dm#L1), [`/obj/item/plate/large`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/equipment/plate.dm#L103), [`/obj/item/plate_shard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/equipment/plate.dm#L128)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/kitchen_ware.rs` (`Plate`, `OnPlate`): еда до `biggest_w_class` и `max_items` кладётся туда, куда кликнули (`max_x_offset`, `max_height_offset`), рисуется на тарелке, свободная другая рука снимает последнюю; тарелкой кормят первой едой (`pre_attack`, через `crate::food::try_to_eat`); брошенная хрупкая тарелка бьётся: еда высыпается, 5 осколков `plate_shard`, звук; малая, большая, железная (не бьётся), противень печи; нет роста веса тарелки от еды, калтропа у осколков (`/datum/component/caltrop`)

### Миски
Миска — посуда для салатов и супов, принимает реагенты и ингредиенты кастомного салата.

- **Код:** [`/obj/item/reagent_containers/cup/bowl`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/salad.dm#L179)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/chem.rs`: работает как обычная ёмкость для реагентов; кастомного салата нет

### Столовые приборы
Вилка (подцепляет кусок омлета, колет), ложка (зачерпывает 5 ед. супа из миски), половник, пластиковые варианты; кухонный нож режет еду.

- **Код:** [`/obj/item/kitchen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/equipment/kitchen_utensils.dm#L12), [`/obj/item/kitchen/fork`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/equipment/kitchen_utensils.dm#L23), [`/obj/item/kitchen/spoon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/equipment/kitchen_utensils.dm#L202), [`/obj/item/kitchen/spoon/soup_ladle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/equipment/kitchen_utensils.dm#L350), [`/obj/item/knife/kitchen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/equipment/kitchen_utensils.dm#L92)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/food.rs`: нож как инструмент нарезки есть; вилка и ложки как приборы не работают

### Скалка
Инструмент раскатки теста (`TOOL_ROLLINGPIN`), заодно дубинка; «нелегальная» версия.

- **Код:** [`/obj/item/kitchen/rollingpin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/equipment/kitchen_utensils.dm#L163)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/food.rs`, `crates/ss13_mapload/src/food.rs` (раскатка через `processable`)

### Щипцы
Берут еду и кормят ею с расстояния, щёлкают.

- **Код:** [`/obj/item/kitchen/tongs`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/equipment/kitchen_utensils.dm#L365)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/kitchen_ware.rs` (`Tongs`): берут еду до `WEIGHT_CLASS_NORMAL` с 2 клеток (`reach = 2`, `RangedClick`), рисуют её на себе, кормят ею моба (`crate::food::try_to_eat` с щипцами в руке), кладут на тарелку; в руке роняют еду или щёлкают (2 с, закрытый спрайт на 0,5 с); выпавшие из рук роняют еду; нет проверки пути `CanReach` на 2 клетки, уменьшения спрайта еды

### Коробка пиццы
Открывается, хранит пиццу, коробки ставятся стопкой, на крышке можно написать ручкой; бесконечная коробка из админки, пицца-бомба (антагонисты, мина в коробке).

- **Код:** [`/obj/item/pizzabox`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/pizzabox.dm#L9), [`/obj/item/pizzabox/infinite`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/pizzabox.dm#L363), [`/obj/item/bombcore/miniature/pizza`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/pizzabox.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/kitchen_ware.rs` (`PizzaBox`): открывается в руке (`pizzabox`/`_open`/`_messy`), пицца кладётся в открытую и вынимается свободной другой рукой; закрытые коробки ставятся стопкой (от 5 — `10%` за коробку рассыпаться), верхняя снимается другой рукой; ручкой/мелком пишется бирка верхней коробки (30 символов, `tgui_input_text`); стопка рассыпается при броске (2+, `20%` за коробку) и ударе (3+, `25%`): коробки разлетаются, половина теряет пиццу; коробки с картой приходят со своей пиццей, бесконечная делает любимую пиццу открывающего; нет складывания в картон (ПКМ в руке), нарезанной пиццы и кусков из коробки, любимой пиццы по ckey и квирков ананаса; пицца-бомба (`bombcore`, `/datum/wires/explosive/pizza`) — → ../engineering/wires.md («Провода взрывчатки»)

### Приправы
Бутылки соусов (кетчуп, соевый, острый, майонез, масло), пакетики приправ, солонка и перечница (посыпать еду, просыпать соль на пол), универсальный фермент, мука, сахар, молоко в пакетах.

- **Код:** [`/obj/item/reagent_containers/condiment`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/condiment.dm#L8), [`/obj/item/reagent_containers/condiment/saltshaker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/condiment.dm#L136), [`/obj/item/reagent_containers/condiment/enzyme`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/condiment.dm#L100), [`/obj/item/reagent_containers/condiment/pack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/condiment.dm#L432)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/chem.rs`: как ёмкости с реагентами (переливание) работают; нет вскрытия пакетиков, просыпания соли, особых иконок по содержимому

### Бак с кулинарным маслом
Стационарный бак масла для фритюрницы и сковород.

- **Код:** [`/obj/structure/reagent_dispensers/cooking_oil`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_dispenser.dm#L671)
- **Статус в ss13-rs:** ✅ реализовано — бак — `reagent_dispensers` (`crates/ss13_mapload/src/reagents.rs` `dispenser_fields`: 1000 ед. масла, `DRAINABLE`), из него набирают стаканами (`crates/ss13_game/src/chem.rs`); кран у `openable` баков (вода, пена, топливо, кега, квас, масло) — `crates/ss13_game/src/kitchen_ware.rs` (`TapOpen`): ключ открывает/закрывает, открытый проливает 10 ед. на пол при открытии и перемещении (`tank_leak`); нет превращения в стационарный бак листом железа (сантехника)

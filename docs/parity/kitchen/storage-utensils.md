# Хранение, посуда и кухонный инвентарь
Умные холодильники, сушилки, тарелки, столовые приборы, коробки пиццы и приправы.

### Умный холодильник (SmartFridge)
Хранит до сотни предметов допустимого типа, выдаёт их через окно SmartVend (по одному или пачкой); загрузка из рук и из сумок. Варианты: для еды, для напитков, а также ксенобиологический, чашек Петри, органов (замедляет гниение), химический, вирусологический, для дисков — последние относятся к своим отделам.

- **Код:** [`/obj/machinery/smartfridge`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/smartfridge.dm#L4), [`proc/accept_check()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/smartfridge.dm#L333), [`/obj/machinery/smartfridge/drinks`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/smartfridge.dm#L619), [`/obj/machinery/smartfridge/food`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/smartfridge.dm#L640), [`/obj/machinery/smartfridge/organ`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/smartfridge.dm#L687); tgui: [`SmartVend.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SmartVend.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/smartfridge.rs`, окно `crates/ss13_tgui/src/interfaces/smart_vend.rs`, данные `crates/ss13_mapload/src/smartfridge.rs`: `initial_contents` (в т.ч. химия и вирусология), приём по виду (растения, напитки, еда, экстракты, чашки Петри, органы, медикаменты с пузырьками таблеток, диски), загрузка из сумки, вместимость × тир бункера, окно с группами «тип-имя» и выдачей по одному или всех (в руки, иначе на пол, с расходом энергии), спрайт заполнения, стекла и экрана; сборка из платы; нет сушки (сушилки ничего не принимают), консервации органов, приварки, взлома, поломки, поиска и плитки в окне

### Сушилка и сушильная стойка
Разновидность умного холодильника: включённая сушит предметы с `dryable` (мясо в вяленое, фрукты в сухофрукты, табак, кожу); деревянная стойка работает без питания.

- **Код:** [`/obj/machinery/smartfridge/drying`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/smartfridge.dm#L449), [`/obj/machinery/smartfridge/drying/rack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/smartfridge.dm#L556), [`/datum/element/dryable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/dryable.dm#L2), [`proc/make_dryable()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/_food.dm#L211)
- **Статус в ss13-rs:** ❌ нет

### Тарелки
Тарелка держит одно (большая — несколько) блюдо, отображая его на себе; бьётся при падении/броске в осколки. Малая, большая, железная.

- **Код:** [`/obj/item/plate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/equipment/plate.dm#L1), [`/obj/item/plate/large`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/equipment/plate.dm#L103), [`/obj/item/plate_shard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/equipment/plate.dm#L128)
- **Статус в ss13-rs:** ❌ нет

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
- **Статус в ss13-rs:** ❌ нет

### Коробка пиццы
Открывается, хранит пиццу, коробки ставятся стопкой, на крышке можно написать ручкой; бесконечная коробка из админки, пицца-бомба (антагонисты, мина в коробке).

- **Код:** [`/obj/item/pizzabox`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/pizzabox.dm#L9), [`/obj/item/pizzabox/infinite`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/pizzabox.dm#L363), [`/obj/item/bombcore/miniature/pizza`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/pizzabox.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Приправы
Бутылки соусов (кетчуп, соевый, острый, майонез, масло), пакетики приправ, солонка и перечница (посыпать еду, просыпать соль на пол), универсальный фермент, мука, сахар, молоко в пакетах.

- **Код:** [`/obj/item/reagent_containers/condiment`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/condiment.dm#L8), [`/obj/item/reagent_containers/condiment/saltshaker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/condiment.dm#L136), [`/obj/item/reagent_containers/condiment/enzyme`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/condiment.dm#L100), [`/obj/item/reagent_containers/condiment/pack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/condiment.dm#L432)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/chem.rs`: как ёмкости с реагентами (переливание) работают; нет вскрытия пакетиков, просыпания соли, особых иконок по содержимому

### Бак с кулинарным маслом
Стационарный бак масла для фритюрницы и сковород.

- **Код:** [`/obj/structure/reagent_dispensers/cooking_oil`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_dispenser.dm#L671)
- **Статус в ss13-rs:** ❌ нет — стационарных баков реагентов (`reagent_dispensers`) в `crates/ss13_game/src/chem.rs` нет

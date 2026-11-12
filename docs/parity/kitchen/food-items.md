# Семейства блюд
Готовые блюда и полуфабрикаты из `code/modules/food_and_drinks/food/` (~1100 типов), сгруппированные по файлам. Общая механика еды — в [food-mechanics.md](food-mechanics.md); выращенные плоды — см. ../botany/.

Во всех записях ниже ss13-rs берёт данные блюда из DM (`DmFood`: реагенты, укус, вкусы, мусор, нарезка, результат микроволновки) и умеет его есть, резать и греть в микроволновке; общие пробелы (запекание, жарка на гриле, крафт, foodtypes) перечислены в соответствующих файлах и здесь не повторяются.

### Хлеб, тосты и сэндвичи
Буханки режутся на ломтики, ломтики — основа сэндвичей и тостов; есть кастомный хлеб и сэндвичи (см. [recipes-crafting.md](recipes-crafting.md)), хлеб с начинкой-тайником.

- **Код:** [`/obj/item/food/bread`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/bread.dm#L3), [`/obj/item/food/breadslice`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/bread.dm#L29), [`/obj/item/food/sandwich`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/sandwichtoast.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/food.rs`, `cooking.rs`: едятся, режутся, тесто запекается в хлеб в духовке (`bake_result`), тосты — на гриле; нет кастомного хлеба и сэндвичей (`ingredients_holder`, см. recipes-crafting.md)

### Тесто
Тесто, сладкое тесто, лепёшки, пицца-основа: раскатываются скалкой, режутся, запекаются в духовке.

- **Код:** [`/obj/item/food/dough`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/dough.dm#L3)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/food.rs` (раскатка скалкой и нарезка, `processable`), `cooking.rs` (запекание в духовке, `bakeable`), `chem.rs` (`refresh_container`: реакции еды с `resulting_food_path` создают тесто, рисовое тесто, тесто для торта и пиццы мотыльков рядом с ёмкостью; данные — `ss13_reagents::ReactionDef::food`, нужен `ss13-mapload convert`)

### Бургеры
Около 40 видов бургеров от обычного до тематических (клоунский, бургер-призрак и др.); крафтятся из булки и начинки.

- **Код:** [`/obj/item/food/burger`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/burgers.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/food.rs`: едятся; крафта нет

### Пицца
Целые пиццы режутся на 6 кусков; есть особые (пицца-бомба в коробке — см. [storage-utensils.md](storage-utensils.md)).

- **Код:** [`/obj/item/food/pizza`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/pizza.dm#L2), [`/obj/item/food/pizzaslice`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/pizza.dm#L121)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/food.rs`: нарезка на куски и поедание; сырая пицца запекается в духовке (`cooking.rs`, `bake_result`); пицца-бомба и коробки — storage-utensils.md

### Торты, пироги и выпечка
Торты (режутся на куски), пироги, маффины, печенье, вафли, блины, пончики. Кремовый пирог при броске залепляет лицо кремом.

- **Код:** [`/obj/item/food/cake`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/cake.dm#L1), [`/obj/item/food/pie`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/pie.dm#L1), [`/obj/item/food/pie/cream`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/pie.dm#L60), [`/obj/item/food/muffin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/pastries.dm#L5), [`/obj/item/food/donut`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/donuts.dm#L3), [`/obj/item/food/pancakes`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/pancakes.dm#L3)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/food.rs`: едятся и режутся, макание в напиток (`dunk`), запекание — `cooking.rs`; брошенный кремовый пирог (`thrown`) оставляет пятно, парализует на 2 с (кроме `nostun`), размывает зрение на 2 с, звук. `// shortcut:` нет крема на лице (`face_decal/splat`) и его мудлета

### Мясо и мясные блюда
Куски мяса разных существ (из гиббера, мясника), котлеты, стейки, отбивные, рыба, морепродукты; сырое мясо жарится на гриле/сковороде в стейк.

- **Код:** [`/obj/item/food/meat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/meatslab.dm#L1), [`/obj/item/food/cubancarp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/meatdish.dm#L6)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/food.rs`: нарезка мяса; жарка на гриле (`cooking.rs`, `grill_result`), фритюр; синтетическое мясо и имитация карпа из реакций (`chem.rs`); мясо из гиббера — `gibber.rs`; нет разделки туш ножом (`butchering`)

### Яйца
Яйца, варёные и жареные яйца, омлеты, шоколадные яйца; яйцо можно разбить о предмет или в посуду.

- **Код:** [`/obj/item/food/egg`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/egg.dm#L19), [`/obj/item/food/boiledegg`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/egg.dm#L235), [`/obj/item/food/friedegg`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/egg.dm#L203)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/kitchen.rs` (варка в микроволновке), `food.rs`: ПКМ яйцом по грилю разбивает его в сырое яйцо на плите (`crack_egg`, жарится в глазунью по `grill_result`), брошенное яйцо оставляет пятно и с шансом 13% цыплёнка (`thrown`). Нет раскраски мелком и клоунского штампа, проверки на тухлость в воде, реагентов яйца на цели броска

### Сыр
Головки сыра режутся на дольки; есть особые сорта (моцарелла, творожный сыр, сыры мотыльков).

- **Код:** [`/obj/item/food/cheese`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/cheese.dm#L6)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/food.rs`: нарезка головки на дольки; головка сыра, творожный сыр и моцарелла появляются из реакций молока (`chem.rs`, `ReactionDef::food`)

### Донк-покеты
Готовая еда в упаковке, греется в микроволновке (тёплая версия добавляет омнизин).

- **Код:** [`/obj/item/food/donkpocket`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/donkpocket.dm#L3)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/kitchen.rs`, `cooking.rs`: разогрев в микроволновке и духовке в тёплую версию, `food.rs` (`warm_bonus`): тёплый получает омнизин холодного (6, у вкусовых 2); перегретый тёплый сгорает

### Супы, салаты, паста, мексиканская и прочие блюда
Супы в мисках (`bowled`), салаты в мисках, спагетти и макароны, буррито/тако/начос, овощные блюда, разное.

- **Код:** [`/obj/item/food/bowled`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/soup.dm#L1), [`/obj/item/food/salad`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/salad.dm#L3), [`/obj/item/food/spaghetti`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/spaghetti.dm#L2), [`/obj/item/food/tortilla`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/mexican.dm#L1), [`/obj/item/food/eggplantparm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/vegetables.dm#L1), [`/obj/item/food/watermelonslice`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/misc.dm#L3)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/food.rs`: едятся; супы варятся в кастрюле на плите и лапша — там же (`crates/ss13_game/src/stove.rs`, реакции `/datum/chemical_reaction/food/soup`); салаты, буррито и прочее собираются в меню крафта, которого нет (recipes-crafting.md)

### Национальные кухни: ящеры, мотыльки, марсианская
Отдельные линейки блюд ящеров (Тизира), мотыльков и марсианской кухни с собственными ингредиентами и рецептами.

- **Код:** [`/obj/item/food/raw_tiziran_sausage`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/lizard.dm#L5), [`/obj/item/food/herby_cheese`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/moth.dm#L6), [`/obj/item/food/kimchi`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/martian.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/food.rs`: едятся; без foodtypes вкусовые предпочтения видов не работают

### Снеки, сладости и упакованная еда
Чипсы, батончики, конфеты, вяленое мясо из торгоматов; консервы, которые надо открыть.

- **Код:** [`/obj/item/food/candy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/snacks.dm#L5), [`/obj/item/food/chips`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/snacks.dm#L87), [`/obj/item/food/candy_corn`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/sweets.dm#L3), [`/obj/item/food/canned`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/packaged.dm#L4)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/food.rs`: едятся, обёртка остаётся (`trash_type`); консервы открываются в руке (`open_can`: язычок, звук, спрайт `_open`, ёмкость открыта) и не едятся закрытыми

### Замороженные десерты
Мороженое-сэндвичи, фруктовый лёд, снежные конусы; мороженое в рожке — см. машину мороженого в [cooking-machines.md](cooking-machines.md).

- **Код:** [`/obj/item/food/icecreamsandwich`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/frozen.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/food.rs`: едятся; таяния и составного мороженого нет

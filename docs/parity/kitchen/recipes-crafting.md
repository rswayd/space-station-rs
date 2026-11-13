# Рецепты и крафт еды
Как получаются блюда: кулинарный раздел меню крафта, кастомная еда, «реакции еды» в посуде и рецепты супов.

### Кулинарный раздел меню крафта
Около 870 рецептов `/datum/crafting_recipe/food` (бургеры, пиццы, торты, салаты, морепродукты, национальные кухни): ингредиенты и инструменты рядом со столом, результат наследует реагенты компонентов и получает сложность рецепта (для баффов). Отдельная вкладка «Еда» окна крафта, массовый крафт. Общий механизм меню крафта — вне этой папки.

- **Код:** [`/datum/crafting_recipe/food`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/recipes/food_mixtures.dm#L1), [`/datum/component/personal_crafting`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/crafting/_crafting.dm#L12), [`proc/construct_item()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/crafting/_crafting.dm#L241), файлы рецептов: [`recipes/tablecraft/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/recipes/tablecraft); tgui: [`PersonalCrafting`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PersonalCrafting)
- **BandaStation:** [`recipes_sandwich.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/recipes/tablecraft/recipes_sandwich.dm#L28) — закомментированные `CRAFT_SKIP_MATERIALS_PARITY` (обход юнит-теста, на игру не влияет)
- **Статус в ss13-rs:** ❌ нет — меню крафта нет (docs/RUNNING.md: «There is no ... crafting menu»)

### Крафтовые напитки
Рецепты меню крафта, дающие бутылки и кувшины (вино ящеров, самогон, шампанское и др., ~15 рецептов).

- **Код:** [`/datum/crafting_recipe/lizardwine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/recipes/tablecraft/recipes_drink.dm#L5)
- **Статус в ss13-rs:** ❌ нет

### Кастомная еда
Ингредиенты, добавленные к основе (хлеб, пицца-основа, тарелка салата, булка, миска), превращают её в «кастомное» блюдо с названием и цветом по начинке; реагенты ингредиентов складываются.

- **Код:** [`/datum/component/ingredients_holder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/ingredients_holder.dm#L11)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_game/src/custom_food.rs`: все 21 основы tg (хлеб и ломтики, булка, тесто для пиццы, основы пирога и торта и их ломтики, лист нори, онигири, тако, варёные спагетти, рис/лапша/окономияки, миска) с `replacement`, типом заливки и `max_ingredients`; любая еда на основу — реагенты (объём растёт), вес, имя «<размер> <первый ингредиент> <основа>», оверлей `_filling` (одна смешанная заливка, россыпь, стопка, стопка с верхом, полосы), «too full!»; основа с заменой превращается в свой тип; нарезка переносит ингредиенты, имя и заливки (`food.rs` `finish_process`); осмотр. Нет: цвет заливки берётся из `color` или смеси реагентов ингредиента, а не из среднего цвета иконки; материалов, гранат как ингредиента, `subjectname` мяса, ломтиков пиццы из коробки

### Реакции еды в посуде
Смешивание реагентов в стакане/миске: из муки и воды — тесто, из сои — тофу, из молока и ферментов — головка сыра, а также соусы (соевый, барбекю, подлива), карамель, тесто для блинов, пудинги и т. п. Часть реакций создаёт предметы еды (`resulting_food_path`), остальные — реагенты.

- **Код:** [`/datum/chemical_reaction/food`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/recipes/food_mixtures.dm#L78), [`/datum/chemical_reaction/food/dough`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/recipes/food_mixtures.dm#L213)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/reagents.rs` (реакции еды включены), `crates/ss13_reagents`: реагентные результаты есть; предметы из `on_reaction` (тесто, тофу, сыр) не появляются

### Супы
~90 рецептов супов: в кастрюле на плите при нагреве вода и ингредиенты (в том числе твёрдые предметы, растворяющиеся по ходу) превращаются в суп; есть «кастомный» водянистый суп. В меню крафта супы показаны как справочные рецепты.

- **Код:** [`/datum/chemical_reaction/food/soup`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/recipes/soup_mixtures.dm#L25), [`/datum/chemical_reaction/food/soup/custom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/recipes/soup_mixtures.dm#L347), [`/datum/crafting_recipe/food/reaction/soup`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/recipes/soup_guide.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — конвертер (`crates/ss13_mapload/src/reagents.rs`) теперь берёт реакции супов с `SoupDef` (`required_ingredients`, множитель, доля нутриентов, `resulting_food_path`), движок `ss13_reagents` сам их не запускает (`Holder::start_reaction`), `crates/ss13_game/src/stove.rs` запускает при ингредиентах (по типу или родителю, стопки по количеству), реагентах и температуре: еда-результат, шаги переноса 20% ингредиентов, выкипание, по окончании — ингредиенты расходуются, лишнее жарится/высыпается; нет кастомного супа и супов со своими проверками (slimesoup/alt, кукурузная каша), пены, нескольких супов одновременно

### Рецепты коктейлей
~240 реакций напитков: коктейли, пиво/вино из ингредиентов, напитки ящеров и мотыльков, безалкогольные смеси; с сообщениями и звуками смешивания.

- **Код:** [`/datum/chemical_reaction/drink`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/recipes/drinks/drinks_alcoholic.dm#L2), [`drinks/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/recipes/drinks)
- **BandaStation:** [`modular_bandastation/objects/code/items/consumables/drinks.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/consumables/drinks.dm#L73) — ~28 дополнительных коктейлей форка (Innocent ERP, Soundhand, Vampiro, «Пятилетка», чифир и др.)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_mapload/src/reagents.rs` (все реакции напитков — чистые данные), `crates/ss13_reagents`: смешивание в посуде с сообщением (без звуков); эффекты самих напитков — область химии

# Кухонные машины
Машины кухни и бара, которые превращают продукты в блюда: от микроволновки до гиббера.

### Микроволновка
До 10 предметов внутрь, радиальное меню «готовить/извлечь», цикл ~10 с с расходом энергии; еда становится своим результатом или «неудачным блюдом». Грязнеет с каждой готовкой, может сломаться и искрить от металла; беспроводная версия заряжает ПДА («вампирская» зарядка). Ремонт, чистка, апгрейды деталей.

- **Код:** [`/obj/machinery/microwave`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/microwave.dm#L20), [`proc/cook()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/microwave.dm#L581), [`proc/start_cycle()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/microwave.dm#L563), [`/obj/machinery/microwave/engineering`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/microwave.dm#L918)
- **BandaStation:** [`modular_bandastation/aesthetics/kitchen/code/microwave.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/kitchen/code/microwave.dm#L2) — своя иконка микроволновки
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/kitchen.rs`: вставка, готовка, грязь, поломка при потере питания есть, сумка, применённая к микроволновке, выгружает в неё еду (`handle_dumping`; поднос сразу, иначе 2 с; перетаскивание сумки на неё, как в tg, высыпает на пол — `drag.rs`); радиальное меню заменено кликами, нет зарядки ПДА, искр от металла, чистки и ремонта, панели/проводов/разборки, апгрейдов, отрисовки содержимого

### Духовка и противень
В открытую духовку ставится противень с едой; включённая духовка запекает предметы с `bakeable` (тесто в хлеб, сырую пиццу в пиццу), передержанное сгорает и дымит. Плита-«range» совмещает духовку и конфорки.

- **Код:** [`/obj/machinery/oven`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/oven.dm#L11), [`/obj/machinery/oven/process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/oven.dm#L74), [`/obj/machinery/oven/range`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/oven.dm#L246), [`/obj/item/plate/oven_tray`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/oven.dm#L263), [`/datum/component/bakeable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/bakeable.dm#L2)
- **Статус в ss13-rs:** ❌ нет — нет духовки (противень учитывается в `crates/ss13_game/src/food.rs` только как «стол» для нарезки)

### Сковорода-гриль (griddle)
Предметы раскладываются на включённой плоской сковороде и жарятся по `grillable` (сырое мясо в стейк, яйцо в яичницу); передержанное горит. Масло на сковороде даёт брызги.

- **Код:** [`/obj/machinery/griddle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/griddle.dm#L1), [`/datum/component/grillable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/grillable.dm#L3), [`/datum/element/grilled_item`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/food/grilled_item.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Угольный гриль
Гриль на топливе (уголь, дерево, стопки досок); жарит положенную еду с «копчёным» эффектом, расходует топливо и дымит.

- **Код:** [`/obj/machinery/grill`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/grill.dm#L6)
- **Статус в ss13-rs:** ❌ нет

### Фритюрница
Заполняется кулинарным маслом; опущенный предмет со временем становится «обжаренным», затем подгоревшим и угольным (обжарить можно почти что угодно). Горячее масло обжигает при погружении руки/головы.

- **Код:** [`/obj/machinery/deepfryer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/deep_fryer.dm#L14), [`/obj/machinery/deepfryer/process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/deep_fryer.dm#L144), [`/datum/element/fried_item`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/food/fried_item.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Плита и кастрюля для супа
Плита греет поставленную на конфорку посуду; кастрюля с водой и ингредиентами при нужной температуре варит суп (реакции супов), который разливается половником по мискам.

- **Код:** [`/obj/machinery/stove`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/stove.dm#L1), [`/datum/component/stove`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/stove_component.dm#L9), [`/obj/item/reagent_containers/cup/soup_pot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/stove.dm#L28)
- **Статус в ss13-rs:** ❌ нет

### Кухонный комбайн (processor)
Еда с рецептом процессора (мясо в фарш, картофель в картофель фри и т. п.) перерабатывается за время рецепта; слайм-процессор (ксенобиология) делает ядра из слаймов.

- **Код:** [`/obj/machinery/processor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/processor.dm#L3), [`/datum/food_processor_process`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/recipes/processor_recipes.dm#L1), [`/obj/machinery/processor/slime`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/processor.dm#L218)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/kitchen.rs`, `crates/ss13_mapload/src/food.rs`: рецепты еды работают; нет переработки мобов, слайм-процессора, апгрейдов, разборки

### Блендер (reagentgrinder)
Перемалывает или выжимает предметы в реагенты в стакане; радиальное меню: молоть, выжать сок, смешать, извлечь. Принимает содержимое сумок.

- **Код:** [`/obj/machinery/reagentgrinder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/machinery/reagentgrinder.dm#L2), [`proc/operate_for()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/machinery/reagentgrinder.dm#L405), [`proc/mix()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/machinery/reagentgrinder.dm#L467)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/kitchen.rs`: смена стакана, загрузка, помол (реагенты предмета) есть; нет выжимки сока, смешивания, переопределений `grind_results()`, высыпания сумок, радиального меню

### Гиббер
Моб (или труп), засунутый внутрь, перерабатывается в куски мяса, кожу и органы; оставляет кровавые следы. Автогиббер срабатывает сам.

- **Код:** [`/obj/machinery/gibber`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/gibber.dm#L1), [`/obj/machinery/gibber/autogibber`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/gibber.dm#L338)
- **Статус в ss13-rs:** ❌ нет

### Переработчик обезьян
Трупы обезьян перерабатываются в обезьяньи кубики (число зависит от деталей).

- **Код:** [`/obj/machinery/monkey_recycler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/monkeyrecycler.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Машина мороженого
Хранит реагенты-ингредиенты; режим «рожки» печёт рожки, режим «мороженое» кладёт шарик выбранного вкуса в рожок (несколько шариков). Вкусы — датумы `ice_cream_flavour`.

- **Код:** [`/obj/machinery/icecream_vat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/icecream_vat.dm#L9), [`/datum/component/ice_cream_holder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/food/ice_cream_holder.dm#L5), [`/datum/ice_cream_flavour`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/food/ice_cream_holder.dm#L200)
- **Статус в ss13-rs:** ❌ нет

### Кофеварка
Картридж с кофе, вода и кофейник: варит кофе разных сортов (картриджи обычный, элитный, без кофеина, бутлег), «Impressa» — с молоком и сахаром.

- **Код:** [`/obj/machinery/coffeemaker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/coffeemaker.dm#L3), [`/obj/item/coffee_cartridge`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/coffeemaker.dm#L442), [`/obj/machinery/coffeemaker/impressa`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/coffeemaker.dm#L518), [`/obj/item/reagent_containers/cup/coffeepot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/_cup.dm#L781)
- **Статус в ss13-rs:** ❌ нет

### Тележка с едой
Передвижная тележка повара: хранилище еды, отсек для напитков с миксером, раскладывается в стойку с зонтиком.

- **Код:** [`/obj/machinery/food_cart`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/food_cart.dm#L2), [`/obj/effect/food_cart_stand`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/food_cart.dm#L134)
- **Статус в ss13-rs:** ❌ нет

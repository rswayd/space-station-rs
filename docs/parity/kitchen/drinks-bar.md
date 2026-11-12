# Напитки и бар
Посуда для напитков, бутылки, банки газировки, раздатчики бара, кальян и вывески бара. Рецепты коктейлей — в [recipes-crafting.md](recipes-crafting.md).

### Стаканы и посуда для напитков
Общий тип посуды для напитков (стаканы, рюмки, кружки, кофейные стаканчики, бумажные стаканчики, бутылка воды, коробки сока, стакан со льдом): пьётся, переливается, бьётся при броске.

- **Код:** [`/obj/item/reagent_containers/cup/glass`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/drinks.dm#L4), [`/obj/item/reagent_containers/cup/glass/drinkingglass`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/drinkingglass.dm#L1), [`/obj/item/reagent_containers/cup/glass/drinkingglass/shotglass`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/drinkingglass.dm#L76), [`/obj/item/reagent_containers/cup/glass/coffee`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/drinks.dm#L116), [`/obj/item/reagent_containers/cup/glass/sillycup`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/drinks.dm#L349), [`/obj/item/reagent_containers/cup/glass/waterbottle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/drinks.dm#L231), [`/obj/item/storage/box/drinkingglasses`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/boxes/service_boxes.dm#L4)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/chem.rs`: питьё, переливание, наполнение, расплёскивание; брошенный стакан разбивается (`crates/ss13_game/src/bar.rs`); нет крышек, звуков

### Внешний вид напитка по содержимому
Стакан или рюмка с напитком принимает имя, описание и спрайт главного реагента (`glass_style` каждого коктейля); бутылки и чашки — по своим стилям.

- **Код:** [`/datum/glass_style`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/_glass_datum.dm#L51), [`/datum/component/takes_reagent_appearance`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/takes_reagent_appearance.dm#L14)
- **BandaStation:** [`modular_bandastation/objects/code/items/consumables/drinks.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/consumables/drinks.dm#L22) — стили стаканов для напитков форка
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/chem.rs` (`glass_style`, `apply_glass_style`): стакан, рюмка и коробка сока принимают имя, описание и спрайт стиля главного реагента (`GLOB.glass_style_singletons` из `reagents.ron`: `crates/ss13_mapload/src/reagents.rs` `glass_styles`, включая стили форка), без стиля возвращаются к своим; стилизованный стакан теряет оверлей заполнения; рюмка зовётся «(filled) shot glass»; спрайт в руке не меняется; миски с супами — с супами

### Разбивание стеклянной посуды
Брошенный стакан или бутылка разбивается о цель, расплёскивая содержимое; бутылка, разбитая о голову, оглушает и оставляет «розочку» (`broken_bottle`) — колющее оружие.

- **Код:** [`proc/smash()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/drinks.dm#L22), [`/obj/item/broken_bottle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/glassbottle.dm#L204)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/bar.rs` (из `crates/ss13_game/src/throw.rs`), `isGlass` как `dm_glass` (`crates/ss13_mapload/src/drinks.rs`): брошенный непойманный стакан/бутылка разбивается о то, во что попал, или о пол: содержимое выплёскивается (сообщение, как сплэш `crate::chem` — без экспозиции), остаётся `broken_bottle`; нет маски «разбитого» спрайта (у осколка свой спрайт), проверки бармена (beer goggles), звуков, разбивания бутылки о голову

### Бутылки алкоголя и соков
~50 видов бутылок (пиво, вино, водка, ром, виски, абсент, ...) и пакеты соков/сливок; бутылки с элитным алкоголем, пена при открытии шампанского. Коктейль Молотова из бутылки — см. боевые предметы.

- **Код:** [`/obj/item/reagent_containers/cup/glass/bottle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/glassbottle.dm#L7), [`/obj/item/reagent_containers/cup/glass/bottle/beer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/glassbottle.dm#L260), [`/obj/item/reagent_containers/cup/glass/bottle/wine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/glassbottle.dm#L427), [`/obj/item/reagent_containers/cup/glass/bottle/juice`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/glassbottle.dm#L1032)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/chem.rs`: бутылки с `list_reagents`, питьё и переливание; брошенная бутылка разбивается (`bar.rs`); нет пробок/открывания, пены, удара бутылкой по голове

### Банки газировки
~20 видов банок; банку нужно открыть (встряхнутая или брошенная может взорваться брызгами при открытии), пустую можно смять; самодельная бомба из банки с фитилём.

- **Код:** [`/obj/item/reagent_containers/cup/soda_cans`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/soda.dm#L11), [`proc/open_soda()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/soda.dm#L111), [`proc/burst_soda()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/soda.dm#L137)
- **BandaStation:** [`modular_bandastation/food_and_drinks/code/drinks.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/food_and_drinks/code/drinks.dm#L1) — банка «Lip-O» (кола с липолитиком)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/chem.rs` (`open_soda`, `soda_thrown`), `crates/ss13_game/src/throw.rs`: клик по банке в руке открывает её (шанс `fizziness` облить открывающего и опустошить), брошенная закрытая полная банка (не пойманная) получает +15 шипучести и с этим шансом лопается в смятую банку того же спрайта; нет встряхивания (ПКМ по руке), смятия о лоб, бомбы с фитилём, разрыва в вакууме, скотча, звуков

### Шейкер, фляга и барный инвентарь
Шейкер смешивает коктейли (переключение «кастомных» напитков Alt-кликом), фляга носится в кармане.

- **Код:** [`/obj/item/reagent_containers/cup/glass/shaker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/drinks.dm#L417), [`/obj/item/reagent_containers/cup/glass/flask`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_containers/cups/drinks.dm#L503)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/chem.rs`: как ёмкости работают; нет режима кастомных напитков шейкера

### Раздатчики газировки и алкоголя
Вариант химического раздатчика с напитками: газировка (кола, вода, лёд, соки...) и алкоголь (пиво, водка, ром...); окно ChemDispenser, апгрейды открывают доп. напитки, emag — скрытые.

- **Код:** [`/obj/machinery/chem_dispenser/drinks`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/machinery/chem_dispenser.dm#L575), [`/obj/machinery/chem_dispenser/drinks/beer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/chemistry/machinery/chem_dispenser.dm#L670); tgui: [`ChemDispenser.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ChemDispenser.tsx)
- **BandaStation:** [`modular_bandastation/objects/code/items/consumables/drinks.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/consumables/drinks.dm#L1) — доп. напитки форка в раздатчике алкоголя
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/chem_dispenser.rs`, окно `crates/ss13_tgui/src/interfaces/chem_dispenser.rs`: списки реагентов типа (`static`-списки tg выгружает `ss13_dm_extract`, `crates/ss13_mapload/src/dispenser.rs`; добавки бара форка: самбука, ягермейстер, блю кюрасао), сортировка по имени, тир-4 сервопривод и `fullupgrade` (emag) добавляют свои; ёмкость вставляется кликом (окно открывается), ПКМ вынимает; окно: энергия, кнопки количества по `possible_transfer_amounts`, выдача (`power_cost` Дж/ед. из ячейки-детали, температура и чистота типа), удаление, вставка/извлечение — всё проверяется на сервере; перезарядка ячейки каждый тик машин (`recharge_amount` × тиры конденсаторов); сборка из платы; нет рецептов (запись/воспроизведение), списка реакций, разбрызгивания при ЭМИ (см. ../chemistry/chem-machines.md), емага в руке, галлюцинаций, оверлея мензурки

### Кеги и бочки напитков
Стационарные бочки (пиво и др.), из которых наливают в посуду.

- **Код:** [`/obj/structure/reagent_dispensers/keg`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reagents/reagent_dispenser.dm#L588)
- **BandaStation:** [`modular_bandastation/objects/code/items/consumables/drinks.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/consumables/drinks.dm#L33) — бочка кваса (`kvasstank`), заказ через карго и экспорт
- **Статус в ss13-rs:** ✅ реализовано — кеги и бочка кваса — подтипы `reagent_dispensers`: реагенты типа, наливание в посуду кликом (`crates/ss13_game/src/chem.rs`), кран гаечным ключом и утечка 10 ед. на пол (`crates/ss13_game/src/kitchen_ware.rs`, `openable`); остальное о баках — `../chemistry/containers.md`

### Напитки форка
~30 новых реагентов-напитков BandaStation: квас, самбука, ягермейстер, блю кюрасао, фраппучино, макиато, чифир, тематические коктейли; у части есть свои эффекты.

- **Код:** [`/datum/reagent/consumable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/consumables/drinks.dm#L13) (определены в форке)
- **BandaStation:** [`modular_bandastation/objects/code/items/consumables/drinks.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/consumables/drinks.dm#L13) — новые реагенты, стили стаканов, реакции
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/reagents.rs`: реагенты и реакции попадают в `reagents.ron` как данные; их `on_mob_life`-эффекты не портированы

### Кальян (BandaStation)
Кальян с мундштуком: в чашу кладут до 5 плодов/сыра, угли (30 ед. топлива) поджигают, вдох через мундштук раз в 5 с переносит реагенты дымом, частицы и свечение; кашель и урон выносливости от затяжки; можно погасить, очистить чашу, «раскурить». Набор кальяна заказывается через карго.

- **Код:** [`/obj/item/hookah`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/bar_hookahs/code/bar_hookahs.dm#L26), [`/obj/item/hookah_mouthpiece`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/bar_hookahs/code/bar_hookahs.dm#L332), [`/obj/item/hookah_coals`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/bar_hookahs/code/bar_hookahs.dm#L496), [`/datum/supply_pack/misc/hookah_kit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/bar_hookahs/code/bar_hookahs.dm#L514)
- **BandaStation:** [`modular_bandastation/bar_hookahs/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/bar_hookahs) — весь модуль добавлен форком
- **Статус в ss13-rs:** ❌ нет

### Вывеска бара
Двухтайловая вывеска над баром: бармен с доступом выбирает её из ~47 вариантов, ЭМИ и emag меняют на «сломанную»/скрытые варианты; монтаж, ремонт.

- **Код:** [`/obj/machinery/barsign`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/barsigns.dm#L1), [`proc/pick_sign()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/barsigns.dm#L215), [`/datum/barsign`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/barsigns.dm#L230)
- **BandaStation:** [`modular_bandastation/barsigns/code/barsigns.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/barsigns/code/barsigns.dm#L1) — 7 своих вывесок со своей иконкой (`ss220_icon`)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/barsign.rs`, данные `crates/ss13_mapload/src/barsign.rs` (`dm_barsign`: `req_access` и все `/datum/barsign` с `ss220_icon` форка): вывеска стартует выключенной, пустая рука с доступом бара — список видимых вывесок по имени, выбор меняет спрайт, имя «bar sign (…)» и описание (с повторной проверкой доступа и досягаемости); без питания — «empty» и не отвечает; ЭМИ — вывеска «EMP'd» и не отвечает 60 с, затем прежняя (`crates/ss13_game/src/emp/machines.rs`); нет переименования зоны, неона, emag, поломки и ремонта, снятия со стены

# Ресторан и роботы-посетители
Система «ресторана» для повара и бармена: портал призывает роботов-посетителей, которые заказывают блюда и напитки и платят за них.

### Подсистема ресторана (SSrestaurant)
Хранит все заведения (`venue`), типы посетителей и кэш внешнего вида заказанных блюд; создаётся при старте сервера (без собственного тика).

- **Код:** [`SUBSYSTEM_DEF(restaurant)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/restaurant.dm#L5)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/restaurant.rs` (`Restaurant`: заведения и типы посетителей из `restaurant.ron`, `crates/ss13_mapload/src/restaurant.rs`); кэш внешнего вида блюд не нужен (облачка заказа нет, см. «Робот-посетитель»)

### Заведения: ресторан и бар
Два заведения: ресторан (заказы еды, доступ кухни) и бар (заказы напитков, доступ бара). Открытое заведение раз в 60–90 с призывает посетителя (до 6 одновременно), считает обслуженных и выручку.

- **Код:** [`/datum/venue`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/_venue.dm#L2), [`/datum/venue/process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/_venue.dm#L34), [`proc/create_new_customer()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/_venue.dm#L42), [`/datum/venue/restaurant`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/generic_venues.dm#L3), [`/datum/venue/bar`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/generic_venues.dm#L71)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/restaurant.rs` (`venues_tick`, `create_new_customer`, `toggle_open`): данные `/datum/venue/*` (доступ, интервалы, `max_guests`, взвешенные типы, уникальные — раз за заведение), первый гость через 4 с, гостей не больше мест + 1, счёт обслуженных и выручки; закрытие отправляет всех домой

### Портал ресторана
Машина, через которую приходят посетители: клик с доступом открывает/закрывает заведение, ID-картой можно запретить приход конкретным посетителям; строится из платы (исследование), разбирается. Плата переключается мультитулом между рестораном и баром.

- **Код:** [`/obj/machinery/restaurant_portal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/_venue.dm#L161), [`/obj/item/circuitboard/machine/restaurant_portal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/circuitboards/machines/machine_circuitboards.dm#L1534)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/restaurant.rs`: пустая рука с ID доступа заведения открывает/закрывает (оверлей двери у закрытого), ID-карта на портале — список заведений (вместо радиального меню) с повторной проверкой, запоминает открывшего (для мотылька); портал без своего заведения (из платы) — заведение платы; нет переключения платы мультитулом, запрета прихода по ID; разборка — общая `crates/ss13_game/src/machine_frame.rs` (не проверено на портале)

### Места для посетителей
Голопроектор размечает голографические места (стулья) для роботов; посетитель ищет свободное место и злится, если не нашёл.

- **Код:** [`/obj/item/holosign_creator/robot_seat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/_venue.dm#L306), [`/obj/structure/holosign/robot_seat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/_venue.dm#L318)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/holosign.rs` (проекторы мест ресторана и бара: 1 с, до 10, в руке не очищаются, места не бьются рукой), `crates/ss13_game/src/restaurant.rs` (`RobotSeat`, `remove_seat`: свой проектор убирает свободное место); поиск свободного места в 7 тайлах раз в 8 с, фраза «не нашёл» один раз и с `SPT_PROB(1.5)`

### Робот-посетитель
Базовый моб с ИИ (дерево поведения): идёт к месту, делает заказ (облачко с картинкой при наведении), ждёт с репликами нетерпения, получает заказ из рук, платит и уходит; при долгом ожидании уходит злым. Защищается, если его тащат или бьют (два предупреждения, потом драка).

- **Код:** [`/mob/living/basic/robot_customer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/robot_customer.dm#L2), [`/datum/ai_controller/robot_customer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/robot_customer/robot_customer_controller.dm#L1)
- **BandaStation:** [`modular_bandastation/translations/code/restaurant_customer.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/translations/code/restaurant_customer.dm#L1) — русские реплики посетителей и TTS-голоса
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/restaurant.rs` (дерево `robot_customer.bt.json` в `tree()`, листья `CustomerLeaf`), `crates/ss13_game/src/mobs/ai`: идёт к месту, поворачивается по стулу, заказывает (реплика, заказ в осмотре), ждёт `total_patience` с репликами `SPT_PROB(0.85)`, берёт заказ из рук или с тайла перед собой, уходит через свой портал довольным/злым; таскание без доступа и атаки — два предупреждения, затем «объятия» (15 brute на все конечности кроме головы) до потери сознания; грязь при ходьбе (`grime_carried`); имя `<префикс>-bot`, случайный тон, одежда поверх; нет облачка заказа при наведении (`food_demands`), TTS-голосов, звука шагов, таскания во время объятий, гибов при смерти; ушедший без портала исчезает сразу (в tg стоит вечно); удар считается одним предупреждением (в tg — двумя)

### Типы посетителей
Национальности посетителей (американец, итальянец, француз, японец/салариман, мотылёк, мексиканец, британец/джентльмен/бобби) со своей внешностью, репликами и списками заказов для ресторана и бара; редкий «неисправный» робот (раз за смену заведения).

- **Код:** [`/datum/customer_data`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/customers/_customer.dm#L1), [`/datum/customer_data/malfunction`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/customers/_customer.dm#L434)
- **BandaStation:** [`modular_bandastation/objects/code/items/consumables/customer_data.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/consumables/customer_data.dm#L1) — коктейли форка в заказах бара
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/restaurant.rs`: все `/datum/customer_data/*` (реплики форка, `strings/names/*_prefix.txt`, одежда, уникальность, заказы с добавками бара форка из `New()`), мотылёк — только если открывший носит шапку, перчатки или обувь; ранец мотылька есть, крыльев (`SSaccessories`) нет; День мороженого не учитывается

### Заказы
Заказ — тип блюда, реагент-напиток (нужный объём в посуде) или особый заказ: мороженое с заданными вкусами, суп в миске, одежда мотыльку. Проверяется соответствие выданного предмета.

- **Код:** [`proc/order_food()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/_venue.dm#L67), [`/datum/custom_order`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/custom_order.dm#L6), [`/datum/custom_order/reagent`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/custom_order.dm#L133), [`/datum/custom_order/icecream`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/custom_order.dm#L87), [`/datum/custom_order/moth_clothing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/custom_order.dm#L40)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/restaurant.rs` (`Order`, `order_food`, `is_correct_order`): предмет точного типа (только ресторан), напиток (10 ед., главный реагент, ≥ 1/3, в стакане), суп в миске (15/20/25 ед.), мороженое (1–3 стандартных вкуса, рожок вафельный/шоколадный, сравнение отсортированных шариков `Scoops`), одежда мотыльку (вещи открывшего 5/5/1, иначе запасной список); строки заказа — переводы форка

### Оплата заказа
У блюд и напитков есть цена для заведения (`venue_price`); при продаже роботу выпадают наличные (голочипы), деньги идут в счёт заведения/отдела, ведётся статистика.

- **Код:** [`/datum/element/venue_price`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/food/venue_price.dm#L2), [`proc/on_get_order()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/_venue.dm#L117)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/restaurant.rs` (цены: `venue_value`, `glass_price`, одежда 70, мелки 550), `crates/ss13_game/src/restaurant.rs` (`eat_order`, `pay`): продажа даёт голочип с ценой (`Holochip`, имя «голочип номиналом N¢», звук кассы), выручку и счётчик заведения; ресторан съедает проданное (мусор еды — в руку), напиток и суп выпиваются из посуды; мороженое — 50 за шарик (не tg: считается обслуженным и съедается); сами голочипы (снятие/зачисление на карту, деление) — у группы экономики, см. «Голочипы» в `../cargo/economy.md`

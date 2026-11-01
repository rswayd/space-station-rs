# Ресторан и роботы-посетители
Система «ресторана» для повара и бармена: портал призывает роботов-посетителей, которые заказывают блюда и напитки и платят за них.

### Подсистема ресторана (SSrestaurant)
Хранит все заведения (`venue`), типы посетителей и кэш внешнего вида заказанных блюд; создаётся при старте сервера (без собственного тика).

- **Код:** [`SUBSYSTEM_DEF(restaurant)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/restaurant.dm#L5)
- **Статус в ss13-rs:** ❌ нет

### Заведения: ресторан и бар
Два заведения: ресторан (заказы еды, доступ кухни) и бар (заказы напитков, доступ бара). Открытое заведение раз в 60–90 с призывает посетителя (до 6 одновременно), считает обслуженных и выручку.

- **Код:** [`/datum/venue`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/_venue.dm#L2), [`/datum/venue/process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/_venue.dm#L34), [`proc/create_new_customer()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/_venue.dm#L42), [`/datum/venue/restaurant`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/generic_venues.dm#L3), [`/datum/venue/bar`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/generic_venues.dm#L71)
- **Статус в ss13-rs:** ❌ нет

### Портал ресторана
Машина, через которую приходят посетители: клик с доступом открывает/закрывает заведение, ID-картой можно запретить приход конкретным посетителям; строится из платы (исследование), разбирается. Плата переключается мультитулом между рестораном и баром.

- **Код:** [`/obj/machinery/restaurant_portal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/_venue.dm#L161), [`/obj/item/circuitboard/machine/restaurant_portal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/circuitboards/machines/machine_circuitboards.dm#L1534)
- **Статус в ss13-rs:** ❌ нет

### Места для посетителей
Голопроектор размечает голографические места (стулья) для роботов; посетитель ищет свободное место и злится, если не нашёл.

- **Код:** [`/obj/item/holosign_creator/robot_seat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/_venue.dm#L306), [`/obj/structure/holosign/robot_seat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/_venue.dm#L318)
- **Статус в ss13-rs:** ❌ нет

### Робот-посетитель
Базовый моб с ИИ (дерево поведения): идёт к месту, делает заказ (облачко с картинкой при наведении), ждёт с репликами нетерпения, получает заказ из рук, платит и уходит; при долгом ожидании уходит злым. Защищается, если его тащат или бьют (два предупреждения, потом драка).

- **Код:** [`/mob/living/basic/robot_customer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/robot_customer.dm#L2), [`/datum/ai_controller/robot_customer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/robot_customer/robot_customer_controller.dm#L1)
- **BandaStation:** [`modular_bandastation/translations/code/restaurant_customer.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/translations/code/restaurant_customer.dm#L1) — русские реплики посетителей и TTS-голоса
- **Статус в ss13-rs:** ❌ нет

### Типы посетителей
Национальности посетителей (американец, итальянец, француз, японец/салариман, мотылёк, мексиканец, британец/джентльмен/бобби) со своей внешностью, репликами и списками заказов для ресторана и бара; редкий «неисправный» робот (раз за смену заведения).

- **Код:** [`/datum/customer_data`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/customers/_customer.dm#L1), [`/datum/customer_data/malfunction`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/customers/_customer.dm#L434)
- **BandaStation:** [`modular_bandastation/objects/code/items/consumables/customer_data.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/consumables/customer_data.dm#L1) — коктейли форка в заказах бара
- **Статус в ss13-rs:** ❌ нет

### Заказы
Заказ — тип блюда, реагент-напиток (нужный объём в посуде) или особый заказ: мороженое с заданными вкусами, суп в миске, одежда мотыльку. Проверяется соответствие выданного предмета.

- **Код:** [`proc/order_food()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/_venue.dm#L67), [`/datum/custom_order`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/custom_order.dm#L6), [`/datum/custom_order/reagent`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/custom_order.dm#L133), [`/datum/custom_order/icecream`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/custom_order.dm#L87), [`/datum/custom_order/moth_clothing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/custom_order.dm#L40)
- **Статус в ss13-rs:** ❌ нет

### Оплата заказа
У блюд и напитков есть цена для заведения (`venue_price`); при продаже роботу выпадают наличные (голочипы), деньги идут в счёт заведения/отдела, ведётся статистика.

- **Код:** [`/datum/element/venue_price`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/food/venue_price.dm#L2), [`proc/on_get_order()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/restaurant/_venue.dm#L117)
- **Статус в ss13-rs:** ❌ нет

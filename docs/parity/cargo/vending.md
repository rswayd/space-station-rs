# Торговые автоматы
Общая механика `/obj/machinery/vending` и семейства автоматов станции.

### Окно автомата и выдача товара
Клик пустой рукой открывает tgui `Vending`: категории, товары с ценами и остатком, данные покупателя по ID. `vend` проверяет запись, остаток, питание, наличие ID и оплату, выдаёт предмет в руку (или на пол); у некоторых товаров выбирается окраска (greyscale).

- **Код:** [`/obj/machinery/vending`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/_vending.dm#L36), [`ui_interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/ui_data.dm#L9), [`ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/ui_data.dm#L138), [`vend()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/inventory.dm#L143), [`dispense()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/inventory.dm#L226); tgui: [`Vending.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Vending.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/vending.rs`, `crates/ss13_tgui/src/interfaces/vending.rs`: окно, категории, остатки, выдача в руку с проверками в момент действия; нет списания денег (`shortcut: no economy yet`), выбора окраски, звука выдачи и сообщений игроку

### Инвентарь автомата (products / contraband / premium)
Из списков `products`, `contraband` (только во взломанном) и `premium` строятся записи с ценами (`default_price`, `extra_price`) и категориями; запас пополняется только канистрой.

- **Код:** [`build_inventory()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/inventory.dm#L14), [`build_inventories()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/inventory.dm#L61), [`/datum/data/vending_product`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/_vending.dm#L7), [`collect_records_for_static_data()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/ui_data.dm#L27)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_mapload/src/vending.rs` (конвертация всех подтипов), `crates/ss13_game/src/vending.rs` `vendor()`

### Оплата, скидки и бесплатные автоматы
Оплата с ID-карты (компонент payment, деньги — в бюджет отдела автомата), скидка 20% своему отделу, премиум-товары дороже; автоматы вне станции (`onstation = FALSE`) и `all_products_free` бесплатны. Цены пересчитываются инфляцией `SSeconomy`.

- **Код:** [`proceed_payment()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/inventory.dm#L270), [`discount_check()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/inventory.dm#L294), [`fetch_balance_to_use()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/ui_data.dm#L97), [`update_vending_prices()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/economy.dm#L241)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/vending.rs`: `all_products_free` и требование ID есть, `DEPARTMENT_DISCOUNT` только в данных окна; денег не списывает, `onstation` и инфляции нет

### Провода автомата (взлом)
`/datum/wires/vending`: стрельба товаром, контрабанда, сканер ID, удар током, динамик (язык).

- **Код:** [`/datum/wires/vending`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/vending.dm#L1), [`throw_item()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/throwing.dm#L8)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/wires.rs` (`vending_on_cut`, `vending_on_pulse`, `throw_item`); смена языка динамиком — упрощение

### Панель, разборка и питание
Отвёртка открывает панель, гаечный ключ откручивает автомат, ломом разбирается; без питания экран гаснет и окно не работает; поломка и ЭМИ.

- **Код:** [`screwdriver_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/interaction.dm#L29), [`wrench_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/interaction.dm#L21), [`crowbar_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/interaction.dm#L16), [`power_change()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/_vending.dm#L434), [`atom_break()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/_vending.dm#L254)
- **BandaStation:** [`modular_bandastation/balance/code/vending.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/vending.dm#L1) — автомат с AI-контроллером (ожившие) нельзя трогать инструментами
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/vending.rs` (питание, спрайт `-off`), `crates/ss13_game/src/construction.rs` (панель, ключ); нет разборки ломом, поломки, ЭМИ

### Опрокидывание автомата
Сильно ударенный (или толкнутый) автомат падает на соседний тайл и давит того, кто там стоит: большой урон, шанс критических эффектов (раздробленные конечности, вбитые осколки и т.д.); поднимается обратно вручную.

- **Код:** [`tilt()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/tilting.dm#L212), [`fall_and_crush()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/tilting.dm#L41), [`untilt()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/tilting.dm#L300), [`attacked_by()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/interaction.dm#L217)
- **Статус в ss13-rs:** ❌ нет

### Пополнение канистрами (vending refill)
Канистра `vending_refill` пополняет запас своего автомата; можно заменить через RPED. Канистры заказываются в карго.

- **Код:** [`restock()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/inventory.dm#L94), [`post_restock()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/interaction.dm#L152), [`/obj/item/vending_refill`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/vending_items.dm#L4), [`exchange_parts()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/interaction.dm#L167)
- **BandaStation:** [`modular_bandastation/aesthetics/vendors_restock/code/vendors_restock.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/vendors_restock/code/vendors_restock.dm#L1) — свои спрайты канистр
- **Статус в ss13-rs:** ❌ нет

### Возврат товара в автомат
Предмет, проданный этим автоматом, можно положить обратно (с доступом/при открытой панели) — он снова в продаже.

- **Код:** [`item_interaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/interaction.dm#L98), [`canLoadItem()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/interaction.dm#L57), [`loadingAttempt()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/interaction.dm#L74)
- **Статус в ss13-rs:** ❌ нет

### Реклама и слоганы
Автомат раз в несколько минут произносит слоган вслух; в окне показывается рекламная строка. Возрастные ограничения на алкоголь/табак с объявлением.

- **Код:** [`process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/_vending.dm#L439), [`speak()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/_vending.dm#L467), [`slogan_list`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/_vending.dm#L122), [`age_restrictions`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/_vending.dm#L158)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/vending.rs`: реклама только в окне (`ads`); слоганов вслух и возрастного контроля нет

### Емаг автомата
Емаг снимает ограничения (возраст, оплата отдела) и включает контрабанду.

- **Код:** [`emag_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/_vending.dm#L426)
- **Статус в ss13-rs:** ❌ нет

### Пользовательский автомат (custom vendor)
Автомат, в который владелец кладёт свои предметы и сам назначает цены; выручка идёт на его счёт.

- **Код:** [`/obj/machinery/vending/custom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/custom.dm#L4), [`item_interaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/custom.dm#L196), [`/obj/item/vending_refill/custom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/custom.dm#L316)
- **Статус в ss13-rs:** ❌ нет

### Автоматы еды и напитков
Снеки, газировки (Robust Softdrinks и варианты), кофе, хот-доги, Donk, советская газировка, пища заключённых, бутылки бара (Booze-O-Mat), посуда.

- **Код:** [`/obj/machinery/vending/snack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/snack.dm#L1), [`/obj/machinery/vending/cola`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/cola.dm#L2), [`/obj/machinery/vending/coffee`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/coffee.dm#L1), [`/obj/machinery/vending/hotdog`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/hotdog.dm#L2), [`/obj/machinery/vending/donksnack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/donk.dm#L1), [`/obj/machinery/vending/sovietsoda`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/sovietsoda.dm#L1), [`/obj/machinery/vending/sustenance`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/sustenance.dm#L1), [`/obj/machinery/vending/boozeomat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/boozeomat.dm#L1), [`/obj/machinery/vending/dinnerware`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/drinnerware.dm#L1), [`/obj/machinery/vending/cigarette`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/cigarette.dm#L1)
- **BandaStation:** [`cola.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/cola.dm#L18) — добавлена банка lipo
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/vending.rs`: продают как общий автомат; особенностей (хот-дог-машина, возраст для сигарет/алкоголя) нет

### Автоматы инструментов и инженерии
YouTool, Engi-Vend, инженерный автомат, Robotech, PTech (картриджи), модульные ПК, плазменные исследования.

- **Код:** [`/obj/machinery/vending/tool`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/youtool.dm#L1), [`/obj/machinery/vending/engivend`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/engivend.dm#L1), [`/obj/machinery/vending/engineering`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/engineering.dm#L2), [`/obj/machinery/vending/robotics`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/robotics.dm#L2), [`/obj/machinery/vending/cart`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/cartridge.dm#L2), [`/obj/machinery/vending/modularpc`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/modularpc.dm#L1), [`/obj/machinery/vending/plasmaresearch`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/plasmaresearch.dm#L2)
- **BandaStation:** [`engivend.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/engivend.dm#L27) — материальный подсумок и RLD
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/vending.rs` (общая механика; без оплаты, см. выше)

### Медицинские автоматы
NanoMed Plus, настенный NanoMed, NanoDrug, синдикатовские варианты.

- **Код:** [`/obj/machinery/vending/medical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/medical.dm#L1), [`/obj/machinery/vending/wallmed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/medical_wall.dm#L1), [`/obj/machinery/vending/drugs`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/medical.dm#L73)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/vending.rs`: общий автомат; настенный вариант не проверялся отдельно

### Автоматы сервиса и ботаники
MegaSeed, NutriMax, CytoPro, игры (карты, настолки), игрушки Donksoft, Autodrobe, ClothesMate.

- **Код:** [`/obj/machinery/vending/hydroseeds`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/megaseed.dm#L1), [`/obj/machinery/vending/hydronutrients`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/nutrimax.dm#L1), [`/obj/machinery/vending/cytopro`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/cytopro.dm#L1), [`/obj/machinery/vending/games`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/games.dm#L1), [`/obj/machinery/vending/donksofttoyvendor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/toys.dm#L1), [`/obj/machinery/vending/autodrobe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/autodrobe.dm#L224), [`/obj/machinery/vending/clothing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/clothesmate.dm#L1)
- **BandaStation:** [`megaseed.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/megaseed.dm#L97) — семена коки; [`clothesmate.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/clothesmate.dm#L51) — убран прайд-значок, добавлен цветной плащ; [`modular_bandastation/objects/code/machinery/vending/vending.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/machinery/vending/vending.dm#L646) — дополнительная одежда в Autodrobe и гардеробах
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/vending.rs` (общая механика; без оплаты)

### Гардеробы отделов (wardrobes)
SecDrobe, MediDrobe, EngiDrobe, AtmosDrobe, CargoDrobe, RoboDrobe, SciDrobe, HyDrobe, CuraDrobe, CoroDrobe, BarDrobe, ChefDrobe, JaniDrobe, LawDrobe, ChapDrobe, ChemDrobe, GeneDrobe, ViroDrobe, DetDrobe, ЦК-гардероб: одежда отдела по доступу.

- **Код:** [`/obj/machinery/vending/wardrobe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/wardrobes.dm#L8)
- **BandaStation:** [`modular_bandastation/aesthetics/vending/code/vending.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/vending/code/vending.dm#L1) — свои ЦК- и юр-гардеробы; [`objects/.../vending.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/machinery/vending/vending.dm#L4) — гардероб робототехника и добавки
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/vending.rs` (общая механика; без оплаты)

### Автомат СБ (SecTech)
Наручники, стяжки, флешки, светошумовые, кобуры, пончики.

- **Код:** [`/obj/machinery/vending/security`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/security.dm#L1)
- **BandaStation:** [`modular_bandastation/balance/code/vending_security.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/vending_security.dm#L1) — свой список товаров (болы, тэклер-перчатки, дзюдо-пояс и др.); [`prime_only/code/bodycam_vending.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/prime_only/code/bodycam_vending.dm#L1) — бодикамеры; [`security.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/security.dm#L35) — кобура-аксессуар
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/vending.rs` (общая механика; без оплаты)

### Особые и вражеские автоматы
Liberation Station (оружие), игрушечный Liberation, MagiVend, SyndiChem, рунический автомат (временный, взрывается), админский subtype vendor.

- **Код:** [`/obj/machinery/vending/liberationstation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/liberation.dm#L1), [`/obj/machinery/vending/toyliberationstation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/liberation_toy.dm#L1), [`/obj/machinery/vending/magivend`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/magivend.dm#L1), [`/obj/machinery/vending/syndichem`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/syndichem.dm#L1), [`/obj/machinery/vending/runic_vendor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/runic_vendor.dm#L3), [`/obj/machinery/vending/subtype_vendor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/subtype.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/vending.rs`: данные конвертируются как у обычных; поведение рунического и subtype-автомата нет

### Автоматы снаряжения NT (форк)
Автоматы ЦК для ОБР: лёгкое, среднее (красный код), тяжёлое и сверхтяжёлое снаряжение, свои канистры.

- **Код:** [`/obj/machinery/vending`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/vendor/_vending.dm#L36)
- **BandaStation:** [`modular_bandastation/objects/code/machinery/vending/vending.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/machinery/vending/vending.dm#L28) — `/obj/machinery/vending/nta` и подтипы
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/vending.rs` конвертирует любой подтип `/obj/machinery/vending`; доступность на картах форка не проверялась

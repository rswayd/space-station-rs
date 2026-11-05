# Рынки и почта
Биржа материалов, чёрный рынок, контрабанда и доставка почты экипажу.

### Рынок материалов (Galactic Materials Market)
Машина в карго: покупка листов материалов по текущему биржевому курсу (за карго-бюджет или частным заказом с доставкой шаттлом/экспрессом) и продажа своих листов в обмен на «акции» `stock_block`.

- **Код:** [`/obj/machinery/materials_market`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/materials_market.dm#L6), [`find_order()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/materials_market.dm#L88); tgui: [`MatMarket.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/MatMarket.tsx)
- **Статус в ss13-rs:** ❌ нет

### Биржевые блоки (stock block)
Блок акций за проданные материалы: его стоимость фиксируется в момент продажи и через некоторое время «ликвидируется» по текущему курсу, о чём предупреждает осмотр.

- **Код:** [`/obj/item/stock_block`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/materials_market.dm#L345), [`become_liquid()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/materials_market.dm#L381)
- **Статус в ss13-rs:** ❌ нет

### Биржа материалов (SSstock_market)
Подсистема, двигающая цену и количество каждого материала по тренду, с редкими рыночными событиями (обвал, бум).

- **Код:** [`SUBSYSTEM_DEF(stock_market)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/stock_market.dm#L2), [`handle_trends_and_price()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/stock_market.dm#L69), [`handle_market_event()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/stock_market.dm#L130)
- **BandaStation:** [`stock_market.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/stock_market.dm#L4) — тик 180 с вместо 60, коридор цен 0.8–2× вместо 0.5–3×
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/cargo/stock_market.rs` (`StockMarket`): восемь торгуемых материалов с ценой, трендом и количеством, тик раз в 180 с с коридором BandaStation, пять рыночных событий с шансом 8%, обвал рынка (`crashing`), продажа листов через биржевой экспорт снижает цену и пополняет запас; очевидные ошибки tg не перенесены (порог разворота вниз, чтение `value_per_unit` с типа, обнуление запаса после локдауна). Новости биржи только в логе — ньюскастер в ../comms/; событие «обвал рынка», ставящее флаг, — в ../events/

### Чёрный рынок: подсистема и товары
`SSmarket` держит рынки и случайный ассортимент `market_item` (оружие, снаряжение, расходники, другое), обновляет его и доставляет покупки (телепадом LTSRBT, «запуском» или капсулой).

- **Код:** [`SUBSYSTEM_DEF(market)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/market.dm#L1), [`/datum/market`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/markets/_market.dm#L1), [`/datum/market/blackmarket`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/markets/_market.dm#L91), [`/datum/market_item`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/markets/market_item.dm#L1), [`market_items/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/markets/market_items)
- **BandaStation:** [`market.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/market.dm#L123) — доставка телепорт-капсулой; [`market_items/misc.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/markets/market_items/misc.dm#L38) — кобура-аксессуар
- **Статус в ss13-rs:** ❌ нет

### Аплинк чёрного рынка
Самодельное устройство (крафт из радио): каталог по категориям, покупка за кредиты с привязанной ID, выбор способа доставки.

- **Код:** [`/obj/item/market_uplink`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/markets/market_uplink.dm#L1), [`/datum/crafting_recipe/radio_containing/blackmarket_uplink`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/markets/market_uplink.dm#L165); tgui: [`BlackMarketUplink.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/BlackMarketUplink.tsx)
- **Статус в ss13-rs:** ❌ нет

### LTSRBT (телепад чёрного рынка)
Машина, принимающая покупки чёрного рынка, и позволяющая выставить свой предмет на продажу.

- **Код:** [`/obj/machinery/ltsrbt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/markets/market_telepad.dm#L15), [`place_on_market()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/markets/market_telepad.dm#L271); tgui: [`LTSRBT.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/LTSRBT.tsx)
- **Статус в ss13-rs:** ❌ нет

### Контрабандные предметы
`TRAIT_CONTRABAND` или нахождение в аплинке предателя делает предмет контрабандой для N-spect сканеров, сканер-рамок и заказов СБ.

- **Код:** [`/obj/item/proc/is_contraband()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items.dm#L1931)
- **BandaStation:** [`modular_bandastation/contraband/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/contraband) — добавляет трейт ряду предметов (магазины 9мм, ПП, пистолеты, энергокатана, ошейник-бомба, ЭМИ-граната, топор, дефибриллятор, MOD-костюмы и др.), напр. [`code/pistol.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/contraband/code/pistol.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Почта: письма и посылки
Шаттл снабжения регулярно привозит почту для живого экипажа (`SSeconomy` копит её, `create_mail()`): письмо адресовано конкретному игроку, открыть может только он (иначе — долго и с шансом ловушки); внутри предметы по должности, деньги, купоны или спам. Письма можно сортировать дисполсалами.

- **Код:** [`/obj/item/mail`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/mail.dm#L2), [`initialize_for_recipient()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/mail.dm#L174), [`unwrap()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/mail.dm#L129), [`junk_mail()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/mail.dm#L217), [`create_mail()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/mobile_port/variants/supply.dm#L344)
- **Статус в ss13-rs:** ❌ нет

### Почтовый ящик, сумка почтальона и почтовая забастовка
Ящик `crate/mail` с письмами, сумка `bag/mail`; событие «почтовая забастовка» — особые письма.

- **Код:** [`/obj/structure/closet/crate/mail`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/mail.dm#L255), [`/obj/item/storage/bag/mail`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/mail.dm#L349), [`/obj/structure/closet/crate/mail/full/mail_strike`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/mail.dm#L330)
- **Статус в ss13-rs:** ❌ нет

### Поддельные письма (mail counterfeit device)
Устройство предателя/контрабанды: упаковать предмет в письмо на любое имя; при вскрытии не адресатом срабатывает ловушка.

- **Код:** [`/obj/item/storage/mail_counterfeit_device`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/mail.dm#L496), [`/obj/item/mail/traitor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/mail.dm#L389)
- **Статус в ss13-rs:** ❌ нет

### Сортировщик почты
Машина в карго: принимает письма и раскладывает их по отделам получателей.

- **Код:** [`/obj/machinery/mailsorter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/vending/mail.dm#L7)
- **Статус в ss13-rs:** ❌ нет

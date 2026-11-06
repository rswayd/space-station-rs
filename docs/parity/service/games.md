# Карты, кости и азартные игры
Игральные карты и колоды, кости, игровой автомат (`code/modules/cards`, `code/game/objects/items/dice.dm`, `code/game/machinery/slotmachine.dm`).

### Колода карт
Колоду тасуют, из неё тянут карту (в руку или лицом вверх), собирают карты обратно; число карт видно в осмотре. Шулерская колода Синдиката, кардшу.

- **Код:** [`/obj/item/toy/cards/deck`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/deck/deck.dm#L4), [`/obj/item/toy/cards`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/cards.dm#L4), [`/datum/deck_card`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/cards.dm#L176), [`/obj/item/toy/cards/deck/syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/deck/deck.dm#L222)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/service/cards.rs`: карты в колоде — данные (атом карты появляется, когда карта выходит из колоды); пустая рука тянет верхнюю карту рубашкой вверх, правый клик — лицом вверх; карта и рука карт возвращаются в низ колоды рубашкой вверх (лимит `card_limit`, «stacked too high»); колода в обеих руках (`two_handed`) раздаёт на карту/руку карт и тасуется alt-кликом (откат 5 с, у Синдиката 3 с); перетаскивание на себя поднимает колоду (`drag_pickup`); спрайт full/half/low/empty; метка на верхней карте в осмотре; шулерская колода Синдиката (её урон — прототип), кардшу на 432 карты. Не портировано: «52 card pickup» при броске, настроение/память карточной игры (`CardgameEvent`), рентген-зрение

### Рука карт и одиночная карта
Карты в руке веером (выбор карты радиальным меню), переворот карты, подпись пустых карт ручкой.

- **Код:** [`/obj/item/toy/cards/cardhand`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/cardhand.dm#L1), [`/obj/item/toy/singlecard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/singlecard.dm#L1)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/service/cards.rs`: карта на карту — рука карт (на месте карты или в активной руке; правый клик — добавить лицом вверх), карта в руку карт (лимит 21); рука карт в руке — список карт вместо радиального меню, из руки двух карт выходят обе; веер из 5 верхних карт оверлеями со смещением (без поворота — у оверлеев нет transform); карта в руке переворачивается (и правым кликом на полу); осмотр: число карт, свои/открытые карты, «The card reads»; ручка/мелок ставят метку цвета (белая ручка и мелок мима — невидимая), пустую карту ручкой подписывают (до 50 символов, проверка ручки и досягаемости после ответа). Не портировано: поворот карты alt-кликом (нет transform), видимость невидимой метки через reagent scanner

### Особые колоды
Таро (и проклятое таро, призывающее призраков), Cards Against Spess, Kotahi (аналог UNO), Wizoff с правилами, пустые колоды.

- **Код:** [`/obj/item/toy/cards/deck/tarot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/deck/tarot.dm#L4), [`/obj/item/toy/cards/deck/cas`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/deck/cas.dm#L6), [`/obj/item/toy/cards/deck/kotahi`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/deck/kotahi.dm#L1), [`/obj/item/toy/cards/deck/wizoff`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/deck/wizoff.dm#L2), [`/obj/item/toy/cards/deck/blank`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/deck/blank.dm#L1)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/service/cards.rs`: таро (78 карт), Kotahi (108), Wiz-Off (27 карт + карта правил сверху, правила в осмотре), пустые колоды (25 пустых карт, белая/чёрная), Cards Against Spess (150 белых / 50 чёрных `pick_n_take` из `strings/cas_*.txt`, конвертер пишет `cards.ron` — `crates/ss13_mapload/src/service.rs`); проклятое таро в обеих руках — сообщения о завесе. Чужие части: шестое чувство (`TRAIT_SIXTHSENSE`) и оповещение призраков (`notify_ghosts`) — группа призраков; не портировано: перевёрнутые карты таро (нет transform), Death/The Tower для проклятых (`TRAIT_CURSED` нет)

### Коллекционные карты (TCG)
Бустеры TCG из карго; хранилище колоды TCG показывает карты.

- **Код:** [`/datum/storage/tcg`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/storage/subtypes/others/cards.dm#L4), [`/datum/supply_pack/costumes_toys/randomised/tcg`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/packs/costumes_toys.dm#L232)
- **Статус в ss13-rs:** ❌ нет

_«Игральные кости» описано в [../items/toys.md](../items/toys.md)._

### Игровой автомат
Однорукий бандит: ставка монетами или голочипами (мультитул переключает способ оплаты), 5 барабанов, выигрыш по линиям, джекпот из банка автомата; emag «подкручивает» автомат.

- **Код:** [`/obj/machinery/computer/slot_machine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/slotmachine.dm#L22), [`spin()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/slotmachine.dm#L321), [`give_prizes()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/slotmachine.dm#L415); tgui: [`SlotMachine.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SlotMachine.tsx)
- **Статус в ss13-rs:** ❌ нет

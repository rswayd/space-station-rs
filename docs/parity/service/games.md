# Карты, кости и азартные игры
Игральные карты и колоды, кости, игровой автомат (`code/modules/cards`, `code/game/objects/items/dice.dm`, `code/game/machinery/slotmachine.dm`).

### Колода карт
Колоду тасуют, из неё тянут карту (в руку или лицом вверх), собирают карты обратно; число карт видно в осмотре. Шулерская колода Синдиката, кардшу.

- **Код:** [`/obj/item/toy/cards/deck`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/deck/deck.dm#L4), [`/obj/item/toy/cards`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/cards.dm#L4), [`/datum/deck_card`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/cards.dm#L176), [`/obj/item/toy/cards/deck/syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/deck/deck.dm#L222)
- **Статус в ss13-rs:** ❌ нет

### Рука карт и одиночная карта
Карты в руке веером (выбор карты радиальным меню), переворот карты, подпись пустых карт ручкой.

- **Код:** [`/obj/item/toy/cards/cardhand`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/cardhand.dm#L1), [`/obj/item/toy/singlecard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/singlecard.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Особые колоды
Таро (и проклятое таро, призывающее призраков), Cards Against Spess, Kotahi (аналог UNO), Wizoff с правилами, пустые колоды.

- **Код:** [`/obj/item/toy/cards/deck/tarot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/deck/tarot.dm#L4), [`/obj/item/toy/cards/deck/cas`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/deck/cas.dm#L6), [`/obj/item/toy/cards/deck/kotahi`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/deck/kotahi.dm#L1), [`/obj/item/toy/cards/deck/wizoff`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/deck/wizoff.dm#L2), [`/obj/item/toy/cards/deck/blank`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cards/deck/blank.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Коллекционные карты (TCG)
Бустеры TCG из карго; хранилище колоды TCG показывает карты.

- **Код:** [`/datum/storage/tcg`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/storage/subtypes/others/cards.dm#L4), [`/datum/supply_pack/costumes_toys/randomised/tcg`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/cargo/packs/costumes_toys.dm#L232)
- **Статус в ss13-rs:** ❌ нет

_«Игральные кости» описано в [../items/toys.md](../items/toys.md)._

### Игровой автомат
Однорукий бандит: ставка монетами или голочипами (мультитул переключает способ оплаты), 5 барабанов, выигрыш по линиям, джекпот из банка автомата; emag «подкручивает» автомат.

- **Код:** [`/obj/machinery/computer/slot_machine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/slotmachine.dm#L22), [`spin()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/slotmachine.dm#L321), [`give_prizes()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/slotmachine.dm#L415); tgui: [`SlotMachine.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SlotMachine.tsx)
- **Статус в ss13-rs:** ❌ нет

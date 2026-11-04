# Экономика: счета, зарплаты, наличные
Банковские счета экипажа и отделов, выплаты, оплата картой и наличные деньги.

### Подсистема экономики (SSeconomy)
Создаёт счета отделов и делит между ними стартовый бюджет (35000 кр), раз в 5 минут выплачивает зарплаты и доход отделам, считает инфляцию от суммы денег на станции и пересчитывает цены вендоматов, ведёт журнал аудита покупок, считает почту к доставке.

- **Код:** [`SUBSYSTEM_DEF(economy)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/economy.dm#L1), [`fire()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/economy.dm#L92), [`price_update()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/economy.dm#L180), [`inflation_value()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/economy.dm#L212)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/cargo/economy.rs`: бюджеты семи отделов (по 4999, у карго 0), зарплаты каждые 5 минут, сводка экономики в итогах раунда; `// shortcut:` нет инфляции, долгов, журнала аудита, видовых модификаторов зарплаты

### Личный банковский счёт
`/datum/bank_account` у каждого члена экипажа: баланс, номер счёта, платёжный токен, должность, долг, история операций, сообщения на ID-карту («bank_card_talk»). Переводы между счетами.

- **Код:** [`/datum/bank_account`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/economy/account.dm#L5), [`adjust_money()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/economy/account.dm#L153), [`transfer_money()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/economy/account.dm#L186), [`pay_debt()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/economy/account.dm#L167)
- **BandaStation:** [`account.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/economy/account.dm#L54) — генерация токена по кириллическому имени (`copytext_char`)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/cargo/economy.rs` (`open_job_account`, `BankAccount`): счёт с шестизначным номером при входе в игру для профессий с `paycheck_department`, 5 бесплатных зарплат, сообщения банковской карты; нет снятия наличных, переводов, долгов

### Зарплаты (payday)
Каждые 5 минут счёт получает `paycheck` своей должности из бюджета её отдела (если там хватает денег); в начале смены — стартовые 5 зарплат. Начальство и ивенты могут менять модификатор или пропускать выплаты.

- **Код:** [`payday()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/economy/account.dm#L215), [`issue_paydays()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/economy.dm#L164), [`/datum/job` `paycheck`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/_job.dm#L60)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/cargo/economy.rs` (`payday`, `departmental_payouts`): раз в 5 минут отделу 500, каждому счёту одна зарплата из бюджета отдела (не больше 50), сообщение карты; `// shortcut:` нет видовых модификаторов и долгов, карта на полу не пищит

### Счета отделов и бюджетные карты
`/datum/bank_account/department` для гражданских, инженерии, науки, медицины, сервиса, карго, СБ; бюджетная карта отдела у главы даёт платить с его счёта.

- **Код:** [`/datum/bank_account/department`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/economy/account.dm#L326), [`departmental_payouts()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/economy.dm#L148), [`/obj/item/card/id/departmental_budget`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L994)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/cargo/economy.rs` (`DEPARTMENTS`): семь бюджетов отделов, карго получает выручку от продаж и платит за заказы; бюджетных карт отделов нет

### Деньги на ID-карте
ID привязана к счёту: наличные, голочипы и монеты, вложенные в карту, зачисляются на счёт; Alt-клик снимает голочип нужного номинала; карта показывает баланс при осмотре.

- **Код:** [`/obj/item/card/id/proc/insert_money()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L599), [`click_alt()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L680), [`set_account()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L478)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/cargo/economy.rs` (`BankAccountId`, `account_of`): номер счёта на ID-карте профессии (также через КПК с картой), им оплачиваются частные заказы в консоли снабжения и покупки в торговых автоматах (`crates/ss13_game/src/vending/`); баланс в осмотре и наличные на карту — нет

### Оплата картой (компонент payment)
Компонент `payment` на машинах (вендоматы, лавки, консоли заказов): списывает цену с карты покупателя, при нехватке — отказ или оплата наличными из рук; скидка своему отделу.

- **Код:** [`/datum/component/payment`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/payment.dm#L12)
- **Статус в ss13-rs:** ❌ нет

### Наличные (space cash)
Стопки купюр номиналом 1–10000 кредитов, стоимость = номинал × количество.

- **Код:** [`/obj/item/stack/spacecash`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/cash.dm#L1), [`/obj/item/stack/spacecash/c1000`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/cash.dm#L137)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: только как разрешённый предмет хранения в слоте костюма; денежной стоимости и оплаты нет

### Голочипы
Голографический чип с произвольной суммой: снимается с ID, делится, сливается, исчезает от ЭМИ.

- **Код:** [`/obj/item/holochip`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/credit_holochip.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Монеты и покерные фишки
Монеты из металлов (стоимость по материалу), подбрасывание орёл/решка; покерные фишки разных цветов.

- **Код:** [`/obj/item/coin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/ores_coins.dm#L520), [`/obj/item/poker_chip`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/economy/poker_chips.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Стоимость предмета в кредитах
`get_item_credit_value()` — сколько денег «стоит» предмет при вкладывании в карту или оплате (наличные, голочипы, монеты).

- **Код:** [`/obj/item/proc/get_item_credit_value()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/economy/_economy.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Банковский терминал хранилища (bank machine)
Консоль в хранилище: показывает бюджет станции; её можно взломать, чтобы откачивать деньги из карго-бюджета наличными с объявлением тревоги.

- **Код:** [`/obj/machinery/computer/bank_machine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/bank_machine.dm#L1), [`ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/bank_machine.dm#L109); tgui: [`BankMachine.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/BankMachine.tsx)
- **Статус в ss13-rs:** ❌ нет

### NT Pay
Программа ПДА для переводов по платёжному токену и просмотра истории операций счёта.

- **Код:** [`/datum/computer_file/program/nt_pay`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/file_system/programs/nt_pay.dm#L8), [`make_payment()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/file_system/programs/nt_pay.dm#L58); tgui: [`NtosPay.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/NtosPay.tsx)
- **Статус в ss13-rs:** ❌ нет

### RETA — временный экстренный доступ
При экстренном вызове отдела с консоли запросов ID-картам сотрудников вызванного отдела на 5 минут выдаётся доступ отдела-источника (медицина, СБ, инженерия, наука, сервис, карго, шахта); перезарядка 15 с между вызовами, индикация на шлюзах.

- **Код:** [`initialize_reta_system()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reta/reta_system.dm#L11), [`reta_find_and_grant_access()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reta/reta_system.dm#L69), [`/obj/item/card/id/proc/grant_reta_access()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/reta/reta_id_card.dm#L13); tgui: [`RequestsConsole/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/RequestsConsole)
- **Статус в ss13-rs:** ❌ нет

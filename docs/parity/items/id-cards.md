# ID-карты и трим-доступы
`/obj/item/card/id`: доступы, привязка к владельцу, трим-шаблоны должностей и особые карты.

### ID-карта и доступ
Карта несёт список доступов (обычных и «wildcard»), имя владельца и должность (подпись «ID-карта «Имя» (Должность)»); носится в слоте ID, в руке или кошельке; двери, шкафы и автоматы проверяют её доступ. Показ карты кликом в руке.

- **Код:** [`/obj/item/card/id`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L58), [`add_access()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L403), [`add_wildcards()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L300), [`update_label()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L837), [`attack_self()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L539)
- **BandaStation:** [`cards_ids.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L1842) — санитизация вводимой должности
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs` (`IdCard`, `check_access`, `get_id`, `wallet_id`), `crates/ss13_game/src/id_access.rs` (`CardAccess`: трим, wildcard-слоты по типу карты, `add_access`/`remove_access` как у tg), `crates/ss13_game/src/outfit.rs`: доступ трима, имя и должность в подписи, проверка доступа в руках, слоте ID и кошельке, КПК отдаёт вставленную карту по ссылке; `crates/ss13_game/src/id_show.rs`: Z показывает карту окружающим, в осмотре — счёт и баланс. Нет пометки несовершеннолетнего (у карт нет возраста)

### Трим-шаблоны (`/datum/id_trim`)
Трим задаёт доступы, должность, иконку SecHUD и вид карты; трим работ добавляет доступы по конфигу (`JOBS_HAVE_MINIMAL_ACCESS`), есть тримы ЦК, ОБР, синдиката, руин, админов.

- **Код:** [`/datum/id_trim`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/id_trim/_id_trim.dm#L2), [`/datum/id_trim/job`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/id_trim/jobs.dm#L7), [`refresh_trim_access()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/id_trim/jobs.dm#L61), [`centcom.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/id_trim/centcom.dm), [`syndicate.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/id_trim/syndicate.dm), [`/datum/id_trim/away`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/id_trim/ruins.dm#L2)
- **BandaStation:** [`jobs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/id_trim/jobs.dm#L256) — часть должностей убрана из консоли ID
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/jobs.rs`: тримы работ с `minimal`/`extra` и wildcard-доступами, `template_access` (шаблоны консоли ID) и `sechud_icon_state` (shortcut: всегда малонаселённый вариант); нет тримов ЦК/синдиката/руин

### Банковский счёт карты
Карта привязана к банковскому счёту: оплата в автоматах, внесение наличных и голочипов прямо на карту, выплата долга. (Экономика — см. ../cargo/.)

- **Код:** [`set_account()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L478), [`insert_money()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L599), [`can_be_used_in_payment()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L533)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/cargo/economy.rs` (`BankAccountId`): счёт профессии привязан к ID-карте и платит за частные заказы снабжения; оплачивает покупки в торговых автоматах (`crates/ss13_game/src/vending/`); баланса в осмотре и ввода наличных нет

### Продвинутые и особые карты
Серебряные, золотые (капитанская запасная), платиновые, ЦК, ОБР, Дедсквад, синдиката, отладочная; перекраска карты мелком, статус стажёра при малом наигрыше.

- **Код:** [`/obj/item/card/id/advanced`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L1027), [`/obj/item/card/id/advanced/gold/captains_spare`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L1263), [`/obj/item/card/id/advanced/centcom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L1271), [`update_intern_status()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L1125)
- **BandaStation:** [`/obj/item/card/id/advanced/nanotrasen_official`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/cards_ids.dm#L1) — карта официального представителя НТ
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: карта, появившаяся на карте мира, получает только статический `access` (`crates/ss13_mapload/src/item.rs`, `storage.rs` `finish_item`); тримы таких карт (капитанская запасная, ЦК) не применяются, нет перекраски и стажёров

_«Карты заключённых» описано в [../security/brig-prison.md](../security/brig-prison.md)._

### Отделовские бюджетные карты
Карты бюджета отделов (карго и др.) для заказов и оплаты.

- **Код:** [`/obj/item/card/id/departmental_budget`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L994)
- **Статус в ss13-rs:** ❌ нет

### Агентская (хамелеон) карта
Карта агента: копирует доступы просканированных карт, произвольное имя и должность через tgui, маскировка под любой трим.

- **Код:** [`/obj/item/card/id/advanced/chameleon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L1584); tgui: [`ChameleonCard.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ChameleonCard.jsx)
- **Статус в ss13-rs:** ❌ нет

### Емаг и прочие карты
Емаг (взлом электроники и дверей), картонная карта, цветные карты для пейнтбола/лазертага, карта шахтёра.

- **Код:** [`/obj/item/card/emag`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/emags.dm#L10), [`/obj/item/card/cardboard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L1936), [`/obj/item/card/id/advanced/mining`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cards_ids.dm#L1531)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/emag.rs`: емаг и доорджек (`can_emag`: емаг не берёт шлюзы, окно-двери и пожарные шлюзы кроме трамвайных, доорджек — только их, 3 заряда по 3 мин, осмотр зарядов), `emag_act` портированных семейств — шлюзы, окно-двери, пожарные шлюзы, шкафы и ящики, ЛКП, воздушные тревоги, консоли снабжения и связи, торгоматы, химраздатчики, боты (`crates/ss13_game/src/bots/core.rs`); `// shortcut:` нет дальнего блюспейс-емага, склеивания емагов, поддельного емага Donk Co.; нет картонной карты, карт пейнтбола/лазертага, карты шахтёра; `emag_act` киборгов, пИИ, консоли аварийного шаттла, голодека и прочих — в своих группах

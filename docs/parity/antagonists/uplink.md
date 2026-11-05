# Аплинк Синдиката
Скрытый магазин антагониста за телекристаллы (ТК): механика аплинка и категории товаров. В ss13-rs каталог — курированный список предметов, механика которых уже есть (`crates/ss13_mapload/src/antag.rs`).

### Компонент аплинка
`/datum/component/uplink` прячется в КПК, радио или ручке; заблокирован кодом, после разблокировки открывает окно магазина; может блокироваться обратно.

- **Код:** [`/datum/component/uplink`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/uplink.dm#L10), [`/datum/component/uplink/proc/setup_unlock_code()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/uplink.dm#L376), [`/datum/component/uplink/proc/lock_uplink()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/uplink.dm#L269), [`/datum/mind/proc/give_uplink()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/antag.dm#L117)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/uplink.rs` (`give_uplink`, `Uplink`): в КПК (по умолчанию tg), без КПК — в гарнитуре; «Закрыть» блокирует обратно; `// shortcut:` предпочтение `uplink_loc` всегда КПК, ручки и импланта-запаски нет

### Способы разблокировки
КПК — ввод кода как рингтона; радио — настройка частоты-кода; ручка — поворот на заданные углы; аплинк-имплант — активация действием.

- **Код:** [`/datum/component/uplink/proc/new_ringtone()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/uplink.dm#L307), [`/datum/component/uplink/proc/new_frequency()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/uplink.dm#L328), [`/datum/component/uplink/proc/pen_rotation()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/uplink.dm#L358), [`/datum/component/uplink/proc/implant_activation()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/uplink.dm#L275), [`/datum/component/uplink/proc/new_message()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/uplink.dm#L341)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/uplink.rs` (`unlock_by_ringtone`, `unlock_by_radio`): код как рингтон мессенджера КПК; в гарнитуре — произнесением кода в `:z` вместо настройки частоты; ручки и импланта нет

### Обработчик покупок и ТК
`/datum/uplink_handler`: баланс ТК, проверки при покупке (население, роль, вид, флаги аплинка, блокировка магазина), выдача предмета в руки.

- **Код:** [`/datum/uplink_handler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/uplink_handler.dm#L6), [`/datum/uplink_handler/proc/can_purchase_item()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/uplink_handler.dm#L64), [`/datum/uplink_handler/proc/purchase_item()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/uplink_handler.dm#L92), [`/datum/uplink_handler/proc/check_if_restricted()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/uplink_handler.dm#L51)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/antag/uplink.rs` (проверки в момент покупки, `restricted_roles`, `purchasable_from`, население)

### Окно аплинка
TGUI `Uplink`: категории, поиск, компактный режим, цены и остатки, баланс ТК, блокировка.

- **Код:** [`/datum/component/uplink/proc/interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/uplink.dm#L138); tgui: [`Uplink.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Uplink/index.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/client.rs`: категории, цены, остатки, баланс (клик — вывод сырых ТК), блокировка, «Заменить задачи»; нет поиска, компактного режима и тем

### Ограниченный запас и эксклюзивные покупки
У части товаров лимит штук (`limited_stock`); некоторые покупки блокируют весь остальной магазин (`lock_other_purchases`).

- **Код:** [`/datum/uplink_item`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items.dm#L61)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/antag/uplink.rs` (`item_stock`, `shop_locked`)

### Скидки
При создании аплинка несколько случайных предметов получают скидку с ограниченным запасом (категория «Discounts»).

- **Код:** [`create_uplink_sales()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items.dm#L3), [`/datum/uplink_item/discounts`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items.dm#L236)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/antag/uplink.rs` (`create_sales`, `add_sales`): 4–6 копий со скидкой 25/50/75% (вдвое меньше от 20 ТК), по одной на категорию, id `<path>?sale`, свой запас; командные скидки нюков (`discount_team_gear`) — в [nuclear-operatives.md](nuclear-operatives.md)

### Телекристаллы как предмет
Вывод сырых ТК из аплинка (`purchase_raw_tc`), пополнение аплинка ТК-стаком, перенос ТК между аплинками.

- **Код:** [`/datum/uplink_handler/proc/purchase_raw_tc()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/uplink_handler.dm#L109), [`/datum/component/uplink/proc/load_tc()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/uplink.dm#L106), [`/obj/item/stack/telecrystal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/telecrystal.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/uplink.rs` (`purchase_raw_tc`, `item_interaction`): `buy_raw_tc` с запросом числа, стак ТК в руки; стак на активный аплинк заряжает его целиком; нет зарядки аплинк-импланта прижатием к себе (импланта-аплинка нет)

### Failsafe-код
Покупаемый код самоуничтожения: ввод его в аплинк взрывает устройство.

- **Код:** [`/datum/component/uplink/proc/failsafe()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/uplink.dm#L413)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/antag/uplink.rs` (`buy_failsafe`, `failsafe`): код (не равный коду разблокировки) рингтоном КПК или на `:z` — взрыв 1/2/3 и удаление устройства; код в панели предателя («Запасной код»); вариант с частотой и ручкой — вместе с этими способами разблокировки

### Лог покупок
Журнал покупок по владельцу (для отчёта конца раунда и админов).

- **Код:** [`/datum/uplink_purchase_log`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_purchase_log.dm#L3)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/antag/uplink.rs` (`PurchaseLog`)

### Готовые аплинк-устройства
Отдельные устройства-аплинки: радиоаплинк нюков, отладочный, клоун-опов, одиночного оперативника, «старый» (10 ТК), мультитул и ручка с аплинком — для спавна и ролей.

- **Код:** [`/obj/item/uplink`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_devices.dm#L6), [`/obj/item/uplink/nuclear`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_devices.dm#L50), [`/obj/item/uplink/loneop`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_devices.dm#L72), [`/obj/item/uplink/clownop`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_devices.dm#L75)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/uplink.rs` (`give_nuclear_uplink`): `/obj/item/uplink/nuclear` операторов (каталог `UPLINK_NUKE_OPS`, без блокировки, открывается в руке); остальных готовых устройств нет

### Категория «Инструменты и устройства»
Ящик инструментов Синдиката, подозрительная сумка, боевая аптечка, ключ шифрования, емаг, термальные очки, телепортер, доступ к ИИ-аплоаду, CRAB-17, FRAME-диск, ложный диск, стимпак и др.

- **Код:** [`/datum/uplink_item/device_tools`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/device_tools.dm#L5)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/antag.rs`: продаются toolbox, duffelbag, military_belt, tactical_medkit, encryptionkey (не вставляется), emag и doorjack (`crates/ss13_game/src/emag.rs`), стимпак (медипен, `crates/ss13_game/src/chem.rs`), failsafe-код; нет термальных очков (нужен `SEE_MOBS` отдельно от x-ray в `ss13_net`), бинарного ключа (нет смены ключей в гарнитуре), гипновспышки (нет гипноза), телепортера, CRAB-17, FRAME/камеры, ложного диска

### Категория «Скрытные инструменты»
Агентская ID-карта, детектор ИИ, хамелеон-кит, нескользящие ботинки, хамелеон-проектор, кодспик, EMP-фонарик, смена внешности (Mulligan), глушилка, сумка контрабандиста, отключение телекомов, блэкаут.

- **Код:** [`/datum/uplink_item/stealthy_tools`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/stealthy_tools.dm#L5)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/syndie_items.rs`: агентская карта (показ, подделка имени и должности, сброс; копирование доступа с другой ID-карты — сразу всё, что влезает в слоты, без окна `ChameleonCard`), syndigaloshes; нет детектора ИИ, хамелеон-кита и проектора, глушилки, кодспика, EMP-фонарика, Mulligan, блэкаутов

### Категория «Скрытное оружие»
Дротиковый пистолет, сухой карп, энергокинжал, помада, набор ядов, глушитель, сонная ручка, оригами, свиток боевых искусств, мини-арбалет, дубинка контрактора.

- **Код:** [`/datum/uplink_item/stealthy_weapons`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/stealthy.dm#L5)
- **BandaStation:** [`stealthy.dm#L63`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/stealthy.dm#L63), [`holster_uniform`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/uplink_items/code/stealthy.dm#L1) — скрытая кобура-аксессуар
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/syndie_items.rs`: энергокинжал (ручка ↔ клинок 18 урона), сонная ручка (укол вводит 20 хлоралгидрата, 15 мутотоксина, 10 тиризена), свиток спящего карпа (`crates/ss13_game/src/martial/`); нет дротикового пистолета, карпа, помады, ядов, глушителя, оригами, мини-арбалета, дубинки контрактора, кобуры

### Категория «Опасное оружие»
Пистолеты (Donksoft, Макаров, револьвер), метательное оружие, энергомеч и двойной энергомеч, силовой кулак, перчатки Северной звезды, голопаразиты.

- **Код:** [`/datum/uplink_item/dangerous`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/dangerous.dm#L6)
- **Статус в ss13-rs:** 🟡 частично — Макаров и револьвер (`crates/ss13_game/src/guns.rs`), энергомеч (`crates/ss13_game/src/antag/syndie_items.rs`: 30 урона, режущий, `WEIGHT_CLASS_HUGE`, цвет сабли; без блока, встраивания и света); нет Donksoft, метательного оружия, двойного меча, силового кулака, перчаток Северной звезды, голопаразитов

### Категория «Взрывчатка»
C-4, X-4, мешок C-4, гранаты (осколочная, EMP, дымовая), бомба-пицца, минибомба, бомба Синдиката, Earthcracker, Detomatix, слипокалипсис.

- **Код:** [`/datum/uplink_item/explosives`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/explosive.dm#L5)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/antag.rs`: продаются C-4, X-4, мешок C-4, осколочная граната, минибомба, EMP-кит (`crates/ss13_game/src/grenades.rs`, `crates/ss13_game/src/implants/`); нет бомбы Синдиката (маяк, ядро, провода), дымового кита (имплант дыма), бомбы-пиццы, Earthcracker, Detomatix, слипокалипсиса

### Категория «Боеприпасы»
Магазины 9 мм (обычные, бронебойные, экспансивные, зажигательные), спидлоадер .357, патроны Donksoft.

- **Код:** [`/datum/uplink_item/ammo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/ammunition.dm#L5)
- **Статус в ss13-rs:** ❌ нет

### Категория «Импланты»
Импланты свободы, радио Синдиката, скрытности, хранилища, аплинк-имплант.

- **Код:** [`/datum/uplink_item/implants`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/implant.dm#L6)
- **Статус в ss13-rs:** 🟡 частично — имплант свободы (`crates/ss13_game/src/implants/mod.rs`); нет радио-, стелс-, хранилища и аплинк-импланта

### Категория «Костюмы»
Инфильтраторский и обычный MODsuit Синдиката, скафандр, модули MOD (термальный визор, хамелеон, компрессия брони, антислип, амортизация, маскировка wraith).

- **Код:** [`/datum/uplink_item/suits`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/suits.dm#L7)
- **Статус в ss13-rs:** ❌ нет

### Категория «Ролевые товары»
Товары для конкретных профессий: проклятый шар-восьмёрка, вирус капитала, инжектор неуклюжести, MMI Синдиката, горячая картошка, перевёрнутый медвежий капкан, мясной крюк, турель, арбалет из арматуры, программы хирургии и т.д.

- **Код:** [`/datum/uplink_item/role_restricted`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/job.dm#L5), [`/datum/uplink_item/species_restricted`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/species.dm#L5)
- **BandaStation:** [`syndiesledge`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/uplink_items/code/role_restricted.dm#L1) — кувалда Синдиката для пролома стен
- **Статус в ss13-rs:** ❌ нет

### Категория «Понты» и наборы
Шарик, карты, сигареты, кейс с деньгами, костюмы, наклейки, постеры, спрей; наборы: случайный предмет, Syndi-kit Tactical/Special, ящик излишков, 1 сырой ТК.

- **Код:** [`/datum/uplink_item/badass`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/badass.dm#L5), [`/datum/uplink_item/bundles_tc`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/bundle.dm#L7)
- **BandaStation:** [`badass.dm#L110`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/badass.dm#L110) — удалён один товар; [`bundle.dm#L131`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/bundle.dm#L131) — капсула-телепорт вместо стильной
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/antag.rs`: только balloon

### Категории нюков и клоун-опов
Оружейные наборы, боеприпасы, подкрепления (оперативники, киборги, обезьяны), мехи, ключи базы; клоунские пироговые пушки, бананиумовые щиты/мечи, бомбы. Используются в аплинке нюков (см. [nuclear-operatives.md](nuclear-operatives.md)).

- **Код:** [`/datum/uplink_item/weapon_kits`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/nukeops.dm#L7), [`/datum/uplink_item/ammo_nuclear`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/nukeops.dm#L18), [`/datum/uplink_item/reinforcements`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/nukeops.dm#L617), [`/datum/uplink_item/mech`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/nukeops.dm#L747), [`/datum/uplink_item/weapon_kits/pie_cannon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/clownops.dm#L5)
- **BandaStation:** [`nukeops.dm#L1005`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/nukeops.dm#L1005), [`modular_bandastation/uplink_items/code/nukeops.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/uplink_items/code/nukeops.dm#L1) — тактическая кобура, кувалда
- **Статус в ss13-rs:** ❌ нет

### Особые товары
Автохирург Синдиката, яркий фонарь для вида, товары контрактора (Contract Kit, реролл, фултон, подкрепление).

- **Код:** [`/datum/uplink_item/special`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/special.dm#L5), [`/datum/uplink_item/contractor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/uplink/uplink_items/contractor.dm#L24)
- **Статус в ss13-rs:** ❌ нет

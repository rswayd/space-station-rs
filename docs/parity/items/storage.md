# Хранилища: рюкзаки, сумки, пояса, коробки
Предметы с `atom_storage` (`/datum/storage`): что и сколько в них помещается и как с ними работают.

### Механика хранилища (`/datum/storage`)
Ограничения по весовому классу предмета, числу слотов и суммарному весу, белые/чёрные списки типов, запрет вложенности крупных хранилищ; окно содержимого открывается кликом по надетому/держимому хранилищу, предмет кладётся кликом им по хранилищу, достаётся кликом по иконке.

- **Код:** [`/datum/storage`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/storage/storage.dm#L8), [`can_insert()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/storage/storage.dm#L401), [`attempt_insert()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/storage/storage.dm#L502), [`open_storage()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/storage/storage.dm#L974), [`/obj/item/storage`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/storage.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/storage.rs`: вес, слоты, суммарный вес, вложенность (крупное хранилище не лезет в меньшее), белые и чёрные списки типов и исключения по размеру — списки `set_holdable` типа хранилища (`ss13_dm_extract::holdable` → `DmHoldable`, проверка в `storage::can_insert`) для 300+ хранилищ; окно `StorageUi` (и у хранилища на полу рядом), вставка и извлечение, перетаскивание (в пустую руку, на себя — открыть, предмет из руки — внутрь, перестановка в сетке, `crates/ss13_game/src/drag.rs`), alt-клик открывает. `// shortcut:` два типа с неконстантными списками (аптечка, синдикатский дафлбэг) не ограничены; нужен `ss13_dm_extract` + `ss13-mapload convert`

### Высыпание, сбор и быстрые действия
Перетаскивание хранилища на тайл высыпает содержимое, на другое хранилище — пересыпает; режимы сбора у сумок (собрать все предметы тайла / одного типа); alt-клик и правый клик достают верхний предмет.

- **Код:** [`mass_empty()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/storage/storage.dm#L718), [`dump_content_at()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/storage/storage.dm#L862), [`handle_mass_pickup()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/storage/storage.dm#L561), [`toggle_collection_mode()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/storage/storage.dm#L1202)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/storage.rs` (`mousedrop_onto`, `dump_content_at`): перетаскивание на тайл или объект высыпает после `do_after` 2 с, на другое хранилище — пересыпает сразу; нет режимов сбора, верба `mass_empty`, quickdraw, случайного смещения высыпанного

### Рюкзаки, сумки-сэтчелы и мессенджеры
Рюкзак на спине — основное хранилище (21 вес, крупные предметы), отделовские варианты, сэтчелы и мессенджеры; рюкзак выдаётся с коробкой выживания.

- **Код:** [`/obj/item/storage/backpack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/backpack.dm#L12), [`/obj/item/storage/backpack/satchel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/backpack.dm#L300), [`/obj/item/storage/backpack/messenger`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/backpack.dm#L419)
- **BandaStation:** [`backpack.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/storage/backpack.dm) — дополнительные рюкзаки форка
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/storage.rs` (`spawn_backpack`), `crates/ss13_game/src/jobs.rs`: всем выдаётся серый рюкзак с коробкой выживания (не отделовский — shortcut в `jobs.rs`)

### Вещмешки
Дафлбэги вмещают больше, но замедляют, пока открыты; проклятый дафлбэг кусается и прирастает; отделовские и синдикатовские наборы в дафлбэгах.

- **Код:** [`/obj/item/storage/backpack/duffelbag`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/dufflebags.dm#L1), [`/obj/item/storage/backpack/duffelbag/cursed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/dufflebags.dm#L109)
- **BandaStation:** [`duffelbag.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/storage/duffelbag.dm) — дополнительные вещмешки
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/storage.rs`: как рюкзак; нет замедления на открытии и проклятия

### Блюспейс-сумка (bag of holding)
Хранилище большой ёмкости; если положить одну блюспейс-сумку в другую — образуется разрыв, поглощающий всё вокруг. Инертная сумка активируется ядром аномалии.

- **Код:** [`/obj/item/storage/backpack/holding`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/backpack.dm#L59), [`/datum/storage/bag_of_holding`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/storage/subtypes/others/bag_of_holding.dm#L1), [`/obj/item/bag_of_holding_inert`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/backpack.dm#L40)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/storage.rs`: работает как хранилище с её лимитами; нет разрыва при вложении

### Пояса
Пояса для инструментов, медицинские, СБ, шахтёрские, гранатные, оружейные, уборщика, ножны, бандольеры, поясные сумки; каждый принимает только свой список предметов, видимые предметы рисуются на поясе.

- **Код:** [`/obj/item/storage/belt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/belt.dm#L1), [`/obj/item/storage/belt/utility`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/belt.dm#L36), [`/obj/item/storage/belt/security`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/belt.dm#L259), [`/obj/item/storage/belt/sheath`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/belt.dm#L682)
- **BandaStation:** [`belt.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/belt.dm#L58) — СИ снова получает инструменты 2-го уровня; [`belt.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/storage/belt.dm) — дополнительные пояса
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/storage.rs`: пояс-хранилище с лимитами веса и слотов, каждый принимает только свой список — списки `set_holdable` типа хранилища (`ss13_dm_extract::holdable` → `DmHoldable`, проверка в `storage::can_insert`); `belt_contents` нарядов кладётся в пояс при спавне (`crates/ss13_game/src/outfit.rs` `fill_storage`); ножны и кобуры — свои записи. Нет отрисовки содержимого на поясе (оверлеи `content_overlays`)

### Кобуры
Поясные и наплечные кобуры для пистолетов, энергетического оружия и револьвера детектива.

- **Код:** [`/obj/item/storage/belt/holster`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/holsters.dm#L2), [`/obj/item/storage/belt/holster/detective`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/holsters.dm#L56)
- **BandaStation:** [`holsters.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/accessories/holsters.dm) — кобуры переделаны в аксессуары униформы (см. [bandastation-clothing.md](bandastation-clothing.md))
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/clothing/holsters.rs` (`SPECS`): кобуры BandaStation как аксессуары униформы со своим хранилищем — лимиты `/datum/storage` и списки `set_holdable` по видам (обычная, энергетическая, детектива с револьвером и спидлоадерами, нюкера и др.), открываются в руке и через комбинезон, одна на комбинезон, вес комбинезона по аксессуару; отказ хранилища — `storage.rs` (`Refusal::CantHold`). `// shortcut:` нет быстрого выхвата клавишей, звуков кобуры

### Специализированные сумки
Мешок для мусора (блюспейс-вариант), сумка для руды (собирает руду при ходьбе), для растений и семян, для листов материалов, для книг, для гильз, химическая и био-сумки, сумки для одежды глав.

- **Код:** [`/obj/item/storage/bag`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/bags.dm#L21), [`/obj/item/storage/bag/trash`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/bags.dm#L27), [`/obj/item/storage/bag/ore`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/bags.dm#L97), [`/obj/item/storage/bag/plants`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/bags.dm#L257), [`/obj/item/storage/bag/garment`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/garment.dm#L1)
- **BandaStation:** [`garment.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/garment.dm#L107) — состав сумки варден; [`garment.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/storage/garment.dm) — сумки форковых должностей
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/storage.rs`: хранилища с лимитами; нет режимов сбора и автосбора руды

### Коробки
Картонные коробки (119 типов): коробка выживания, наборы отделов, патроны, импланты, медицина, еда, одежда; складываются в плоский картон и собираются обратно.

- **Код:** [`/obj/item/storage/box`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/boxes/_boxes.dm#L2), [`/obj/item/storage/box/survival`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/boxes/job_boxes.dm#L4), [`code/game/objects/items/storage/boxes`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/boxes)
- **BandaStation:** [`boxes.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/boxes/code/boxes.dm) — новые спрайты коробок; коробки форка в [`objects/code/items/storage`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/storage)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/storage.rs` (`spawn_contents`): коробки заполняются по `PopulateContents`, открываются и принимают предметы; нет складывания в картон

### Наборы «fancy» (сигареты, пончики, яйца, свечи)
Упаковки, чей спрайт показывает число оставшихся предметов и которые открываются/закрываются: пачки сигарет, коробки пончиков, яиц, свечей, наггетсов, банка огурцов.

- **Код:** [`/obj/item/storage/fancy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/fancy.dm#L16), [`/obj/item/storage/fancy/cigarettes`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/fancy.dm#L183), [`/obj/item/storage/fancy/donut_box`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/fancy.dm#L90)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/fancy.rs`, данные `DmFancy` (`crates/ss13_mapload/src/contents.rs`): Z в руке открывает и закрывает коробку, взятие или вкладывание предмета открывает закрытую, открытая показывает число предметов на спрайте (`[base_icon_state][contents.len]`) и в осмотре, пустая складывается в картон в те же руки; наполнение по `spawn_type`/`spawn_count`, списки `set_holdable`. `// shortcut:` нет оверлеев содержимого (пончики по вкусу, сигареты и зажигалка в пачке), сигареты в рот ПКМ

### Ящики для инструментов
Тулбоксы (механический, электрический, аварийный, синдикатовский и т.д.) — хранилище и тяжёлое оружие ближнего боя.

- **Код:** [`/obj/item/storage/toolbox`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/toolboxes/_toolbox.dm#L1), [`/obj/item/storage/toolbox/mechanical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/toolboxes/mechanical.dm#L1), [`code/game/objects/items/storage/toolboxes`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/toolboxes)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/storage.rs`, `crates/ss13_game/src/combat.rs`: хранилище с содержимым по типу (`PopulateContents`), лимиты `/datum/storage/toolbox`, удар по `force`. Нет оверлеев защёлок (`has_latches`)

### Запираемые хранилища (локбоксы, кейсы)
Локбоксы открываются ID с доступом (коробки медалей, имплантов лояльности, кластербангов); защищённые кейсы — кодовым замком; ломаются емагом.

- **Код:** [`/obj/item/storage/lockbox`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/lockbox.dm#L1), [`/obj/item/storage/briefcase/secure`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/briefcase.dm#L86), [`/datum/component/lockable_storage`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/lockable_storage.dm#L7), [`/obj/item/storage/briefcase`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/briefcase.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/lockbox.rs`, `crates/ss13_mapload/src/contents.rs` (`DmLockbox`): локбоксы (медали, импланты, кластербанги, кейс заказа карго) появляются запертыми, ID с доступом (у кейса заказа — карта счёта покупателя) отпирает/запирает, запертый не открывается, не принимает и не вытряхивает (`crate::storage`), спрайт замка, емаг ломает навсегда; нет крышки коробки медалей и их оверлеев, ограничения «только медали», защищённых кейсов с кодом

### Кошельки
Кошелёк в слоте ID: держит ID-карту, деньги, фото; доступ берётся из карты внутри, на спрайте видна карта.

- **Код:** [`/obj/item/storage/wallet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/wallets.dm#L1)
- **BandaStation:** [`wallets.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/wallets.dm) — варианты кошельков форка
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs` (`wallet_id`, `get_id`, `IdCards::id_of`): кошелёк носится в слоте ID и в руках, доступ и имя берутся из первой ID-карты внутри; нет выбора «передней» карты (магнитная/агентская, больше командных допусков), суммарного доступа всех карт, карты на спрайте и в названии, alt-клика для извлечения ID

### Прочие контейнеры
Аптечки (наборы медицины — содержимое см. медицинский раздел), баночки для таблеток, упаковки банок (шестибанка), корзины, наборы аплинка.

- **Код:** [`/obj/item/storage/medkit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/medkit.dm#L11), [`/obj/item/storage/pill_bottle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/pillbottles.dm#L5), [`/obj/item/storage/cans`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/sixpack.dm#L1), [`/obj/item/storage/basket`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/basket.dm#L1), [`/obj/item/storage/box/syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/uplink_kits.dm#L28)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/storage.rs`: аптечки и баночки наполняются и открываются, баночки, корзины и наборы принимают свои списки (списки `set_holdable` типа хранилища (`ss13_dm_extract::holdable` → `DmHoldable`, проверка в `storage::can_insert`)); нет списка аптечки (в DM он не константный), упаковка банок не рисует содержимое

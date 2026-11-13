# BandaStation: прочие объекты модпака objects
Всё, что модпак `modular_bandastation/objects` добавляет помимо одежды: хранилища, напитки, химия, импланты, машины, структуры, декор.

### Хранилища форка
Рюкзаки и сумки Блюшилда, ТСФ, СССП; военные пояса фракций; вещмешки ЦК; сумки для одежды форковых глав; коробки (импланты «death alarm», дымовые гранаты, перцовые баллончики, ID-наклейки, кассеты); коробочки для колец; хирургический набор; сумка для материалов.

- **Код:** [`/obj/item/storage/backpack/blueshield`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/storage/backpack.dm#L1), [`/obj/item/storage/belt/military/army/tsf`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/storage/belt.dm#L2), [`/obj/item/storage/backpack/duffelbag/syndie/centcom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/storage/duffelbag.dm#L1), [`/obj/item/storage/bag/garment/blueshield`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/storage/garment.dm#L1), [`/obj/item/storage/fancy/ringbox`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/storage/ringbox.dm#L1), [`/obj/item/storage/medkit/surgery_kit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/storage/surgery_kit.dm#L1), [`/obj/item/storage/bag/material_pouch`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/material_pouch.dm#L1), [`objects/code/items/storage`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/storage)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/storage.rs`: как любые хранилища (лимиты, содержимое); собственные `can_hold` и спецповедение нет

### Шкафы форковых должностей
Защищённые шкафы Блюшилда, Представителя НТ, Магистрата, адвоката, отряда быстрого реагирования ЦК и шкафы «Гамма» (броня, оружие, дробовики, биозащита) для гамма-тревоги.

- **Код:** [`/obj/structure/closet/secure_closet/blueshield`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/crates_lockers/closets/secure/blueshield.dm#L1), [`/obj/structure/closet/secure_closet/nanotrasen_representative`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/crates_lockers/closets/secure/nanotrasen_representative.dm#L1), [`/obj/structure/closet/secure_closet/magistrate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/crates_lockers/closets/secure/magistrate.dm#L1), [`/obj/structure/closet/secure_closet/lawyer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/crates_lockers/closets/secure/lawyer.dm#L2), [`/obj/structure/closet/secure_closet/gamma_armor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/crates_lockers/closets/secure/gamma.dm#L1), [`/obj/structure/closet/secure_closet/tac/srt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/crates_lockers/closets/secure/centcom.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/closets.rs`: открываются по доступу, наполняются (`PopulateContents`/`generate_items_inside` из DM-данных); в DM гамма-шкафы с уровнем тревоги не связаны — они приходят гамма-шаттлом оружейной, который вызывает админ (группа админки)

### Гарнитуры и ключи форковых должностей
Гарнитуры и ключи шифрования Блюшилда, Представителя НТ, Магистрата, адвоката (канал «Правосудие»).

- **Код:** [`/obj/item/radio/headset/blueshield`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/devices/radio.dm#L9), [`/obj/item/encryptionkey/heads/magistrate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/devices/radio.dm#L52)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/radio.rs`: каналы из `keyslot` гарнитуры, ключ канала «Правосудие» `:l` (`crates/ss13_chat/src/speech.rs`)

### Глушилка НТ
Корпоративный генератор радиопомех радиусом 18, не глушащий частоту ЦК (белый список частот глушилки — правка форка в `traitordevices.dm`).

- **Код:** [`/obj/item/jammer/nt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/devices/nanotrasen_radio_jammer.dm#L1)
- **BandaStation:** [`traitordevices.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/traitordevices.dm#L325) — белый список частот
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/devices/jammer.rs`: глушилки синдиката, самодельная и НТ (радиус 12/5/18), ПКМ по глушилке в руке включает и выключает, активная не даёт рациям в радиусе на своём уровне передавать (`crates/ss13_game/src/radio.rs`, `talk_into` и ретрансляция), кроме белого списка частот (синдикат; у НТ — ЦК); у синдикатской и самодельной — волна помех в руке (микрофоны раций в поле зрения выключаются, синдикатские не трогает, перезарядка 15 с) и направленная волна по цели; у НТ их нет; нет глушения PDA-мессенджера и внутренних раций машин, осмотра «examine_more»

_«Напитки и реагенты-напитки форка» описано в [../kitchen/drinks-bar.md](../kitchen/drinks-bar.md)._

### Кокаин, крэк и трамадол
Реагенты и реакции кокаина (порошок, фрибейс), предметы-дозы и кирпичи крэка, самокрутка с крэком; обезболивающее трамадол.

- **Код:** [`/datum/reagent/drug/cocaine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/chems/drugs/cocaine_chem.dm#L26), [`/obj/item/reagent_containers/cocaine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/chems/drugs/cocaine_item.dm#L32), [`/obj/item/cigarette/crackrollie`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/chems/drugs/crack_pipe.dm#L1), [`/datum/reagent/medicine/tramadol`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/chems/painkillers/painkillers.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_reagents`: реагенты и реакции загружаются и метаболизируются; эффектов нет, предметов-доз нет

### Импланты и кибернетика форка
Имплант «death alarm» (оповещение о смерти по радио), имплант защиты от ЭМИ, кибер-насос в грудь (ЦК, с санфентанилом), набор инструментов в руку ЦК и настраиваемый, экранированный HUD СБ в глаза, тактический визор ЦК. Механика имплантов — см. ../security/.

- **Код:** [`/obj/item/implant/death_alarm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/implants/death_alarm.dm#L1), [`/obj/item/implant/empprotection`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/implants/empprotection.dm#L1), [`/obj/item/organ/cyberimp/chest/pump`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/implants/pump.dm#L8), [`/obj/item/organ/cyberimp/arm/toolkit/centcom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/implants/cyberimp_arm.dm#L1), [`/obj/item/organ/cyberimp/eyes/hud/security/shielded`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/implants/cyberimp_eye.dm#L1), [`/obj/item/organ/eyes/robotic/tacvisor/centcom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/centcom_tacvisor.dm#L12)
- **Статус в ss13-rs:** ❌ нет

### Усиленные протезы (strongarm/strongleg)
Протезы рук и ног: мощные броски, особые удары и пинки.

- **Код:** [`/datum/component/strongarm_combat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/bodyparts/strongarm_bodypart.dm#L16), [`/datum/component/strongleg_combat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/bodyparts/strongleg_bodypart.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Чайник
Настольная машина: греет или охлаждает напиток в стакане до фиксированной температуры; собирается из платы.

- **Код:** [`/obj/machinery/teapot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/machinery/teapot.dm#L1), [`/obj/item/circuitboard/machine/teapot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/machine_circuitboards.dm#L1), [`/datum/design/board/teapot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/machine_designs.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/teapot.rs`: стакан внутрь/наружу (ПКМ), меню температуры (±30 K, 183–363 K) и включения, нагрев `(цель − T) × 0.45 × сек × теплоёмкость` каждый машинный тик и шаг реакции, выключение на цели, ключ/отвёртка/лом и сборка из платы (`crate::machine_frame`, семейство `Teapot`); нет доступа ИИ/боргов к меню

### Шредер бумаги
Машина уничтожает бумагу в обрывки. (Бумага — см. ../service/.)

- **Код:** [`/obj/machinery/papershredder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/machinery/papershredder.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/papershredder.rs`: фото, бумага, газеты, ID-карты, папки, книги уничтожаются в корзину (до 15), ПКМ пустой рукой выдаёт обрывок, ключ откручивает, спрайт по заполнению; подтипы считаются (в DM точное `type in` отвергает все ID-карты — баг не перенесён); обрывки не горят (поджиг предметов — другая группа)

### Торговые автоматы форка
Автоматы НТ (снаряжение для должностей форка), гардероб роботехника и др. (Механика вендоров — см. ../cargo/.)

- **Код:** [`/obj/machinery/vending/nta`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/machinery/vending/vending.dm#L28), [`/obj/machinery/vending/wardrobe/robo_wardrobe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/machinery/vending/vending.dm#L4)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/vending/`: автоматы с ассортиментом из DM-данных, оплата, канистры, разборка (механика — ../cargo/vending.md)

### Прочие машины и техника
Синдикатовский ксерокс, хранилище скафандров Блюшилда, уникальный код у каждой ядерной бомбы, сломанный крематорий, мехи ОБР, ядро меха, КПК офицера ЦК.

- **Код:** [`/obj/machinery/photocopier/syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/machinery/photocopier.dm#L1), [`/obj/machinery/suit_storage_unit/blueshield`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/machinery/suit_storage_unit.dm#L1), [`/obj/machinery/nuclearbomb/Initialize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/machinery/nuclear_bomb.dm#L1), [`morgue.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/morgue.dm), [`mecha.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/mecha/mecha.dm), [`mech_core.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/mech_core.dm), [`role_tablet_presets.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/role_tablet_presets.dm)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/nukeop/bomb.rs`: у каждой ядерной бомбы свой случайный код с загрузки карты; синдикатовского ксерокса, хранилища скафандров Блюшилда, крематория (морга нет — ../medical/), мехов ОБР, ядра меха и КПК ЦК нет

### Мебель, фальшивые лампы и платформы
Барные табуреты, тактические и корпоративные кресла, витрины капитана и ГСБ, декоративные лампы без питания (в т.ч. разбитые, аварийные, цветные), платформы (ограждения-перила, усиленные).

- **Код:** [`/obj/structure/chair/stool/bar/dark`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/chairs.dm#L1), [`/obj/structure/displaycase/captain`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/displaycase.dm#L1), [`/obj/structure/light_fake`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/fake_light.dm#L5), [`/obj/structure/railing/platform`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/platform.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload`: рисуются на карте, перила-платформы — граничные объекты (`crates/ss13_mapload/src/border.rs`); нет сидения и витрин; фальшивые лампы светят только через статические `light_*`

### Флаги, плакаты, таблички и статуи
Флаги НТ, ТСФ и др. (сворачиваются/поджигаются), ~40 контрабандных и легальных плакатов, указатели трамвая, статуи (Фемида, карта станции «Кибериада»), знамёна отделов (поднимают дух).

- **Код:** [`/obj/structure/sign/flag`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/flags.dm#L2), [`posters.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/posters.dm), [`signs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/signs.dm), [`/obj/structure/statue/themis`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/structures/statues.dm#L2), [`/obj/item/banner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/banners.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/decor.rs`: флаг складывается ПКМ в свой сложенный флаг в руки (ключ и сварка его не трогают), плакаты срываются/снимаются/вешаются, статуи откручиваются и режутся; нет поджигания флагов, вешания сложенного флага обратно (нет настенных рамок) и знамён (механика знамён апстрима — ../service/)

### Декали, деревянные полы, спавнеры и маяки трамвая
Логотипы НТ/Синдиката/SolGov на полу, золотые бордюры, деревянные полы и плитка, случайные спавнеры (ID-наклейки, советская форма), маяки платформ трамвая «Кибериады».

- **Код:** [`/obj/effect/decal/nt_logo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/effects/decal.dm#L9), [`turf_decal.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/effects/turf_decal.dm), [`/turf/open/floor/wood`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/floors/wooden.dm#L78), [`spawners.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/effects/spawners.dm), [`landmark.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/effects/landmark.dm)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload`: декали и полы рисуются, случайные спавнеры раскрываются (`crates/ss13_mapload/src/spawner.rs`); трамвая нет

### Музыкальные инструменты форка
Электрогитары и бас Soundhand, барабаны. (Инструменты — см. ../service/.)

- **Код:** [`/obj/item/instrument/soundhand_metal_guitar`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/instruments.dm#L33)
- **Статус в ss13-rs:** ❌ нет

### Щиты, печати, бумаги и золотая кружка
Противоударные щиты СССП, печати Представителя НТ, Магистрата, ТСФ, СССП, бумаги ЦК, золотая кружка.

- **Код:** [`/obj/item/shield/riot/ussp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/shields.dm#L1), [`/obj/item/stamp/magistrate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/stamp.dm#L6), [`papers.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/papers.dm), [`/obj/item/reagent_containers/cup/golden_mug`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/storage/reagent_containers.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`, `crates/ss13_game/src/chem.rs`: предметы поднимаются, кружка — контейнер реагентов; печатей и щитов как механик нет

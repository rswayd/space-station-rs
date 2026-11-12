# BandaStation: одежда форка
Одежда и механики одежды из модпака `modular_bandastation/objects` (`code/items/clothing`) и `aesthetics/clothing`.

### Форма форковых должностей
Униформа, головные уборы, костюмы и зимние куртки Блюшилда, Представителя НТ, Магистрата, ЦК (адмирал, дипломат, офицер), корпоративная форма СБ, роботехники и прочие альтернативы отделов, бронированный комбинезон плазмена-Блюшилда.

- **Код:** [`/obj/item/clothing/under/rank/blueshield`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/under/jobs/blueshield.dm#L10), [`/obj/item/clothing/under/rank/nanotrasen_representative`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/under/jobs/nanotrasen_representative.dm#L1), [`/obj/item/clothing/under/rank/magistrate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/under/jobs/magistrate.dm#L1), [`/obj/item/clothing/under/rank/centcom/admiral`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/under/jobs/centcom.dm#L4), [`/obj/item/clothing/under/rank/security/officer/corporate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/under/jobs/security.dm#L1), [`research.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/under/jobs/research.dm), [`/obj/item/clothing/head/hats/nanotrasen_representative`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/head/jobs.dm#L2), [`/obj/item/clothing/suit/toggle/nanotrasen_representative`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/suits/jobs.dm#L2), [`/obj/item/clothing/suit/hooded/wintercoat/blueshield`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/suits/wintercoats.dm#L14), [`/obj/item/clothing/under/plasmaman/blueshield`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/under/jobs/plasmaman/blueshield.dm#L7)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_mapload` берёт типы из полного `tgstation.dme` (с модпаками), `crates/ss13_game/src/inventory.rs`: надеваются как обычная одежда; капюшоны и `toggle` (`crates/ss13_game/src/clothing/toggles.rs`); броня — общая механика (`DmArmor` из `armor_type`, `crates/ss13_game/src/armor.rs`), в т.ч. у бронированного комбинезона плазмена-Блюшилда

### Форма фракций (ТСФ, СССП, Etamin Industries, синдикат)
Униформа и наряды ТСФ (морпехи, командир), СССП (солдат, командир, чиновник), корпорации Etamin, русские армейские камуфляжи синдиката.

- **Код:** [`/obj/item/clothing/under/rank/tsf`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/under/jobs/tsf.dm#L2), [`/obj/item/clothing/under/rank/ussp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/under/jobs/ussp.dm#L2), [`/obj/item/clothing/under/rank/etamin_ind`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/under/jobs/etamin_ind.dm#L2), [`/obj/item/clothing/under/syndicate/rus_army`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/under/syndicate.dm#L4)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/inventory.rs`: надеваются как обычная одежда; броня — общая механика (`DmArmor` из `armor_type`, `crates/ss13_game/src/armor.rs`)

### Броня, плащи и куртки форка
27 видов брони (ЦК, Блюшилд со сменой облика, ОБР), 22 плаща с капюшонами и накидки (роботехник, ЦК, отделы), куртки-«бомберы», куртки по мотивам игр, карнавальные и новогодние костюмы, костюм акулы.

- **Код:** [`/obj/item/clothing/suit/armor/centcom_formal/field`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/suits/armor.dm#L4), [`/obj/item/clothing/suit/hooded/roboticist_cloak`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/suits/cloaks.dm#L4), [`/obj/item/clothing/suit/jacket/bomber`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/suits/bomber.dm#L1), [`/obj/item/clothing/suit/v_jacket`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/suits/jackets.dm#L1), [`/obj/item/clothing/suit/hooded/shark_costume`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/suits/suits.dm#L1), [`/obj/item/clothing/suit/carnival`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/suits/carnival.dm#L1), [`/obj/item/clothing/suit/ny_sweater`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/suits/holiday.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: надеваются; капюшоны (`crates/ss13_game/src/clothing/toggles.rs`); нет брони, смены облика (`atom_skin`)

### Головные уборы, маски, очки, обувь и шейные вещи форка
29 шляп и беретов, 13 шлемов (байкерский, хардсьют СБ, крестоносец ОБР), кепки, карнавальные маски, дыхательные маски-шарфы (GAGS), HUD-очки СБ ЦК/ТСФ и термальная повязка ТСФ, ботинки-акулы и ЦК, плащи-накидки, гирлянды.

- **Код:** [`/obj/item/clothing/head/cowboy/roboticist`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/head/hat.dm#L4), [`/obj/item/clothing/head/helmet/biker_helmet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/head/helmet.dm#L1), [`/obj/item/clothing/mask/breath/breathscarf`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/head/mask.dm#L12), [`/obj/item/clothing/mask/carnival`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/masks/carnival.dm#L8), [`/obj/item/clothing/glasses/hud/security/sunglasses/soo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/glasses/glasses.dm#L2), [`/obj/item/clothing/shoes/shark`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/shoes/shoes.dm#L4), [`/obj/item/clothing/neck/crow`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/neck/fancy.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`, `crates/ss13_game/src/internals.rs`: надеваются, дыхательные маски работают с баллоном; нет HUD, термального зрения, GAGS-спрайтов на мобе

### Кобуры-аксессуары
Кобуры (16 видов: энергетические, тактические, детектива, хамелеон, ОБР) крепятся на униформу как аксессуар: хранят один пистолет, доступны, только если не закрыты верхней одеждой, утяжеляют униформу. Заменяют поясные кобуры в шкафах, нарядах и наборах аплинка.

- **Код:** [`/obj/item/clothing/accessory/holster`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/accessories/holsters.dm#L8), [`can_access_holster()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/accessories/holsters.dm#L46), [`is_accessory_covered()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/accessories/holsters.dm#L1)
- **BandaStation:** [`_under.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/_under.dm#L462) — вес униформы с кобурой
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/clothing/holsters.rs`: лимиты и `set_holdable` всех кобур, разгрузки и кармашка (`create_storage` в `Initialize` — таблицей), содержимое `PopulateContents`/`Initialize`; на форме кобура открывается кликом по форме и принимает предмет из руки (`crate::storage::storage_of`), одна кобура на форму («isn't compatible!»), детективная держит один ствол; вес формы — по самому тяжёлому аксессуару; `accessory` нарядов (ОБР, ТСФ, СССП, ветеран) крепится к форме (`crate::outfit`). Нет: быстрого выхватывания по хоткею (`quick_accessory_draw` — хоткеев нет, ui), `TRAIT_GUNFLIP`, звуков кобуры

### Нательная камера
Аксессуар-камера: видна в сети камер станции, только пока её смотрят; включается кнопкой, ломается ЭМИ и чинится кабелем.

- **Код:** [`/obj/item/clothing/accessory/bodycam`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/accessories/bodycam.dm#L8)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/silicon/mob_cameras.rs`: камера на форме надевшего даёт ему `pausable_bodycam` «!<имя> (Нательная камера)» в сети `ss13` (обзор 3), в сети — только пока её смотрит консоль и она включена (алерт «Нательная камера активна»); кнопка вкл/выкл, ЭМИ ломает, кабель чинит, консоль теряет выключенную; нет продажи подписчикам (`prime_only`) и `rotate_cam`

### Аксессуары перчаток (кольца)
На перчатки надеваются кольца (бриллиантовое, серебряное и др.) — рисуются поверх перчаток, снимаются alt-кликом.

- **Код:** [`/obj/item/clothing/gloves`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/gloves/gloves.dm#L1), [`/obj/item/clothing/accessory/gloves_accessory`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/accessories/gloves_accessories.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/clothing/accessories.rs`: кольцо крепится к перчаткам (одно, по `attachment_slot`), рисуется поверх надетых перчаток, alt+ПКМ снимает, осмотр «Имеет прикрепленное», высыпается при разрушении; на форму кольца не крепятся. `TRAIT_FINGERPRINT_PASSTHROUGH` кольца — к криминалистике (forensics)

### Перчатки форка
Боевые перчатки ЦК и дипломата, криминалистические перчатки и др.

- **Код:** [`/obj/item/clothing/gloves/combat/centcom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/gloves/gloves.dm#L131)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: надеваются, изоляция по `siemens_coefficient`

### Разгрузочные жилеты
Аксессуар-разгрузка с карманами для патронов (своё хранилище).

- **Код:** [`/obj/item/clothing/accessory/ammo_vest`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/accessories/accessories.dm#L8)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/clothing/holsters.rs`: 4 слота, 9 веса, только магазины и спидлоадеры; на форме — как кобура, несовместима с другим хранилищем на форме

### HEV-костюм
Скафандр в стиле Half-Life: голосовые оповещения о состоянии носителя (урон, смерть, огонь, речь).

- **Код:** [`/obj/item/clothing/suit/space/hev`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/spacesuits/hev_suit.dm#L4), [`add_queue()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/spacesuits/hev_suit.dm#L69)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/breath.rs`: защита как у космического скафандра; голосовых оповещений нет (нет звука)

### Модульные скафандры фракций
MOD-темы и готовые MODsuit-ы: преторианец и спецоперации НТ, ТСФ, СССП, скреллы (Раскинта, гвардия императора); модуль-монитор. Механика MODsuit — см. ../science/.

- **Код:** [`/datum/mod_theme/praetorian`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/mod/types/nanotrasen/mod_theme.dm#L1), [`/datum/mod_theme/tsf_standart`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/mod/types/tsf/mod_theme.dm#L1), [`/datum/mod_theme/ussp_standart`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/mod/types/ussp/mod_theme.dm#L1), [`/datum/mod_theme/skrell_raskinta`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/mod/types/skrell/mod_theme.dm#L1), [`/obj/item/mod/module/monitor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/mod/modules/monitor_module.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/modsuit.rs`: темы и готовые MODsuit-ы форка (преторианец, спецоперации, ТСФ, СССП, скреллы) конвертируются из полного `tgstation.dme` как tg-шные (броня, скины, модули, пины), механика — `crates/ss13_game/src/modsuit`. Нет: модуль-монитор ставится и тратит энергию, но не выдвигает `sensor_device` в руку — у MOD-модулей нет `device` (общий пробел modsuit, ../science/)

### Ножны и сабля капитана со сменой облика
Капитанские ножны и сабля выбирают облик (классика, церемониальная, казачья, катана).

- **Код:** [`/datum/atom_skin/cap_sheath`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/belt.dm#L1), [`/datum/atom_skin/cap_sabre`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/sabre.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/clothing/reskin.rs`: ножны показывают `-full` (иконка, в руке, на поясе), пока в них что-то есть; alt-клик по ножнам сабли с саблей внутри — список обликов (Ceremonial, Classic, Cossack, Default, Katana), выбор перекрашивает ножны и саблю (иконка, in-hand файлы и состояние, worn), опция одноразовая; пустые ножны — «You need to have a sabre in the sheath to reskin it.»; выбор перепроверяется (в досягаемости, сабля внутри, может действовать). `// shortcut:` список вместо радиального меню с превью; выхватывание клинка alt-кликом (tg) не портировано — alt-клик после смены облика открывает хранилище

### Женские версии униформ
Форк хранит оверрайды женской обрезки униформ (`female_version`).

- **Код:** [`/obj/item/clothing/under`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/under/under.dm#L1)
- **BandaStation:** [`_under.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/_under.dm#L367) — маска применяется и к подкатанной форме
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/human.rs`: «a female body wears the uniform uncropped»; обрезка — попиксельная маска `female_full` (`wear_female_version`), оверлеи её не выражают: нужен рендер маски на клиенте или маскированные спрайты при импорте (render/assets, ../clothing-core.md)

### GAGS-конфиги одежды форка
Конфиги перекраски (GAGS) для форковой одежды.

- **Код:** [`greyscale_clothes.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/greyscale/config_types/greyscale_clothes.dm)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_assets/src/usage.rs` (`gags`), `crates/ss13_assets/src/gags.rs`, `crates/ss13_mapload/src/gags.rs`: конфиги `/datum/greyscale_config/*` форка берутся из полного `tgstation.dme` вместе с tg-шными (подтипы `/worn` наследуют `json_config`), рендерятся импортом в цветах типа; иконка, worn- и in-hand-файлы вещи указывают на отрендеренные (17 JSON форка, `ICON-SOURCES.txt`: `gags:/datum/greyscale_config/carnival_*`). Перекраска на лету — ../clothing-core.md («Меню перекраски GAGS»)

### Ресурсы одежды ЦК (aesthetics)
Новые спрайты формы, беретов, масок, брони, курток и MOD ЦК.

- **Код:** [`centcom.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/clothing/centcom/code/under/centcom.dm), [`hat.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/clothing/centcom/code/head/hat.dm), [`armor.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/clothing/centcom/code/suits/armor.dm), [`mod_theme.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/clothing/centcom/code/mod/mod_theme.dm)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_dm_extract`, `crates/ss13_assets`: `icon`/`worn_icon` берутся из полного `tgstation.dme` (модпаки включены), спрайты импортируются как любые другие

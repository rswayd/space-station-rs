# Одежда: верхняя одежда и скафандры
Костюмы слота `suit` (`/obj/item/clothing/suit`), бронежилеты и космические скафандры.

### Бронежилеты и тяжёлая броня
Жилеты СБ, броня ГСБ, противоударная, пуленепробиваемая и лазеростойкая броня, SWAT, тяжёлая, ЦК и прочие; основная защита от урона, в хранилище костюма помещаются оружие и спецсредства.

- **Код:** [`/obj/item/clothing/suit/armor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/armor.dm#L1), [`/obj/item/clothing/suit/armor/vest`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/armor.dm#L47), [`/obj/item/clothing/suit/armor/riot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/armor.dm#L298), [`/obj/item/clothing/suit/armor/bulletproof`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/armor.dm#L366), [`/obj/item/clothing/suit/armor/laserproof`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/armor.dm#L386)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: надевается, `allowed` для хранилища костюма работает; броня не снижает урон мобу (см. [clothing-core.md](clothing-core.md))

### Реактивная броня
Аномальная броня с ядром: при попадании срабатывает эффект (телепорт, огонь, невидимость, тесла, отталкивание, стол, галлюцинации), ЭМИ вызывает сбой.

- **Код:** [`/obj/item/clothing/suit/armor/reactive`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/reactive_armour.dm#L34), [`/obj/item/reactive_armor_shell`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/reactive_armour.dm#L1)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/clothing/reactive.rs`: ядро аномалии в оболочку, включение в руке, 50% реакции на удар в ближнем бою и снаряд с блоком, перезарядка 10 с, ЭМИ на 30 с; эффекты телепорта, огня, невидимости, теслы, отталкивания, стола, галлюцинаций; биоскрамблер, баррикада, эктоплазма и погода создаются, но не срабатывают (их механики не портированы)

### Халаты, куртки и рабочая одежда
Лабораторные халаты (расстёгиваются), куртки, фартуки, сигнальные жилеты, плащи детектива, костюмы адвоката, куртки СБ, пальто моли и эфириалов, рубашки.

- **Код:** [`/obj/item/clothing/suit/toggle/labcoat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/labcoat.dm#L1), [`/obj/item/clothing/suit/jacket`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/jacket.dm#L1), [`/obj/item/clothing/suit/apron`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/jobs.dm#L6), [`/obj/item/clothing/suit/hazardvest`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/jobs.dm#L183), [`/obj/item/clothing/suit/toggle/jacket/det_trench`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/jobs.dm#L129), [`/obj/item/clothing/suit/costume/wellworn_shirt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/shirt.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: надеваются, хранилище костюма по `allowed`; расстёгиваются alt-кликом (`crates/ss13_game/src/clothing/toggles.rs`)

### Зимние куртки с капюшоном
Зимние куртки всех отделов с капюшоном и защитой от холода; EVA-вариант, капитанская, ЦК.

- **Код:** [`/obj/item/clothing/suit/hooded/wintercoat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/wintercoats.dm#L2), [`/obj/item/clothing/head/hooded/winterhood`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/wintercoats.dm#L67)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/breath.rs`: теплоизоляция куртки учитывается; капюшон кнопкой (`crates/ss13_game/src/clothing/toggles.rs`); нет EVA-особенностей

### Защитные костюмы (био, огнеупорный, сапёрный, радиационный)
Биокостюмы с капюшоном (общий, вирусолог, СБ, уборщик), огнеупорные костюмы пожарных и атмосферщиков, сапёрный костюм, радиационный костюм.

- **Код:** [`/obj/item/clothing/suit/bio_suit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/bio.dm#L32), [`/obj/item/clothing/suit/utility/fire`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/utility.dm#L17), [`/obj/item/clothing/suit/utility/bomb_suit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/utility.dm#L129), [`/obj/item/clothing/suit/utility/radiation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/utility.dm#L214)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/breath.rs`, `crates/ss13_game/src/fire.rs`: тепловая защита (огнеупорные); броня `bomb` защищает от взрывов (`crates/ss13_game/src/explosions/ex_act.rs`); радзащита радкостюма с капюшоном (`crates/ss13_game/src/radiation.rs`); нет биозащиты, замедления

### Костюмы и маскарад
Костюмы для вечеринок и праздников: фальшивый синдикатовский скафандр, костюмы животных, призрачная простыня, знак мокрого пола (носится как костюм) и т.п.

- **Код:** [`/obj/item/clothing/suit/costume`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/costume.dm#L1), [`/obj/item/clothing/suit/costume/ghost_sheet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/ghostsheet.dm#L1), [`/obj/item/clothing/suit/caution`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/wetfloor.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: надеваются; специальные эффекты отдельных костюмов нет

### Смирительная рубашка
Связывает руки носителя, как наручники; снимается долго.

- **Код:** [`/obj/item/clothing/suit/jacket/straight_jacket`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/straightjacket.dm#L1)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/restraints.rs`: надетая рубашка связывает руки и прекращает тягу, сопротивление 5 минут снимает её; при снятии выпадает то, что в руках; задержки надевания (5 с) и снятия (6 с) общие для инвентаря и не портированы (см. [clothing-core.md](clothing-core.md))

### Лазертаг
Синие и красные жилеты для лазертага: попадание вражеским лазертаг-оружием «выбивает» носителя.

- **Код:** [`/obj/item/clothing/suit/bluetag`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/lasertag.dm#L1), [`/datum/component/lasertag`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/_lasertag_components.dm#L2)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/clothing/lasertag.rs`: команда по жилету, лазертаг-ударник стреляет только для своей команды (`crates/ss13_game/src/guns.rs`), чужой лазертаг-луч снимает 34 выносливости (`crates/ss13_game/src/projectile.rs`)

### Аблятивный плащ
Плащ с капюшоном, отражающий лазеры; в капюшоне — HUD СБ.

- **Код:** [`/obj/item/clothing/suit/hooded/ablative`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/suits/ablativecoat.dm#L27)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/clothing/ablative.rs`: отражение `reflectable` снарядов с шансом 50% по закрытым зонам (плащ — тело, капюшон — голова); HUD СБ в капюшоне (`crates/ss13_game/src/records/hud.rs`); отражение предметами в руках — у оружия

### Космические скафандры (базовые и EVA)
Скафандр и шлем защищают от вакуума и холода; терморегулятор на батарее держит температуру тела, батарея меняется; надевание чужому занимает 5 с; в шлем нельзя надеть шапку сверху (стабилизатор).

- **Код:** [`/obj/item/clothing/suit/space`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/spacesuits/_spacesuits.dm#L81), [`/obj/item/clothing/head/helmet/space`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/spacesuits/_spacesuits.dm#L6), [`toggle_spacesuit()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/spacesuits/_spacesuits.dm#L304), [`/obj/item/clothing/suit/space/eva`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/spacesuits/softsuit.dm#L31)
- **BandaStation:** [`_spacesuits.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/spacesuits/_spacesuits.dm#L96) — в хранилище скафандра помещается любой джетпак
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/breath.rs`: защита от давления и холода, терморегулятор; батарея не расходуется и не меняется, нет задержек надевания, замедления, HUD-иконки батареи

### Специальные скафандры
Синдикатовские (разноцветные), пиратские, «Свобода», охотника за головами, Санты, спецназа ЦК, хрупкие аварийные и NASA-скафандры.

- **Код:** [`/obj/item/clothing/suit/space/syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/spacesuits/syndi.dm#L36), [`/obj/item/clothing/suit/space/pirate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/spacesuits/pirate.dm#L26), [`/obj/item/clothing/suit/space/freedom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/spacesuits/freedom.dm#L25), [`/obj/item/clothing/suit/space/hunter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/spacesuits/bountyhunter.dm#L1), [`/obj/item/clothing/suit/space/santa`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/spacesuits/santa.dm#L20), [`/obj/item/clothing/suit/space/officer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/spacesuits/specialops.dm#L39), [`/obj/item/clothing/suit/space/fragile`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/spacesuits/softsuit.dm#L81)
- **BandaStation:** [`specialops.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/spacesuits/specialops.dm#L15) — снаряжение спецопераций ЦК ослаблено (не космическое)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/breath.rs`: как любой `/obj/item/clothing/suit/space`; особенностей нет

### Скафандры плазменов
EVA-костюм плазмамена с самотушением и шлемы с фонарём по должностям.

- **Код:** [`/obj/item/clothing/suit/space/eva/plasmaman`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/spacesuits/plasmamen.dm#L3), [`/obj/item/clothing/head/helmet/space/plasmaman`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/spacesuits/plasmamen.dm#L74)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/clothing/envirosuit.rs`: самотушение EVA-костюма (10 зарядов) и комбинезона плазмамена (5), перезарядка 10 с, заряды при осмотре, картридж-перезаправка; нет шлемов с фонарём, сварочным визором и рисунком мелком; шлем работает как `HEADINTERNALS` (`crates/ss13_game/src/internals.rs`)

MODsuit-ы (модульные скафандры) — см. ../science/.

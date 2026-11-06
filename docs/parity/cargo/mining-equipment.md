# Шахтёрское снаряжение и лаваленд
Инструменты шахтёра, вспомогательные устройства, вендор снаряжения, шаттл и база, лут лаваленда.

### Кирки, дрели и лопаты
Кирки (ржавая, мини, серебряная, алмазная, самодельная), дрели и алмазная дрель, отбойный молоток (звуковой); лопата и садовая лопатка копают песок/снег. Скорость копания по `toolspeed`.

- **Код:** [`/obj/item/pickaxe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/mining_tools.dm#L2), [`/obj/item/pickaxe/drill`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/mining_tools.dm#L65), [`/obj/item/pickaxe/drill/jackhammer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/mining_tools.dm#L89), [`/obj/item/shovel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/mining_tools.dm#L112)
- **Статус в ss13-rs:** 🟡 `ss13_game::mining::rocks`: кирки, дрели и отбойный молоток (`TOOL_MINING`) копают минеральные стены кликом или шагом в стену за `tool_mine_speed × toolspeed`; нет копания песка/снега лопатами, звуков инструментов, навыка шахтёра

### Шахтёрские сканеры
Ручной и автоматический (продвинутый) сканеры подсвечивают руду в стенах вокруг на несколько секунд.

- **Код:** [`/obj/item/mining_scanner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/mineral_scanner.dm#L2), [`/obj/item/t_scanner/adv_mining_scanner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/mineral_scanner.dm#L36)
- **Статус в ss13-rs:** 🟡 `ss13_game::mining::rocks`: сканер только обезвреживает активный гибтонит; подсветки руды в стенах нет

### Резонатор
Ставит резонансные поля, которые через паузу взрываются, ломая породу и раня фауну; улучшенная версия.

- **Код:** [`/obj/item/resonator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/resonator.dm#L3), [`/obj/effect/temp_visual/resonance`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/resonator.dm#L58)
- **Статус в ss13-rs:** ❌ нет

### Кинетический крушитель и трофеи
Двуручное оружие шахтёра: выстрел дестабилизатором метит цель, удар по метке наносит бонусный урон в спину; трофеи с фауны и мегафауны дают особые эффекты; скины (меч, гарпун, кинжал, глефа). Кинетический акселератор (ПКА) — см. боевое оружие (не этот каталог); его перезарядка и бурение стен — `ss13_game::mining::rocks`.

- **Код:** [`/obj/item/kinetic_crusher`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/kinetic_crusher/kinetic_crusher.dm#L8), [`/obj/item/crusher_trophy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/kinetic_crusher/kinetic_crusher_trophies.dm#L5), [`trophies_fauna.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/kinetic_crusher/trophies_fauna.dm#L6), [`trophies_megafauna.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/kinetic_crusher/trophies_megafauna.dm#L6)
- **Статус в ss13-rs:** ❌ нет

### Маркерные маяки и маяки Candela
Цветные светящиеся маркеры для разметки пути; сеть маяков Candela (освещение/связь маршрута).

- **Код:** [`/obj/item/stack/marker_beacon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/marker_beacons.dm#L17), [`/obj/structure/marker_beacon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/marker_beacons.dm#L69), [`/obj/item/stack/candela_beacon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/candela_network/candela_beacons.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Фултон-эвакуация
Фултон-пакет с привязкой к точке эвакуации поднимает предмет/моба на воздушном шаре и переносит к маяку.

- **Код:** [`/obj/item/extraction_pack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/fulton.dm#L3), [`/obj/structure/extraction_point`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/fulton.dm#L301), [`/obj/item/fulton_core`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/fulton.dm#L271)
- **Статус в ss13-rs:** ❌ нет

### Капсулы-убежища (survival capsules)
Капсула разворачивает готовое убежище по шаблону (стандартное, люкс, элитное, туалет, рыболовные); внутри свои шлюзы, столы, слипер, стазис.

- **Код:** [`/obj/item/survivalcapsule`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/survival_pod.dm#L12), [`/datum/map_template/shelter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/shelters.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Прочие шахтёрские устройства
Червоточный прыжок (wormhole jaunter, спасает от падения в пропасть), инъектор Лазаря (оживляет фауну как питомца), крюк-кошка, кейральные наручники (GPS вне станции), указатель вентилей, радио погоды (штормы).

- **Код:** [`/obj/item/wormhole_jaunter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/wormhole_jaunter.dm#L2), [`/obj/item/lazarus_injector`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/lazarus_injector.dm#L9), [`/obj/item/grapple_gun`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/grapple_gun.dm#L3), [`/obj/item/clothing/accessory/kheiral_cuffs`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/kheiral_cuffs.dm#L3), [`/obj/item/pinpointer/vent`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/vent_pointer.dm#L1), [`/obj/item/radio/weather_monitor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/miningradio.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Снаряжение исследователя
Костюм исследователя с капюшоном, противогаз исследователя, плащ и броня из шкур голиафа и костей.

- **Код:** [`/obj/item/clothing/suit/hooded/explorer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/explorer_gear.dm#L2), [`/obj/item/clothing/mask/gas/explorer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/explorer_gear.dm#L58), [`/obj/item/clothing/suit/hooded/cloak/goliath`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/explorer_gear.dm#L113), [`/obj/item/clothing/suit/armor/bone`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/explorer_gear.dm#L186)
- **Статус в ss13-rs:** ❌ нет

### Органы фауны (monster cores)
Регенеративное ядро легиона (полное лечение, портится без стабилизатора), железа ускорения, мешок бримпыли; можно вживить как орган с активной способностью.

- **Код:** [`/obj/item/organ/monster_core`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/monster_organs/monster_organ.dm#L32), [`/obj/item/organ/monster_core/regenerative_core`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/monster_organs/regenerative_core.dm#L5), [`/obj/item/organ/monster_core/rush_gland`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/monster_organs/rush_gland.dm#L8), [`/obj/item/organ/monster_core/brimdust_sac`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/monster_organs/brimdust_sac.dm#L16), [`/obj/item/mining_stabilizer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/monster_organs/monster_organ.dm#L4)
- **Статус в ss13-rs:** ❌ нет

### Вендор шахтёрского снаряжения, очки и ваучеры
Консоль заказа за шахтёрские очки с ID (ORM начисляет очки), карта очков для передачи, ваучер на набор (крушитель, фултон, резонатор, капсула, минибот, набор новобранца, перчатки). Консоль корабля големов.

- **Код:** [`/obj/machinery/computer/order_console/mining`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/orders/order_computer/mining_order.dm#L3), [`/obj/item/mining_voucher`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/orders/order_computer/mining_order.dm#L83), [`/obj/item/card/mining_point_card`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/orders/order_computer/mining_order.dm#L95), [`/datum/voucher_set/mining`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/voucher_sets.dm#L38), [`/obj/machinery/computer/order_console/mining/golem`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/orders/order_computer/golem_order.dm#L1); tgui: [`ProduceConsole.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ProduceConsole.tsx)
- **Статус в ss13-rs:** 🟡 `ss13_game::mining::console` (окно `ProduceConsole`): корзина товаров консоли (каталог `/datum/orderable_item` по её категориям), оплата шахтёрскими очками счёта ID; экспресс — сразу на тайл покупателя (без пода), обычная доставка — ящиком на ближайшем шаттле снабжения; ORM начисляет очки (`ss13_game::mining::orm`); `ss13_game::mining::equipment`: ваучер на консоли — выбор набора `/datum/voucher_set/mining` списком (в tg радиальное меню), предметы набора на тайле консоли; карта очков — ID на карте, направление и количество, перенос не больше остатка источника; нет объявления в канал снабжения, консоли корабля големов (корабль — ../round/ghost-roles.md)

### Шахтёрский шаттл и шкафы шахтёров
Консоль шаттла между станцией и аванпостом на лаваленде/ледяной луне; гардероб и шкаф шахтёра со снаряжением.

- **Код:** [`/obj/machinery/computer/shuttle/mining`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/mine_items.dm#L92), [`/obj/docking_port/stationary/mining_home`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/mine_items.dm#L126), [`/obj/structure/closet/secure_closet/miner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/mine_items.dm#L57)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mining_shuttles.rs`: шахтёрский (`mining_delta`) и общий лавалендский (`mining_common_meta`) шаттлы грузятся на свои доки станции (`roundstart_template` типа дока), консоли `mining` и `mining/common` (`crates/ss13_game/src/shuttle_nav.rs`) возят их к `mining_away` / `lavaland_common_away` на лаваленде (`ss13_mapload::MiningLevel`); нет отказа консоли при импланте изгнания и главе революции, нет `landing_zone_dock`/`mining_public` (вспомогательной базы); шкафы шахтёров здесь не проверялись

### Вспомогательная база (aux base)
Строительная база шахтёров: консоль с дронами-строителями, выбор места посадки маяком, запуск базы на планету.

- **Код:** [`/obj/machinery/computer/auxiliary_base`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/aux_base.dm#L14), [`/obj/item/assault_pod/mining`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/aux_base.dm#L257), [`/obj/structure/mining_shuttle_beacon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/aux_base.dm#L332); tgui: [`AuxBaseConsole.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AuxBaseConsole.tsx)
- **Статус в ss13-rs:** ❌ нет

### Флора лаваленда и ледяной луны
Грибы, кактусы, серака, огнецвет и прочие растения: собираются в ингредиенты (лекарства пепельных ходоков, еда), отрастают со временем.

- **Код:** [`/obj/structure/flora/ash`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/lavaland/ash_flora.dm#L4)
- **BandaStation:** [`ash_flora.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/lavaland/ash_flora.dm#L728) — обход юнит-теста крафта
- **Статус в ss13-rs:** ❌ нет

### Сундуки некрополя и лут мегафауны
Сундуки с тендрилов и боссов; лут: берсерк-броня, мементо мори, жезл Асклепия, варп-куб, талисман бессмертия, общее хранилище, кровь дракона, лавовый посох, пила-тесак, клуб иерофанта, посох бури, вокальные связки колосса, аномальные кристаллы, глаз бога, плащ богоубийцы, проклятая катана, лестница Иакова, «Каин и Авель» и др.

- **Код:** [`/obj/structure/closet/crate/necropolis`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/lavaland/necropolis_chests.dm#L3), [`mining_loot/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/lavaland/mining_loot), [`/obj/item/cain_and_abel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/lavaland/cain_and_abel/_cain_and_abel.dm#L4)
- **Статус в ss13-rs:** ❌ нет

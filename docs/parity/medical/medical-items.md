# Медицинские предметы
Медицинские стаки, сканеры, дефибрилляторы, аптечки и средства транспортировки пациентов.

### Медицинские стаки: лечение brute и burn
Пакеты от ушибов, швы, мази, регенеративные сетки, алоэ, припарки: клик по мобу лечит выбранную зону после задержки (на себя дольше), левый клик переходит к следующей раненой части, швы и сетки повторяются; плотная одежда мешает.

- **Код:** [`/obj/item/stack/medical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/medical.dm#L1), [`/obj/item/stack/medical/bruise_pack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/medical.dm#L374), [`/obj/item/stack/medical/suture`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/medical.dm#L689), [`/obj/item/stack/medical/ointment`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/medical.dm#L724), [`/obj/item/stack/medical/mesh`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/medical.dm#L750), [`/obj/item/stack/medical/aloe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/medical.dm#L834), [`/obj/item/stack/medical/poultice`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/medical.dm#L910)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical.rs`: лечение по зоне с задержками, авто-переход, повтор швов и сеток, остановка кровотечения швами, дезинфекция и заживление ожогов; нет проверки одежды, звуков

### Бинты, марля, шины и костный гель
Марля перевязывает конечность (останавливает кровотечение, держит шину), бинты, костный гель и хирургическая лента лечат переломы.

- **Код:** [`/obj/item/stack/medical/wrap`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/medical.dm#L395), [`/obj/item/stack/medical/bandage`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/medical.dm#L938), [`/obj/item/stack/medical/bone_gel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/stacks/medical.dm#L861)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical.rs`, `crates/ss13_game/src/medical/wounds.rs`: марля впитывает кровь и гной и спадает, шинирует переломы, лучшая повязка заменяет; костный гель и хирургическая лента сращивают переломы; повязку нельзя снять руками, шин из досок нет

### Жгут
Пережимает конечность: останавливает кровотечение ниже, но вредит конечности со временем.

- **Код:** [`/obj/item/tourniquet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tourniquet.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Анализатор здоровья
Клик по мобу выводит скан: состояние, урон по типам и частям тела, температуру, органы, кровь, раны, болезни, квирки; правый клик — химический скан; продвинутый и супер-анализатор показывают больше.

- **Код:** [`/obj/item/healthanalyzer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/scanners/health_analyzer.dm#L6), [`/obj/item/healthanalyzer/advanced`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/scanners/health_analyzer.dm#L622)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/health_scan.rs`, `crates/ss13_game/src/medical.rs`: состояние, урон по типам и частям с ранами, температура, кровь и группа, режим сканирования ран; нет таблицы органов, болезней, химического скана

### Анализатор ран
Упрощённый сканер, описывающий раны и способы их лечения.

- **Код:** [`/obj/item/healthanalyzer/simple`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/scanners/health_analyzer.dm#L677)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/health_scan.rs`: простой скан ран по частям тела; нет бонуса к лечению (`TRAIT_WOUND_SCANNED`) и голограмм

### Дефибриллятор
Рюкзак/компактный дефибриллятор с лопатками: разряд по груди мёртвого (если тело не слишком повреждено, мозг цел и прошло мало времени) оживляет; в бою — оглушающий удар; ЭМИ и емаг; настенный дефибриллятор-зарядник.

- **Код:** [`/obj/item/defibrillator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/medical/defib.dm#L6), [`/obj/item/shockpaddles`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/medical/defib.dm#L340), [`/obj/item/defibrillator/compact`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/medical/defib.dm#L278), [`/obj/machinery/defibrillator_mount`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/defibrillator_mount.dm#L4), [`proc/can_defib()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L719)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/defib.rs`: лопасти из надетого блока, заряд ячейки (10 разрядов, у боевого бесконечный), 3 с + 2 с, оживление по условиям tg, перезарядка, боевой удар током и сбивание с ног; нет остановки/запуска сердца, emag, настенного крепления, двуручного хвата (свободная вторая рука)

### Аптечки
Обычная, экстренная, от ожогов, токсинов, ушибов, продвинутая, тактическая, хирургическая, коронерская, синдикатская — с соответствующим содержимым.

- **Код:** [`/obj/item/storage/medkit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/medkit.dm#L11), [`/obj/item/storage/medkit/regular`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/medkit.dm#L34), [`/obj/item/storage/medkit/advanced`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/medkit.dm#L260), [`/obj/item/storage/medkit/tactical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/storage/medkit.dm#L302)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/storage.rs`: аптечки на карте с содержимым tg

### Мешки для тел и каталки
Мешок для тел (складывается, хранит труп), медицинская кровать и экстренная каталка (пристёгивают, катятся).

- **Код:** [`/obj/item/bodybag`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/medical/bodybag.dm#L2), [`/obj/structure/closet/body_bag`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/crates_lockers/closets/bodybag.dm#L1), [`/obj/structure/bed/medical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/beds_chairs/bed.dm#L116), [`/obj/item/emergency_bed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/beds_chairs/bed.dm#L251)
- **Статус в ss13-rs:** ✅ реализовано — мешки для тел: `crates/ss13_game/src/closets/kinds.rs` (разворачивание из руки, мешок-шкаф на 2 моба, блюспейс на 15, сворачивание правым кликом пустого); медицинская кровать: `crates/ss13_game/src/furniture.rs` (`brakes`, `refresh_medical_bed`) — пристёгивание (общее с кроватями), катится с пациентом (сдвинутая мебель тянет пристёгнутого моба — `crates/ss13_game/src/medical/surgery/buckle.rs` `carry_buckled`, tg `handle_buckled_mob_movement`), Alt-клик — тормоза (не самим пациентом), с пациентом плотная и поднята (`*_up`), осмотр про тормоза; экстренная кровать — складывается правым кликом пустой в руки человека (`fold_emergency_bed`), сложенная разворачивается в руке на свой тайл или на клик по открытому полу (`roller_attack_self`, `roller_on_floor`). `// shortcut:` нет оверлея тормозов, звука `noisy_movement`, док-станции кровати для киборгов. Нужен `ss13-mapload convert` (новые runtime-типы)

### Медицинский HUD и монитор экипажа
Медицинские очки показывают полоски здоровья и статус; монитор экипажа (консоль и переносной) — данные сенсоров костюмов.

- **Код:** [`/obj/item/clothing/glasses/hud/health`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/glasses/hud.dm#L57), [`/obj/machinery/computer/crew`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/crew.dm#L6), [`/obj/item/sensor_device`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/sensor_device.dm#L1); tgui: [`CrewConsole.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/CrewConsole.tsx)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/records/hud.rs`: очки медицинского HUD (и импланты/визоры) показывают носителю полоску здоровья (`RoundHealth`) и статус (`med_hud_set_status`: `huddead`/`huddefib` — мёртв и дефибриллятор вернёт игрока, `hudill0`–`hudill6`/`hudbuff` — худшая видимая сканеру болезнь по `check_virus`, `hudnosensor` — сломанные датчики костюма, иначе `hudhealthy`); медицинский блок при осмотре со ссылками — `crates/ss13_game/src/records/medhud.rs` (см. ../mobs/examine-emotes.md); монитор экипажа (консоль `CrewConsole` и ручной) — `crates/ss13_game/src/clothing/sensors.rs`, см. ../items/clothing-under.md и ../items/devices.md «Ручной монитор экипажа». `// shortcut:` нет носителей ксеноморфа, возрождения гост-ролью и «болезнеподобных» трейтов (не портированы)

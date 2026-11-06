# Рыбалка и аквариумы
Удочки, мини-игра поимки, источники рыбы, рыба с генетикой и аквариумы (`code/modules/fishing`).

### Удочка
Заброс на точку ловли (вода, лава, ямы, туалет, аквариум, портал), натяжение лески; снасти: леска, крючок, наживка вставляются в слоты (tgui). Подтипы: костяная, телескопическая, мастерская, спасательная.

- **Код:** [`/obj/item/fishing_rod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fishing_rod.dm#L1), [`reel()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fishing_rod.dm#L251), [`/obj/item/fishing_rod/telescopic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fishing_rod.dm#L678); tgui: [`FishingRod.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/FishingRod.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/fishing/server.rs`: заброс (клик рядом или издалека) на воду, аквариум и портал в пределах `cast_range` + 2 тайла лески, ожидание 3–25 с, поклёвка, промах кликом, обрыв лески при уходе/падении удочки, остановка удочкой в руке; статы подтипов из прототипа (`DmFishingRod`); наживка — еда, вставленная в удочку; нет окна `FishingRod`, слотов лески/крючка, полёта поплавка и подцепа предметов

### Мини-игра поимки
После поклёвки открывается экранная мини-игра: держать «поплавок» на движущейся рыбе до заполнения шкалы; сложность зависит от рыбы, снастей, навыка рыбалки и черт рыбы; движения рыбы (медленная, юркая, рывками и др.).

- **Код:** [`/datum/fishing_challenge`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fishing_minigame.dm#L37), [`start()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fishing_minigame.dm#L223), [`/datum/fish_movement`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fish_movement.dm#L11)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/fishing/minigame.rs` (общая симуляция `move_bait`/`move_fish`, все типы движения, эффекты антигравитации и камуфляжа), окно `FishingMinigame` (`fishing/client.rs`, клиент симулирует, сервер отклоняет победу быстрее минимального времени), сложность из источника, удочки, рыбы и черт; правила снастей (двунаправленность, авто-катушка) — у «Снастей», навык — у «Навыка рыбалки»

### Снасти: леска, крючки, наживка
Лески (усиленная, маскировочная, прыгучая, из жил, авто-катушка), крючки (магнитный, ржавый, спасательный и др.), банки червей, суперприманки, блёсны.

- **Код:** [`/obj/item/fishing_line`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fishing_equipment.dm#L9), [`/obj/item/fishing_hook`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fishing_equipment.dm#L161), [`/obj/item/bait_can`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/bait.dm#L1), [`/obj/item/fishing_lure`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/bait.dm#L64)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/fishing/server.rs`: наживка (любая еда; черви, шарики и суперприманки по качеству) меняет веса таблицы и убирает «пустышку»; нет лесок, крючков, блёсен и банок наживки

### Точки и источники рыбы
Компонент точки ловли на турфах/объектах ссылается на источник с таблицей добычи: океан, лёд, река, песок, горячий источник, лава, ямы (шахтёрские останки), туалет, аквариум, разломы, хирургия; добыча включает не только рыбу.

- **Код:** [`/datum/fish_source`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/sources/_fish_source.dm#L72), [`calculate_difficulty_minigame()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/sources/_fish_source.dm#L170), [`/datum/component/fishing_spot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/fishing_spot.dm#L2), [`sources/subtypes/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/sources/subtypes), [`/datum/chasm_detritus`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fish/chasm_detritus.dm#L18)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/fishing.rs` (таблицы река, океан, пляж, Тизира, горячий источник, лёд, аквариум, порталы), `fishing/server.rs`: водные турфы, аквариумы (их живая рыба), активный портал, ограниченные награды с восстановлением, веса по наживке и чертам; нет лавы, песка, разломов, ям, туалетов (убраны BandaStation), голодека, хирургии и наград-не-предметов; нет рыболовных капсул (`/obj/item/survivalcapsule/fishing`: выбор шаблона радиальным меню, emag) — развёртывание шаблона капсулой есть в `ss13_game::mining::capsule` (../cargo/mining-equipment.md)

### Рыба
~50 видов (пресноводные, морские, станционные, шахтёрские, космические, голографические и т.д.) с размером/весом, здоровьем, голодом, нуждами среды; рыбу едят, готовят, мелют, ею можно шлёпнуть; вне воды она задыхается.

- **Код:** [`/obj/item/fish`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fish/_fish.dm#L19), [`process_health()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fish/_fish.dm#L1245), [`feed()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fish/_fish.dm#L928), [`make_edible()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fish/_fish.dm#L356), [`fish/types/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fish/types)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/fishing/fish.rs`: размер/вес по Гауссу при поимке, класс веса по размеру, голод, здоровье, удушье вне воды, смерть со спрайтом и текстом, осмотр; еда, готовка, помол, филе — у группы еды

### Черты, размножение и эволюция рыбы
Черты (пугливая, любит блестящее, привередливая, ночная, тяжёлая, ...) влияют на ловлю и поведение и наследуются; рыбы в аквариуме размножаются, потомство может эволюционировать в другой вид; рост через компонент.

- **Код:** [`/datum/fish_trait`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fish/fish_traits.dm#L24), [`try_to_reproduce()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fish/_fish.dm#L1321), [`/datum/fish_evolution`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fish/fish_evolution.dm#L12), [`/datum/component/fish_growth`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/fish_growth.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/fishing/server.rs`: черты из прототипа влияют на ловлю (пугливая, любит блестящее, привередливая, плотоядная/травоядная, тяжёлая, смазанная, антигравитация, камуфляж, амфибия); нет наследования, размножения, эволюции и роста

### Навык рыбалки
Навык растёт с уловом, облегчает мини-игру и открывает сведения о рыбе.

- **Код:** [`/datum/skill/fishing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/skills/fishing.dm#L5)
- **Статус в ss13-rs:** ❌ нет

### Аквариум
Аквариум и аквариумный бак держат рыбу с настройкой температуры и типа воды, кормлением, анимацией рыб внутри (tgui); улучшения (биоэлектрогенератор, блюспейс-бак), корм и кейсы для переноски рыбы.

- **Код:** [`/obj/structure/aquarium`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/aquarium/aquarium.dm#L1), [`/obj/item/fish_tank`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/aquarium/aquarium.dm#L122), [`/datum/component/aquarium`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/aquarium.dm#L9), [`/obj/item/aquarium_upgrade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/aquarium/aquarium_upgrades.dm#L3), [`/obj/item/storage/fish_case`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/aquarium/aquarium_kit.dm#L22); tgui: [`Aquarium.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Aquarium.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/fishing/aquarium.rs`: аквариум и бак (лимиты размера), окно `Aquarium` (жидкость, температура, режимы Manual/Assisted/Stasis, интервал, извлечение рыбы), корм, заполненный аквариум; нет улучшений, кейсов, реквизита, автокормления и анимации рыб внутри

### Анализатор рыбы и каталог
Анализатор показывает здоровье, черты и эволюции рыбы; книга-каталог описывает виды и где их ловить.

- **Код:** [`/obj/item/fish_analyzer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/aquarium/fish_analyzer.dm#L2), [`/obj/item/book/manual/fish_catalog`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fish_catalog.dm#L2); tgui: [`FishAnalyzer.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/FishAnalyzer.tsx), [`FishCatalog.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/FishCatalog.tsx)
- **Статус в ss13-rs:** ❌ нет

### Портал рыбалки
Машина открывает портал в выбранный «водоём» (океан, гипер-пространство, Синдикат при emag и т.д.) — точку ловли на станции; набор доступных зависит от исследованных источников.

- **Код:** [`/obj/machinery/fishing_portal_generator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fishing_portal_machine.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/fishing/aquarium.rs`: включение/выключение рукой, измерение аквариума, Синдикат после emag (выбор списком), deluxe — все источники, отключение без питания; нет открытия измерений исследованиями и привязки мультитулом

### Трофейная рыба на стене
Крепление для рыбы-трофея; трофеи сохраняются между раундами.

- **Код:** [`/obj/structure/fish_mount`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fish_mount.dm#L17), [`/obj/item/wallframe/fish`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/fish_mount.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Админский калькулятор рыбалки
Админ-инструмент: шансы и сложность улова для источника/снастей.

- **Код:** [`/datum/fishing_calculator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/fishing/admin.dm#L5); tgui: [`FishingCalculator.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/FishingCalculator.tsx)
- **Статус в ss13-rs:** ❌ нет

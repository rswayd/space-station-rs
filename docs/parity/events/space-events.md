# Космические угрозы и вторжения существ

События, приходящие из космоса: метеоры, пыль, неподвижный стержень, кротовины, карпы, лоза, порталы.

### Meteor Wave (Normal / Threatening / Catastrophic)
После объявления с обратным отсчётом (направление удара) станцию обстреливают волны метеоров со стороны; уровень задаёт таблицу типов (`meteors_normal`, `meteors_threatening`, `meteors_catastrophic` с шансом тунгуски). Щиты метеорной защиты (станционная цель) сбивают их.

- **Код:** [`/datum/round_event_control/meteor_wave`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/meteors/meteor_wave_events.dm#L3), [`/datum/round_event_control/meteor_wave/threatening`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/meteors/meteor_wave_events.dm#L59), [`/datum/round_event_control/meteor_wave/catastrophic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/meteors/meteor_wave_events.dm#L71), [`meteor_waves.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_waves.dm#L7)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/meteors.rs`: объявление, волна 2 мин по 5 метеоров каждые 6 с из таблиц tg, метеоры летят от края уровня станции к противоположному (10 клеток/с), таранят клетки `ex_act` и ломают стены/пол через ChangeTurf (`crates/ss13_game/src/explosions/ex_act.rs`), кластерные, блюспейс и тунгуска по tg, взрывы `meteor_effect` настоящие (`crates/ss13_game/src/explosions/`: блокировка, вспышка, звук и тряска взрыва); электромагнитный — `empulse(3, 8)` на месте (`crate::emp`, без звука); нет тряски и звука удара тяжёлых метеоров, карпов, гринглоу, щитов метеорной защиты и Хэллоуина

### Meteor Wave: Meaty
Волна «мясных» метеоров (`meateors`): при ударе разлетаются мясом и потрохами, редкий ксено-вариант.

- **Код:** [`/datum/round_event_control/meteor_wave/meaty`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/meteors/meteor_wave_events.dm#L83)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/meteors.rs` `MEATEORS`: мясные метеоры бьют `ex_act` только то, во что врезались; нет крови, гибов и выпадения мяса/органов (нет прототипов)

### Major Space Dust / Space Dust: Minor
Major — волна космической пыли (`meteors_dust`), царапает обшивку; Minor — одиночная пылинка, частое фоновое событие (вес 200).

- **Код:** [`/datum/round_event_control/meteor_wave/dust_storm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/meteors/meteor_wave_events.dm#L96), [`/datum/round_event_control/space_dust`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/dust.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/meteors.rs`: Major — волна пыли с объявлением (шанс 85%, тексты BandaStation), Minor — одна пылинка (вес 200); пыль пролетает решётки и столы, `ex_act` лёгкий; без звука

### Sandstorm (Directional / Classic)
Directional: после предупреждения одна сторона станции минутами перемалывается песчаными метеорами (`meteors_sandstorm`); Classic (только админы): со всех сторон, очень разрушительно.

- **Код:** [`/datum/round_event_control/sandstorm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/sandstorm.dm#L10), [`/datum/round_event_control/sandstorm_classic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/sandstorm.dm#L67)
- **Статус в ss13-rs:** ❌ нет — нет метеоров

### Stray Meteor
Один случайный метеор (`meteors_stray`) брошен в сторону станции без волны.

- **Код:** [`/datum/round_event_control/stray_meteor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/meteors/stray_meteor_event.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/meteors.rs`: один метеор из `meteors_stray` и объявление с его сигнатурой; без выбора типа админом

### Dark Matt-eor
Админское событие: метеор тёмной материи, после удара порождает сингулярность.

- **Код:** [`/datum/round_event_control/dark_matteor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/meteors/dark_matteor_event.dm#L1), [`/obj/effect/meteor/dark_matteor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_dark_matteor.dm#L2)
- **Статус в ss13-rs:** ❌ нет — нет метеоров и сингулярности

### Immovable Rod
Неподвижный стержень пролетает через станцию по прямой, пробивая стены, машины и людей; админ может задать цель. Его можно «поймать» особыми способами.

- **Код:** [`/datum/round_event_control/immovable_rod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/immovable_rod/immovable_rod_event.dm#L3), [`/obj/effect/immovablerod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/immovable_rod/immovable_rod.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/immovable_rod.rs`: стержень с 13 тайлов от случайной стороны летит к краю карты за случайным тайлом станции (10 тайлов/с, общий `Towards` с метеорами), фазой проходит всё: стена — `ex_act` DEVASTATE, плотные объекты уничтожаются (`take_damage(INFINITY)`; машины и двери без модели целостности — сразу `deconstruct(FALSE)`, кроме `INDESTRUCTIBLE`), живые пробиваются (100 брута по телу, 60 в грудь, тяжёлый `ex_act` если плотный или 10%), лязг 10%; объявление на 5 тике; нет админского прицела и зацикливания, сингулярности при встрече стержней (оба исчезают), счётчика убийств в имени, суплекса и оповещения призраков

### Wormholes
На станции появляются нестабильные порталы, телепортирующие вошедших в случайную точку; закрываются через некоторое время.

- **Код:** [`/datum/round_event_control/wormholes`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wormholes.dm#L3)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/wormholes.rs`: 400 плотных кротовин на случайных полах станции, каждые 3 тика переезжают, в конце (40–80 тиков) исчезают; врезавшийся незакреплённый попадает на свободный тайл рядом со случайной кротовиной, звуки портала; объявление `ANNOUNCER_SPANOMALIES`; брошенные предметы и снаряды не входят (нет `Bumped` от бросков)

### Carp Migration
Косяк космических карпов (иногда с «магикарпом») появляется у краёв станции и плывёт через неё; объявление о миграции.

- **Код:** [`/datum/round_event_control/carp_migration`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/carp_migration.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/carp_migration.rs`: объявление, карп (5% мегакарп) на каждом `carpspawn`, путь косяка: безопасная клетка станции (`// shortcut:` случайная точка спавна экипажа) и край уровня; нет оповещения призраков и админ-настройки

### Mice Migration
Орда мышей вылезает на станцию, грызёт провода; с шансом приходит Крысиный король.

- **Код:** [`/datum/round_event_control/mice_migration`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/mice_migration.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/mice_migration.rs` + `crates/ss13_game/src/mobs/spawn.rs` `trigger_migration`: 5–15 мышей на открытых проводах станции с пригодным воздухом, объявление с писком; нет Крысиного короля и прочих `vermin_chances` (нет мобов)

### Space Vines (кудзу)
В случайной точке прорастает космическая лоза, расползается по полу, получает мутации (ядовитая, взрывная, газовая и др.), может породить мухоловок; вырубается острым или огнём.

- **Код:** [`/datum/round_event_control/spacevine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/space_vines/vine_event.dm#L1), [`/datum/spacevine_controller`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/space_vines/vine_controller.dm#L13), [`/datum/spacevine_mutation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/space_vines/vine_mutations.dm#L1)
- **BandaStation:** [`code/modules/events/space_vines/vine_controller.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/space_vines/vine_controller.dm#L128) — лоза растёт быстрее (стартовый бонус 20 вместо 5, база 1 вместо 0,5)
- **Статус в ss13-rs:** ❌ нет — нет лозы

### Portal Storm (Syndicate / Constructs)
Над станцией открываются порталы, из которых волнами выходят штурмовики Синдиката (или конструкты Нар'Си — только админы); объявление о шторме.

- **Код:** [`/datum/round_event_control/portal_storm_syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/portal_storm.dm#L1), [`/datum/round_event_control/portal_storm_narsie`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/portal_storm.dm#L17)
- **Статус в ss13-rs:** ❌ нет — нет события и враждебных мобов

### Aurora Caelus
Мирное событие: через окна в космос видно полярное сияние, освещение космоса и кухни переливается цветами несколько минут.

- **Код:** [`/datum/round_event_control/aurora_caelus`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/aurora_caelus.dm#L1)
- **Статус в ss13-rs:** ❌ нет — звёздный свет статичен (`crates/ss13_shared/src/light.rs`): нужна смена цвета звёздного света на клиенте (контракт протокола); нет `SSmapping.empty_space`

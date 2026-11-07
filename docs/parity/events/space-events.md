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
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/meteors.rs` (`Sandstorm`, `SandstormClassic`, `SAND`, `METEORS_SANDSTORM`, `spawn_meteors_from`): Directional — предупреждение на 1 тике со стороной (носа/кормы/правого/левого борта, `make_sandstorm_from` задаёт сторону заранее), с 70–90 по 110–140 тик по 15 метеоров с этой стороны из `meteors_sandstorm` (песок 45, пыль 5); космический песок (2 удара, лёгкий `ex_act`, ничего не роняет) не трогает стены с `hardness ≤ 25` (`wall_hardness`: укреплённые, пластитановые, алмазные…) и зоны `EVENT_PROTECTED` (прибытие); Classic — вес 0, 150 тиков по 10 пылинок со всех сторон без объявления. Проверено: `crates/ss13_game/tests/it/space_events.rs`, юнит-тесты `meteors.rs`. Админский выбор стороны в панели — к группе админки (нет `event_admin_setup`)

### Stray Meteor
Один случайный метеор (`meteors_stray`) брошен в сторону станции без волны.

- **Код:** [`/datum/round_event_control/stray_meteor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/meteors/stray_meteor_event.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/meteors.rs`: один метеор из `meteors_stray` и объявление с его сигнатурой; без выбора типа админом

### Dark Matt-eor
Админское событие: метеор тёмной материи, после удара порождает сингулярность.

- **Код:** [`/datum/round_event_control/dark_matteor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/meteors/dark_matteor_event.dm#L1), [`/obj/effect/meteor/dark_matteor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/meteors/meteor_dark_matteor.dm#L2)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/meteors.rs` (`DarkMatteorEvent`, `DARK_MATTEOR_KIND`, `DarkMatteor`, `moved_off_z`): вес 0; объявление с `airraid`; метеор (15 ударов, `ex_act` DEVASTATE) стартует на 10 клеток глубже края и летит в живого моба на уровне станции не над пустотой (`dark_matteor_target`); пока летит — красный код (прежний ниже красного запоминается); промах (дошёл до цели или края невредимым) возвращает прежний код (кроме дельты) и объявляет «Отслеживание метеора»; иссякнув, оставляет `/obj/singularity/dark_matter`. Проверено: `crates/ss13_game/tests/it/space_events.rs`. `// shortcut:` цель — тайл моба на момент запуска (tg преследует моба), без варп-кольца и квантовых искр. Поведение самой сингулярности тёмной материи — ../power/singularity.md (там ❌)

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
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/space_vines.rs`: событие (вес 15, до 100 проб коридорных полов, случайная мутация, потенция 50–100, продуктивность 1–4); контроллер раз в 2 с с формулами tg и BandaStation (бонус старта 20, база 1), очередь роста, рост (Med/Hvy, непрозрачность), опутывание (пристёгивает, свободная рука или сопротивление освобождают), распространение по открытым полам без космоса/лавы/пропастей; все 20 мутаций с тяжестью, весами и цветами, наследование и случайная мутация в пределах `max_mutation_severity`; действуют свет, токсичность (толстая одежда защищает), взрывная (взрыв 2 при гибели, всегда гибнет от взрыва), огне- и морозоупорность, стабилизация температуры, поедатели газов, пожирание лозы, агрессивное распространение, прозрачность, шипы, укреплённая, робкая, цветущая (бутон), скользкая, радиоактивная; удар предметом ×4 огнём и острым (огнеупорная ×0, укреплённая ×0,5 к острому), звуки; горячий воздух (>150 °C) сжигает, <100 K останавливает; последняя лоза оставляет семена кудзу с потенцией и продуктивностью контроллера. Проверено: `crates/ss13_game/tests/it/space_events.rs`, юнит-тесты. `// shortcut:` нет удара током у проводящей, скользких предметов, фракций растений, поедания ксеносами, анимации; рост бутона в мухоловку — к ../mobs/ (нет `venus_human_trap`); админская настройка — к админке

### Portal Storm (Syndicate / Constructs)
Над станцией открываются порталы, из которых волнами выходят штурмовики Синдиката (или конструкты Нар'Си — только админы); объявление о шторме.

- **Код:** [`/datum/round_event_control/portal_storm_syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/portal_storm.dm#L1), [`/datum/round_event_control/portal_storm_narsie`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/portal_storm.dm#L17)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/portal_storm.rs`: звук зарядки, через 8 с объявление, через 2 с — удар молнии всем; точки выхода (по одной на моба) — случайные тайлы станции; с 8 тика каждый тик вспышка шторма (`energy_ball_fast`, лайм, 1,5 с) с громом на случайном тайле, каждый чётный тик выходит рядовой (тип равновероятно из оставшихся, счётчики tg), боссы — на `7 + ⌈2·рядовых/боссов⌉` и далее каждые `⌈рядовых/боссов⌉` тиков; событие кончается, когда вышли все (`time_to_end`). Синдикат — 8 мечников и 2 стрелка в скафандрах + 2 штурмовика; Нар'Си (вес 0) — 8 джаггернаутов, 6 призраков + 6 ремесленников. Проверено: `crates/ss13_game/tests/it/space_events.rs`. Поведение штурмовиков и враждебных конструктов — к ../mobs/basic-mobs-misc.md (там ❌: мобы появляются по типу без своего ИИ)

### Aurora Caelus
Мирное событие: через окна в космос видно полярное сияние, освещение космоса и кухни переливается цветами несколько минут.

- **Код:** [`/datum/round_event_control/aurora_caelus`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/aurora_caelus.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/aurora_caelus.rs`: объявление «Отдела Метеорологии» с `notice2`, музыка `aurora_caelus.ogg` игрокам на уровнях станции (громкость 20); `set_starlight` перекрашивает источники звёздного света (`Optics::light` освещённых тайлов космоса, реплицируются как обычный свет, без смены протокола); затухание tg — 5 шагов по 8 с (цвет по HSL-градиенту, дальность ×1,75, сила ×0,6) и обратно в конце; каждые 8 тиков цвет идёт по градиенту `#A2FF80 → #A2FFEE`; пол кухни (`/area/station/service/kitchen`) светится тем же (1 тайл, 0,75); 1% (всегда на 1 апреля) — в духовках кухни 100 моль плазмы при 1000 K. Проверено: `crates/ss13_game/tests/it/space_events.rs`, юнит-тесты. `// shortcut:` фон самих тайлов космоса (`TileLighting::Space`) не перекрашивается, нет проверки `SSmapping.empty_space`, громкости `sound_midi`, крика повара

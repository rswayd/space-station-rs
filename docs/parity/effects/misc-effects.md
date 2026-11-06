# Прочие эффекты: точки спавна, спавнеры, временные визуалы, ловушки, порталы

Остальное из `code/game/objects/effects`: служебные метки карты, спавнеры, кратковременные визуальные эффекты, постеры, мины, порталы, силовые поля и мелкие объекты-эффекты. Аномалии (`effects/anomalies`) — наука; RCD-эффекты — инженерия.

## Метки и спавнеры

### Точки старта профессий, позднего входа и наблюдателей
`/obj/effect/landmark/start/*` — место появления каждой профессии в начале раунда; `latejoin` — точки позднего входа (на станциях их заменяют кресла шаттла прибытия); `observer_start` — точка появления призрака.

- **Код:** [`/obj/effect/landmark`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/landmarks.dm#L1), [`/obj/effect/landmark/start`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/landmarks.dm#L27), [`/obj/effect/landmark/latejoin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/landmarks.dm#L342), [`/obj/effect/landmark/observer_start`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/landmarks.dm#L356)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/server.rs` (правила выбора точки спавна), `crates/ss13_game/src/jobs.rs`, `crates/ss13_mapload/src/jobs.rs`

### Служебные метки событий и антагонистов
Точки спавна карпов (`carpspawn`), блоба, событийные (`event_spawn`), обслуживания (`generic_maintenance_landmark`), ERT, тюремного варпа, Тандердома, руин, выходов порталов, лифта ядерных оперативников, снаряжения СБ.

- **Код:** [`/obj/effect/landmark/carpspawn`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/landmarks.dm#L351), [`/obj/effect/landmark/blobstart`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/landmarks.dm#L372), [`/obj/effect/landmark/event_spawn`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/landmarks.dm#L472), [`/obj/effect/landmark/generic_maintenance_landmark`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/landmarks.dm#L361), [`/obj/effect/landmark/ert_spawn`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/landmarks.dm#L401), [`/obj/effect/landmark/prisonwarp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/landmarks.dm#L392), [`/obj/effect/landmark/thunderdome/one`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/landmarks.dm#L437)
- **Статус в ss13-rs:** 🟡 частично — точки спавна карпов используются миграцией карпов (`crates/ss13_game/src/events/carp_migration.rs`); остальные метки (блоб, event_spawn, ERT, Тандердом и др.) не используются

### Метки навигации
`navigate_destination/*`: именованные цели (мостик, медбей, бар, отсеки побега...) для глагола «Navigate», рисующего путь к цели.

- **Код:** [`/obj/effect/landmark/navigate_destination`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/landmarks.dm#L621), [`/obj/effect/landmark/navigate_destination/bridge`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/landmarks.dm#L640)
- **Статус в ss13-rs:** ❌ нет — нет навигации

### Случайные спавнеры лута
`/obj/effect/spawner/random/*`: при загрузке заменяются взвешенным выбором из списка (`loot`, вложенные веса, шанс, количество, разброс); десятки таблиц — техтоннели, инженерия, медицина, еда, одежда, оружие, контрабанда, мусор, торговые автоматы, техсклад.

- **Код:** [`/obj/effect/spawner/random`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/spawners/random/_random.dm#L4), [`spawners/random/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/spawners/random)
- **BandaStation:** [`spawners/random/engineering.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/spawners/random/engineering.dm#L124), [`clothing.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/spawners/random/clothing.dm#L69), [`entertainment.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/spawners/random/entertainment.dm#L304), [`ai_module.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/spawners/random/ai_module.dm#L26) — форк правит отдельные таблицы лута
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/spawner.rs`: статическая модель `spawn_loot` (веса, шанс, количество, разброс, сдвиги) решается один раз при загрузке; спавнеры с лутом, вычисляемым кодом (глобальные таблицы, станционные трейты, праздники), удаляются без лута

### Спавнеры структур и костюмов
`/obj/effect/spawner/structure` ставит набор объектов (окно с решёткой и т.п.), `/obj/effect/spawner/costume` — комплект костюма.

- **Код:** [`/obj/effect/spawner/structure`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/spawners/structure.dm#L7), [`/obj/effect/spawner/costume`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/spawners/costume.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_mapload/src/spawner.rs` (`spawn_list`, `items` по порядку)

### Спавнеры гиббов, бомб и прочие
Гиб-спавнеры разбрасывают ошмётки и кровь (при гибе), бомбоспавнеры собирают готовые ТТВ-бомбы, праздничные спавнеры, послание в бутылке, доставка ксено-яйца.

- **Код:** [`/obj/effect/gibspawner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/spawners/gibspawner.dm#L1), [`/obj/effect/spawner/newbomb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/spawners/bombspawner.dm#L5), [`code/game/objects/effects/spawners/holiday.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/spawners/holiday.dm), [`code/game/objects/effects/spawners/message_in_a_bottle.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/spawners/message_in_a_bottle.dm), [`code/game/objects/effects/spawners/xeno_egg_delivery.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/spawners/xeno_egg_delivery.dm)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/effects/gibs.rs`: гиб-спавнеры `generic`, `human`, `robot` (типы ошмётков с дедупликацией ассоциативного списка, направления, звук `blobattack`, искры у робота); каждый ошмёток «размазывается» (`streak`: 0/1/2/3/50 шагов с весами 200/150/50/17/1, брызги крови на каждом тайле, стоп у препятствия); `health::gib` — человеческие для тела с конечностями, иначе generic. Нет: анимации полёта, ДНК крови и болезней на ошмётках, ксено/личинок; бомбоспавнеров ТТВ (группа ТТВ), праздничных спавнеров (группа праздников), послания в бутылке (персистентность), доставки ксено-яйца (ксеноморфы)

## Визуальные эффекты

### Временные визуалы (temp_visual)
Кратковременные анимации, удаляющиеся сами: попадания и трассеры снарядов, вспышки дульного пламени, эффекты ударов, телепортации, исцеления, культа, взрывов, «мигание» блинка и сотни других.

- **Код:** [`/obj/effect/temp_visual`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/temporary_visuals/temporary_visual.dm#L2), [`code/game/objects/effects/temporary_visuals/miscellaneous.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/temporary_visuals/miscellaneous.dm), [`temporary_visuals/projectiles/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/temporary_visuals/projectiles), [`code/game/objects/effects/temporary_visuals/cult.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/temporary_visuals/cult.dm)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/effects/temp_visual.rs`: базовый `temp_visual` (некликабельный спрайт над мобами, исчезает через `duration`, `randomdir` или `dir_setting`), `spawn` для кода механик; брызги крови `bloodsplatter` при brute-попадании снаряда в кровоточащего моба (`splatter1-6` по направлению, слой над мобом при полёте на юг, `angle2dir`). Конкретные визуалы — у своих механик (указатель `point.rs`, ЭМИ, мегафауна, резонатор уже свои). Нет: скольжения и затухания брызг (`animate`), цвета крови по типу

### Обратные отсчёты
Плавающие цифры над объектом: таймер бомбы Синдиката, ядерной бомбы, целостности суперматерии, аномалий, песочных часов.

- **Код:** [`/obj/effect/countdown`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/countdown.dm#L1), [`/obj/effect/countdown/syndicatebomb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/countdown.dm#L82), [`/obj/effect/countdown/supermatter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/countdown.dm#L103)
- **Статус в ss13-rs:** ❌ нет — нет механик с таймером

### Постеры
Постеры (официальные, контрабандные, «разыскивается», «пропал без вести») вешаются на стены из свёрнутого постера, срываются руками (остаётся рваный); случайный постер выбирается при загрузке.

- **Код:** [`/obj/item/poster`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/posters/poster.dm#L14), [`/obj/structure/sign/poster`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/posters/poster.dm#L139), [`/obj/structure/sign/poster/random`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/posters/poster.dm#L309), [`/obj/item/poster/wanted`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/wanted_poster.dm#L10)
- **Статус в ss13-rs:** 🟡 частично — постеры карты рисуются (`crates/ss13_mapload`); `crates/ss13_game/src/decor.rs`: пустая рука срывает постер (на его месте рваный, с тем же смещением), кусачки снимают целый в свёрнутый постер (`RolledPoster`: тип, вид, имя) в руку или убирают рваный; свёрнутый постер на стену по прямой от игрока (3,7 с) вешается на эту грань тайла игрока, один постер на грань. Нет: ловушек из осколка стекла, «разыскивается»/«пропал без вести», случайных свёрнутых постеров из лута, запрета диагональных стен, анимации вешания

## Ловушки и порталы

### Мины
Мины срабатывают при наступании: взрывная, огненная, вспышка, оглушающая, газовая (плазма, пар), шрапнельная, звуковая, «пинок» (кикает из игры — админская).

- **Код:** [`/obj/effect/mine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/mines.dm#L1), [`/obj/effect/mine/explosive`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/mines.dm#L147), [`/obj/effect/mine/stun`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/mines.dm#L181), [`/obj/effect/mine/gas`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/mines.dm#L197), [`/obj/effect/mine/shrapnel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/mines.dm#L230), [`/obj/effect/mine/kickmine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/mines.dm#L189)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/effects/mines.rs`: наступивший нажимает пластину («*щёлк*»), мина срабатывает, когда он с неё сходит (искры, мина исчезает); летающие/призраки/закреплённое/эффекты не нажимают; взрывные (обычная, слабая, огненная, ослепляющая — радиусы подтипов), оглушающая (паралич 8 с рядом стоящему), газовые (O2, плазма, N2O, пар), звуковые (хонк, bwoink). Нет: шрапнельных мин (`pellet_cloud` — только искры), кика клиента у kickmine, `arm_delay` и раскладчика мин (`minespawner`), подрыва уроном, строки осмотра о нажатой пластине, проезда бордер-объектов

### Триггеры наступания и бамп-телепорты
`step_trigger`: сообщение, бросок, стоп, телепорт (фиксированный, случайный, со смещением), звук, выдача снаряжения при входе на тайл (руины, админские локации); `bump_teleporter` телепортирует при столкновении.

- **Код:** [`/obj/effect/step_trigger`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/step_triggers.dm#L3), [`/obj/effect/step_trigger/thrower`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/step_triggers.dm#L51), [`/obj/effect/step_trigger/teleporter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/step_triggers.dm#L115), [`/obj/effect/bump_teleporter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/bump_teleporter.dm#L2)
- **BandaStation:** [`code/game/objects/effects/bump_teleporter.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/bump_teleporter.dm#L47) — телепортирует любой movable, а не только мобов
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/effects/teleporters.rs`, переменные карты (`id`, `id_target`, `teleport_*`, `message`, ...) — `crates/ss13_mapload/src/triggers.rs` (`DmTrigger`): бамп-телепорты CentCom переносят любой movable на телепорт с `id` = `id_target`; триггеры наступания: сообщение (по умолчанию один раз), телепорт абсолютный, случайный в прямоугольнике, со смещением (зацикленные коридоры), `teleport_fancy` (искры, дым, число использований), звук (всем или только наступившему); призраки — только с `affect_ghosts`, `mobs_only`. Нет: `thrower` (только музей), `outfitter`, срабатывания от телепорта на тайл (только шаги), фильтра `NO_TELEPORT` у `bump_teleporter/filtering`

### Порталы
Парные порталы (от ручного телепорта, кротовины, аномалии), постоянные и одноразовые: вошедший переносится к выходу.

- **Код:** [`/obj/effect/portal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/portals.dm#L15), [`/obj/effect/portal/anom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/portals.dm#L51), [`/obj/effect/portal/permanent`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/portals.dm#L185), [`/obj/effect/portal/permanent/one_way`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/portals.dm#L205)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/effects/teleporters.rs`: постоянные порталы карты (пары по `id`), `one_way` на случайный `landmark/portal_exit` с тем же `id`, `one_use` закрывается после живого; бамп или клик рукой/предметом рядом переносит объект или моба (не эффекты, мехи, закреплённое), звуки входа; кротовины события — `crates/ss13_game/src/events/wormholes.rs`. Нет: временных пар порталов (`create_portal_pair`: ручной телепорт, джонтер — группа устройств), пролёта снарядов и клика призрака, анимации входа (`temp_visual/portal_animation`) и «колыхания»

### Силовые поля и щиты
Непроходимые временные поля: стена мага, культа, мима (невидимая стена), психическое, космическое поле еретика (замедляет); `/obj/effect/shield` — неразрушимый барьер.

- **Код:** [`/obj/effect/forcefield`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/forcefields.dm#L1), [`/obj/effect/forcefield/wizard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/forcefields.dm#L21), [`/obj/effect/forcefield/mime`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/forcefields.dm#L59), [`/obj/effect/forcefield/cosmic_field`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/forcefields.dm#L101), [`/obj/effect/shield`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/shield.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/effects/forcefields.rs`: `spawn_forcefield` — поле любого подтипа с его `initial_duration` (визард/мим 30 с, мим advanced 60 с, культ 20 с, культ permanent навсегда, психическое 10 с, космическое 30/5/2,5 с), плотное (кроме космического), воздух по `can_atmos_pass` прототипа, само исчезает; Forcewall мага (`spells/effects.rs`) ставит его. Нет: прохода заклинателя и антимагии (`CanAllowThrough` — нужен хук движения в `ss13_world`), рикошета от психического, замедления снарядов и бонуса скорости космического, опоры о стену мима; заклинания культа, мима, псайкера и еретика — их группы

## Растительность и мелочи

### Светогрибы
Светящиеся грибы на стенах и полу (обычные, glowcap, тенегрибы — излучают «тень» вместо света, бурые), размножаются по соседним тайлам.

- **Код:** [`/obj/structure/glowshroom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/glowshroom.dm#L4), [`/obj/structure/glowshroom/glowcap`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/glowshroom.dm#L41), [`/obj/structure/glowshroom/shadowshroom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/glowshroom.dm#L47)
- **Статус в ss13-rs:** 🟡 частично — размещённые картой грибы рисуются и светятся по своим light-переменным (`crates/ss13_mapload/src/light.rs`); роста и отрицательного света тенегрибов нет

### Паутина и коконы пауков
Липкая паутина замедляет и задерживает не-пауков, проход для пауков, коконы с жертвами, шипы, «чучело»; режется острым и горит.

- **Код:** [`/obj/structure/spider`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/spiderwebs.dm#L3), [`/obj/structure/spider/stickyweb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/spiderwebs.dm#L35), [`/obj/structure/spider/cocoon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/spiderwebs.dm#L225), [`/obj/structure/spider/passage`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/spiderwebs.dm#L202)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/effects/webs.rs`: липкая паутина (50 %, very_sticky 100 %) отнимает 10–15 выносливости вошедшему живому мобу, при выносливости >90 сбивает с ног на 3 с; пауки (`TRAIT_WEB_SURFER`) и те, кого они тащат, проходят свободно; запечатанная паутина плотная (из `density`); урон ближнего боя: brute ×0,25, burn ×1,25 (`obj_damage::take_damage`); горит на воздухе >350 K (`atmos_exposure`). Нет: перехвата снарядов, тряски, плетения ткани, генетической паутины, коконов, проходов, шипов

### Горение предметов и одежды
Горючий предмет или структура от огня получает компонент горения: горит, получает урон, может сгореть в пепел; на горящем человеке одежда повреждается и сгорает (`burn_clothing`). Перенесено из ../atmos/fire.md.

- **Код:** [`/datum/component/burning`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/burning.dm#L8), [`proc/burn_clothing()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/human_defense.dm#L674)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/fire.rs` (`Burning`): горят только структуры и машины; предметы и одежда не горят

### Заморозка предметов
Конденсация пара при очень низкой температуре (`freeze_turf`) замораживает предметы на тайле: элемент `frozen` (синий оттенок, ломкость). Перенесено из ../atmos/reactions.md.

- **Код:** [`/datum/element/frozen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/frozen.dm#L4), [`proc/freeze_turf()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/open/_open.dm#L540)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/effects/frozen.rs`: `freeze_turf` из конденсации (`janitor.rs`) замораживает все объекты тайла без `FREEZE_PROOF`: имя «замороженный …», ледяной оттенок, −25 альфы; брошенный разбивается при приземлении или ударе, если не пойман (`throw.rs`); перемещённый на открытый тайл ≥ 0 °C оттаивает. Упрощения: матрица цвета — умножающий оттенок, разбитый объект удаляется без `deconstruct`, органы не перестают гнить. Статус `freon` замёрзших мобов — группа статус-эффектов

### Бонусы (powerup) и прочие эффекты
Подбираемые бонусы (здоровье, патроны, скорость, «безумие» — CTF и арены), фазовый моб (бестелесное перемещение заклинаний и демонов), информационный значок (`abstract/info`), трещины-«слабые точки» (`weakpoint`, от Earthcracker из аплинка или при генерации карты): от взрыва проламывают пол цепочкой трещин.

- **Код:** [`/obj/effect/powerup`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/powerup.dm#L1), [`/obj/effect/dummy/phased_mob`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/phased_mob.dm#L1), [`/obj/effect/abstract/info`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/info.dm#L2), [`/obj/effect/weakpoint`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/turfdecal/weakpoint.dm#L9)
- **Статус в ss13-rs:** ❌ нет — не портированы

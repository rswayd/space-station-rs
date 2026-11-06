# Simple animals и мегафауна
Устаревшая основа `/mob/living/simple_animal` (без контроллеров ИИ, собственный `Life()` и поиск целей). Из неё в апстриме остались мегафауна, элиты Лаваленда и слизни-oozes; боты — в каталоге силиконов. В ss13-rs отсутствует.

### Базовый simple animal
Старая модель моба: `Life()` с собственной атмосферой/температурой, `speak`/`emote` по таймеру, реакция на руку; урон по `damage_coeff`.

- **Код:** [`/mob/living/simple_animal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/simple_animal.dm#L2), [`Life()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/simple_animal.dm#L211), [`attack_hand()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/animal_defense.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/mobs.rs`: оставшиеся в апстриме simple animals (мегафауна) читаются конвертером как базовые мобы (`maxHealth`/`health`, урон, `del_on_death` → `DEL_ON_DEATH`, `minbodytemp`/`maxbodytemp` как диапазон выживания, глаголы `response_*`, `speak_emote`), и дальше живут на механике `crates/ss13_game/src/mobs/mod.rs` (атмосфера/температура в `Life`, реакция на руку и удары — `defense.rs`, `death`/`del_on_death`); нет `speak`/`emote_hear` по таймеру, `damage_coeff` (читается `physiology`), прочих подтипов `simple_animal` (элиты, слизни — свои записи; боты — группа силиконов)

### Враждебные simple animals
`hostile`: поиск целей в обзоре (`FindTarget`), преследование, ближний и дальний бой, ломание препятствий, агрессия к обидчикам.

- **Код:** [`/mob/living/simple_animal/hostile`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/hostile.dm#L1), [`proc/FindTarget()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/hostile.dm#L183)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/megafauna/mod.rs`: hostile-ИИ мегафауны — `FindTarget`/`PickTarget` (ближайшая видимая цель не своей фракции в `vision_range`/`aggro_vision_range`, по типу: 5/18, Легион 13, защитник 9/9), `Aggro`/`LoseAggro`, поиск по урону, преследование `Goto` с `minimum_distance`, ближний бой с телеграфом и `rapid_melee`, `OpenFire` по `ranged_cooldown`; нет `environment_smash`, отступления (`retreat_distance`), блуждания (`wander`), поиска по звуку/обидчику вне обзора

### Общие механики мегафауны
Боссы Лаваленда/Ледяной луны: огромное здоровье, иммунитет к оглушениям, награды за убийство (медали, лут-сундук, трофеи крашера), запись в БД достижений.

- **Код:** [`/mob/living/simple_animal/hostile/megafauna`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/megafauna/_megafauna.dm#L1), [`death()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/megafauna/_megafauna.dm#L102)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/megafauna/mod.rs`: мегафауна — базовые мобы из `mobs.ron` (`ss13-mapload convert` читает `simple_animal`-переменные: здоровье, `physiology`, урон, `del_on_death`, температуры, полёт); мозг hostile-ИИ (тик `SSnpcpool` 2 с: цель в `vision_range` 5/18 сквозь стены, ближайшая видимая живая или мёртвая не своей фракции, поиск по урону, `Goto` ИИ-движением, удар через 0.25 с телеграфа, `OpenFire`), общий кулдаун способностей с блоком ближнего боя, пожирание трупа (500 brute, `gutted`, лечение вне станции), `ex_act` 250/100/50 без гиба, не тащится, лут-сундук некрополя с содержимым при смерти; `placement.rs`: расстановка на Лаваленде по биому tg (`SPAWN_MEGAFAUNA`, веса `megafauna_spawn_list`, радиус 7, уникальный Баблгам — не больше одного за раунд); нет медалей/БД достижений, GPS-сигналов, трофеев и сундуков крашера, ломания стен, управления игроком, удаления органов при пожирании

### Баблгам
Демон крови: прыгает через лужи крови, рывки, призывает галлюцинации-копии; лут — бутылка крови/плащ.

- **Код:** [`/mob/living/simple_animal/hostile/megafauna/bubblegum`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/megafauna/bubblegum.dm#L30)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/mobs/megafauna/bubblegum.rs`: кровавый след при ходьбе (с грохотом) и густая кровь при ранении (25%), любой кровяной декаль — лужа; руки из луж под целями (шлепок 10 brute через 0.4 с; лежачего — хватка: протаскивает через кровь и пожирает через 0.2 с; две цели по очереди, одной — обе руки); кровавый варп (нет целей на крови или 25% + гнев: из лужи рядом в лужу в 5 тайлах от цели, 0.3 с) и ярость (неуязвим для снарядов, 0.375 с/тайл, 3.5–7 с по гневу, не чаще раза в 3 срока); выше половины здоровья тройной рывок (0.6/0.4/0.2 с, 2 тайла за цель, тайл за 0.05 с, 30 brute, стены и порода на пути взрываются), ниже — галлюцинации (50% + гнев: кольцо из 6 копий радиуса 6; в ярости 2/3 раз — три кольца по 4 и тройной рывок) или окружение (5 раундов по 2 копии и свой рывок); копии бьют на 15 и лопаются кровью; держит дистанцию 5, вплотную — в ярости или к обездвиженной цели; лёгкие взрывы игнорирует, сильные — как лёгкие; саркофаг: H.E.C.K.-костюм и бутылка безумия или коса душ (предметы — группа лута Лаваленда); съеденный труп оставляет жвачку; один за раунд на Лаваленде (`placement.rs`); `// shortcut:` нет отступления (держит дистанцию), брони на шлепок, урона плотным объектам/мехам при рывке, ломания стен вдоль пути, разблокировки шаттла

### Колосс
Стреляет узорами снарядов (спирали, залпы), лут — кристаллы аномалий.

- **Код:** [`/mob/living/simple_animal/hostile/megafauna/colossus`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/megafauna/colossus.dm#L23)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/mobs/megafauna/colossus.rs`: снаряды смерти (25 brute, пробитие 100, 5 тайлов/с; по турфу/объекту — тяжёлый взрыв), «Judgement.» спираль 80 снарядов (на трети здоровья — две встречные), «Wrath.» 32 случайных, «Retribution.» дробовик 6, «Lament.» диагонали/кардинали, «Perish.» финал на 10% здоровья, телеграф (тряска, звук), гнев по потерянному здоровью, ярость против спящего карпа, щит при попадании; сундук: случайный аномальный кристалл, голосовые связки колосса, Каин и Авель (предметы-заглушки — их механики в группе лута Лаваленда); `// shortcut:` нет вспышки экрана, распыления трупов, пробивания своего сундука

### Пепельный дракон
Огненное дыхание, лавовые поля, налёты с неба; малый дракон.

- **Код:** [`/mob/living/simple_animal/hostile/megafauna/dragon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/megafauna/drake.dm#L32)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/megafauna/drake.rs`: огненный конус (3 линии по 15 тайлов, 0.15 с на тайл, 20 burn + поджог), метеоры (11% турфов в радиусе 9, 10 burn, бурят породу), налёт (неуязвим, бесплотен, перелёт на цель, лавовые лужи 30 шт., приземление 75 brute с отбрасыванием, затем конус и метеоры), ниже половины здоровья — лавовая арена (огненные стены, 3 раунда лавы с безопасной клеткой на игрока; сбежавший игрок — исцеление 250, ускорение, массовый огонь) и массовый огонь (12 линий × 3 оборота); лёгкие взрывы игнорирует, труп остаётся, сундук с одним из 4 предметов; малый дракон (`dragon/lesser`: без метеоров и массового огня, без арены, кулдаун налёта 20 с, без лута; превращение в него от крови дракона — группа лута Лаваленда); лава — только визуал (лавовые турфы — группа окружения Лаваленда), урона мехам

### Иерофант
Босс-арена: кубы-волны, телепорт, лабиринт ударных линий; лут — посох иерофанта.

- **Код:** [`/mob/living/simple_animal/hostile/megafauna/hierophant`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/megafauna/hierophant.dm#L37)
- **Статус в ss13-rs:** ❌ нет

### Легион (мегафауна)
Гигантский череп, при ранении раскалывается на меньших легионов; лут после последнего.

- **Код:** [`/mob/living/simple_animal/hostile/megafauna/legion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/megafauna/legion.dm#L24)
- **BandaStation:** [`legion.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/megafauna/legion.dm#L66) — флаг `last_legion` для выдачи лута только с последнего
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/mobs/megafauna/legion.rs`: `OpenFire` раз в 2 с — `rand(4)`: череп-`legion_brood` на цель с фракцией Легиона (0–2), рывок (3: «charges!», 6 с без дистанции и на полной скорости, через 2 с бросок на цель на 7 тайлов, удар `22 × size / 2` brute и отброс на 2 тайла), стражи (4: от 2 до `size × 2` `legionturret` на свободных турфах в 4 тайлах; через 1.8 с прицел на видимого сознательного не-шахтёрского моба в 9 тайлах или случайную сторону, через 0.8 с пронзающий хитскан 19 burn на 6 тайлов, распад через 0.5 с); держит дистанцию 5; лежащего без сознания человека заражает новый череп; смерть раскалывает: большой → 3 средних (left/right/eye, 350) → по 3 малых (200); последний живой Легион роняет посох бурь (BandaStation `last_legion`), остальные — сундук щупальца в 20%, иначе кости; `// shortcut:` нет отступления, анимации вращения, визуала луча, ворот некрополя, брони против импульса; содержимое сундука щупальца и механика посоха — группа лута Лаваленда

### Мегафауна Ледяной луны
Вендиго (ударные волны, рывки), демонический морозный шахтёр (фазы, ледяные снаряды), заводной защитник.

- **Код:** [`/mob/living/simple_animal/hostile/megafauna/wendigo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/megafauna/wendigo.dm#L9), [`/mob/living/simple_animal/hostile/megafauna/demonic_frost_miner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/megafauna/demonic_frost_miner.dm#L10), [`/mob/living/simple_animal/hostile/megafauna/clockwork_defender`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/megafauna/clockwork_knight.dm#L9)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/megafauna/mod.rs`: заводной защитник (`clockwork_defender`: без дальних атак, обзор 9/9, `rapid_melee = 2` — два удара за тик ИИ, при смерти — заводной сплав); нет вендиго (ударные волны, рывки, фазы) и демонического морозного шахтёра (фазы, ледяные снаряды)

### Элиты Лаваленда
Опухоль-элита призывает гост-игрока в элитного моба для дуэли с шахтёром на арене: мать голиафов, геральд, легионер, пандора; победитель получает награду или элиту-питомца.

- **Код:** [`/mob/living/simple_animal/hostile/asteroid/elite`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/mining_mobs/elites/elite.dm#L9), [`broodmother`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/mining_mobs/elites/goliath_broodmother.dm#L19), [`herald`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/mining_mobs/elites/herald.dm#L19), [`legionnaire`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/mining_mobs/elites/legionnaire.dm#L19), [`pandora`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/mining_mobs/elites/pandora.dm#L19)
- **Статус в ss13-rs:** ❌ нет

### Слизни-ooze
Желеобразные игровые мобы (от ксенобиологии/админов): поглощают предметы и мобов, слизь-виноградины, способности кислоты и паутины.

- **Код:** [`/mob/living/simple_animal/hostile/ooze`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/ooze.dm#L2), [`ooze/gelatinous`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/ooze.dm#L107), [`ooze/grapes`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/hostile/ooze.dm#L296)
- **Статус в ss13-rs:** ❌ нет

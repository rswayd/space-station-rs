# ИИ мобов
Контроллеры ИИ на деревьях поведения (`code/datums/ai`), подсистемы, которые их крутят, и админ-инструменты. В ss13-rs: `crates/ss13_game/src/mobs/ai/`.

### Контроллер ИИ
Датум, «вселяющийся» в моба или предмет (`pawn`): хранит «доску» (`blackboard`) с ключами-целями, текущее дерево поведения, способ движения, статус (вкл/выкл/простаивает); отключается, когда моб под управлением игрока.

- **Код:** [`/datum/ai_controller`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/_ai_controller.dm#L7), [`/datum/ai_controller/basic_controller`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/basic_mobs/base_basic_controller.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ai/mod.rs` (`AiController`, `Blackboard`, `Key`): контроллер на мобе, доска, статус по tg `get_expected_ai_status`: выкл мертвым и под игроком, вкл в зонах станции и шаттлов всегда и рядом с игроком, иначе `AI_STATUS_ON_LOW` с `RUN_WHILE_UNWATCHED` (вкл с `ALWAYS_HIGH_PRIORITY`), иначе выкл; `STOP_MOVING_WHEN_PULLED`; нет паузы при do_after, переопределений поддеревьев

### Деревья поведения
Решения ИИ — дерево узлов: композиты (sequence, selector, subplan, parallel), декораторы-условия (ключ доски задан, цель видна, кулдаун, дистанция и т.п.), поддеревья; деревья хранятся в файлах `*.bt.json` рядом с кодом.

- **Код:** [`/datum/bt_node`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/_ai_bt_node.dm#L2), [`/datum/bt_node/composite`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/_ai_bt_composites.dm#L4), [`/datum/bt_node/decorator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/_ai_bt_decorators.dm#L7), [`/datum/bt_node/subtree`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/_ai_bt_subtree.dm#L13), [`generic_decorators/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/generic_decorators)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ai/bt.rs`: selector, sequence, subplan, parallel (политики tg), декораторы с observer abort (self/lower по индексам исполнения; условия опрашиваются каждый тик), `true_for_time`, листья с `setup`/`perform`/`finish_action` и кулдауном; деревья собраны в коде по `*.bt.json` (`crates/ss13_game/src/mobs/ai/trees.rs`), JSON не читается

### Действия ИИ (behaviors)
Листья дерева: подойти к цели, ближняя и дальняя атака, бродить, говорить фразы, есть, подбирать/использовать предметы, лезть в вентиляцию, копать стены, плести паутину, звать подмогу, убегать, притворяться мёртвым, реакция на опрокидывание, приручение; асинхронные действия с кулдауном.

- **Код:** [`/datum/bt_node/ai_behavior`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/_ai_behavior.dm#L4), [`basic_melee_attack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/basic_mobs/basic_ai_behaviors/basic_attacking.dm#L5), [`basic_ranged_attack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/basic_mobs/basic_ai_behaviors/basic_attacking.dm#L66), [`idle_random_walk`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/generic_behaviors/random_walk.dm#L2), [`random_speech`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/generic_behaviors/speech.dm#L2), [`enter_vent`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/basic_mobs/basic_ai_behaviors/ventcrawling.dm#L7), [`call_reinforcements`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/basic_mobs/basic_ai_behaviors/call_reinforcements.dm#L3), [`generic_behaviors/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/generic_behaviors)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ai/behaviors.rs`: `idle_random_walk`, `idle_dog`, `random_speech` и `random_speech_blackboard` по данным `mobs.ron` (типы `random_speech/*` и `BB_BASIC_MOB_SPEAK_LINES` контроллера, со звуками), `basic_melee_attack`, `move_to_target`, `find_flee_location` (`DEFAULT_BASIC_FLEE_DISTANCE` 9), `capricious_retaliate`, поиск и поедание еды (`find_food`, `move_to_and_eat` → `basic_eating`), `play_with_mouse`, укус провода, шаги миграции карпа, `clear_key`/`set_bb_key`, `tipped_reaction`, команды питомцев (`wait`, `play_dead`, `clear_pet_command`), `find_partner`/`make_babies`, `parrot_repeat_speech`, `basic_ranged_attack`, `maintain_distance`, `use_mob_ability` черепахи, `hunt_target` черепахи, `set_bb_cooldown` (`crates/ss13_game/src/mobs/ai/fauna.rs`); нет вентиляции, подкреплений, паутины, остальных способностей мобов, `perform_emote` (в tg ключ «eats up happily!» не находит эмоцию)

### Выбор целей
Стратегии таргетинга (кого можно атаковать: живые, не друзья, без фракции моба; особые — повреждённые машины, кабели, гидропоника, еда для кошек) и приоритеты целей; обновление целей в обзоре.

- **Код:** [`/datum/targeting_strategy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/basic_mobs/targeting_strategies/_targeting_strategy.dm#L3), [`/datum/targeting_strategy/basic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/basic_mobs/targeting_strategies/basic_targeting_strategy.dm#L1), [`acquire_target/update_combat_targets`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/basic_mobs/basic_ai_behaviors/targeting.dm#L10)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ai/behaviors.rs` (`Strategy`, `acquire_combat_target`): `targeting_strategy/basic` (фракция с `BB_TEMPORARILY_IGNORE_FACTION`, смерть, видимость через непрозрачные клетки), `holding_object` (щипцы для корги), `huntable_mouse`, `pickup_item` для еды из `BB_BASIC_FOODS` (`crates/ss13_game/src/mobs/eating.rs`), `update_combat_targets` (9 клеток, удержание до 16), `target_from_retaliate_list` (без проверки фракции при выборе, как tg `check_faction = FALSE`); нет приоритетов, поиска спрятавшихся в шкафах, машин, `of_size`

### Типовые контроллеры мобов
Готовые шаблоны поведения: враждебный, ломающий препятствия, стрелок, отступающий стрелок, застрельщик, со способностями, мстительный (бьёт только в ответ), капризный, пугливый, шуганый, «гопник».

- **Код:** [`/datum/ai_controller/basic_controller/simple`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/basic_mobs/generic_controllers.dm#L2), [`simple_hostile`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/basic_mobs/generic_controllers.dm#L15), [`simple_retaliate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/basic_mobs/generic_controllers.dm#L90), [`simple_fearful`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/basic_mobs/generic_controllers.dm#L100)
- **BandaStation:** [`modular_bandastation/mobs/code/ai_controller.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/ai_controller.dm#L6) — базовый контроллер `base_animal` и фразы для мобов форка
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ai/trees.rs`: шаблоны `simple_hostile` (и `_obstacles` без ломания препятствий), `simple_retaliate`, `simple_fearful`, `simple_skittish`, `simple_capricious`, `talk`, поддеревья `skittish_and_speak`, `skittish_brawler_combat`, `forage_and_retaliate`, `capricious_pick_target`, `find_food`/`move_to_and_eat`; контроллеры форка `base_animal`, `possum`, `lizard/big`, `crocodile`, `chicken/cock`, `mouse/rat/syndi`; стрелковые `simple_ranged`, `simple_ranged_retaliate`, застрельщик `simple_skirmisher`, хайвботы, краб, черепаха, попугай, стикмены, зомби, скелеты, клоуны, ксеноморфы, гномы, кот-хирург; поддерево команд питомцев; `STOP_MOVING_WHEN_PULLED` из `ai_traits` контроллера; ветки tg с `BT_ABORT_SELF` рядом с бесконечной прогулкой прерывают её (в tg они недостижимы); нет способностей (кроме дерева черепахи), `simple_goon`; непортированные контроллеры только бродят (`// shortcut:`)

### Месть и бегство при ранении
Элементы `ai_retaliate` (обидчик заносится в цели) и `ai_flee_while_injured` (моб убегает при низком здоровье).

- **Код:** [`/datum/element/ai_retaliate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/ai_retaliate.dm#L5), [`/datum/element/ai_flee_while_injured`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/ai_flee_while_injured.dm#L4)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/mobs/ai/mod.rs` (`record_attacker`, `flee_while_injured`): обидчик в `BB_BASIC_MOB_RETALIATE_LIST` у мобов с `ai_retaliate` (карп, кошка, лиса и мобы шаблонов в `trees.rs`), `ai_flee_while_injured` (карп, свинья, пони, пингвин) бегут при здоровье ≤ 50% и возвращаются при полном

### Размножение и голод
Поддеревья поиска партнёра и рождения детёнышей (лимит популяции), поиска и поедания еды.

- **Код:** [`/datum/bt_node/ai_behavior/find_partner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/babies/babies_behaviors.dm#L4), [`consume`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/generic_behaviors/consume.dm#L2), [`generic_subtrees/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/generic_subtrees)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ai/trees.rs` (`find_food`, `move_to_and_eat`), `crates/ss13_game/src/mobs/eating.rs`: мобы ищут в обзоре свою еду (`BB_BASIC_FOODS`), подходят и съедают её, `BB_NEXT_FOOD_EAT` через `EAT_FOOD_COOLDOWN` (45 с; моли-тараканы 60 с); кошки и корги ищут партнёра и рожают детёнышей (`find_partner`, `make_babies`, `crates/ss13_game/src/mobs/pets.rs`); нет `consume`/голода

### Движение ИИ
Способы перемещения: «тупой» шаг к цели, обход препятствий, поиск пути JPS (боты, мулботы), полная остановка; счётчик неудач пути.

- **Код:** [`/datum/ai_movement`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/movement/_ai_movement.dm#L2), [`/datum/ai_movement/dumb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/movement/ai_movement_dumb.dm#L2), [`/datum/ai_movement/basic_avoidance`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/movement/ai_movement_basic_avoidance.dm#L2), [`/datum/ai_movement/jps`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/movement/ai_movement_jps.dm#L4)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ai/movement.rs`: `basic_avoidance` (шаг к цели с обходом на 45°), объезд через ограниченный A* (`// shortcut:` вместо JPS; для обычных мобов двери — стены, боты прокладывают путь через шлюзы, открываемые их картой доступа, длиной до `AI_BOT_PATH_LENGTH`), провал после 8 заблокированных шагов; скорость моба `speed`

### ИИ мартышек
Мартышки (и Пун-Пун бармена) ищут оружие, мстят обидчикам, дурачатся, разносят еду; применяется и к людям, обращённым в мартышек.

- **Код:** [`/datum/ai_controller/monkey`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/monkey/monkey_controller.dm#L7)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/mobs/ai/monkey.rs`, `trees.rs` (`monkey`): контроллер `monkey`/`monkey/angry` на карбоне без игрока (мартышки консоли ксенобиологии), ненависть к обидчику (85%, 4 очка; удар в боевом режиме или предметом), выбор цели среди врагов в обзоре 9 клеток с весом `100 / дистанция` (злые — любые, кроме мартышек и выведенных из строя), бой через обычный ближний бой человека (свой `ClickResolved` в боевом режиме), снижение ненависти на 40% удачных ударов, бегство ниже 40 здоровья, поиск и подбор самого сильного предмета в 5 клетках (черный список неудачных), `monkey_idle`; нет карманных краж, обезоруживания, толчков, утилизации в мусоропровод, созыва мартышек, Пун-Пуна (подача еды, звонок), инструментов, оружия, эмоций (`// shortcut:`)

### ИИ собак и кошек
Собаки апортируют, преследуют обидчиков хозяина, корги открывают двери по доступу; кошки охотятся на мышей, носят еду, выпрашивают.

- **Код:** [`/datum/ai_controller/basic_controller/dog`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/dog/dog_controller.dm#L1), [`/datum/ai_controller/basic_controller/cat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/cat/cat_ai.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ai/trees.rs`: собаки бродят/крутятся (`idle_dog`) и лают, корги убегают от держащих щипцы, кошки охотятся на мышей (70%, Том 5%) и бьют враждебных мелких мобов; команды питомцам и размножение; нет апорта, открывания дверей корги, еды, домиков

### ИИ предметов и машин
Проклятые и одержимые предметы летают и бьют людей, «бейн»-предмет, робот-посетитель ресторана заказывает еду, торговый автомат-мститель падает на людей.

- **Код:** [`/datum/ai_controller/cursed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/cursed/cursed_controller.dm#L8), [`/datum/ai_controller/haunted`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/hauntium/haunted_controller.dm#L2), [`/datum/ai_controller/bane`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/bane/bane_controller.dm#L5), [`/datum/ai_controller/robot_customer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/robot_customer/robot_customer_controller.dm#L1), [`/datum/ai_controller/vending_machine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/objects/vending_machines/vending_machine_controller.dm#L2), [`/datum/bt_node/ai_behavior/throw_attack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/_item_behaviors.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Телеграф атак ИИ
Временный визуальный эффект-предупреждение на клетке перед атакой моба.

- **Код:** [`/obj/effect/temp_visual/telegraphing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/telegraph_effects.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/effects/temp_visual.rs` (`Telegraph`, `telegraph`): `telegraphing` (рамка, 2 с), `vending_machine_tilt` (1 с), `lift_travel` с длительностью, `circle`/`circle/short`, `line`/`line/short` под мобами; без свечения (`light_range`, эмиссив — `// shortcut:`); вызовы из механик (автомат-мститель, посох Легиона, реактивная броня) — у их групп

### Подсистема ai_controllers
`SSai_controllers` тикает активные контроллеры; контроллеры на z-уровнях без игроков простаивают (`ai_controllers_by_zlevel`); отдельная подсистема низкого приоритета для второстепенного ИИ.

- **Код:** [`SUBSYSTEM_DEF(ai_controllers)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ai_controllers.dm#L5), [`fire()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ai_controllers.dm#L75), [`low_priority_ai_controllers`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/ai_controllers_low_priority.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ai/mod.rs` (`plan`, `AiStats`): планирование раз в 0,25 с с реальным `seconds_per_tick`, выключенные контроллеры стоят только проверки статуса, `ON_LOW` планируется с той же частотой (MetaStation, 56 мобов с косяком карпов: ~230 мкс на проход без игроков, ~340 мкс с игроком, debug); нет отдельного низкоприоритетного тикера

### Подсистема npcpool
Процессинг старых `simple_animal` без контроллеров (мегафауна, элиты): их `handle_automated_action`/движение.

- **Код:** [`SUBSYSTEM_DEF(npcpool)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/npcpool.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Просмотрщик деревьев поведения (админ)
Админ-верб «View Behavior Tree»: окно с живым деревом выбранного моба — какие узлы активны, лог выполнения.

- **Код:** [`/datum/bt_viewer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/bt_viewer.dm#L3), [`ADMIN_VERB(view_behavior_tree)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/admin_verbs.dm#L784); tgui: [`BehaviorTreeViewer`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/BehaviorTreeViewer)
- **Статус в ss13-rs:** ❌ нет

### Выдача ИИ админом
Админ-верб «Give AI Controller»: шаблоны ИИ (`admin_ai_template`) вешаются на любого живого моба с настройкой фракции и поведения.

- **Код:** [`/datum/admin_ai_template`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ai/basic_mobs/admin_ai_templates.dm#L2), [`ADMIN_VERB(give_ai_controller)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/admin_verbs.dm#L742)
- **Статус в ss13-rs:** ❌ нет

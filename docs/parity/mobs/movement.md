# Передвижение мобов, скорость движения и действий
Шаги моба, ходьба/бег, модификаторы скорости, невесомость, подскальзывание, перемещение по z-уровням и вентиляции.

### Шаг моба (client/Move)
Нажатая клавиша направления двигает моба на тайл не чаще его `movement_delay`; проверяются оглушение, захват, пристёгнутость, наручники; при неудаче моб поворачивается. Диагональные шаги — два кардинальных.

- **Код:** [`client/Move()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob_movement.dm#L46), [`/mob/Moved()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob_movement.dm#L589)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/server.rs`, `crates/ss13_game/src/prediction.rs`: шаги с кулдауном, клиентское предсказание, проверки статуса и захвата

### Ходьба и бег (move intent)
Переключатель ходьбы/бега: бег быстрее (`RUN_DELAY`, из конфига), ходьба тише и не подскальзывает на мокром полу.

- **Код:** [`proc/toggle_move_intent()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob_movement.dm#L515), [`proc/set_move_intent()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob_movement.dm#L526), [`/datum/movespeed_modifier/config_walk_run`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/movespeed/modifiers/mobs.dm#L51)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/janitor.rs` (`Walking`, `ClientIntent::ToggleMoveIntent`), `crates/ss13_game/src/health.rs` (`WALK_DELAY` вместо `RUN_DELAY`), `crates/ss13_game/src/interact_client.rs` (кнопка HUD): ходьба медленнее, тише (`crates/ss13_game/src/footsteps.rs`) и не скользит по воде

### Модификаторы скорости движения
Система `movespeed_modifier`: аддитивные и мультипликативные модификаторы от урона, ожирения, одежды (`slowdown`), волочения, реагентов, статус-эффектов, конечностей, вида; результат кэшируется в `cached_multiplicative_slowdown`.

- **Код:** [`/datum/movespeed_modifier`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/movespeed/_movespeed_modifier.dm#L28), [`proc/add_movespeed_modifier()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/movespeed/_movespeed_modifier.dm#L77), [`proc/update_movespeed()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/movespeed/_movespeed_modifier.dm#L175), [`code/modules/movespeed/modifiers/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/movespeed/modifiers)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/health.rs`, `crates/ss13_game/src/pull.rs`, `crates/ss13_game/src/hunger.rs`, `crates/ss13_game/src/status_effects.rs`: урон, крит/ползание, волочение, ожирение, stagger; нет общей системы модификаторов, замедления одежды, реагентов, конечностей

### Модификаторы скорости действий
`actionspeed_modifier` ускоряет или замедляет `do_after` (наркотики, настроение, раны, зависимости, статус-эффекты).

- **Код:** [`/datum/actionspeed_modifier`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/actionspeed/_actionspeed_modifier.dm#L23), [`proc/add_actionspeed_modifier()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/actionspeed/_actionspeed_modifier.dm#L64), [`proc/update_actionspeed()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/actionspeed/_actionspeed_modifier.dm#L150), [`code/modules/actionspeed/modifiers/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/actionspeed/modifiers)
- **Статус в ss13-rs:** ❌ нет — `do_after` в `crates/ss13_game/src/tools.rs` без модификаторов

### Спотыкание от слишком быстрого бега (BandaStation)
Модпак форка: если суммарный множитель скорости человека ниже порога (0.85), он с шансом 50% спотыкается, а при очень высокой скорости (ниже 0.3) может сломать ноги.

- **Код:** [`/datum/movespeed_modifier/dna_vault_speedup`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/movespeed/modifiers/innate.dm#L18)
- **BandaStation:** [`modular_bandastation/movespeed_limit/code/movement.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/movespeed_limit/code/movement.dm#L16) — механика спотыкания
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/stumble.rs`: после каждого перемещения человека с задержкой шага < 0,85 дс — 50% упасть (нокдаун 2 с), < 0,3 дс — ещё шанс `(0,3 − d) / 0,3` повредить обе ноги (40 brute); исключения `status_speed_boost` нет — его не даёт ни один перенесённый эффект

### Невесомость и инерция
Без гравитации моб плывёт по инерции (`newtonian_move`, `drift_handler`) и может оттолкнуться только от стены, решётки или предмета; магнитные ботинки и джетпаки дают контроль.

- **Код:** [`mob/Process_Spacemove()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob_movement.dm#L264), [`proc/newtonian_move()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/atoms_movable.dm#L1324), [`/datum/drift_handler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/drift_handler.dm#L4), [`proc/mob_negates_gravity()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob_movement.dm#L367)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/spacemove.rs`: без гравитации живой моб `Weightless` (тревога «Невесомость»), шагает только отталкиваясь от опоры в радиусе тайла (плотный турф, плотный предмет, решётка/мостик, не то, во что идёт); без опоры шаг отклоняется, HUD сообщает клиенту, что моб неподвижен (клиент не предсказывает шаги); незакреплённая опора (ящик, моб) отлетает в обратную сторону (`last_pushoff`); всё, что сдвинуто новой силой (шаг, космический ветер, толчок) и не держится на новом тайле, дрейфует 1 тайл / 0,5 с (`get_loop_delay`) до опоры, столкновения (двери слышат `Bumped`) или гравитации, не поворачиваясь; MOD-магнитные ботинки держат. `// shortcut:` невесомость только над космическими турфами космических зон (нет генераторов гравитации), дрейф только по сторонам света, нет джетпаков (нет действий предметов для них), магнитных ботинок-предметов, отдачи броска, плавающей анимации (`DO_FLOATING_ANIM` — нужна реплицируемая петля, не событие)

### Подскальзывание
Мокрый пол, лёд, смазка, банановая кожура сбивают с ног (`slip`), ходьба спасает от воды; галоши защищают.

- **Код:** [`proc/slip()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob_movement.dm#L381), [`living/slip()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob_movement.dm#L385)
- **BandaStation:** [`code/modules/mob/mob_movement.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob_movement.dm#L381) — параметр обездвиживающего подскальзывания
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/janitor.rs` (`try_slip`): мокрый пол сбивает стоящего (паралич 0,5 с BandaStation, нокдаун, daze), ходьба спасает от воды; `crates/ss13_game/src/botany/produce.rs`: скользкая кожура; MOD-магнитные ботинки (`crates/ss13_game/src/modsuit/modules.rs`); нет льда, смазки, галош, скольжения (`TRAIT_NO_SLIP_SLIDE`)

### Перемещение по z-уровням
Глаголы «Move Upwards/Down» поднимают и опускают моба на соседний z-уровень (по лестницам, в невесомости, полётом); открытое пространство роняет вниз.

- **Код:** [`proc/up()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob_movement.dm#L535), [`proc/down()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob_movement.dm#L559)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/multiz.rs` (`can_z_move`, `z_move`, `move_vertical_intents`): PgUp/PgDn — призрак сквозь перекрытия, живой — по стремянке под ногами или полётом за секунду (летающие простые мобы `simple_flying`, без гравитации); ступени и падение — записи в ../engineering/; клиент видит уровень ниже сквозь открытое пространство и стекло (`crates/ss13_render/src/sprites.rs`, `level_offset`; репликация — `ViewBox::below`, только под прозрачными тайлами; при смене уровня видимое с обоих остаётся на клиенте, без переспавна), кликает и осматривает его (`crates/ss13_game/src/client.rs`). Нет крыльев, джетпаков в невесомости, `NEGATIVE_GRAVITY`, ИИ и камер между уровнями; гравитация есть везде, кроме космоса (генераторов нет)

### Ползание по вентиляции
Мобы с `TRAIT_VENTCRAWLER_*` (мыши, ксенолярвы, голые мартышки и др.) входят в вентиляцию и скрубберы, ползут по трубам и видят их сеть.

- **Код:** [`proc/can_enter_vent()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/ventcrawling.dm#L17), [`proc/handle_ventcrawl()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/ventcrawling.dm#L68), [`proc/update_pipe_vision()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/ventcrawling.dm#L132)
- **Статус в ss13-rs:** ❌ нет

### Навигация
Глагол «Navigate» строит путь к выбранной точке (маяки, лестницы на другой z) и рисует линию на полу.

- **Код:** [`proc/create_navigation()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/navigation.dm#L29), [`proc/find_nearest_stair_or_ladder()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/navigation.dm#L147)
- **Статус в ss13-rs:** ❌ нет

### Поле зрения (FOV)
Мобы с ограниченным полем зрения (некоторые шлемы, черты) не видят, что у них за спиной.

- **Код:** [`proc/in_fov()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_fov.dm#L2), [`proc/update_fov()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_fov.dm#L53)
- **Статус в ss13-rs:** ❌ нет

### Звуки шагов
Элемент `footstep` проигрывает звуки шагов по типу пола и обуви (босиком, когти, тяжёлые ботинки).

- **Код:** [`/datum/element/footstep`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/footstep.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/footsteps.rs`: люди (`FOOTSTEP_MOB_HUMAN`) — каждый второй шаг бегом, обувь/босиком по ключам турфа из таблиц `GLOB.footstep`/`barefootstep` (дамп глобалов, `crates/ss13_mapload/src/sound.rs`), громкость и дальность таблиц, ползком `crawl1.ogg`; `// shortcut:` нет шагов базовых мобов (когти, тяжёлые), `footstep_override` (кэтуоки, столы), видов и протезов, `TRAIT_LIGHT_STEP`

### Попиксельный сдвиг (BandaStation)
Модпак форка: зажатая клавиша сдвигает спрайт моба в пределах тайла (и слой), чтобы встать к стене или за стол.

- **Код:** [`/datum/component/pixel_shift`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/pixel_shift/code/pixel_shift_component.dm#L1), [`/datum/keybinding/mob/pixel_shift`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/pixel_shift/code/pixel_shift_keybind.dm#L1)
- **BandaStation:** [`modular_bandastation/pixel_shift/code/pixel_shift_component.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/pixel_shift/code/pixel_shift_component.dm#L1) — весь механизм
- **Статус в ss13-rs:** ❌ нет

### Езда верхом и транспорт
Компонент `riding` позволяет ехать на мобах (боргах, животных, людях на плечах) и транспорте, с правилами смещения и скорости.

- **Код:** [`/datum/component/riding`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/riding/riding.dm#L14)
- **Статус в ss13-rs:** ❌ нет

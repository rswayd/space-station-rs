# Пожарные, гермо- и окно-двери
Пожарные шлюзы и пожарные тревоги, гермодвери и ставни с кнопками, окно-двери (windoors) и их сборка, таймеры камер, парольные и минеральные двери.

### Пожарные шлюзы (firelocks)
Закрываются сами при опасном давлении/температуре на соседних тайлах или пожарной тревоге области и открываются обратно, когда опасность ушла; ломом/руками открываются вручную; есть краевые (border) и тяжёлые варианты.

- **Код:** [`/obj/machinery/door/firedoor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/firedoor.dm#L6), [`proc/check_atmos()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/firedoor.dm#L279), [`proc/activate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/firedoor.dm#L364), [`proc/reset()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/firedoor.dm#L397), [`/obj/machinery/door/firedoor/border_only`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/firedoor.dm#L745), [`/obj/machinery/door/firedoor/heavy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/firedoor.dm#L813)
- **BandaStation:** [`firedoor.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/firedoor.dm#L500) — вместо стука: пожарный шлюз открывается рукой за 5 с и через 2 с возвращается в нужное состояние
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/wallmount/firelock.rs`, `crates/ss13_atmos/src/level.rs` (`watch`, `drain_watch_reports`), `crates/ss13_game/src/doors.rs` (`DoorKind::Firedoor`): шлюз следит за воздухом своего и четырёх соседних тайлов (тайл обработан LINDA или изменилась смежность — `process_results`): от `FIRE_MINIMUM_TEMPERATURE_TO_EXIST` или с антинобилием — жар, до `BODYTEMP_COLD_DAMAGE_LIMIT` — холод; ряд соседних шлюзов (merge group) с общим списком проблемных тайлов закрывается, огни `firelock_alarm_type_hot/cold/generic` (у краевых со сдвигом), зоны красные, тревоги мигают, заметивший шлюз воет; ушла последняя проблема — ряд открывается; сброс пожарной тревоги приоткрывает (`crack_open`) и через 3 с закрывает снова, если проблема осталась; рукой (BandaStation) — 5 с `do_after`, активный закрывается через 2 с; ломом ЛКМ — держать открытым, пока стоишь рядом (3 с без атмос-активации), ПКМ — открыть (активный закроется через 2 с), открытый — закрыть; удар в боевом режиме; осмотр; `/closed` с тревогой с карты; краевые и тяжёлые грузятся с карты. Нет: сварки (`welded`), разборки, емага, открытия силиконами/ксеноморфами, `TRAIT_FIREDOOR_STOP`, отчётов менеджеру тревог, пересчёта зон при смене соседнего тайла; tg-баг с повторным закрытием ряда без проблем после `crack_open` не перенесён

### Рама пожарного шлюза
Сборка пожарного шлюза: рама из листов, кабель, электроника, укрепление пласталью; разборка ломом и сваркой.

- **Код:** [`/obj/structure/firelock_frame`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/firedoor.dm#L827)
- **Статус в ss13-rs:** ❌ нет

### Пожарная тревога
Настенная пожарная тревога: срабатывает от жара в области или вручную, закрывает пожарные шлюзы области, мигает свет; сброс; показывает уровень тревоги станции; сборка из рамы, кабеля и электроники.

- **Код:** [`/obj/machinery/firealarm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/firealarm.dm#L20), [`proc/alarm()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/firealarm.dm#L250), [`proc/reset()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/firealarm.dm#L273), [`/obj/item/wallframe/firealarm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/firealarm.dm#L5)
- **BandaStation:** [`firealarm.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/firealarm.dm#L625) — смещение на стене; [`aesthetics/firealarm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/firealarm/code/firealarm.dm) — спрайты
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/wallmount/fire_alarm.rs`, `crates/ss13_mapload/src/wallmount.rs` (`DmWallMount`): ЛКМ — тревога (питание, зона ещё не в тревоге): пожарные шлюзы своей зоны и граничащих с ней открытых тайлов закрываются, первый активный шлюз ставит зоне `fire` (лампы красные, `fire_*` в `crates/ss13_game/src/power/mod.rs` `set_area_fire`), тревога воет (сирена 2,4 с), мигает красным и переходит на активное потребление (`update_use_power`); ПКМ — сброс (`crack_open`, обратно в простой). От жара/холода срабатывает через шлюзы (запись «Пожарные шлюзы»): зона краснеет, тревога мигает без воя. Осмотр: огонь опасности, место и тип неисправности (автоматически/вручную), уровень угрозы, датчик. Оверлеи/свет по состоянию и питанию. Уровень угрозы: оверлей `fire_<номер>` и цвет света уровня, смена уровня обновляет все тревоги (`on_security_level_changed`). Нет: менеджера тревог (консоли, ИИ), отключения термодатчиков (ctrl-клик силикона, мультитул), вида вне станции, сборки из рамы/проводов/платы, урона, емага, ЭМИ

### Гермодвери и ставни
Гермодвери (`poddoor`) и ставни управляются кнопками/контроллерами по `id`, рукой не открываются; ломом — только без питания; разборка (отвёртка, кусачки, сварка); радиационные, оконные, неразрушимые ставни.

- **Код:** [`/obj/machinery/door/poddoor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/poddoor.dm#L1), [`poddoor/item_interaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/poddoor.dm#L95), [`/obj/machinery/door/poddoor/shutters`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/shutters.dm#L1), [`/obj/machinery/door/poddoor/shutters/radiation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/shutters.dm#L52)
- **BandaStation:** [`aesthetics/blastdoor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/blastdoor/code/blastdoor.dm) — спрайты и звуки гермодверей
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/doors.rs` (`DoorKind::Poddoor`: кадры и тайминги `open/opening/closed/closing`), `crates/ss13_game/src/wallmount/controls.rs`: открываются и закрываются кнопками по `id`; нет лома без питания, разборки, радиационных/неразрушимых особенностей

### Кнопки и контроллеры дверей
Настенные кнопки с устройством внутри (`assembly/control`): открыть гермодвери по `id`, болтировать шлюзы, включить аварийный доступ, зажигатели, массдрайверы; доступ по ID, установка платы и сборки.

- **Код:** [`/obj/machinery/button`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/buttons.dm#L1), [`proc/attempt_press()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/buttons.dm#L302), [`/obj/machinery/button/door`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/buttons.dm#L397), [`/obj/item/assembly/control`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/doorcontrol.dm#L1), [`/obj/item/assembly/control/airlock`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/doorcontrol.dm#L130)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/wallmount/controls.rs`, `crates/ss13_mapload/src/wallmount.rs`, `crates/ss13_mapload/src/door.rs` (`id`, `id_tag`): нажатие (пустая рука или предмет вне боевого режима) с питанием и доступом по `req_access`/`req_one_access`, мигание `-overlay-success/error`; контроллер гермодверей (`sync_doors`), шлюзов (`specialfunctions`: открыть/закрыть, ID-скан, болты, ток, безопасность), масс-драйвера (открыть двери, через 1 с выстрел масс-драйверов, через 6 с закрыть), зажигания (искрители и зажигатели, `crates/ss13_game/src/wallmount/machines.rs`), вспышек; откаты контроллеров. Нет: штор, крематория, панели (сборки и платы внутрь/наружу, скины), префиксов id шаттлов, монтажа

### Окно-двери (windoors)
Краевые стеклянные двери: открываются толчком или кликом с доступом и закрываются через несколько секунд; емаг ломает; лом без питания; разбиваются на стекло и прутья; бриговые (укреплённые) варианты.

- **Код:** [`/obj/machinery/door/window`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/windowdoor.dm#L1), [`proc/open_and_close()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/windowdoor.dm#L144), [`emag_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/windowdoor.dm#L373), [`crowbar_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/windowdoor.dm#L408), [`/obj/machinery/door/window/brigdoor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/windowdoor.dm#L497)
- **BandaStation:** [`windowdoor.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/windowdoor.dm#L22) — звук стука по стеклу; [`aesthetics/windoors`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/windoors/code/windoors.dm) — спрайты
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/doors.rs` (`DoorKind::Windoor`), `crates/ss13_mapload/src/border.rs`: проход только через край, доступ, тайминги, питание; нет емага, лома, разрушения, разборки

### Сборка окно-двери
Каркас окно-двери (`windoor_assembly`): стекло/укреплённое стекло, кабель, электроника, лом завершает; поворот и отражение.

- **Код:** [`/obj/structure/windoor_assembly`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/windoor_assembly.dm#L4)
- **BandaStation:** [`windoor_assembly.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/windoor_assembly.dm#L380) — санитизация имени
- **Статус в ss13-rs:** ❌ нет

_«Таймер камеры брига» описано в [../security/brig-prison.md](../security/brig-prison.md)._

### Парольные двери
Двери, открывающиеся по произнесённому паролю (голосовые) или вводу — для руин и ивентов.

- **Код:** [`/obj/machinery/door/password`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/passworddoor.dm#L1), [`/obj/machinery/door/password/voice`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/passworddoor.dm#L25)
- **Статус в ss13-rs:** ❌ нет

### Минеральные двери
Простые двери-структуры из материалов (железо, серебро, золото, уран, песчаник, дерево, бумага): открываются рукой без доступа, копаются киркой, деревянные горят.

- **Код:** [`/obj/structure/mineral_door`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/mineral_doors.dm#L4), [`proc/TryToSwitchState()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/mineral_doors.dm#L82), [`proc/pickaxe_door()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/mineral_doors.dm#L164)
- **Статус в ss13-rs:** ❌ нет

### Блокировка дверей ИИ/враждебная
`hostile_lockdown`: двери закрываются и болтируются при угрозе (например, ИИ-малф или админ-событие).

- **Код:** [`proc/hostile_lockdown()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/door.dm#L752), [`proc/disable_lockdown()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/doors/door.dm#L756)
- **Статус в ss13-rs:** ❌ нет

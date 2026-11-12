# Сборки (assemblies): сигналеры, таймеры, сенсоры

Маленькие устройства, которые посылают и принимают импульсы: соединяются попарно в держатель, вешаются на провода машин, бомбы и шлюзы.

Вспышка (`/obj/item/assembly/flash`) — см. ../security/; транзитный клапан (TTV) и гранаты — см. ../items/.

### Базовая сборка и импульсы
Сборка: состояние «закреплено/открыто» (отвёртка), импульс `pulse()`/`pulsed()`, присоединение к проводам (`/datum/wires/attach_assembly`) и к держателю.

- **Код:** [`/obj/item/assembly`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/assembly.dm#L2), [`pulsed()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/assembly.dm#L87), [`/datum/wires/proc/attach_assembly()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/_wires.dm#L214)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/assemblies/server.rs`: `Assembly` на каждом сигналере, таймере, датчиках, воспламенителе, анализаторе и ИК-излучателе при спавне; отвёртка переключает `secured`; `pulse()` идёт в провода (`crate::wires::pulse_assembly`), в держатель (остальные части `pulsed` -> `activate` с откатом 3 с, затем бак-хозяин) или в клапан TTV; поведение `ASSEMBLY_INPUT/TOGGLE_ARMED/FUNCTIONAL_OUTPUT` решает, что вешается на провода (../engineering/wires.md); детали внутри держателя/проводов не берутся рукой. Нет: `on_found`/`attack_hand`-срабатываний (мышеловки), значков-оверлеев состояния, переменных экземпляров с карты (сигналеры с карты — на частоте и коде по умолчанию)

### Держатель сборок
Две сборки, соединённые вместе (например, таймер + воспламенитель); держатель передаёт импульс от одной к другой, крепится к бакам (бомба из бака) и к проводам.

- **Код:** [`/obj/item/assembly_holder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/holder.dm#L1), [`assemble()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/holder.dm#L31), [`/obj/item/tank/proc/bomb_assemble()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/tanks.dm#L454)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/assemblies/server.rs` (`combine`, `try_add`, `disassemble`, `process_activation`), `crates/ss13_game/src/assemblies/ttv.rs` (`bomb_assemble`, `ignite`): две незакреплённые сборки дают держатель на полу (имя «A-B assembly»), в него добавляются ещё (до 12, без второго воспламенителя), отвёртка разбирает (части падают закреплёнными), использование в руке — каждая часть (одна часть: «part missing!»); держатель с воспламенителем вешается на бак не в контейнере, сварка запечатывает бак («bomb armed»), ключ снимает держатель; сигнал — писк и через 1 с `ignite`: запаянный бак взрывается по температуре и топливу (плазма + O₂/6, пороги tg), иначе газ выходит на тайл. Нет: составного спрайта держателя, вращения (`simple_rotation`), веса бака-бомбы (`WEIGHT_CLASS_BULKY`)

### Удалённый сигналер
Радиосигналер: частота и код, отправка сигнала всем сигналерам с теми же настройками (импульс), приём; варианты для киборга, внутренний, малого радиуса.

- **Код:** [`/obj/item/assembly/signaler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/signaler.dm#L1), [`signal()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/signaler.dm#L151), [`receive_signal()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/signaler.dm#L164); tgui: [`Signaler.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Signaler.tsx)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/assemblies/server.rs` (`signal`, `receive_signal`, `signaler_acts`), окно `Signaler` (`crates/ss13_game/src/assemblies/client.rs`): частота (свободный диапазон, `sanitize_frequency`) и код (1–100) кнопками-шагами, сброс, отправка с откатом 1 с; сигнал — всем другим сигналерам на частоте (range 0: везде, малого радиуса: 9 клеток на уровне), совпавший код — импульс и писк (слышно в 1 клетке); два закреплённых сигналера синхронизируются; киборговый и внутренний не трогаются инструментами. Нет: суицида, логов сигналов (`investigate_log`), сигнала правым кликом в руке, сигнала при вскрытии предательской почты, прочих приёмников (электропак — `comms/radio.md`)

### Таймер
Отсчитывает заданное время и даёт импульс; режим повтора.

- **Код:** [`/obj/item/assembly/timer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/timer.dm#L1); tgui: [`Timer.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Timer.tsx)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/assemblies/server.rs` (`timer_tick`, `timer_acts`), окно `Timer`: старт/стоп, повтор, ±1/±30 с (1–600), по истечении импульс и писк (3 клетки), повтор перезапускает; откат 3 с; осмотр показывает время. Нет: огоньков-оверлеев, суицида

### Датчик движения (proximity)
Срабатывает при движении в радиусе; настраиваемые радиус и задержка взвода.

- **Код:** [`/obj/item/assembly/prox_sensor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/proximity.dm#L1); tgui: [`ProximitySensor.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ProximitySensor.jsx)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/assemblies/server.rs` (`prox_tick`, `sense_moves`, `prox_acts`), окно `ProximitySensor`: взвод через заданное время (затем сброс к 20 с, как tg), чувствительность 0–5, срабатывание на шаг любого в радиусе от хоста, лежащего на тайле (в химгранате-мине — где угодно, кроме шагов её носителя), откат 3 с, писк. Нет: оверлеев

### Воспламенитель и конденсатор
Воспламенитель по импульсу поджигает тайл/содержимое бака (бомбы) и даёт искру; конденсатор — охлаждающий вариант.

- **Код:** [`/obj/item/assembly/igniter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/igniter.dm#L6), [`igniter/condenser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/igniter.dm#L56)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/assemblies/server.rs` (`igniter_activate`): `hotspot_expose(1000, 1000)` тайла, конденсатор — `hotspot_expose(50)` и охлаждение воздуха (`min(293, T·0.85)`, 50…1e6 K); в руке вне держателя активируется сам; в бомбе из бака — см. «Держатель сборок». Нет: искр, `COMSIG_IGNITER_ACTIVATE` (заминированные топливные баки, нагрев стаканов), поджигания предметов (`ignition_effect`), суицида

### Инфракрасный излучатель
Луч в направлении установки; пересечение луча даёт импульс.

- **Код:** [`/obj/item/assembly/infra`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/infrared.dm#L1); tgui: [`InfraredEmitter.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/InfraredEmitter.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/assemblies/server.rs` (`beam_tiles`, `sense_moves`, `infra_acts`), окно `InfraredEmitter`: луч от держателя/клапана/самого излучателя на тайле до 8 клеток, обрывается плотным (кроме `PASSTABLE|PASSGLASS|PASSGRILLE`), плотный шагнувший в луч (не в первую клетку) — импульс, писк, откат 3 с. Нет: видимого луча и его оверлея, вращения (`simple_rotation`: луч идёт туда, куда смотрел включивший, иначе на юг), брошенных предметов в луче

### Голосовой анализатор
Записывает фразу/голос и срабатывает на неё (режимы: включение, распознавание, голос, тишина).

- **Код:** [`/obj/item/assembly/voice`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/voice.dm#L6)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/assemblies/server.rs` (`hear_speech`, `voice_matches`): в руке включает/выключает запись (только отдельно), мультитул переключает режимы (включающий — подстрока без регистра, исключающий — точное совпадение, распознаватель — голос/имя говорящего, датчик голоса — любая речь), услышанная речь в 7 клетках (не шёпот) записывается или после «clicks.» и звука через 2 с даёт импульс. Нет: срабатывания на речь из радио, языков, `htmlrendertext`

### Датчик здоровья
Импульс при смерти/критическом состоянии носителя.

- **Код:** [`/obj/item/assembly/health`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/health.dm#L1); tgui: [`HealthSensor.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/HealthSensor.tsx)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/assemblies/server.rs` (`health_tick`, `health_acts`), окно `HealthSensor`: скан здоровья моба, несущего датчик (или держатель/гранату с ним), при здоровье ≤ цели (крит 0 / смерть −100) — импульс, писк и выключение; осмотр показывает здоровье. Нет: мэптекста «HP: %» над датчиком

### Мышеловка
Взведённая мышеловка ранит наступившего/схватившего (руки/ноги), убивает мышей, даёт импульс.

- **Код:** [`/obj/item/assembly/mousetrap`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/mousetrap.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/assemblies/server.rs` (`trap_moves`, `triggered`): взвод/разряд в руке (и импульсом), иконка `mousetrap[armed]`; бегущий человек, наступивший на тайл хоста-предмета (сама мышеловка, держатель, граната), или плотный предмет её спускает: без обуви 1 урона по случайной ноге и паралич 6 с, обувь защищает; щелчок и импульс. Нет: неуклюжих, срабатывания при подъёме рукой/поиске в сумке (`on_found`, `attack_hand`), от брошенного предмета (`hitby`), убийства мышей и крыс (нет мышей), летающих

### Электрошоковый набор
Сборка на электрический стул: импульс бьёт током сидящего.

- **Код:** [`/obj/item/assembly/shock_kit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/shock_kit.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/electric_chair.rs` (`ShockKit`): шлем на электропак (с пола или во второй руке) собирает «электрошлем» с обеими частями в руки, картовый набор создаёт свои части, гаечный ключ разбирает; набор на стуле — см. «Электрический стул» (`engineering/structures-furniture.md`); сигнал на частоте электропака набора переключает цепь стула. Окна набора (`attack_self` открывает окно электропака) нет — оно у электропака (`comms/radio.md`)

### Контроллеры кнопок (door control)
Сборки внутри настенных кнопок: открыть шлюзы/бласт-двери/шторы по id, управлять болтами/аварийным доступом, запустить масс-драйвер.

- **Код:** [`/obj/item/assembly/control`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/doorcontrol.dm#L1), [`control/blast_door`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/doorcontrol.dm#L60), [`control/airlock`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/doorcontrol.dm#L130), [`/obj/machinery/button`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/buttons.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/wallmount/controls.rs` (`Device`): контроллеры гермодверей, шлюзов, масс-драйвера (двери), вспышек живут внутри кнопок с карты; как отдельные предметы-сборки (вставка в кнопку, мультитул, копирование id) нет; зажигатель, шторы, крематорий не работают

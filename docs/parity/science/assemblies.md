# Сборки (assemblies): сигналеры, таймеры, сенсоры

Маленькие устройства, которые посылают и принимают импульсы: соединяются попарно в держатель, вешаются на провода машин, бомбы и шлюзы.

Вспышка (`/obj/item/assembly/flash`) — см. ../security/; транзитный клапан (TTV) и гранаты — см. ../items/.

### Базовая сборка и импульсы
Сборка: состояние «закреплено/открыто» (отвёртка), импульс `pulse()`/`pulsed()`, присоединение к проводам (`/datum/wires/attach_assembly`) и к держателю.

- **Код:** [`/obj/item/assembly`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/assembly.dm#L2), [`pulsed()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/assembly.dm#L87), [`/datum/wires/proc/attach_assembly()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/_wires.dm#L214)
- **Статус в ss13-rs:** ❌ нет (`crates/ss13_game/src/wires.rs`: сигналеры на проводах не портированы)

### Держатель сборок
Две сборки, соединённые вместе (например, таймер + воспламенитель); держатель передаёт импульс от одной к другой, крепится к бакам (бомба из бака) и к проводам.

- **Код:** [`/obj/item/assembly_holder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/holder.dm#L1), [`assemble()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/holder.dm#L31), [`/obj/item/tank/proc/bomb_assemble()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tanks/tanks.dm#L454)
- **Статус в ss13-rs:** ❌ нет

### Удалённый сигналер
Радиосигналер: частота и код, отправка сигнала всем сигналерам с теми же настройками (импульс), приём; варианты для киборга, внутренний, малого радиуса.

- **Код:** [`/obj/item/assembly/signaler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/signaler.dm#L1), [`signal()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/signaler.dm#L151), [`receive_signal()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/signaler.dm#L164); tgui: [`Signaler.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Signaler.tsx)
- **Статус в ss13-rs:** ❌ нет

### Таймер
Отсчитывает заданное время и даёт импульс; режим повтора.

- **Код:** [`/obj/item/assembly/timer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/timer.dm#L1); tgui: [`Timer.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Timer.tsx)
- **Статус в ss13-rs:** ❌ нет

### Датчик движения (proximity)
Срабатывает при движении в радиусе; настраиваемые радиус и задержка взвода.

- **Код:** [`/obj/item/assembly/prox_sensor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/proximity.dm#L1); tgui: [`ProximitySensor.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ProximitySensor.jsx)
- **Статус в ss13-rs:** ❌ нет

### Воспламенитель и конденсатор
Воспламенитель по импульсу поджигает тайл/содержимое бака (бомбы) и даёт искру; конденсатор — охлаждающий вариант.

- **Код:** [`/obj/item/assembly/igniter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/igniter.dm#L6), [`igniter/condenser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/igniter.dm#L56)
- **Статус в ss13-rs:** ❌ нет

### Инфракрасный излучатель
Луч в направлении установки; пересечение луча даёт импульс.

- **Код:** [`/obj/item/assembly/infra`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/infrared.dm#L1); tgui: [`InfraredEmitter.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/InfraredEmitter.tsx)
- **Статус в ss13-rs:** ❌ нет

### Голосовой анализатор
Записывает фразу/голос и срабатывает на неё (режимы: включение, распознавание, голос, тишина).

- **Код:** [`/obj/item/assembly/voice`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/voice.dm#L6)
- **Статус в ss13-rs:** ❌ нет

### Датчик здоровья
Импульс при смерти/критическом состоянии носителя.

- **Код:** [`/obj/item/assembly/health`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/health.dm#L1); tgui: [`HealthSensor.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/HealthSensor.tsx)
- **Статус в ss13-rs:** ❌ нет

### Мышеловка
Взведённая мышеловка ранит наступившего/схватившего (руки/ноги), убивает мышей, даёт импульс.

- **Код:** [`/obj/item/assembly/mousetrap`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/mousetrap.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Электрошоковый набор
Сборка на электрический стул: импульс бьёт током сидящего.

- **Код:** [`/obj/item/assembly/shock_kit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/shock_kit.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Контроллеры кнопок (door control)
Сборки внутри настенных кнопок: открыть шлюзы/бласт-двери/шторы по id, управлять болтами/аварийным доступом, запустить масс-драйвер.

- **Код:** [`/obj/item/assembly/control`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/doorcontrol.dm#L1), [`control/blast_door`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/doorcontrol.dm#L60), [`control/airlock`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/doorcontrol.dm#L130), [`/obj/machinery/button`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/buttons.dm#L1)
- **Статус в ss13-rs:** ❌ нет

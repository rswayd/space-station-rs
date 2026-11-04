# Схемы (wiremod / integrated circuits)

Визуальное программирование: интегральные схемы из компонентов с портами, вставляемые в оболочки (shells) — от пульта до дрона и импланта.

### Интегральная схема и её редактор
Плата-контейнер с компонентами, соединяемыми портами (типы: число, строка, сигнал, сущность, список, таблица, опции и др.), переменными и батареей; редактор — tgui-холст с перетаскиванием и проводами.

- **Код:** [`/obj/item/integrated_circuit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/core/integrated_circuit.dm#L11), [`/obj/item/circuit_component`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/core/component.dm#L10), [`/datum/port`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/core/port.dm#L6), [`/datum/circuit_datatype`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/core/datatypes.dm#L15), [`/datum/circuit_variable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/core/variable.dm#L6); tgui: [`IntegratedCircuit/index.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/IntegratedCircuit/index.jsx)
- **Статус в ss13-rs:** ❌ нет

### Принтер компонентов и дубликатор модулей
Принтер печатает компоненты и оболочки по исследованным чертежам (узлы техвеба схем); дубликатор сохраняет схему-модуль и печатает копии. Сохранение/загрузка схем в JSON.

- **Код:** [`/obj/machinery/component_printer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/core/component_printer.dm#L2), [`/obj/machinery/module_duplicator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/core/component_printer.dm#L348), [`convert_to_json()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/core/duplicator.dm#L130), чертежи: [`designs/wiremod/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/designs/wiremod); tgui: [`ComponentPrinter.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ComponentPrinter.tsx)
- **Статус в ss13-rs:** ❌ нет

### Инструменты схем: маркер и USB-кабель
Мультитул-маркер помечает сущность для передачи в схему; USB-кабель подключает схему к машинам с USB-портом (эмиттер, термомашина, температурный насос, телепорт-консоль, квантпад, лаунчпад).

- **Код:** [`/obj/item/multitool/circuit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/core/marker.dm#L1), [`/obj/item/usb_cable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/core/usb_cable.dm#L2), USB-компоненты: [`components/usb/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/usb)
- **Статус в ss13-rs:** ❌ нет

### Логика, математика, строки, списки, таблицы
Базовые компоненты вычислений: арифметика, сравнение, логика, тригонометрия, рандом, переключатель; строки (склейка, замена, регистр, преобразования); списки и ассоц. списки (индекс, фильтр, foreach, split); таблицы; переменные (getter/setter); утилиты (часы, задержка, роутер, typecast/typecheck, NFC, пучок проводов).

- **Код:** [`/obj/item/circuit_component/arithmetic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/math/arithmetic.dm#L16), [`math/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/math), [`string/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/string), [`list/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/list), [`table/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/table), [`variables/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/variables), [`abstract/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/abstract), [`utility/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/utility)
- **Статус в ss13-rs:** ❌ нет

### Компоненты действий
Речь, звук, свет, радио (сигналер), лазерная указка, камера, тяга (pull), поиск пути, управление MMI, действия экипировки.

- **Код:** [`/obj/item/circuit_component/speech`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/action/speech.dm#L6), [`action/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/action)
- **Статус в ss13-rs:** ❌ нет

### Компоненты сущностей и сенсоры
Направление, GPS, здоровье и его состояние, прослушивание речи, сканер материалов/реагентов, пинпоинтер, удалённая камера, «self», вид; сенсоры давления, температуры, обзора.

- **Код:** [`atom/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/atom), [`/obj/item/circuit_component/remotecam`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/atom/remotecam.dm#L12), [`/obj/item/circuit_component/view_sensor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/sensors/view_sensor.dm#L7), [`sensors/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/sensors)
- **BandaStation:** [`remotecam.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/atom/remotecam.dm#L179) — санитизация имени сети камер
- **Статус в ss13-rs:** ❌ нет

### Компоненты интегральных схем воздушной тревоги
Чтение показаний и управление вентиляциями/скрубберами тревоги из интегральных схем.

- **Код:** [`circuit_component/air_alarm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/air_alarm/air_alarm_circuit.dm#L101), [`air_alarm_vents`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/air_alarm/air_alarm_circuit.dm#L448), [`air_alarm_scrubbers`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/air_alarm/air_alarm_circuit.dm#L238)
- **Статус в ss13-rs:** ❌ нет

### Компоненты ID и доступа
Чтение доступа и информации с ID, проверка доступа (с отдельным окном настройки).

- **Код:** [`id/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/id); tgui: [`CircuitAccessChecker.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/CircuitAccessChecker.tsx)
- **Статус в ss13-rs:** ❌ нет

### Сетевые компоненты (NTNet, wirenet)
Передача данных между схемами по NTNet (зашифрованные ключом) и по проводной сети.

- **Код:** [`/obj/item/circuit_component/ntnet_send`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/ntnet/ntnet_send.dm#L7), [`ntnet/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/ntnet), [`/obj/item/circuit_component/wirenet_send`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/wirenet/send.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### BCI (нейроинтерфейс) и его компоненты
Имплант-оболочка в мозг: схема читает мысли, рисует HUD-оверлеи (объекты, полосы, счётчики, перехват цели), вводит реагенты, VOX; ставится в BCI-имплантере.

- **Код:** [`/obj/item/organ/cyberimp/bci`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/shell/brain_computer_interface.dm#L1), [`/obj/machinery/bci_implanter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/shell/brain_computer_interface.dm#L247), [`/obj/item/circuit_component/object_overlay`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/bci/hud/object_overlay.dm#L10), [`bci/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/bci)
- **Статус в ss13-rs:** ❌ нет

### Оболочки схем (shells)
Компонент `shell` делает объект носителем схемы. Оболочки: компактный пульт, контроллер, сканер, клавиатура, сборка (assembly), бот, дрон (моб), диспенсер, денежный бот, сервер, настенный, подпольный (under-tile), шлюз, сканер-гейт, имплант, модуль MOD, пушка.

- **Код:** [`/datum/component/shell`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/shell.dm#L2), [`/obj/item/compact_remote`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/shell/compact_remote.dm#L6), [`/obj/item/assembly/wiremod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/shell/assembly.dm#L6), [`/mob/living/circuit_drone`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/shell/drone.dm#L6), [`/obj/structure/bot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/shell/bot.dm#L6), [`/obj/structure/server`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/shell/server.dm#L7), [`/obj/machinery/door/airlock/shell`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/shell/airlock.dm#L11), [`/obj/item/mod/module/circuit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/shell/module.dm#L1), [`/obj/item/gun/energy/wiremod_gun`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/shell/gun.dm#L6), [`shell/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/shell); tgui: [`CircuitModule.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/CircuitModule.jsx)
- **Статус в ss13-rs:** ❌ нет

### Админ-компоненты и панель схем
Админские компоненты (proccall, SDQL, getvar/setvar, спавн, анимация, фильтры, обработчик сигналов, запрос ввода) и панель всех схем раунда.

- **Код:** [`/datum/circuit_admin_panel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/core/admin_panel.dm#L6), [`/obj/item/circuit_component/proccall`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/admin/proccall.dm#L10), [`admin/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/wiremod/components/admin); tgui: [`CircuitAdminPanel.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/CircuitAdminPanel.tsx), [`CircuitSignalHandler.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/CircuitSignalHandler.tsx)
- **Статус в ss13-rs:** ❌ нет

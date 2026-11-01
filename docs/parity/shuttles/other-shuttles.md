# Прочие шаттлы
Шаттл прибытия, грузовой и рабочие шаттлы, консоли управления шаттлами, навигационный компьютер, самодельные шаттлы и шаттлы форка.

### Универсальная консоль шаттла
`/obj/machinery/computer/shuttle`: tgui-список доступных доков (`possible_destinations`), отправка, блокировка консоли, запрос у админов на разблокировку; основа для консолей майнинга, лагеря, синдиката, белого корабля и т. д.

- **Код:** [`/obj/machinery/computer/shuttle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/shuttle_consoles/shuttle_console.dm#L9); tgui: [`ShuttleConsole.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ShuttleConsole.jsx)
- **Статус в ss13-rs:** ❌ нет

### Навигационный компьютер (свой док)
`shuttle_docker`: камера над космосом, игрок ставит и поворачивает собственный стационарный порт для шаттла (синдикат, белый корабль, кастомные шаттлы); проверка, что место свободно.

- **Код:** [`/obj/machinery/computer/camera_advanced/shuttle_docker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/shuttle_consoles/navigation_computer.dm#L1), [`/obj/machinery/computer/camera_advanced/shuttle_docker/whiteship`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/shuttle_consoles/white_ship.dm#L19)
- **BandaStation:** [`dockers.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/shuttles/code/dockers.dm#L45) — переопределение доков синдиката и доки для шаттлов форка
- **Статус в ss13-rs:** ❌ нет

### Шаттл прибытия
Привозит опоздавших игроков на станцию, сам летает между ЦК и станцией, не улетает, пока на борту есть люди или открыта дверь; латеджойн-места — кресла на нём.

- **Код:** [`/obj/docking_port/mobile/arrivals`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/mobile_port/variants/arrivals.dm#L1), [`/datum/map_template/shuttle/arrival`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/shuttles/arrival.dm#L1)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/server.rs` ставит опоздавших на `/obj/effect/landmark/latejoin`

_«Грузовой шаттл (перемещение)» описано в [../cargo/supply-orders.md](../cargo/supply-orders.md)._

_«Шаттлы майнинга и трудового лагеря» описано в [../cargo/mining-equipment.md](../cargo/mining-equipment.md)._

### Паром ЦК, шаттлы ОБР и белый корабль
Админский паром на ЦК, шаттлы отрядов быстрого реагирования, «белый корабль» (заброшенный шаттл с собственным навигатором и GPS), монастырь, руины-шаттлы.

- **Код:** [`/obj/machinery/computer/shuttle/ferry`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/mobile_port/variants/ferry.dm#L1), [`/datum/map_template/shuttle/ert`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/shuttles/ert.dm#L1), [`/obj/machinery/computer/shuttle/white_ship`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/shuttle_consoles/white_ship.dm#L1), [`/datum/map_template/shuttle/whiteship`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/shuttles/whiteship.dm#L1), [`/obj/machinery/computer/shuttle/monastery_shuttle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/shuttle_consoles/monastery.dm#L1), [`/datum/map_template/shuttle/ruin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/shuttles/ruin.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Шаттлы антагонистов
Шаттл ядерных оперативников (консоль `syndicate`, запрет на полёт при объявленной войне), инфильтратор, крейсер «Starfury», пираты, охотники за беглецами, штурмовая капсула (`assault_pod`: игрок выбирает точку высадки на станции).

- **Код:** [`/obj/machinery/computer/shuttle/syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/shuttle_consoles/syndicate.dm#L3), [`/obj/docking_port/mobile/infiltrator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/mobile_port/variants/infiltrator.dm#L2), [`/datum/map_template/shuttle/starfury`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/shuttles/starfury.dm#L1), [`/datum/map_template/shuttle/pirate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/shuttles/pirate.dm#L1), [`/datum/map_template/shuttle/hunter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/shuttles/hunter.dm#L1), [`/obj/docking_port/mobile/assault_pod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/mobile_port/variants/assault_pod.dm#L1)
- **BandaStation:** [`assault_pod.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/mobile_port/variants/assault_pod.dm#L63) — доработка штурмовой капсулы (эффект высадки)
- **Статус в ss13-rs:** ❌ нет

### Самодельные шаттлы и чертежи шаттла
Чертежи шаттла (`shuttle_blueprints`) превращают отстроенную в космосе конструкцию с двигателями в мобильный порт; консоль кастомного шаттла и её навигатор; визуализация границ при постройке.

- **Код:** [`/obj/item/shuttle_blueprints`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/mobile_port/variants/custom/blueprints.dm#L122), [`/obj/docking_port/mobile/custom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/mobile_port/variants/custom/custom.dm#L1), [`/obj/machinery/computer/shuttle/custom_shuttle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/mobile_port/variants/custom/custom_consoles.dm#L1); tgui: [`ShuttleBlueprints.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ShuttleBlueprints.tsx)
- **Статус в ss13-rs:** ❌ нет

### Пульт шаттла и навигационный маяк
Ручной пульт вызова шаттла (`shuttle_remote`) и разворачиваемый навигационный маяк (`spaceship_navigation_beacon`), который добавляет точку прыжка для навигаторов.

- **Код:** [`/obj/item/shuttle_remote`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/misc/shuttle_remote.dm#L1), [`/obj/machinery/spaceship_navigation_beacon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/misc/spaceship_navigation_beacon.dm#L1)
- **BandaStation:** [`spaceship_navigation_beacon.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/misc/spaceship_navigation_beacon.dm#L90) — изменено сообщение маяка
- **Статус в ss13-rs:** ❌ нет

### Шаттл-лифт
Старый тип лифта на механизме шаттлов (`/obj/docking_port/mobile/elevator`), сейчас почти не используется картами (лифты — см. transport.md).

- **Код:** [`/obj/docking_port/mobile/elevator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/mobile_port/variants/elevator.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Шаттлы форка: SIT, SST, Аргос, спецназ, Гамма, капсула NT
Дополнительные шаттлы BandaStation: ударные отряды Синдиката (SIT/SST), «Аргос», спецопераций ЦК, шаттл Гамма-арсенала, штурмовая капсула Nanotrasen; свои области, консоли, платы, доки и звук высадки.

- **Код:** [`/obj/docking_port/mobile`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/shuttle/mobile_port/mobile_port.dm#L1)
- **BandaStation:** [`docking_ports.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/shuttles/code/docking_ports.dm#L77), [`terminals.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/shuttles/code/terminals.dm#L1), [`areas.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/shuttles/code/areas.dm#L1) — новые шаттлы и их оборудование
- **Статус в ss13-rs:** ❌ нет

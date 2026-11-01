# Телепортация, гейтвей и спутники

Станционные телепорты (хаб, квантовые и блюспейс-пады), ручной телепорт, врата гейтвея в away-миссии и спутники метеорного щита.

### Телепорт (хаб, станция, консоль)
Консоль выбирает цель (маяк, имплант трекинга, станция-партнёр в режиме «врата»), станция калибрует, хаб телепортирует входящих; некалиброванный хаб может промахнуться и мутировать. Консоль доступна схемам через USB.

- **Код:** [`/obj/machinery/teleport/hub`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/teleporter.dm#L6), [`teleport()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/teleporter.dm#L66), [`/obj/machinery/teleport/station`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/teleporter.dm#L109), [`/obj/machinery/computer/teleporter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/teleporter.dm#L4); tgui: [`Teleporter.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Teleporter.jsx)
- **Статус в ss13-rs:** ❌ нет (порт `Teleporter.jsx` есть только как пример в `crates/ss13_ui_kit/examples/gallery.rs`, без игровой логики)

_«Блюспейс-маяк» описано в [../items/devices.md](../items/devices.md)._

### Ручной телепорт и блюспейс-локатор
Ручной телепорт открывает портал к маяку/случайной точке (лимит порталов); локатор показывает маяки и импланты трекинга.

- **Код:** [`/obj/item/hand_tele`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/teleportation.dm#L74), [`/obj/item/locator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/teleportation.dm#L13); tgui: [`BluespaceLocator.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/BluespaceLocator.tsx)
- **Статус в ss13-rs:** ❌ нет

### Квантовый пад и ключ-карта
Пад, связанный мультитулом с другим падом, телепортирует стоящих на нём; ключ-карта копирует связь пада; управление из схем.

- **Код:** [`/obj/machinery/quantumpad`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/quantum_pad.dm#L1), [`doteleport()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/quantum_pad.dm#L140), [`/obj/item/quantum_keycard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/quantum_keycard.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Блюспейс-лаунчпад, консоль и брифкейс-пад
Лаунчпад отправляет/забирает предметы и людей в точку со смещением X/Y в пределах радиуса (зависит от деталей); управляется консолью или пультом брифкейс-пада (синдикат).

- **Код:** [`/obj/machinery/launchpad`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/launch_pad.dm#L3), [`doteleport()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/launch_pad.dm#L176), [`/obj/machinery/computer/launchpad`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/launchpad_control.dm#L1), [`/obj/machinery/launchpad/briefcase`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/launch_pad.dm#L277), [`/obj/item/launchpad_remote`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/launch_pad.dm#L334); tgui: [`LaunchpadConsole.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/LaunchpadConsole.tsx), [`LaunchpadRemote.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/LaunchpadRemote.tsx)
- **Статус в ss13-rs:** ❌ нет

### Гейтвей
Станционные врата и консоль: после прогрева соединяются с вратами away-миссии или точкой назначения; проход через портал. Мех-пад для мехов — см. робототехнику.

- **Код:** [`/obj/machinery/gateway`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/awaymissions/gateway.dm#L145), [`activate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/awaymissions/gateway.dm#L259), [`/datum/gateway_destination`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/awaymissions/gateway.dm#L11), [`/obj/machinery/computer/gateway_control`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/awaymissions/gateway.dm#L331); tgui: [`Gateway.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Gateway.jsx)
- **Статус в ss13-rs:** ❌ нет

### Спутники и консоль спутников
Спутники (метеорный щит — цель станции) разворачиваются в космосе, включаются консолью; щит сбивает метеоры, покрытие засчитывает цель.

- **Код:** [`/obj/machinery/satellite`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/satellite/satellite.dm#L1), [`/obj/machinery/satellite/meteor_shield`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/station_goals/meteor_shield.dm#L60), [`/obj/machinery/computer/sat_control`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/satellite/satellite_control.dm#L1); tgui: [`SatelliteControl.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SatelliteControl.tsx)
- **Статус в ss13-rs:** ❌ нет

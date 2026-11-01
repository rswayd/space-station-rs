# Эксплоратор-дроны

Карго/научная мини-игра: сканирование космоса сканерным массивом, отправка дронов к найденным местам, события и приключения, добыча.

### Сканерный массив и его консоль
Антенны (`exoscanner`) и консоль сканирования: широкое сканирование ищет места исследования, точечное и глубокое раскрывают их свойства; число и качество антенн ускоряют сканы.

- **Код:** [`/obj/machinery/computer/exoscanner_control`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/explorer_drone/scanner_array.dm#L78), [`/obj/machinery/exoscanner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/explorer_drone/scanner_array.dm#L193), [`/datum/exoscan`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/explorer_drone/scanner_array.dm#L22); tgui: [`ExoscannerConsole.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ExoscannerConsole.tsx)
- **Статус в ss13-rs:** ❌ нет

### Дрон, пусковая установка и топливо
Дрон с грузовым отсеком и инструментами (сканер, лазер, взломщик и др.) запускается с пусковой установки, расходуя топливные пеллеты; возвращается с добычей.

- **Код:** [`/obj/item/exodrone`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/explorer_drone/exodrone.dm#L22), [`/obj/machinery/exodrone_launcher`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/explorer_drone/exodrone.dm#L352), [`/obj/item/fuel_pellet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/explorer_drone/exodrone.dm#L464)
- **Статус в ss13-rs:** ❌ нет

### Консоль управления дронами
Пульт выбора дрона, места назначения, инструментов, запуска событий на месте и возврата.

- **Код:** [`/obj/machinery/computer/exodrone_control_console`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/explorer_drone/control_console.dm#L1); tgui: [`ExodroneConsole.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ExodroneConsole.tsx)
- **Статус в ss13-rs:** ❌ нет

### Места исследования
Генерируемые сайты (заброшенная заправка, торговый пост, астероиды и т. п.) с набором событий и требованиями к инструментам.

- **Код:** [`/datum/exploration_site`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/explorer_drone/exploration_site.dm#L16)
- **Статус в ss13-rs:** ❌ нет

### События на местах
Флафф-события, ресурсы (тайники, остатки), опасности (карпы и др., повреждают дрон), торговцы (обмен предметами).

- **Код:** [`/datum/exploration_event`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/explorer_drone/exploration_events/_exploration_event.dm#L2), [`resource`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/explorer_drone/exploration_events/resource.dm#L2), [`danger`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/explorer_drone/exploration_events/danger.dm#L2), [`trader`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/explorer_drone/exploration_events/trader.dm#L2), [`fluff`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/explorer_drone/exploration_events/fluff.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Текстовые приключения и генераторы добычи
JSON-приключения с узлами выбора и проверками инструментов; генераторы добычи (техника, карго, деньги и т. д.).

- **Код:** [`/datum/adventure`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/explorer_drone/adventure.dm#L160), [`/datum/exploration_event/adventure`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/explorer_drone/exploration_events/adventure.dm#L2), [`/datum/adventure_loot_generator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/explorer_drone/loot.dm#L13)
- **Статус в ss13-rs:** ❌ нет

### Админ-браузер приключений
Админ-инструмент просмотра, загрузки и правки приключений в БД.

- **Код:** [`/datum/adventure_browser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/explorer_drone/manager.dm#L2), [`/datum/adventure_db_entry`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/explorer_drone/adventure.dm#L65); tgui: [`AdventureBrowser.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AdventureBrowser.tsx)
- **Статус в ss13-rs:** ❌ нет

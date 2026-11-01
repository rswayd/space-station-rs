# Битраннинг

Профессия битраннера: подключение к виртуальным доменам через нетподы, прохождение доменов, получение наград в станционной реальности, угрозы-глитчи.

_«Профессия битраннера» описано в [../round/jobs-technical.md](../round/jobs-technical.md)._

### Квантовый сервер
Загружает выбранный домен на отдельный z-уровень (холодная загрузка), спавнит тайники и кураторов, рассчитывает награды, остывает между запусками, улучшается деталями (больше очков, меньше остывание).

- **Код:** [`/obj/machinery/quantum_server`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/server/_parent.dm#L4), [`cold_boot_map()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/server/map_handling.dm#L31), [`calculate_rewards()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/server/loot.dm#L2), [`reset()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/server/map_handling.dm#L200)
- **Статус в ss13-rs:** ❌ нет

### Квантовая консоль
Выбор домена по сложности и стоимости (очки битраннинга), загрузка/остановка, список аватаров и угроз.

- **Код:** [`/obj/machinery/computer/quantum_console`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/objects/quantum_console.dm#L1); tgui: [`QuantumConsole.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/QuantumConsole.tsx)
- **Статус в ss13-rs:** ❌ нет

### Нетпод и подключение аватара
Капсула: игрок ложится, выбирает костюм аватара, тело лечится; сознание переходит в аватара в домене. Урон аватара частично отражается на теле, насильственное отключение (взлом капсулы, потеря питания, смерть аватара) наносит урон.

- **Код:** [`/obj/machinery/netpod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/netpod/_netpod.dm#L4), [`enter_matrix()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/netpod/utils.dm#L63), [`disconnect_occupant()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/netpod/utils.dm#L24), [`/datum/component/avatar_connection`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/components/avatar_connection.dm#L5), [`/datum/component/netpod_healing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/components/netpod_healing.dm#L3); tgui: [`NetpodOutfits.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/NetpodOutfits.tsx)
- **Статус в ss13-rs:** ❌ нет

### Виртуальные домены
29 доменов (пляжный бар, мегафауна Лаваленда, пираты, ксено-гнездо, клоун-планета, синдикат и др.): карта, сложность, стоимость, награда, ограничения снаряжения, модульные сегменты мобов. Цель — доставить зашифрованный тайник на ячейку отправки или убить босса.

- **Код:** [`/datum/lazy_template/virtual_domain`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/virtual_domain/virtual_domain.dm#L5), [`/datum/modular_mob_segment`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/virtual_domain/modular_mob_segment.dm#L28), домены: [`virtual_domain/domains/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/virtual_domain/domains)
- **BandaStation:** [`modular_bandastation/domains/code/domains.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/domains/code/domains.dm#L7) — свои домены `inka_base` (с командой NT и снаряжением ТСФ) и `water_grot` (подземные воды, водовороты, туман); [`outfits.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/outfits.dm#L50) — замена магазина в снаряжении домена; [`npc_friendly.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/components/npc_friendly.dm#L16) — больше фракций, дружественных аватару
- **Статус в ss13-rs:** ❌ нет

### Голо-лестница и выход из домена
Лестница в домене для безопасного выхода аватара обратно в тело.

- **Код:** [`/obj/structure/hololadder`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/objects/hololadder.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Байтфорж и награды
Байтфорж материализует на станции зашифрованный ящик наград (оценка прохождения, бонусы за мультиплеер и без урона, сертификат), иногда диск техвеба.

- **Код:** [`/obj/machinery/byteforge`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/objects/byteforge.dm#L1), [`generate_loot()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/server/loot.dm#L44), [`grade_completion()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/server/loot.dm#L160), [`bitrunning/encrypted`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/objects/loot_crate.dm#L18)
- **Статус в ss13-rs:** ❌ нет

### Вендор битраннинга и диски
Консоль заказов за очки битраннинга: диски способностей и предметов для аватара, гиммик-диски (спорт, данж), флер, BEPIS-предметы.

- **Код:** [`/obj/machinery/computer/order_console/bitrunning`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/objects/vendor.dm#L1), [`/obj/item/disk/bitrunning`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/objects/disks.dm#L7), [`/obj/item/disk/bitrunning/gimmick`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/objects/gimmick_disks/_gimmick_disk.dm#L5), [`orders/tech.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/orders/tech.dm#L1); tgui: [`ProduceConsole.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ProduceConsole.tsx)
- **Статус в ss13-rs:** ❌ нет

### Монитор хоста и помощь аватару
Монитор показывает здоровье подключённых битраннеров; действие аватара показывает правила и информацию о домене.

- **Код:** [`/obj/item/bitrunning_host_monitor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/objects/host_monitor.dm#L1), [`/datum/action/avatar_domain_info`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/abilities.dm#L25); tgui: [`AvatarHelp.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AvatarHelp.tsx)
- **Статус в ss13-rs:** ❌ нет

### Угрозы: глитчи и кибер-полиция
Событие/угроза сервера: призрак вселяется в моба домена и становится кибер-полицейским, тактиком или Netguardian'ом, охотится на аватаров и может выйти на станцию через байтфорж.

- **Код:** [`/datum/round_event_control/bitrunning_glitch`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/event.dm#L1), [`/datum/antagonist/bitrunning_glitch`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/antagonists/_parent.dm#L5), [`setup_glitch()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/server/threats.dm#L65), [`/mob/living/basic/netguardian`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/antagonists/netguardian.dm#L6)
- **Статус в ss13-rs:** ❌ нет

# Персистентность между раундами
`SSpersistence` и `SSpersistent_paintings`: то, что переживает раунд в JSON-файлах `data/`.

### Подсистема персистентности
Загрузка при старте и сохранение в конце раунда (`collect_data`), недавние карты (для голосования), попугай Поли.

- **Код:** [`SUBSYSTEM_DEF(persistence)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/_persistence.dm#L4), [`/datum/controller/subsystem/persistence/proc/collect_data()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/_persistence.dm#L82), [`/datum/controller/subsystem/persistence/proc/load_poly()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/_persistence.dm#L99), [`/datum/controller/subsystem/persistence/proc/load_recent_maps()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/_persistence.dm#L106)
- **Статус в ss13-rs:** ❌ нет — персистентности между раундами нет (перезагрузка поднимает мир заново, `crates/ss13_game/src/round.rs`)

### Трофеи
Витрины трофеев: игроки кладут предмет с подписью, он появляется в следующих раундах; менеджер трофеев для админов.

- **Код:** [`/datum/controller/subsystem/persistence/proc/load_trophies()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/trophies.dm#L11), [`/datum/controller/subsystem/persistence/proc/save_trophy()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/trophies.dm#L118), [`trophy_manager.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/trophy_manager.dm#L1)
- **Статус в ss13-rs:** ❌ нет — персистентности между раундами нет

_«Картины» описано в [../service/art.md](../service/art.md)._

### Гравировки на стенах
Надписи, выцарапанные на стенах, переносятся в следующие раунды.

- **Код:** [`/datum/controller/subsystem/persistence/proc/load_wall_engravings()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/engravings.dm#L2), [`/datum/controller/subsystem/persistence/proc/save_wall_engravings()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/engravings.dm#L48)
- **Статус в ss13-rs:** ❌ нет — персистентности между раундами нет

_«Фотоальбомы и рамки» описано в [../service/photography.md](../service/photography.md)._

### Шрамы и татуировки
Постоянные шрамы персонажа (по предпочтению) и тюремные татуировки переносятся между раундами.

- **Код:** [`/datum/controller/subsystem/persistence/proc/save_scars()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/scars.dm#L2), [`/datum/controller/subsystem/persistence/proc/load_prisoner_tattoos()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/tattoos.dm#L2), [`/datum/controller/subsystem/persistence/proc/save_prisoner_tattoos()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/tattoos.dm#L26)
- **Статус в ss13-rs:** ❌ нет — персистентности между раундами нет

### Бутылки с посланиями и копилки
Послания в бутылках, выброшенные в один раунд, находят в другом; копилки сохраняют деньги.

- **Код:** [`/datum/controller/subsystem/persistence/proc/save_message_bottle()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/message_bottles.dm#L2), [`/datum/controller/subsystem/persistence/proc/load_message_bottle()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/message_bottles.dm#L25), [`/datum/controller/subsystem/persistence/proc/load_piggy_bank()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/piggy_banks.dm#L2), [`/datum/controller/subsystem/persistence/proc/save_piggy_bank()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/piggy_banks.dm#L35)
- **Статус в ss13-rs:** ❌ нет — персистентности между раундами нет

### Счётчики и рекорды
Дни без деламинации суперматерии, сбитые трамваем, трофейные рыбы, рандомизированные рецепты (сохраняются между раундами).

- **Код:** [`/datum/controller/subsystem/persistence/proc/load_delamination_counter()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/counter_delamination.dm#L5), [`/datum/controller/subsystem/persistence/proc/load_tram_counter()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/counter_tram_hits.dm#L55), [`/datum/controller/subsystem/persistence/proc/load_trophy_fish()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/trophy_fishes.dm#L11), [`/datum/controller/subsystem/persistence/proc/save_randomized_recipes()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/recipes.dm#L2)
- **Статус в ss13-rs:** ❌ нет — персистентности между раундами нет

### Пользовательские аутфиты админов
Аутфиты, созданные в редакторе, сохраняются между раундами.

- **Код:** [`/datum/controller/subsystem/persistence/proc/load_custom_outfits()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/custom_outfits.dm#L2), [`/datum/controller/subsystem/persistence/proc/save_custom_outfits()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/persistence/custom_outfits.dm#L24)
- **Статус в ss13-rs:** ❌ нет — персистентности между раундами нет

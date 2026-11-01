# Лоадаут
Личные вещи, выбираемые в настройке персонажа и выдаваемые при спавне поверх аутфита профессии.

### Механизм лоадаута
Категории вещей (головные уборы, очки, шея, обувь, костюмы, аксессуары, карманы, в руки), выбор с превью, перекраска/переименование/рефлавор, выдача при спавне (вещи из рюкзака или вместо слота).

- **Код:** [`/datum/loadout_item`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/loadout/loadout_items.dm#L32), [`/datum/loadout_category`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/loadout/loadout_categories.dm#L6), [`/datum/preference/loadout`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/loadout/loadout_preference.dm#L1), [`loadout_menu.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/loadout/loadout_menu.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Категории и предметы BandaStation
Свои категории и вещи: униформа, маски, перчатки, кошельки, наклейки на карты, музыкальные инструменты, прочее, свой порядок вкладок.

- **Код:** [`modular_bandastation/loadout/code/categories`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/loadout/code/categories), [`loadout_items.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/loadout/code/loadout_items.dm#L1)
- **BandaStation:** [`_loadout.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/loadout/_loadout.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Очки лоадаута и донат-уровни (BandaStation)
Каждая вещь стоит очков (5 базовых + 3 за уровень доната); часть вещей требует уровня подписки; уровень доната берётся с хаба/центрального сервера (локальный хост — из конфига).

- **Код:** [`/datum/preferences/proc/get_loadout_max_points()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/preferences/code/modules/client/preferences.dm#L29), [`/client/proc/get_donator_level()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/loadout/code/client.dm#L12), [`config.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/loadout/code/config.dm#L1)
- **Статус в ss13-rs:** ❌ нет

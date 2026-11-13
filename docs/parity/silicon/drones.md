# Дроны техобслуживания
Мобы `/mob/living/basic/drone`: роль призраков для ремонта станции.

### Дрон техобслуживания
Маленький синтетик с инструментами и хранилищем, законы «не вмешиваться» (`drone`), видит только других дронов в речи, свой канал дронов, ограниченное взаимодействие с экипажем; переворот ящиков, режим ношения шапки.

- **Код:** [`/mob/living/basic/drone`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/drone/_drone.dm#L16), [`drone_tools.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/drone/drone_tools.dm#L1), [`interaction.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/drone/interaction.dm#L3), [`drone_say.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/drone/drone_say.dm#L51)
- **BandaStation:** [`drone_say.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/drone/drone_say.dm#L33) — TTS в речи дронов
- **Статус в ss13-rs:** ❌ нет — нужен «ловкий» базовый моб (`dextrous`: руки, внутреннее хранилище и слот головы у basic mob, HUD не человека); в порту такого механизма нет, это отдельная задача

### Оболочки дронов и раздатчик
Оболочка-спавнер для призрака; раздатчик дронов печатает новые оболочки из материалов.

- **Код:** [`/obj/effect/mob_spawn/ghost_role/drone`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/drone/drones_as_items.dm#L7), [`/obj/machinery/drone_dispenser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/drone_dispenser.dm#L5)
- **Статус в ss13-rs:** ❌ нет

### Особые дроны
Синдидрон (оперативный, с аплинком), «снежинка», классический, заброшенный (derelict), полиморфный.

- **Код:** [`/mob/living/basic/drone/syndrone`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/drone/extra_drone_types.dm#L5), [`/mob/living/basic/drone/snowflake`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/drone/extra_drone_types.dm#L73), [`/mob/living/basic/drone/classic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/drone/extra_drone_types.dm#L105), [`/mob/living/basic/drone/derelict`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/drone/extra_drone_types.dm#L114)
- **Статус в ss13-rs:** ❌ нет

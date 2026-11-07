# Призрачные роли и спавнеры
`/obj/effect/mob_spawn`: точки, где призрак может взять роль (спящие капсулы, яйца, оболочки), и трупы для маппинга.

### Механизм спавнеров
`mob_spawn`: призрачная роль (лимит использований, правила роли, запрет для джоббан/недавно умерших) и труп (готовое тело с аутфитом и повреждениями); меню спавнеров для призраков.

- **Код:** [`/obj/effect/mob_spawn`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob_spawn/mob_spawn.dm#L1), [`/obj/effect/mob_spawn/ghost_role`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob_spawn/mob_spawn.dm#L135), [`/obj/effect/mob_spawn/corpse`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob_spawn/mob_spawn.dm#L324), [`/datum/spawners_menu`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/spawners_menu.dm#L1); tgui: [`SpawnersMenu.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SpawnersMenu.tsx)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_mapload/src/mob_spawn.rs` (`DmMobSpawn`, `mob_spawns.ron`: имя, тексты роли, `uses`, `infinite_use`, `deletes_on_zero_uses_left`, вид, фракция, аутфит, урон трупа, `husk`), `crates/ss13_game/src/ghost_roles.rs` (`create_human`, `create_corpse`), `ghost_verbs.rs` (`GhostRoleSpawner`, `take_role`, меню спавнеров, клик призрака по спавнеру открывает меню): человек вида роли со случайной внешностью, `mob_name` или именем персонажа, аутфитом и рюкзаком, фракцией; тексты роли в чат; расход использований; только после начала раунда. Тест: `tests/it/ghost_roles.rs`. Без джоббанов и запрета для недавно умерших (нет банов в порте), `outfit_override`, причёсок/цвета кожи из переменных, правок переменных спавнера на карте

### Роли Лаваленда и ледяной луны
Сломанный криосон, пляжный бездельник, спасатель, бармен, террариум, яйцо пеплоходцев, учёный-биооружейник и связисты Синдиката.

- **Код:** [`mining_roles.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob_spawn/ghost_roles/mining_roles.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/ghost_roles.rs`: спавнеры руин Лаваленда (и ледяной луны, если её руины загружены) становятся ролями с аутфитом, видом (пеплоходцы — `ashwalker`, семенное хранилище — `pod`), текстами и фракцией; нет их `special()`/`post_equip()`: племя и рост яйца пеплоходцев, ID бармена, имя спасателя, команда и доступы связистов Синдиката

### Космические роли
Старая криокапсула (Oldstation), экипаж боевого крейсера Синдиката, криокапсулы руин.

- **Код:** [`space_roles.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob_spawn/ghost_roles/space_roles.dm#L1), [`unused_roles.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob_spawn/ghost_roles/unused_roles.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/ghost_roles.rs`: криокапсулы Oldstation, крейсера Синдиката и прочих руин дают человека с аутфитом и текстами роли там, где их карты загружены; нет `create()`/`special()`/`allow_spawn()` типов (доступы Oldstation, команда крейсера и её лимиты); загрузка космических руин — группа карт

### Големы
Оболочки свободного голема и голема-слуги.

- **Код:** [`golem_roles.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob_spawn/ghost_roles/golem_roles.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Яйца пауков, бутон мухоловки, охотники
Кладки яиц гигантских пауков разных видов, бутон венериной мухоловки, капсулы охотников за беглецами; дроны техобслуживания.

- **Код:** [`spider_roles.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob_spawn/ghost_roles/spider_roles.dm#L1), [`venus_human_trap.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob_spawn/ghost_roles/venus_human_trap.dm#L1), [`fugitive_hunter_roles.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob_spawn/ghost_roles/fugitive_hunter_roles.dm#L1), [`drone_roles.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob_spawn/ghost_roles/drone_roles.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Трупы
Маппинговые трупы: по профессиям, NT, видам, нечеловеческие, шахтёрские, мобов.

- **Код:** [`/obj/effect/mob_spawn/corpse/human`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob_spawn/mob_spawn.dm#L375), [`code/modules/mob_spawn/corpses`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob_spawn/corpses)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/ghost_roles.rs` (`create_corpse`): человеческие трупы (`corpse/human/*`: профессии, NT, виды, шахтёры) при инициализации карты — мёртвый человек вида типа в аутфите с уроном brute/burn/oxy, `husk` высушивает, спавнер удаляется; нет трупов мобов и нечеловеческих трупов (`corpse/<моб>`), `corpse_description`, выключения сенсоров и PDA (`conceal_presence`), `CORPSE_ROUNDSTART`

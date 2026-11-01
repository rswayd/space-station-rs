# Призрачные роли и спавнеры
`/obj/effect/mob_spawn`: точки, где призрак может взять роль (спящие капсулы, яйца, оболочки), и трупы для маппинга.

### Механизм спавнеров
`mob_spawn`: призрачная роль (лимит использований, правила роли, запрет для джоббан/недавно умерших) и труп (готовое тело с аутфитом и повреждениями); меню спавнеров для призраков.

- **Код:** [`/obj/effect/mob_spawn`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob_spawn/mob_spawn.dm#L1), [`/obj/effect/mob_spawn/ghost_role`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob_spawn/mob_spawn.dm#L135), [`/obj/effect/mob_spawn/corpse`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob_spawn/mob_spawn.dm#L324), [`/datum/spawners_menu`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/spawners_menu.dm#L1); tgui: [`SpawnersMenu.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SpawnersMenu.tsx)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/ghost.rs`: призрак только наблюдает

### Роли Лаваленда и ледяной луны
Сломанный криосон, пляжный бездельник, спасатель, бармен, террариум, яйцо пеплоходцев, учёный-биооружейник и связисты Синдиката.

- **Код:** [`mining_roles.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob_spawn/ghost_roles/mining_roles.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Космические роли
Старая криокапсула (Oldstation), экипаж боевого крейсера Синдиката, криокапсулы руин.

- **Код:** [`space_roles.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob_spawn/ghost_roles/space_roles.dm#L1), [`unused_roles.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob_spawn/ghost_roles/unused_roles.dm#L1)
- **Статус в ss13-rs:** ❌ нет

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
- **Статус в ss13-rs:** ❌ нет

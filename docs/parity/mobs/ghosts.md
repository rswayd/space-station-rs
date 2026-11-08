# Призраки и наблюдатели
Моб `/mob/dead/observer`: призрак мёртвого игрока или наблюдателя из лобби, его глаголы и меню. Роли призраков (спавнеры) создают агенты своих областей.

### Призрак (ghostize)
Мёртвый (или живой через глагол «Ghost») игрок становится призраком у тела: невидим живым, проходит сквозь стены, видит других призраков. Призрак живого моба теряет право вернуться.

- **Код:** [`/mob/dead/observer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer.dm#L6), [`proc/ghostize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer.dm#L264), [`verb/ghost`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer.dm#L307)
- **BandaStation:** [`code/modules/mob/dead/observer/observer.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer.dm#L294) — контекстное обновление `time_of_death`
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/ghost.rs`: призрак при попытке движения мёртвого, невидимость для живых, движение сквозь стены; нет глагола «Ghost» для живых и запрета возврата

### Возвращение в тело и DNR
«Re-enter Corpse» возвращает призрака в его тело (когда его оживили или для дефибрилляции); «Do Not Resuscitate» навсегда отказывается от возвращения; призраку приходит уведомление о попытке оживления.

- **Код:** [`verb/reenter_corpse`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer.dm#L363), [`verb/do_not_resuscitate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer.dm#L384), [`proc/send_revival_notification()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer.dm#L415)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/ghost.rs` (`return_to_body`): дефибриллятор возвращает призрака в оживлённое тело; `crates/ss13_game/src/ghost_verbs.rs` (`reenter_corpse`, `do_not_resuscitate`): кнопка «В тело» HUD призрака и DNR в меню призрака; нет уведомлений о попытке оживления

### Перемещение призрака: телепорт, прыжок к мобу, Orbit
Глаголы телепорта по зонам, прыжка к мобу и меню Orbit (tgui) со списками живых, мёртвых, антагов, ботов и т.д.; призрак кружит вокруг выбранного объекта.

- **Код:** [`verb/dead_tele`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer.dm#L438), [`verb/jumptomob`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer.dm#L469), [`/datum/orbit_menu`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/orbit.dm#L3), [`/datum/component/orbiter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/orbiter.dm#L1); tgui: [`index.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Orbit/index.tsx)
- **BandaStation:** [`code/modules/mob/dead/observer/orbit.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/orbit.dm#L114) — список SSD в меню Orbit
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/ghost.rs`: свободное движение призрака по карте; `crates/ss13_game/src/ghost_verbs.rs` (`orbit`, `follow_orbits`), `ghost_menu_client.rs`: меню Orbit (живые, мёртвые, призраки, прочие мобы), призрак следует за целью на её тайл, свой шаг прекращает орбиту; нет телепорта по зонам, кругового движения орбиты и разделов антагов/SSD

### Наблюдение глазами моба (Observe)
«Observe» показывает экран выбранного игрока, включая его HUD и открытые окна.

- **Код:** [`verb/observe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer.dm#L538), [`proc/do_observe()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer.dm#L890)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_game/src/ghost_verbs.rs` (`orbit(.., observe)`, `open_observed_huds`), `ghost.rs` (`Observing`, `close_foreign_windows`): «Наблюдать» в меню Orbit — призрак следует за мобом, клиенту открываются HUD-окна моба (алерты, голод, настроение, fullscreen-оверлеи) только для просмотра; шаг призрака или пропажа цели прекращают наблюдение. Без зеркала инвентаря, куклы здоровья и кнопок действий (shortcut в коде)

### Настройки зрения призрака
Дальность обзора, видимость других призраков, темнота, T-ray, data-HUD (мед/охрана/диаг), скан здоровья, химии и газа по клику.

- **Код:** [`verb/change_view_range`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer.dm#L497), [`verb/toggle_darkness`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer.dm#L520), [`verb/toggle_data_huds`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer.dm#L575), [`verb/toggle_health_scan`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer.dm#L583)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/ghost_verbs.rs` (`set_darkness`, `toggle_vision`, `toggle_health_scan`, `ghost_scans`), `crates/ss13_render/src/lighting` (`LightingCutoff`): темнота (4 уровня `ghost_lightings` как нижний порог освещения клиента), Ghost Vision (`SEE_INVISIBLE_OBSERVER`/`LIVING`, свой призрак виден всегда), скан здоровья по клику, разовый T-ray-скан; нет дальности обзора, data-HUD, сканов химии и газа; настройки не переживают призрака

### Внешний вид призрака
Призрак выглядит как персонаж (с причёской) или как выбранная форма из настроек; «Restore Ghost Character» восстанавливает облик.

- **Код:** [`proc/set_ghost_appearance()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer.dm#L829), [`verb/restore_ghost_appearance`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer.dm#L607)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/ghost.rs`: стандартный спрайт `ghost`; нет облика персонажа и форм из настроек

### Boo и вселение
Призрак может изредка мигнуть лампой (`boo`); админ-призрак может вселиться в моба (`possess`).

- **Код:** [`proc/boo()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer.dm#L678), [`proc/possess()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer.dm#L725)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_game/src/ghost_verbs.rs` (`boo`, `possess`, `FunVerbs`): с `fun_verbs` (как в tg — компонент `FunVerbs`) Boo мигает лампой в радиусе тайла, перезарядка 60 с только после удачного мигания; Possess — живой моб без разума, не мегафауна, проверка в момент вселения, фракция становится neutral, призрак исчезает. Выдача `fun_verbs` событием мага «Possessing G-G-G-Ghosts!» — группа событий мага

### Меню призрака, спавнеры, мини-игры
Меню призрака (tgui) с действиями; меню спавнеров ролей призраков (`mob_spawn`), голосования-опросы на роли (`SSpolling`), меню мини-игр.

- **Код:** [`/datum/ghost_menu`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/ghost_menu.dm#L3), [`/datum/spawners_menu`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/spawners_menu.dm#L1), [`/obj/effect/mob_spawn/ghost_role`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob_spawn/mob_spawn.dm#L135), [`proc/poll_ghost_candidates()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/polling.dm#L208), [`/datum/minigames_menu`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/minigames_menu.dm#L1); tgui: [`GhostMenu.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/GhostMenu.tsx), [`SpawnersMenu.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SpawnersMenu.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/ghost_menu.rs`, `ghost_verbs.rs`, `ghost_menu_client.rs`: кнопки HUD призрака (Orbit, меню призрака, спавнеры, в тело, респаун); меню призрака (тело/DNR, темнота, HUD-флаги, Boo/Possess); меню спавнеров для `GhostRoleSpawner` (группировка по роли, Jump, Spawn — человек на спавнере, расход `uses`); опросы на роли — `crates/ss13_game/src/ghost_poll.rs` (окном, без экранной тревоги со стопками); нет pAI, манифеста; спавнеры с карты (`/obj/effect/mob_spawn/ghost_role`, их снаряжение) — группы ролей призраков; меню мини-игр — нет мини-игр в порте

### Речь призраков (deadchat)
Призраки говорят в мёртвый чат, слышат всех живых (по настройке) и радио.

- **Код:** [`observer_say.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer_say.dm#L1)
- **BandaStation:** [`code/modules/mob/dead/observer/observer_say.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/observer/observer_say.dm#L59) — правки форка в речи призраков
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_chat/src/server.rs` (`say_dead`): мёртвый чат для мёртвых и призраков; нет слышимости живых и радио призраками

### Респаун
Глагол «Respawn» возвращает наблюдателя в лобби, если сервер разрешает респаун по конфигу.

- **Код:** [`verb/abandon_mob`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob.dm#L807)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_game/src/ghost_verbs.rs` (`respawn`, `RespawnConfig`), `round.rs` (`return_to_lobby`): кнопка «Респаун» HUD призрака; `SS13_ALLOW_RESPAWN` (0/1/2 как `ALLOW_RESPAWN`, по умолчанию выключено) и `SS13_RESPAWN_DELAY` (секунды с момента смерти); призрак удаляется, клиент снова в лобби с сохранёнными настройками. Без админского обхода запрета и задержки

### Камера-глаз (mob/eye)
Невидимый моб-глаз для удалённого обзора (консоли камер, ИИ, удалённое управление); игрок управляет им как телом.

- **Код:** [`/mob/eye`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/eye/eye.dm#L2), [`/mob/eye/camera`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/eye/camera/camera.dm#L5), [`/mob/eye/camera/remote`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/eye/camera/remote.dm#L5)
- **Статус в ss13-rs:** 🟡 частично — глаз ИИ (`crates/ss13_game/src/silicon/ai.rs`: управляемая клиентом сущность, вьюер следует за ней, шаг за тик сквозь стены, невидим живым); консоль камер переносит вид без отдельного моба-глаза; удалённого управления и продвинутой консоли нет

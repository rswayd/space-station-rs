# Лобби
Новый игрок (`/mob/dead/new_player`) до входа в раунд: экран-заставка BandaStation с кнопками, готовность, латджойн, наблюдение.

### Экран-заставка BandaStation
Лобби — HTML-страница поверх окна вместо объекта в мире: фон из пула изображений, имя персонажа, информация о раунде, загрузка подсистем, уведомление, кнопки вики/Discord/GitHub/баг-репорт/чейнджлог.

- **Код:** [`/datum/controller/subsystem/title`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/title_screen/code/title_screen_subsystem.dm#L1), [`title_screen_html.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/title_screen/code/title_screen_html.dm#L1), [`/datum/controller/subsystem/title/proc/show_title_screen_to()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/title_screen/code/title_screen_subsystem.dm#L154)
- **BandaStation:** [`_title_screen.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/title_screen/_title_screen.dm#L1), [`new_player.dm#L89`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/new_player/new_player.dm#L89)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/round.rs` (`LobbyUi`), `crates/ss13_game/src/lobby_client.rs`: tg-заставка `default_title.dmi` с кнопками BandaStation и блоком информации; нет HTML-страницы, пула фонов, уведомлений, ссылок, индикатора загрузки

### Готовность и галочка «Антагонисты»
Кнопка «Готов» до старта; «Антагонисты» — согласие на роли антагов (по умолчанию включено).

- **Код:** [`/mob/dead/new_player/Topic()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/title_screen/code/new_player.dm#L6), [`/mob/dead/new_player/proc/is_ready_to_play()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/new_player/new_player.dm#L71)
- **BandaStation:** [`new_player.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/title_screen/code/new_player.dm#L44) — переключатель `antag_roll`
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/round.rs` (`LobbyAct::ToggleReady`, `ToggleAntagRoll`; готовность требует хотя бы одну профессию)

### Наблюдение
«Наблюдать» с подтверждением: призрак у `observer_start`, без возможности войти в раунд.

- **Код:** [`/mob/dead/new_player/proc/make_me_an_observer()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/new_player/new_player.dm#L75)
- **BandaStation:** [`new_player.dm#L109`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/new_player/new_player.dm#L109) — корректное время смерти
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/round.rs` (`LobbyAct::Observe`), `crates/ss13_game/src/ghost.rs`

### Меню латджойна
TGUI `JobSelection`: профессии по отделам со свободными слотами, причины недоступности, время раунда; проверка на сервере, спавн на `latejoin` или прибытием шаттла.

- **Код:** [`/datum/latejoin_menu`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/new_player/latejoin_menu.dm#L48), [`/mob/dead/new_player/proc/AttemptLateSpawn()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/new_player/new_player.dm#L161), [`/mob/dead/new_player/proc/IsJobUnavailable()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/new_player/new_player.dm#L139); tgui: [`JobSelection.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/JobSelection.tsx)
- **BandaStation:** [`latejoin_menu.dm#L7`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/new_player/latejoin_menu.dm#L7) — ограничения профессий по виду/возрасту
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`, `crates/ss13_tgui/src/interfaces/job_selection.rs`: нет шаттла прибытия, ограничений по виду/возрасту/времени игры, объявления о прибытии

### Манифест экипажа
Кнопка «Манифест персонала» показывает список экипажа по отделам.

- **Код:** [`/mob/dead/new_player/proc/ViewManifest()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/new_player/new_player.dm#L324); tgui: [`CrewManifest.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/CrewManifest.jsx)
- **Статус в ss13-rs:** ❌ нет

### Кнопки настройки персонажа и игры
«Настройка персонажа» и «Настройки игры» открывают окна предпочтений (см. ../ui/).

- **Код:** [`new_player.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/title_screen/code/new_player.dm#L94)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/round.rs` (`JobPrefsUi`): только страница профессий и антагов; настроек игры нет

### Проверка Discord и интервью
С `force_discord_verification` вход и готовность требуют привязки Discord (OAuth-модалка); новичкам — интервью перед игрой.

- **Код:** [`new_player.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/title_screen/code/new_player.dm#L35), [`/mob/dead/new_player/proc/register_for_interview()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/new_player/new_player.dm#L364)
- **BandaStation:** [`new_player.dm#L375`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/new_player/new_player.dm#L109) — интервью только с привязанным Discord
- **Статус в ss13-rs:** ❌ нет

### Админ-кнопки лобби
Для админов: «Запустить раунд», «Отложить начало», оставить уведомление, сменить фон, проиграть видео YouTube/Rutube.

- **Код:** [`/datum/controller/subsystem/title/proc/set_notice()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/title_screen/code/title_screen_subsystem.dm#L192), [`/datum/controller/subsystem/title/proc/set_title_image()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/title_screen/code/title_screen_subsystem.dm#L278), [`/datum/controller/subsystem/title/proc/play_youtube_video()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/title_screen/code/title_screen_subsystem.dm#L245)
- **Статус в ss13-rs:** ❌ нет — `SS13_LOBBY_SECONDS=0` только для разработки

### Запись на станционные трейты
Кнопки в лобби для трейтов с записью (например, добровольцы на особую роль).

- **Код:** [`station_traits.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/title_screen/code/station_traits.dm#L1), [`create_trait_buttons`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/title_screen/code/title_screen_html.dm#L47)
- **Статус в ss13-rs:** ❌ нет

### Сброс HUD лобби
Верб «Reset Lobby Menu HUD» перерисовывает лобби.

- **Код:** [`reset_menu_hud`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/new_player/new_player.dm#L37)
- **BandaStation:** [`new_player.dm#L384`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/dead/new_player/new_player.dm#L384)
- **Статус в ss13-rs:** ❌ нет

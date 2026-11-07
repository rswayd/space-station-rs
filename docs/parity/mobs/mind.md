# Разум (mind)
`/datum/mind` — «душа» игрока, отделённая от тела: ключ, роль, навыки, антагонисты, память; переходит между телами и призраками. Антаг-датумы разума — см. ../antagonists/.

### Разум и перенос между телами
Разум создаётся для моба с ключом (`mind_initialize`) и переносится в новое тело (`transfer_to`: смена тела, превращения, пересадка мозга, обмен разумами), сохраняя роль, навыки, антагонистов и действия.

- **Код:** [`/datum/mind`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L32), [`proc/transfer_to()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L173), [`proc/mind_initialize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/initialization.dm#L2), [`proc/sync_mind()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L534)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/mind.rs`: `MindData` (ключ, имя, счётчики смертей) создаётся мобу с клиентом без разума (`mind_initialize`); `mind::transfer` переносит разум, зависимости, навыки, датумы предателя и кровного брата с антаг-HUD и перенацеливает цели на новое тело; клиент следует в `brainmob::transfer_mind` (пересадка мозга, битраннинг) и при развёртывании ИИ в оболочку. Тест: `tests/it/mood.rs` `mind_follows_transfers_and_ghosts`, юнит-тесты `mind.rs`. Остальные антаг-датумы (генокрад, вампир, культ, еретик, революция, малф) пока на теле — см. ../antagonists/; обмен разумами волшебника идёт через `possess` — см. ../antagonists/

### Призрак разума и возвращение в тело
Поиск призрака разума и его принудительное возвращение в тело (`grab_ghost`) при воскрешении, клонировании, дефибрилляции.

- **Код:** [`proc/get_ghost()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L464), [`proc/grab_ghost()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L472), [`proc/ghost_before_admin_delete()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/initialization.dm#L41)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/mind.rs` `get_ghost`/`grab_ghost(force)`: призрак тела возвращается в него (`ghost::return_to_body` — дефибрилляция, хирургия, заклинания); DNR-призрак помнит тело (`DnrBody`) и возвращается только принудительно (админское лечение). Тест: `tests/it/mood.rs` `mind_follows_transfers_and_ghosts`. `ghost_before_admin_delete` ждёт админского удаления мобов — см. ../admin/

### Назначенная роль и святой статус
Разум хранит назначенную должность (`assigned_role`, приветствие должности, рабочие зоны) и святую роль (капеллан и его помощники).

- **Код:** [`proc/set_assigned_role()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L495), [`proc/set_assigned_role_with_greeting()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L525), [`proc/get_work_areas()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L545), [`proc/set_holy_role()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L507)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs` (`PlayerJob` на теле): должность есть, но не в разуме; нет святой роли и рабочих зон

### Очки зависимостей
Разум накапливает очки зависимости по типам веществ; при пороге начинается зависимость с ломкой (механика — см. ../chemistry/).

- **Код:** [`proc/add_addiction_points()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L479), [`proc/remove_addiction_points()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L486)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/addictions.rs` (`add_points`/`remove_points`, пороги 600/400, ломка); очки лежат на мобе разума и переносятся с ним (`mind::transfer`). Тест: юнит-тесты `addictions.rs`, перенос — `tests/it/mood.rs` `mind_follows_transfers_and_ghosts`. Начисление очков при метаболизме реагентов — см. ../chemistry/

### Свидетель смерти и пережитая смерть
Разум запоминает увиденные смерти и собственную смерть (время смерти, муд-события, память, условия воскрешения).

- **Код:** [`proc/witnessed_death()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L555), [`proc/experienced_death()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L568), [`proc/set_death_time()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L231)
- **Статус в ss13-rs:** ✅ есть — `crates/ss13_game/src/mind.rs` `witnessed_death`/`experienced_death`: каждая увиденная смерть человека (и своя) снижает `desensitized_level` на 0.025 до 0.1, он масштабирует муд-событие `saw_death` (`mood::send_death_moodlets`); `last_death` ставится при смерти моба разума. Тест: `tests/it/mood.rs` `needs_deaths_hugs_and_sanity`, юнит-тест `mind.rs`. Без достижения «desensitized» (достижений нет); память о смерти (`/datum/memory`) — см. систему памяти

### Админ-панель разума (Topic)
Обработчик ссылок панели разума: смена роли, выдача/снятие антагонистов и целей, аплинк. Сама админка — см. ../admin/.

- **Код:** [`/datum/mind/Topic()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L236)
- **BandaStation:** [`_mind.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L338) — уведомление антаг-операций при замене/удалении цели админом
- **Статус в ss13-rs:** 🟡 частично — `AdminRequest::MindPanel`/`MindAct` (`crates/ss13_admin/src/server.rs`, кнопка «Mind» в списке игроков): панель разума (ключ, тело, смерти, зависимости, антаги, цели), снятие предателя/брата, удаление цели, объявление целей; выдача антагов — существующий `MakeAntag`. Логика — `crates/ss13_game/src/mind.rs` `panel`/`remove_objective`/`remove_antag`, юнит-тест там же. Нет: смены должности, добавления/редактирования целей, аплинка, снятия остальных антагов; уведомление антаг-операций BandaStation — см. ../antagonists/

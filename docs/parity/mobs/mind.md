# Разум (mind)
`/datum/mind` — «душа» игрока, отделённая от тела: ключ, роль, навыки, антагонисты, память; переходит между телами и призраками. Антаг-датумы разума — см. ../antagonists/.

### Разум и перенос между телами
Разум создаётся для моба с ключом (`mind_initialize`) и переносится в новое тело (`transfer_to`: смена тела, превращения, пересадка мозга, обмен разумами), сохраняя роль, навыки, антагонистов и действия.

- **Код:** [`/datum/mind`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L32), [`proc/transfer_to()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L173), [`proc/mind_initialize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/initialization.dm#L2), [`proc/sync_mind()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L534)
- **Статус в ss13-rs:** ❌ нет — игрок управляет сущностью тела, «разумом» служит само тело (`crates/ss13_game/src/antag/objectives.rs`); переноса между телами нет

### Призрак разума и возвращение в тело
Поиск призрака разума и его принудительное возвращение в тело (`grab_ghost`) при воскрешении, клонировании, дефибрилляции.

- **Код:** [`proc/get_ghost()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L464), [`proc/grab_ghost()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L472), [`proc/ghost_before_admin_delete()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/initialization.dm#L41)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/ghost.rs`: призрак создаётся, но в тело не возвращается

### Назначенная роль и святой статус
Разум хранит назначенную должность (`assigned_role`, приветствие должности, рабочие зоны) и святую роль (капеллан и его помощники).

- **Код:** [`proc/set_assigned_role()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L495), [`proc/set_assigned_role_with_greeting()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L525), [`proc/get_work_areas()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L545), [`proc/set_holy_role()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L507)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs` (`PlayerJob` на теле): должность есть, но не в разуме; нет святой роли и рабочих зон

### Очки зависимостей
Разум накапливает очки зависимости по типам веществ; при пороге начинается зависимость с ломкой (механика — см. ../chemistry/).

- **Код:** [`proc/add_addiction_points()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L479), [`proc/remove_addiction_points()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L486)
- **Статус в ss13-rs:** ❌ нет

### Свидетель смерти и пережитая смерть
Разум запоминает увиденные смерти и собственную смерть (время смерти, муд-события, память, условия воскрешения).

- **Код:** [`proc/witnessed_death()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L555), [`proc/experienced_death()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L568), [`proc/set_death_time()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L231)
- **Статус в ss13-rs:** ❌ нет

### Админ-панель разума (Topic)
Обработчик ссылок панели разума: смена роли, выдача/снятие антагонистов и целей, аплинк. Сама админка — см. ../admin/.

- **Код:** [`/datum/mind/Topic()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L236)
- **BandaStation:** [`_mind.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/_mind.dm#L338) — уведомление антаг-операций при замене/удалении цели админом
- **Статус в ss13-rs:** ❌ нет

# Праздники и достижения
Календарные праздники, меняющие раунд, и система наград за раунды.

### Праздники
64 праздника (Валентинов день, 1 апреля, Хэллоуин, Рождество, День космонавтики, День Ина, Пиратский день, Октябрьская революция и др.) по реальному календарю (включая «n-я неделя» и восточный календарь): меняют имя станции, лобби-музыку, спавнят предметы, включают события.

- **Код:** [`/datum/holiday`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holiday/holidays.dm#L4), [`/datum/holiday/proc/celebrate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holiday/holidays.dm#L39), [`check_holidays()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/events.dm#L175), [`code/modules/holiday`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/holiday)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/holidays.rs`: все праздники tg по датам и часовым поясам (фиксированные, n-й день недели, пятница 13-е, День программиста, Пасха, исламский и еврейский календари — с исправленной на день эпохой григорианского календаря tg), без Pride Week (BandaStation); `Holidays`/`check_holidays` при старте сервера (`ALLOW_HOLIDAYS`: `SS13_HOLIDAYS=0` выключает, `SS13_HOLIDAY_DATE=YYYY-MM-DD` в debug), «и...» и поздравления после приветствия в начале раунда, праздничные события (events/wizard-holiday-events.md); `// shortcut:` нет праздничных имён станции (имя станции — карта), шапок ассистентов и дронов, праздничной почты (cargo/markets-mail.md), плакатов и цветов, эффектов `celebrate()` (клоуны и лобби-музыка 1 апреля, Рождество, Пан-Пан) и разрозненных проверок праздников в механиках

### Достижения и счёт
Награды (achievements) и счётчики (scores) за профессии, боссов, мафию, навыки, разное; хранятся в БД (см. ../persistence-sql/); окно достижений; секция в отчёте раунда; медали через хаб BYOND.

- **Код:** [`/datum/award`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/achievements/_awards.dm#L1), [`/datum/award/achievement`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/achievements/_awards.dm#L84), [`/datum/award/score`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/achievements/_awards.dm#L137), [`/datum/achievement_data`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/achievements/_achievement_data.dm#L2), [`job_achievements.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/achievements/job_achievements.dm#L1), [`boss_achievements.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/achievements/boss_achievements.dm#L1); tgui: [`Achievements.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Achievements.tsx)
- **Статус в ss13-rs:** ❌ нет

### Админ-панель достижений
Выдача и отзыв достижений игрокам.

- **Код:** [`admin_panel.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/achievements/admin_panel.dm#L1); tgui: [`AchievementsAdminPanel.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AchievementsAdminPanel.tsx)
- **Статус в ss13-rs:** ❌ нет

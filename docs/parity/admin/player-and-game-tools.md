# Инструменты управления игроками и раундом
Панели игроков, управление ролями и профессиями, перемещение и вмешательство, сообщения от лица ЦК.

### Панель игроков и игрока
Список игроков с ролями/статусом/ссылками; панель игрока: кик, бан, заметки, мьют, тело, VV, тп, оживление, роли, джоббан.

- **Код:** [`/datum/admins/proc/player_panel_new()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/player_panel.dm#L1), [`player_panel.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/player_panel.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_admin`: список (ckey, имя, должность, антаг, жив/мёртв/призрак/лобби, координаты, ранг) с JMP, GET, FLW, PM, Mute, Kick, Ban, Heal, Smite, Traitor, Brother. Нет заметок, VV, джоббанов, отдельной панели игрока

### Панели антагонистов
Check Antagonists, Traitor Panel разума (выдача ролей и целей), Team Panel (см. ../antagonists/antag-framework.md).

- **Код:** [`check_antagonists.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/check_antagonists.dm#L1), [`antag_panel.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/antag_panel.dm#L1), [`team_panel.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/team_panel.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_admin`: «make antag» предатель/кровный брат из панели игроков, view-only сводка (антагонисты, принудительные правила, события) вместо Dynamic Panel. Нет Traitor Panel целей, Team Panel, Check Antagonists

### Управление раундом
Start Now, задержка старта, End Round, Reboot World/Cancel Reboot, смена карты, задержка конца раунда, звук конца раунда.

- **Код:** [`server.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/server.dm#L1), [`maprotation.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/maprotation.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_admin`: End Round (`Ticker::force_ending`, `+SERVER`) и Reboot World из админ-панели; нет Start Now, задержек старта/конца, смены карты

### Слоты профессий и респавн
Manage Job Slots (открыть/закрыть слоты), Respawn Character, переключение респавна.

- **Код:** [`manage_job_slots`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/admingame.dm#L351), [`respawn_character`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/admingame.dm#L249)
- **Статус в ss13-rs:** ❌ нет — админ-системы нет

### Админ-призрак, перемещение и невидимость
AGhost, Invisimin, Jump to area/turf/mob/key/coordinate, Get Mob, Send Mob, ESP, комбо-HUD.

- **Код:** [`adminjump.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/adminjump.dm#L1), [`invisimin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/admin_verbs.dm#L65)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_admin`: AGhost (F6, туда и обратно в тело), Jump to Mob, Get Mob, Follow (призрак держится на тайле цели). Нет Invisimin, прыжков по зоне/координатам, Send Mob, ESP, комбо-HUD

### Вселение и прямой контроль
Possess Object, Assume/Give Direct Control, Give AI Controller.

- **Код:** [`possess.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/possess.dm#L1), [`cmd_assume_direct_control`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/debug.dm#L166)
- **Статус в ss13-rs:** ❌ нет — админ-системы нет

### Нарратив и сообщения
Global/Local/Direct Narrate, Subtle Message, Headset Message, Object Say, ответ на молитвы, Requests Manager (молитвы, запросы ЦК/Синдиката, нюк-код).

- **Код:** [`pray.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/pray.dm#L1), [`requests`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/requests.dm#L2), [`special_verbs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/special_verbs.dm#L1)
- **Статус в ss13-rs:** ❌ нет — админ-системы нет

### Отчёты ЦК, факсы и КПК
Create Command Report (объявление/отчёт), смена имени командования, Fax Panel (ответы на факсы ЦК), PDA Message, газета.

- **Код:** [`commandreport.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/commandreport.dm#L1), [`/datum/fax_panel_interface`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/admin_fax_panel.dm#L6), [`/datum/admin_pda_panel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/admin_pda_message.dm#L18), [`admin_newscaster.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/admin_newscaster.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_admin`: объявление ЦК (заголовок и текст, `priority_announce`). Нет выбора отправителя, звука, факсов и КПК

_«Законы ИИ и киборги» описано в [../silicon/laws.md](../silicon/laws.md)._

### Уровень тревоги и шаттл
Set Security Level, вызов/отзыв/отключение шаттла, враждебная среда, менеджер шаттлов, события шаттла.

- **Код:** [`adminshuttle.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/adminshuttle.dm#L1)
- **Статус в ss13-rs:** ❌ нет — админ-системы нет

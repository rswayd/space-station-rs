# Dynamic: выбор антагонистов
Подсистема `SSdynamic` решает, сколько и каких антагонистов появится в начале раунда, по ходу раунда и среди опоздавших. Значения берутся из `config/dynamic.toml`.

### Подсистема Dynamic и тиры угрозы
В начале раунда по весам для текущего населения выбирается тир (Greenshift, Low, Low-Medium, Medium-High, High Chaos); тир задаёт число раундстарт-рулсетов и лимиты мидраундов/латджойнов.

- **Код:** [`SUBSYSTEM_DEF(dynamic)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic.dm#L1), [`/datum/dynamic_tier`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/_dynamic_tier.dm#L9), [`/datum/controller/subsystem/dynamic/proc/pick_tier()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic.dm#L202)
- **BandaStation:** [`dynamic.dm#L282`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic.dm#L282) — исправление веса повторяемых рулсетов
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/antag/dynamic.rs` (`TIERS`, `pick_tier`, `rulesets_to_spawn`), `crates/ss13_game/src/antag/midround.rs` (`TIER_RANGES`, `kind_count`, `start`, `fire` раз в 5 минут, кулдауны 10-20 минут)

### Конфиг dynamic.toml
Тиры, веса рулсетов по тирам, минимальное население, лимиты антагов (`antag_cap`), запрещённые профессии, кулдауны — всё задаётся в `config/dynamic.toml` и читается при старте; `build_dynamic_toml` генерирует шаблон.

- **Код:** [`/datum/controller/subsystem/dynamic/proc/load_config()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic.dm#L170), [`config/dynamic.toml`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/config/dynamic.toml#L1)
- **BandaStation:** [`modular_bandastation/dynamic/code/config.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/dynamic/code/config.dm#L1) — добавляет опции взвешенного выбора кандидатов и порогов угрозы (`roundstart_security_for_threat`, `min/max_threat_to_roundstart_percent`, `min_threat_level` объявлены, но в коде не используются)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/dynamic.rs`: значения тиров и раундстарт-рулсетов BandaStation захардкожены константами, файл конфигурации не читается

### Выбор раундстарт-рулсетов
`pick_roundstart_rulesets` выбирает рулсеты по весу для тира и населения; повторяемые рулсеты (предатели) могут выпасть несколько раз с понижением веса.

- **Код:** [`/datum/controller/subsystem/dynamic/proc/pick_roundstart_rulesets()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic.dm#L257), [`/datum/dynamic_ruleset/proc/get_weight()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/_dynamic_ruleset.dm#L176)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/dynamic.rs` (`pick_roundstart_rulesets`, `ROUNDSTART_RULESETS`): выбор по весам есть, но спавнит только предателя, остальные выпавшие рулсеты никого не создают

### Отбор кандидатов
Кандидат должен включить роль в `be_special`, не иметь джоббана на роль и подходить по возрасту аккаунта/игровому времени; `select_candidates` выбирает до `antag_cap` человек, `prepare_for_role` запрещает им защищённые профессии.

- **Код:** [`/datum/dynamic_ruleset/proc/select_candidates()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/_dynamic_ruleset.dm#L298), [`/datum/dynamic_ruleset/proc/prepare_for_role()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/_dynamic_ruleset.dm#L363), [`/datum/dynamic_ruleset/proc/can_be_selected()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/_dynamic_ruleset.dm#L148)
- **BandaStation:** [`_dynamic_ruleset.dm#L283`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/_dynamic_ruleset.dm#L283) — галочка «Антагонисты» в лобби исключает из всех ролей; [`_dynamic_ruleset.dm#L305`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/_dynamic_ruleset.dm#L305) — взвешенный выбор
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/dynamic.rs` (`select_traitors`, `blacklisted_jobs`, `AntagPreferences`): лобби-галочка, `be_special`, защищённые профессии есть; нет джоббанов, возраста аккаунта и взвешенного выбора

### Взвешенный выбор по истории антагов (BandaStation)
Шанс стать антагом снижается за каждую роль антагониста за последние N дней (по умолчанию 14) — история берётся запросом к таблице feedback БД с кэшем; то же применяется к призрачным ролям событий.

- **Код:** [`select_player_antag_episodes()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/dynamic/code/antag_activity.dm#L15), [`calculate_candidate_weight()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/dynamic/code/dynamic.dm#L8)
- **BandaStation:** [`_dynamic.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/dynamic/_dynamic.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет БД и истории ролей

### Раундстарт-рулсеты
Предатели, малф ИИ, кровные братья, генокрады, кровяные черви, еретики, маг, кровавый культ, нюки и клоун-опы, революция, шпионы, вампиры (BandaStation), а также режимы без антагов.

- **Код:** [`dynamic_ruleset_roundstart.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic_ruleset_roundstart.dm#L1), [`/datum/dynamic_ruleset/roundstart/traitor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic_ruleset_roundstart.dm#L34), [`/datum/dynamic_ruleset/roundstart/vampire`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/dynamic_rulesets.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/dynamic.rs` (`ROUNDSTART_RULESETS`, `implemented`): реализованы `Roundstart Traitor`, `Roundstart Revolution` (`crates/ss13_game/src/antag/revolution/server.rs`), `Roundstart Blood Brothers`, `Roundstart Vampire` (с его `blacklisted_roles`, `ruleset_blacklisted_roles`), `Roundstart Changeling` и `Roundstart Nukeops` (`nukeops_cap`, `select_between`; `crates/ss13_game/src/antag/nukeop/server.rs`), остальные выпадения никого не спавнят; сами антагонисты описаны в соседних файлах

### Режимы Extended, Meteor и Nations
Особые раундстарт-рулсеты: раунд без антагонистов, метеорный дождь, «нации» (департаменты становятся враждующими командами).

- **Код:** [`/datum/dynamic_ruleset/roundstart/extended`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic_ruleset_roundstart.dm#L486), [`/datum/dynamic_ruleset/roundstart/meteor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic_ruleset_roundstart.dm#L499), [`/datum/dynamic_ruleset/roundstart/nations`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic_ruleset_roundstart.dm#L510)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/antag/dynamic.rs` (`EXTENDED_RULESET`, `METEOR_RULESET`, `NATIONS_RULESET`, `is_crewless`, `execute_crewless`): в `ROUNDSTART_RULESETS` с весом 0 (бросок их не выбирает, ставятся админом через `forced_rulesets`), исполняются без кандидатов (`min_antag_cap = 0`) после раздачи профессий; Extended и Nations обнуляют лёгкие/тяжёлые мидраунды и латджойны, Meteor запускает `MeteorMode` (`crates/ss13_game/src/events/meteors.rs`, 5 мин задержки, рост раз в 5 мин), Nations отделяет silicon, cargo, engineering, medical, science, security и service (`crate::antag::separatist`, мирные нации). Нет `solo`-отмены остальных выпадений (при весе 0 недостижимо) и смены `round_default_lawset` для новых синтетиков

### Мидраунд-рулсеты
Периодические попытки заспавнить антагониста по ходу раунда: из живых игроков (предатель, массовые предатели, малф, блоб-инфекция, одержимый) или из призраков через опрос (маг, нюки, блоб, ксеноморфы, кровяные черви, кошмар, космический дракон, абдукторы, ниндзя, ревенант, генокрад, парадокс-клон, войдволкер, беглецы, морф, демон резни, пауки, пираты).

- **Код:** [`/datum/dynamic_ruleset/midround`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic_ruleset_midround.dm#L1), [`/datum/dynamic_ruleset/midround/from_ghosts`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic_ruleset_midround.dm#L222), [`/datum/dynamic_ruleset/midround/from_living`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic_ruleset_midround.dm#L1113), [`/datum/controller/subsystem/dynamic/proc/try_spawn_midround()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic.dm#L329)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/midround.rs` (`RULESETS`, `try_spawn_midround`, `get_weight`, `force_midround`): все мидраунд-рулсеты BandaStation с весами конфига, шанс по доле мёртвых, лёгкие/тяжёлые; спавнят `Midround Traitor`, `Mass Traitors`, `Midround Nukeops`, `Nightmare`, `Morph` и `Voidwalker` (`crate::antag::voidwalker`; маг и парадокс-клон — в своих файлах), остальные проваливают подготовку; нет окна отмены/переброса админом и ложных тревог

### Латджойн-рулсеты
Опоздавший игрок с шансом становится предателем, генокрадом или революционером.

- **Код:** [`/datum/dynamic_ruleset/latejoin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic_ruleset_latejoin.dm#L1), [`/datum/controller/subsystem/dynamic/proc/on_latejoin()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic.dm#L454)
- **BandaStation:** [`dynamic.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/dynamic/code/dynamic.dm#L64) — дополнительные проверки кандидата
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/midround.rs` (`on_latejoin`, `latejoin_chance`, `queued_latejoin`): `Latejoin Traitor`, `Latejoin Changeling`, `Latejoin Revolution` (провокатор), +15% за неудачу, очередь админов; без проверки точки невозврата вызова шаттла

### Отчёт ЦК об угрозе (advisory report)
В начале раунда командование получает отчёт с примерным уровнем угрозы в зависимости от выпавшего тира.

- **Код:** [`/datum/controller/subsystem/dynamic/proc/get_advisory_report()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic.dm#L307)
- **BandaStation:** [`communications.dm#L79`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/communications.dm#L79) — отчёт на старте собирается по шаблону `modular_bandastation/paperwork/templates/station_report.md` (цели смены, отклонения станции, сноски, подпись); уровня угрозы в нём нет, и `get_advisory_report()` нигде не вызывается
- **Статус в ss13-rs:** ✅ как в BandaStation — отчёт на старте раунда (`crates/ss13_game/src/announce.rs`, `queue_roundstart_report`, `crates/ss13_game/src/station_goals.rs`) идёт по шаблону BandaStation без уровня угрозы; мёртвый код tg `get_advisory_report` и тексты `advisory_report` тиров не переносятся

### Админ-панель Dynamic
TGUI-панель: смена тира, постановка рулсетов в очередь, принудительный мидраунд, форс extended, просмотр кулдаунов и шансов.

- **Код:** [`/datum/dynamic_panel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic_admin.dm#L15), [`/datum/controller/subsystem/dynamic/proc/queue_ruleset()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic.dm#L539), [`/datum/controller/subsystem/dynamic/proc/force_run_midround()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic.dm#L419); tgui: [`DynamicAdmin.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/DynamicAdmin.tsx)
- **BandaStation:** [`dynamic.dm#L350`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic.dm#L350) — проверка прав R_ADMIN
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_admin/src/server.rs` (`dynamic_report`, `AdminRequest::ForceMidround`), `crates/ss13_game/src/antag/midround.rs` (`report_lines`): сводка с тиром, остатком рулсетов и таймерами мидраунда/латджойна, принудительный мидраунд; нет правки тира и очереди из окна

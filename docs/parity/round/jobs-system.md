# Система профессий
Подсистема `SSjob`, датум профессии, отделы, распределение на старте, требования по времени игры и изменения BandaStation (приоритеты укомплектования, новые отделы, слоты персонажей, ограничения, русские названия).

### Датум профессии и отделы
`/datum/job`: название, отделы, слоты (total/spawn), доступ через ID-трим, аутфит, флаги (присоединяемая, защищённая от антагов и т.п.), приветствие и инфо о радиоканалах; `/datum/job_department` с цветом и порядком.

- **Код:** [`/datum/job`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/_job.dm#L1), [`/datum/job_department`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/departments/departments.dm#L2), [`/datum/job/proc/get_spawn_message()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/_job.dm#L302), [`/datum/job/proc/after_spawn()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/_job.dm#L152)
- **BandaStation:** [`modular_bandastation/jobs/code/departments/justice.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/departments/justice.dm#L1), [`modular_bandastation/jobs/code/departments/nt_representation.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/departments/nt_representation.dm#L1) — отделы «Юстиция» и «Представительство NT»
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/jobs.rs` (`jobs.ron`), `crates/ss13_game/src/jobs.rs`: профессии, отделы, слоты, аутфит, доступ; нет приветствия с радиоканалами, `after_spawn`-эффектов, почтовых подарков

### Распределение профессий на старте
`divide_occupations`: сначала главы (`force_one_head_assignment`), ИИ, затем по уровням предпочтений; невыбранные — по настройке «если профессия недоступна» (ассистент/случайная/в лобби).

- **Код:** [`/datum/controller/subsystem/job/proc/divide_occupations()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/job.dm#L413), [`/datum/controller/subsystem/job/proc/fill_all_head_positions_at_priority()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/job.dm#L366), [`/datum/controller/subsystem/job/proc/fill_ai_positions()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/job.dm#L393), [`/datum/controller/subsystem/job/proc/handle_unassigned()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/job.dm#L543)
- **BandaStation:** [`job.dm#L464`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/job.dm#L464) — приоритетное укомплектование
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/round.rs` (`divide_occupations`)

### Приоритеты укомплектования (BandaStation)
Каждой профессии задан приоритет укомплектования; на каждом уровне предпочтений сначала заполняются самые нужные (капитан, ИИ, СБ, ...), overflow-галочки обрабатываются первыми.

- **Код:** [`cmp_job_staffing_priority()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/job_types/job_staffing_priority.dm#L8), [`/datum/controller/subsystem/job/proc/give_priority_job()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/controller/subsystem/job.dm#L9), [`/datum/controller/subsystem/job/proc/shuffle_and_sort_jobs_by_staffing_priority()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/controller/subsystem/job.dm#L40)
- **BandaStation:** [`_jobs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/_jobs.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/round.rs` (`divide_occupations` с приоритетами BandaStation)

### Overflow-профессия
Профессия по умолчанию (ассистент), может меняться (станционный трейт бюрократии); бесконечные слоты.

- **Код:** [`/datum/controller/subsystem/job/proc/set_overflow_role()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/job.dm#L122), [`/datum/controller/subsystem/job/proc/get_valid_overflow_jobs()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/job.dm#L110)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/round.rs`: ассистент-overflow есть, смены overflow нет

### Слоты офицеров СБ от населения
Число офицеров = готовые игроки / `security_scaling_coeff`, в пределах настраиваемых минимума и максимума; дополнительные шкафчики снаряжения.

- **Код:** [`/datum/controller/subsystem/job/proc/setup_officer_positions()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/job.dm#L624)
- **BandaStation:** [`job.dm#L630`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/job.dm#L630) — `security_min_positions`/`security_max_positions`
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/round.rs` (`divide_occupations`: готовые / `SECURITY_SCALING_COEFF` с округлением вниз, как DM `round()`, в пределах мин. 5 и макс. 12); `crates/ss13_game/src/jobs/security.rs` (`spawn_extra_lockers`): шкафчики офицера `secure_closet/security/sec` на landmark `secequipment` по числу недостающих (BandaStation `security_closets_count`). Нет: чтения коэффициента и пределов из конфига (константы = дефолты конфига) — группа конфига

### Требования по времени игры
Профессии требуют наигранных часов (общих или по отделу) и возраста аккаунта; окно `TrackedPlaytime` показывает наигранное время.

- **Код:** [`/datum/job/proc/required_playtime_remaining()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_exp.dm#L6), [`/datum/job/proc/player_old_enough()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/_job.dm#L242), [`/datum/job_report_menu`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_report.dm#L4); tgui: [`TrackedPlaytime.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/TrackedPlaytime.jsx)
- **Статус в ss13-rs:** ❌ нет — нет БД и учёта времени (см. ../persistence-sql/); отложено до волны SQL/persistence

### Проверка допуска к профессии
`check_job_eligibility`: джоббан, время игры, возраст аккаунта, конфиг, карта, лимит слотов.

- **Код:** [`/datum/controller/subsystem/job/proc/check_job_eligibility()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/job.dm#L949), [`/datum/job/proc/config_check()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/_job.dm#L276), [`/datum/job/proc/map_check()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/_job.dm#L285)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs` (`unavailable`): только слоты и играбельность; нет джоббанов, времени, конфига

### Ограничения профессий по виду (BandaStation)
Конфиги `job_restrictions` и `allowed_species` запрещают расам некоторые профессии (проверка в лобби и латджойне); станционный трейт «ошибка ксенобюрократии».

- **Код:** [`/client/proc/validate_job_restrictions()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/title_screen/code/title_screen_controls.dm#L79), [`job_restrictions config`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/job_restrictions/code/config.dm#L1), [`/datum/station_trait/xenobureaucracy_error`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/job_restrictions/code/xenobureaucracy_trait.dm#L1)
- **BandaStation:** [`_job_restrictions.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/job_restrictions/_job_restrictions.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/jobs.rs` (`species_restricted`, `restricted_jobs`, `validate_job_restrictions`): `SS13_JOB_RESTRICTIONS`/`SS13_ALLOWED_SPECIES` (`RoundConfig`), проверка «Готов» в лобби (`round.rs`), снятие готовности при смене расы/профессий, отказ в латджойне; случайная профессия и распределение на старте обходят запрещённые (отклонение: BandaStation проверяет только явный выбор); трейт `xenobureaucracy_error` в `station_traits.rs` (вес 0, только принудительно)

### Слоты персонажей по профессиям (BandaStation)
Профессии можно привязать к слотам персонажей: при входе на эту профессию загружается нужный персонаж.

- **Код:** [`/datum/preference_middleware/pref_job_slots`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/job_slots_preferences/code/pref_job_slots_middleware.dm#L1), [`/datum/preferences/proc/get_slot_options()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/job_slots_preferences/code/job_slots_preferences.dm#L20)
- **BandaStation:** [`_job_slots_preferences.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/job_slots_preferences/_job_slots_preferences.dm#L1), [`_job.dm#L568`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/_job.dm#L568)
- **Статус в ss13-rs:** ❌ нет — слотов персонажей нет

### Русские названия профессий (BandaStation)
Русские названия профессий и отделов, русские названия ID-карт и КПК («ID-карта «Имя» (Должность)»).

- **Код:** [`ru_jobs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/ru_jobs/code/ru_jobs.dm#L1), [`ru_id_cards.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/ru_jobs/code/ru_id_cards.dm#L1), [`ru_pda.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/ru_jobs/code/ru_pda.dm#L1)
- **BandaStation:** [`_ru_jobs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/ru_jobs/_ru_jobs.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_tgui/src/interfaces/ru_jobs.rs` (`JOBS_RU`, `DEPARTMENTS_RU`), `crates/ss13_game/src/jobs.rs` (название ID-карты), `crates/ss13_game/src/modpc/pda.rs` (название КПК «КПК «Имя» (Должность)»)

### Точки спавна и латджойн-спавн
Старт на `/obj/effect/landmark/start` профессии; латджойн — шаттл прибытия, крио или `latejoin`-маркеры, запасные точки.

- **Код:** [`/datum/job/proc/get_roundstart_spawn_point()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/_job.dm#L474), [`/datum/job/proc/get_latejoin_spawn_point()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/_job.dm#L511), [`/datum/controller/subsystem/job/proc/send_to_late_join()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/job.dm#L745), [`/datum/controller/subsystem/job/proc/get_last_resort_spawn_points()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/job.dm#L761)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs` (`job_spawn`, `join`): маркеры старта и латджойна; шаттла прибытия нет

### Объявления о главах и капитане
При входе главы/капитана — объявление по радио/станции; повышение до капитана, если капитана нет; рассылка кода запасной ID.

- **Код:** [`/datum/job/proc/announce_head()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/_job.dm#L236), [`/datum/controller/subsystem/job/proc/promote_to_captain()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/job.dm#L843), [`/datum/controller/subsystem/job/proc/send_spare_id_safe_code()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/job.dm#L874)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/jobs.rs` (`announce_head`, `latejoin_captaincy`, `roundstart_captaincy`, `promote_to_captain`): AAS «назначен(а) главой отдела» на канале отдела, капитанство по `chain_of_command` на старте и в латджойне с объявлением, доступ `command` на ID; 🟡 папка с кодом запасной ID и `send_spare_id_safe_code` — нет сейфа запасной ID (`SSid_access`, группа ID/доступа)

### Профессии не-экипажа
Датумы профессий для антагонистов (нюки, маг, абдукторы, ...), спавнеров призраков, событий (Санта, беглецы, админ), ОБР и станционных трейтов (ассистент мостика, ИИ-человек, ветеран-советник, горилла карго, Пун-Пун).

- **Код:** [`code/modules/jobs/job_types/antagonists`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/antagonists), [`code/modules/jobs/job_types/spawner`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/spawner), [`code/modules/jobs/job_types/event`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/event), [`code/modules/jobs/job_types/station_trait`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/station_trait), [`/datum/job/veteran_advisor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/station_trait/veteran_advisor.dm#L1)
- **BandaStation:** [`veteran_advisor.dm#L77`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/station_trait/veteran_advisor.dm#L77) — кобура советника
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/noncrew_jobs.rs` (`NON_CREW_JOBS`, `NonCrewJob`, `set_assigned_role`): датумы антагонистов, спавнеров, событий и ОБР; назначаются нюкам, магу и ученику, кошмару, парадокс-клону, морфу, гост-ролям; видны в панели разума и `job_title`; 🟡 профессии станционных трейтов (ассистент мостика, ветеран-советник, ИИ-человек, горилла, Пун-Пун) — у станционных трейтов (их трейтов нет в пуле); `spawner_job_path` спавнеров не конвертируется (все — `ghost_role`)

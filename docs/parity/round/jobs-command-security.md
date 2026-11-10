# Профессии: командование и СБ
Главы отделов, капитан, служба безопасности и юстиция (включая профессии BandaStation).

### Капитан
Глава станции: полный доступ, запасная ID в сейфе (код рассылается главам), объявление о прибытии капитана; при отсутствии — исполняющий обязанности из глав.

- **Код:** [`/datum/job/captain`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/captain.dm#L1)
- **BandaStation:** [`captain.dm#L101`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/captain.dm#L101) — флафф станции
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет кода сейфа запасной ID, объявления о прибытии, повышения до капитана

### Глава персонала
Глава сервиса, управляет ID-картами и профессиями; особая цель предателя для ГП.

- **Код:** [`/datum/job/head_of_personnel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/head_of_personnel.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет консоли ID и цели предателя ГП

### Глава службы безопасности
Глава СБ: оружие на выбор (маяк), импланты лояльности.

- **Код:** [`/datum/job/head_of_security`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/head_of_security.dm#L1)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID; `crates/ss13_game/src/outfit.rs` (`implant_outfit`): имплант защиты разума из `implants` аутфита (тихо, как `implant(user, null, TRUE)`); маяк оружия ГСБ — в его шкафчике (`populate_contents_immediate`, `crates/ss13_game/src/closets.rs`), выбор и доставка капсулой — `crates/ss13_game/src/devices/beacons.rs` + `cargo/supplypod.rs`. Нет (чужие группы): «свинина» при разделке (`PIG_COP_PROBABILITY`, гиббер/разделка), черты печени и `HEAD_OF_STAFF_MIND_TRAITS` (метаболизм, выкуп)

### Смотритель
Управляет бригом и арсеналом.

- **Код:** [`/datum/job/warden`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/warden.dm#L1)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID; `crates/ss13_game/src/outfit.rs` (`implant_outfit`): имплант защиты разума; механики брига — в ../security/ (таймеры, консоль заключённых, `crates/ss13_game/src/brig/`). Нет (чужие группы): «свинина» при разделке, черты печени

### Детектив
Расследует: сканер улик, револьвер, кобура.

- **Код:** [`/datum/job/detective`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/detective.dm#L1)
- **BandaStation:** [`detective.dm#L39`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/detective.dm#L39) — кобура-аксессуар
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID; сканер улик в рюкзаке (`backpack_contents`, `crates/ss13_game/src/forensics/`); револьвер — в кобуре-аксессуаре из шкафа детектива (`crates/ss13_game/src/clothing/holsters.rs`); сигарета зажжена при спавне (`crates/ss13_game/src/jobs/security.rs`, `light_mask_cigarette`); `crates/ss13_game/src/outfit.rs` (`implant_outfit`): имплант защиты разума. Нет: скиллчипа `detectives_taste` (скиллчипы), детской шляпы/конфеты для несовершеннолетних (нет возраста персонажа)

### Офицер СБ
Офицеры распределяются по отделам (предпочтение отдела), число слотов зависит от населения.

- **Код:** [`/datum/job/security_officer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/security_officer.dm#L1)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/jobs/security.rs`: распределение по наименее занятому отделу (`get_officer_department`, `SecurityDistribution`), нарукавная повязка, гарнитура отдела, трим отдела (назначение, `department_access` + `elevated_access`), КПК по триму, сообщение офицеру; на старте раунда — пост отдела (landmark `*_sec`, иначе свободная клетка чекпоинта); опоздавший — строка АСО на канале СБ и КПК офицерам отдела; `crates/ss13_game/src/outfit.rs` (`implant_outfit`): имплант защиты разума. Нет: предпочтения отдела в настройках персонажа (все — «Нет») и чтения `DEPSEC_ACCESS_LEVEL`/`SEC_START_BRIG` из конфига (взяты значения `config/game_options.txt`: 2 и выкл.) — группа префов/конфига; «свинина» при разделке

### Заключённый
Начинает в пермабриге со сроком и преступлением.

- **Код:** [`/datum/job/prisoner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/prisoner.dm#L1)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/jobs.rs`: старт на landmark `Prisoner` (как у tg; без него — точка опоздавших), тюремная ID с карточкой срока (`crates/ss13_game/src/brig/prisoner.rs`, `brig/sentence.rs`); `crates/ss13_game/src/jobs/security.rs` (`prisoner_joined`): случайное преступление из `GLOB.prisoner_crimes` в записи СБ от «Central Command» (`crates/ss13_game/src/records/manifest.rs`) и сообщение игроку. Нет: выбора преступления в префах (всегда «Random»), тюремных татуировок (персистентность — в конце), памяти о преступлении, шапки D-boy (1%)

### Синий щит (BandaStation)
Телохранитель глав и представителя NT, отдел представительства NT.

- **Код:** [`/datum/job/blueshield`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/job_types/blueshield/job.dm#L1)
- **BandaStation:** [`modular_bandastation/jobs/code/job_types/blueshield/job.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/job_types/blueshield/job.dm#L1)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/jobs.rs`: профессия, аутфит BandaStation (электропосох, коробка имплантов смерти, маяк оружия в рюкзаке — `crates/ss13_game/src/devices/beacons.rs`), ID; `crates/ss13_game/src/outfit.rs` (`implant_outfit`): имплант защиты разума. «Целей охраны» в DM нет. Нет: черты печени (метаболизм)

### Представитель NanoTrasen (BandaStation)
Наблюдатель корпорации, отдел представительства NT, свой КПК и доступ.

- **Код:** [`/datum/job/nanotrasen_representative`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/job_types/nanotrasen_representative/job.dm#L1)
- **BandaStation:** [`modular_bandastation/jobs/code/job_types/nanotrasen_representative/job.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/job_types/nanotrasen_representative/job.dm#L1)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/jobs.rs`: профессия, аутфит (трость NT, КПК представителя), ID, отделы представительства и командования; `crates/ss13_game/src/outfit.rs` (`implant_outfit`): имплант защиты разума. Нет (чужие группы): `HEAD_OF_STAFF_MIND_TRAITS` (быстрое связывание, ценный выкуп), черты печени

### Магистрат (BandaStation)
Судья станции, отдел юстиции.

- **Код:** [`/datum/job/magistrate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/job_types/job_staffing_priority.dm#L141)
- **BandaStation:** [`modular_bandastation/jobs/code/job_types/magistrate/job.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/job_types/magistrate/job.dm#L1)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/jobs.rs`: профессия, аутфит (телескопическая дубинка, гарнитура магистрата), ID, отдел юстиции, объявление главы на канале юстиции (`HEAD_ANNOUNCE`); `crates/ss13_game/src/outfit.rs` (`implant_outfit`): имплант защиты разума. Нет: черты печени (метаболизм)

### Адвокат
Защитник; в BandaStation перенесён в отдел юстиции.

- **Код:** [`/datum/job/lawyer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/lawyer.dm#L1)
- **BandaStation:** [`modular_bandastation/jobs/code/job_types/lawyer/trim.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/job_types/lawyer/trim.dm#L1) — свой трим, отдел юстиции
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/jobs.rs`: профессия с формальным аутфитом BandaStation (`lawyer_formal`: портфель, значок адвоката, лазерная указка), отдел юстиции и трим BandaStation; других механик у профессии в DM нет. Нет: черты печени (метаболизм)

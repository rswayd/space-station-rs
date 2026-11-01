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
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет маяка оружия и имплантов

### Смотритель
Управляет бригом и арсеналом.

- **Код:** [`/datum/job/warden`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/warden.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет особых механик брига (см. ../security/)

### Детектив
Расследует: сканер улик, револьвер, кобура.

- **Код:** [`/datum/job/detective`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/detective.dm#L1)
- **BandaStation:** [`detective.dm#L39`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/detective.dm#L39) — кобура-аксессуар
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет сканера улик и огнестрела

### Офицер СБ
Офицеры распределяются по отделам (предпочтение отдела), число слотов зависит от населения.

- **Код:** [`/datum/job/security_officer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/security_officer.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет распределения по отделам

### Заключённый
Начинает в пермабриге со сроком и преступлением.

- **Код:** [`/datum/job/prisoner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/prisoner.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет срока, преступления и тюремного спавна, если нет landmark

### Синий щит (BandaStation)
Телохранитель глав и представителя NT, отдел представительства NT.

- **Код:** [`/datum/job/blueshield`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/job_types/blueshield/job.dm#L1)
- **BandaStation:** [`modular_bandastation/jobs/code/job_types/blueshield/job.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/job_types/blueshield/job.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет особой экипировки и целей охраны

### Представитель NanoTrasen (BandaStation)
Наблюдатель корпорации, отдел представительства NT, свой КПК и доступ.

- **Код:** [`/datum/job/nanotrasen_representative`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/job_types/nanotrasen_representative/job.dm#L1)
- **BandaStation:** [`modular_bandastation/jobs/code/job_types/nanotrasen_representative/job.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/job_types/nanotrasen_representative/job.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет особых механик

### Магистрат (BandaStation)
Судья станции, отдел юстиции.

- **Код:** [`/datum/job/magistrate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/job_types/job_staffing_priority.dm#L141)
- **BandaStation:** [`modular_bandastation/jobs/code/job_types/magistrate/job.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/job_types/magistrate/job.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет особых механик

### Адвокат
Защитник; в BandaStation перенесён в отдел юстиции.

- **Код:** [`/datum/job/lawyer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/lawyer.dm#L1)
- **BandaStation:** [`modular_bandastation/jobs/code/job_types/lawyer/trim.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/job_types/lawyer/trim.dm#L1) — свой трим, отдел юстиции
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет особых механик

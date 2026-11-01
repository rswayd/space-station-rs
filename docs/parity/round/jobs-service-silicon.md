# Профессии: сервис, ассистенты, синтетики
Сервис, ассистенты, ИИ и киборги, профессии станционных трейтов.

### Бармен
Бар, дробовик бармена.

- **Код:** [`/datum/job/bartender`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/bartender.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет механик бара (см. ../service/)

### Повар
Кухня.

- **Код:** [`/datum/job/cook`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/cook.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет полной кухни (см. ../kitchen/)

### Ботаник
Гидропоника.

- **Код:** [`/datum/job/botanist`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/botanist.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`, `crates/ss13_game/src/botany/`: профессия с аутфитом и доступом, лотки, семена, урожай; нет биогенератора, сумок для растений, пчёл (см. ../botany/)

### Уборщик
Уборка, ведро, знаки.

- **Код:** [`/datum/job/janitor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/janitor.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет уборки

### Клоун
Клоунская неуклюжесть, гудок, имя-клоун.

- **Код:** [`/datum/job/clown`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/clown.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет неуклюжести и клоунского имени

### Мим
Обет молчания, невидимые стены, имя-мим.

- **Код:** [`/datum/job/mime`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/mime.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет обета молчания и заклинаний мима

### Капеллан
Выбор религии и божества, нуллрод из многих вариантов, святая вода, костюмы.

- **Код:** [`/datum/job/chaplain`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/chaplain/chaplain.dm#L1)
- **BandaStation:** [`chaplain_nullrod.dm#L52`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/chaplain/chaplain_nullrod.dm#L52) — нуллрод против вампиров
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет религии и нуллродов

### Куратор
Библиотека, набор исследователя.

- **Код:** [`/datum/job/curator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/curator.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет библиотеки

### Психолог
Терапия, мудлеты.

- **Код:** [`/datum/job/psychologist`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/psychologist.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет механик настроения

### Ассистент
Профессия по умолчанию (overflow), разноцветные и гиммик-ассистенты.

- **Код:** [`/datum/job/assistant`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/assistant/assistant.dm#L6)
- **BandaStation:** [`assistant.dm#L73`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/assistant/assistant.dm#L73) — учёт лоадаута
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет цветных/гиммик-вариантов

### ИИ
Искусственный интеллект станции (законы, ядро).

- **Код:** [`/datum/job/ai`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/ai.dm#L1)
- **Статус в ss13-rs:** ❌ нет — профессия не играбельна (`unresolved` в `crates/ss13_game/src/jobs.rs`), ИИ нет (см. ../silicon/)

### Киборг
Киборг с модулями.

- **Код:** [`/datum/job/cyborg`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/cyborg.dm#L1)
- **Статус в ss13-rs:** ❌ нет — профессия не играбельна, киборгов нет

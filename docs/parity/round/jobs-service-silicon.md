# Профессии: сервис, ассистенты, синтетики
Сервис, ассистенты, ИИ и киборги, профессии станционных трейтов.

### Бармен
Бар, дробовик бармена.

- **Код:** [`/datum/job/bartender`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/bartender.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; собственной логики у профессии в DM нет (награда за сервис — достижения); бар и дробовик — в ../kitchen/ и ../combat/. Нет: скиллчипы (../mobs/skills.md), черты печени (метаболизм), `mind_traits`

### Повар
Кухня.

- **Код:** [`/datum/job/cook`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/cook.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; кухня — в ../kitchen/. Нет понижения второго повара до «Cook» (`/datum/outfit/job/cook/pre_equip`: трим, фартук и колпак). Нет (чужие группы): скиллчипы (../mobs/skills.md), черты печени (метаболизм), `mind_traits`

### Ботаник
Гидропоника.

- **Код:** [`/datum/job/botanist`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/botanist.dm#L1)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; собственной логики у профессии в DM нет, гидропоника — `crates/ss13_game/src/botany/` (../botany/)

### Уборщик
Уборка, ведро, знаки.

- **Код:** [`/datum/job/janitor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/janitor.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; собственной логики у профессии в DM нет, кроме револьвера в День мусора (праздники — ../round/); уборка — `crates/ss13_game/src/janitor.rs` (../service/). Нет: скиллчипы (../mobs/skills.md), черты печени (метаболизм), `mind_traits`

### Клоун
Клоунская неуклюжесть, гудок, имя-клоун.

- **Код:** [`/datum/job/clown`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/clown.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; при спавне клоун получает неуклюжесть (`make_clown_clumsy`, `/datum/mutation/clumsy`); гудок и обувь — `service/clown.rs`. Нет: имя клоуна из настроек персонажа (../ui/), бананиум по трейту станции, фракция клоунов. Нет: скиллчипы (../mobs/skills.md), черты печени (метаболизм), `mind_traits`

### Мим
Обет молчания, невидимые стены, имя-мим.

- **Код:** [`/datum/job/mime`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/mime.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; при спавне обет молчания (`crates/ss13_game/src/service/clown.rs`, `MimeVow`: не говорит и не шепчет), заклинания мима из книги — `spells/`. Нет: имя мима из настроек персонажа (../ui/), нарушение обета (действие заклинания)

### Капеллан
Выбор религии и божества, нуллрод из многих вариантов, святая вода, костюмы.

- **Код:** [`/datum/job/chaplain`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/chaplain/chaplain.dm#L1)
- **BandaStation:** [`chaplain_nullrod.dm#L52`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/chaplain/chaplain_nullrod.dm#L52) — нуллрод против вампиров
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; при спавне Библия и названия религии (`crate::service::chaplain_spawned`), нуллроды, святая вода, обряды — `crates/ss13_game/src/service/` (../service/). Нет: скиллчипы (../mobs/skills.md), черты печени (метаболизм), `mind_traits`

### Куратор
Библиотека, набор исследователя.

- **Код:** [`/datum/job/curator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/curator.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; при спавне знает все языки (`grant_all`, `/datum/outfit/job/curator/post_equip`); библиотека — `service/library*.rs` (../service/). Нет вещательной камеры первому куратору в рюкзаке (сама камера — `devices/broadcast_camera.rs`). Нет: скиллчипы (../mobs/skills.md), черты печени (метаболизм), `mind_traits`

### Психолог
Терапия, мудлеты.

- **Код:** [`/datum/job/psychologist`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/psychologist.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; собственной логики у профессии в DM нет; настроение — `crates/ss13_game/src/mood.rs` (../mobs/). Нет: скиллчипы (../mobs/skills.md), черты печени (метаболизм), `mind_traits`

### Ассистент
Профессия по умолчанию (overflow), разноцветные и гиммик-ассистенты.

- **Код:** [`/datum/job/assistant`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/assistant/assistant.dm#L6)
- **BandaStation:** [`assistant.dm#L73`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/assistant/assistant.dm#L73) — учёт лоадаута
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: выбор профессии, спавн с аутфитом и ID-доступом есть; нет цветных/гиммик-вариантов

### ИИ
Искусственный интеллект станции (законы, ядро).

- **Код:** [`/datum/job/ai`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/ai.dm#L1)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/silicon/mod.rs` (`join`), `silicon/ai.rs`: профессия играбельна — ядро на точке старта ИИ (первичная незанятая, иначе любая), законы, связь киборгов без ИИ с новым (`after_spawn`); механики ИИ — в ../silicon/

### Киборг
Киборг с модулями.

- **Код:** [`/datum/job/cyborg`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/cyborg.dm#L1)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/silicon/mod.rs` (`join`), `silicon/cyborg.rs`: профессия играбельна — тело киборга, привязка к ИИ, выбор модели; механики киборгов — в ../silicon/

# Профессии: инженерия, медицина, наука, карго
Технические отделы станции.

### Старший инженер
Глава инженерии: продвинутые магнитные ботинки, чертежи.

- **Код:** [`/datum/job/chief_engineer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/chief_engineer.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ, объявление главы; собственная логика профессии в DM — только память с ключом сервера сообщений (`after_spawn`), её нет (нет воспоминаний); чертежи и магбутсы — предметы в шкафу СИ (../engineering/). Нет: скиллчипы (../mobs/skills.md), черты печени (метаболизм), `mind_traits`

### Инженер станции
Ремонт, питание, двигатель.

- **Код:** [`/datum/job/station_engineer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/station_engineer.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; собственной логики у профессии в DM нет, двигатель и ремонт — в ../engineering/ и ../power/. Нет: скиллчипы (../mobs/skills.md), черты печени (метаболизм), `mind_traits`

### Атмосферный техник
Атмосфера и пожаротушение.

- **Код:** [`/datum/job/atmospheric_technician`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/atmospheric_technician.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; собственной логики у профессии в DM нет, атмосфера — в ../atmos/. Нет: скиллчипы (../mobs/skills.md), черты печени (метаболизм), `mind_traits`

### Главврач
Глава медицины: гипоспрей, наладонный мед-HUD.

- **Код:** [`/datum/job/chief_medical_officer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/chief_medical_officer.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ, объявление главы; собственной логики у профессии в DM нет; гипоспрей — предмет шкафа главврача (`crates/ss13_game/src/chem.rs`), мед-HUD — `records/medhud.rs`. Нет: скиллчипы (../mobs/skills.md), черты печени (метаболизм), `mind_traits`

### Врач
Лечение и хирургия.

- **Код:** [`/datum/job/doctor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/medical_doctor.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; собственной логики у профессии в DM нет, хирургия и лечение — в ../medical/ (`crates/ss13_game/src/medical/`). Нет: скиллчипы (../mobs/skills.md), черты печени (метаболизм), `mind_traits`

### Парамедик
Спасение раненых по станции.

- **Код:** [`/datum/job/paramedic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/paramedic.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; собственной логики у профессии в DM нет. Нет: скиллчипы (../mobs/skills.md), черты печени (метаболизм), `mind_traits`

### Химик
Химия и лекарства.

- **Код:** [`/datum/job/chemist`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/chemist.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; собственной логики у профессии в DM нет, химия — в ../chemistry/. Нет: скиллчипы (../mobs/skills.md), черты печени (метаболизм), `mind_traits`

### Коронер
Морг и вскрытия.

- **Код:** [`/datum/job/coroner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/coroner.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; собственной логики у профессии в DM нет, вскрытие — операция в ../medical/ (`medical/operating.rs`). Нет: скиллчипы (../mobs/skills.md), черты печени (метаболизм), `mind_traits` (в т.ч. `TRAIT_MORBID`)

### Директор исследований
Глава науки: исследования, ИИ, роботы.

- **Код:** [`/datum/job/research_director`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/research_director.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ, объявление главы; собственной логики у профессии в DM нет, исследования — в ../science/, ИИ и киборги — в ../silicon/. Нет: скиллчипы (../mobs/skills.md), черты печени (метаболизм), `mind_traits`

### Учёный
Исследования, ксенобиология, аномалии.

- **Код:** [`/datum/job/scientist`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/scientist.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; исследования, ксенобиология, аномалии — в ../science/ (`crates/ss13_game/src/science/`). Нет «ужасного галстука» (0,4% при спавне). Нет: скиллчипы (../mobs/skills.md), черты печени (метаболизм), `mind_traits`

### Робототехник
Мехи, киборги, импланты.

- **Код:** [`/datum/job/roboticist`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/roboticist.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; собственной логики у профессии в DM нет; мехи — `crates/ss13_game/src/mecha/`, киборги — `silicon/`, импланты — `implants/`, `medical/cyberimps.rs`. Нет: скиллчипы (../mobs/skills.md), черты печени (метаболизм), `mind_traits`

### Генетик
Генетика и мутации.

- **Код:** [`/datum/job/geneticist`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/geneticist.dm#L1)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; собственной логики у профессии в DM нет, генетика — в ../genetics/ (`crates/ss13_game/src/genetics/`)

### Квартирмейстер
Глава карго: заказы, бюджет.

- **Код:** [`/datum/job/quartermaster`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/quartermaster.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ, объявление главы; собственной логики у профессии в DM нет; консоль снабжения, экспресс-консоль, бюджет — в ../cargo/ (`crates/ss13_game/src/cargo/`). Нет: скиллчипы (../mobs/skills.md), черты печени (метаболизм), `mind_traits`

### Грузчик
Доставка заказов.

- **Код:** [`/datum/job/cargo_technician`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/cargo_technician.dm#L1)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; собственной логики у профессии в DM нет; шаттл снабжения, продажа, почта — в ../cargo/

### Шахтёр
Добыча на Лаваленде.

- **Код:** [`/datum/job/shaft_miner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/shaft_miner.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; собственной логики у профессии в DM нет; шахтёрское снаряжение, фауна и шаттл — `crates/ss13_game/src/mining/`, `mobs/lavaland/`, `mining_shuttles.rs` (../cargo/). Нет: скиллчипы (../mobs/skills.md), черты печени (метаболизм), `mind_traits` (в т.ч. `TRAIT_DETECT_STORM`)

### Битраннер
Виртуальные домены за награды.

- **Код:** [`/datum/job/bitrunner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/bitrunning/job.dm#L1)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ; собственной логики у профессии в DM нет, битраннинг — `crates/ss13_game/src/science/bitrunning/` (../science/)

### Исследователь (BandaStation)
Карго-профессия для исследования космоса.

- **Код:** [`/datum/job/explorer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/job_types/explorer/job.dm#L1)
- **BandaStation:** [`modular_bandastation/jobs/code/job_types/explorer/job.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/jobs/code/job_types/explorer/job.dm#L1)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/jobs.rs`: профессия, аутфит, ID-трим и доступ (аутфит, трим и КПК форка); собственной логики у профессии в DM нет; экспедиции — в ../map/

# Станционные трейты и цели станции
Случайные модификаторы раунда (`SSstation`) и цели станции от ЦК.

### Механизм станционных трейтов
`SSstation` в начале раунда выбирает по весам положительные, нейтральные и отрицательные трейты (часть с объявлением в отчёте ЦК); трейты могут добавлять кнопки записи в лобби.

- **Код:** [`PROCESSING_SUBSYSTEM_DEF(station)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/processing/station.dm#L1), [`/datum/station_trait`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/station_traits/_station_trait.dm#L5)
- **BandaStation:** [`station_traits.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/station_traits.dm#L1) — часть трейтов отключена (вес 0)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/station_traits.rs` (`roll`, `pick_traits`, `StationTraitsConfig`): бюджеты BandaStation по знаку, веса, стоимость, чёрные списки, раздел «Обнаруженные отклонения» в отчёте ЦК, `SS13_STATION_TRAITS` (принудительные, как файл будущих трейтов), `SS13_FORBID_STATION_TRAITS`; в пуле только портированные трейты; кнопки записи в лобби — у трейтов-профессий

### Положительные трейты
26 трейтов: галактический грант, премиальные коробки, щедрые награды, сильные линии снабжения, заполненные техтоннели, быстрый шаттл, отделы «с предсмертным хрипом» (death rattle-импланты), кибернетическая революция, роскошные капсулы, продвинутые медботы, арендованный шаттл, мудрые коровы, яркий день, распродажа шаттлов, снаряжённые ассистенты и др.

- **Код:** [`positive_traits.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/station_traits/positive_traits.dm#L1)
- **BandaStation:** [`positive_traits.dm#L39`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/station_traits/positive_traits.dm#L39), [`positive_traits.dm#L299`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/station_traits/positive_traits.dm#L299)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/station_traits.rs`: галактический грант, сильные линии снабжения, заполненные техтоннели (`ss13_mapload::spawner::MaintLootScale`), быстрый шаттл; нет остальных (их системы не портированы)

### Нейтральные трейты
19 трейтов: поставка бананиума, необычная атмосфера, нашествие пауков, уникальный ИИ, сбой КПК, объявления интерна, цветные ассистенты, дни рождения, кошельки, лес, аномалия в шкафу, Скуб, проверка биографий, питомцы на работе и др.

- **Код:** [`neutral_traits.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/station_traits/neutral_traits.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/station_traits.rs`: объявления интерна и медбота (`Announcer`); нет остальных

### Отрицательные трейты
27 трейтов: нашествие карпов, удалённые линии снабжения, забастовка почты, позднее прибытие, высадка на ходу, похмелье, блэкаут, вычищенные техтоннели, медленный шаттл, сбои языковых матриц, ионная/радиационная/пыльная буря, тесные капсулы, постреволюционный пыл, туманности, вечный шторм, добавки в напитках, структурные слабости, дефицит товаров.

- **Код:** [`negative_traits.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/station_traits/negative_traits.dm#L1)
- **BandaStation:** [`negative_traits.dm#L109`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/station_traits/negative_traits.dm#L109) — блэкаут затрагивает меньше APC (30% вместо 60%)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/station_traits.rs`: нашествие карпов, удалённые линии снабжения, забастовка почты (`cargo/mail.rs`), позднее прибытие (`jobs.rs`), блэкаут (30% APC), вычищенные техтоннели, медленный шаттл, радиационная и пыльная бури, напитки с добавками, дефицит товаров; нет ионной бури, высадки на ходу, похмелья, языковых сбоев, тесных капсул, постреволюционного пыла, туманностей, шторма, структурных слабостей, мести Пун-Пуна Генераторы защиты от туманности (`/obj/machinery/nebula_shielding`, перенесены из `docs/parity/engineering/machines.md`) — вместе с трейтом туманностей: нет

### Трейты-профессии
Добавляют особые профессии: горилла карго, ассистент мостика, ветеран-советник, ИИ-человек, Пун-Пун в экипаже.

- **Код:** [`job_traits.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/station_traits/job_traits.dm#L1)
- **BandaStation:** [`job_traits.dm#L22`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/station_traits/job_traits.dm#L22) — кнопки записи перенесены в HTML-лобби
- **Статус в ss13-rs:** ❌ нет

### Админ-панель трейтов
TGUI-панель для форсирования/запрета трейтов на следующий раунд.

- **Код:** [`/datum/station_traits_panel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/station_traits/admin_panel.dm#L5); tgui: [`StationTraitsPanel.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/StationTraitsPanel.tsx)
- **Статус в ss13-rs:** ❌ нет — трейты форсируются только `SS13_STATION_TRAITS` (без админов)

### Цели станции
ЦК выдаёт цель на раунд (в отчёте конца раунда — выполнена ли): ДНК-хранилище (сбор ДНК видов и растений, мутации экипажу), метеорный щит (спутники), блюспейс-артиллерия.

- **Код:** [`/datum/station_goal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/station_goals/station_goal.dm#L1), [`/datum/station_goal/dna_vault`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/station_goals/dna_vault.dm#L14), [`/datum/station_goal/station_shield`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/station_goals/meteor_shield.dm#L15), [`/datum/station_goal/bluespace_cannon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/station_goals/bsa.dm#L16), [`generate_goals.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/station_goals/generate_goals.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/station_goals.rs`: генерация по бюджету с отчётом ЦК, раздел «Цели станции» в итоговом отчёте, метеорный щит (спутники из карго, включение в космосе за 2 с, сбивают метеоры в 14 тайлах по линии через космос, цель — 500 тайлов покрытия); нет ДНК-хранилища, блюспейс-артиллерии, консоли спутников, взлома спутников емагом, админ-панели целей

# Настроение и рассудок
Система `/datum/mood`: сумма муд-событий задаёт настроение, настроение тянет за собой рассудок (sanity), а рассудок влияет на голод, скорость и порог крита.

### Настроение и муд-события
У каждого моба с разумом есть `/datum/mood`: муд-события (`add_mood_event` по категории, с таймаутом) складываются в значение настроения 1–9, которое показывается иконкой-смайликом на HUD.

- **Код:** [`/datum/mood`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L11), [`proc/add_mood_event()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L155), [`proc/update_mood()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L266), [`/datum/mood_event`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/_mood_event.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/mood.rs`: `Mood` у каждого человека, события по категориям с таймаутом, обновление/замена в категории, модификаторы силы и длительности, уровни 1–9; иконка на HUD (`MoodHudUi`, `crates/ss13_game/src/mood_client.rs`). Особые иконки событий (`special_screen_obj`) придут с их событиями, множители личностей — с личностями (personality.md)

### Рассудок (sanity) и его последствия
Каждые 2 с рассудок дрейфует к уровню настроения; низкий рассудок ускоряет голод, замедляет движение и действия, снижает порог крита; «безумие» добавляет галлюцинации.

- **Код:** [`proc/process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L87), [`proc/set_sanity()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L578), [`proc/set_movespeed_effect()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L645), [`proc/set_actionspeed_effect()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L656), [`proc/set_crit_threshold()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L633)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mood.rs`: дрейф рассудка раз в секунду по уровню настроения, уровни рассудка, замедление движения (`SanitySlowdown` в `crate::health`), скорость действий (`do_after` в `crate::tools`), скорость голода (`crate::hunger`), цвет иконки, Unstable; нет сдвига порога крита (пороги в `crate::health` — константы); галлюцинации при безумии — `crate::hallucination::SanityHallucinations` (hallucinations.md)

### Окно настроения (клик по иконке)
Клик по смайлику печатает текущее настроение, рассудок, голод и список активных муд-событий.

- **Код:** [`proc/print_mood()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L394), [`proc/hud_click()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L384)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/mood.rs` (`print_mood`, акт `click` окна `MoodHudUi`): голод, опьянение, рассудок, настроение, список событий и квирков; строка личностей — с личностями (personality.md)

### Красота помещений и муд зон
Красота зоны (грязь, картины, растения) и муд-бонус зоны (бар, часовня) влияют на настроение при входе.

- **Код:** [`proc/check_area_mood()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L507), [`proc/update_beauty()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L519), [`/datum/mood_event/area`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/area_events.dm#L11), [`beauty_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/beauty_events.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Муд-события потребностей
Голод/сытость, ожирение, гигиена, отсутствие одежды и т.п. (`needs_events.dm`); муд от питания пересчитывается из уровня сытости.

- **Код:** [`proc/update_nutrition_moodlets()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L117), [`/datum/mood_event/hungry`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/needs_events.dm#L18)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mood.rs`: голод/сытость/переедание и ожирение (с Voracious); нет гигиены и душа, свежего белья, заряда (этериалы), отвращения и окружения синтетиками

### Муд-события еды, напитков и наркотиков
Любимая/отвратительная еда, качество блюд, опьянение, кайф и ломка от наркотиков.

- **Код:** [`food_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/food_events.dm#L1), [`drink_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/drink_events.dm#L1), [`drug_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/drug_events.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mood.rs`: опьянение (`drunk`, сила по степени опьянения); нет любимой/мерзкой еды, качества блюд, наркотиков и ломки

### Муд-события смерти и морбидные
Увидеть смерть (с учётом питомцев, гиба, пыли), мрачные события для морбидных персонажей (расчленение, вскрытие).

- **Код:** [`/datum/mood_event/conditional/see_death`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/death.dm#L14), [`morbid_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/morbid_events.dm#L10)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mood.rs`: базовое «видел смерть» (зрители в поле зрения, не слепые и в сознании, усиление при повторе, должность умершего), кроме ложной смерти; нет условных вариантов (культ, революция, питомцы, равнодушные), гиба/пыли и морбидных событий

### Общие положительные и отрицательные муд-события
Около 250 событий: объятия, подарки, музыка, наручники, удары, мокрая одежда, жалобы на хвост и т.д.

- **Код:** [`generic_positive_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/generic_positive_events.dm#L1), [`generic_negative_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/generic_negative_events.dm#L1)
- **BandaStation:** [`generic_positive_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/generic_positive_events.dm#L80) — дёрнутый хвост даёт −1 вместо +1; кокаин, обезболивающие, напитки и кальяны форка добавляют свои события (`modular_bandastation/objects`, `bar_hookahs`)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mood.rs`: объятия (тёплые, Friendly), наручники, горение, удушье, жара/холод, депрессия, Jolly, больная спина; остальные события ждут своих механик

### Особые муд-события
Потеря топора (пожарный топор), еретические картины, ДНК-инфузер («это было в мыши»).

- **Код:** [`axe_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/axe_events.dm#L2), [`eldritch_painting_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/eldritch_painting_events.dm#L5), [`dna_infuser_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/dna_infuser_events.dm#L1)
- **Статус в ss13-rs:** ❌ нет

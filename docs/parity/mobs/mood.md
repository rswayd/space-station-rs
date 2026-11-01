# Настроение и рассудок
Система `/datum/mood`: сумма муд-событий задаёт настроение, настроение тянет за собой рассудок (sanity), а рассудок влияет на голод, скорость и порог крита.

### Настроение и муд-события
У каждого моба с разумом есть `/datum/mood`: муд-события (`add_mood_event` по категории, с таймаутом) складываются в значение настроения 1–9, которое показывается иконкой-смайликом на HUD.

- **Код:** [`/datum/mood`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L11), [`proc/add_mood_event()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L155), [`proc/update_mood()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L266), [`/datum/mood_event`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/_mood_event.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Рассудок (sanity) и его последствия
Каждые 2 с рассудок дрейфует к уровню настроения; низкий рассудок ускоряет голод, замедляет движение и действия, снижает порог крита; «безумие» добавляет галлюцинации.

- **Код:** [`proc/process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L87), [`proc/set_sanity()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L578), [`proc/set_movespeed_effect()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L645), [`proc/set_actionspeed_effect()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L656), [`proc/set_crit_threshold()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L633)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/hunger.rs` фиксирует рассудок на нейтральных 100 (скорость голода 0.04/с)

### Окно настроения (клик по иконке)
Клик по смайлику печатает текущее настроение, рассудок, голод и список активных муд-событий.

- **Код:** [`proc/print_mood()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L394), [`proc/hud_click()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L384)
- **Статус в ss13-rs:** ❌ нет

### Красота помещений и муд зон
Красота зоны (грязь, картины, растения) и муд-бонус зоны (бар, часовня) влияют на настроение при входе.

- **Код:** [`proc/check_area_mood()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L507), [`proc/update_beauty()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L519), [`/datum/mood_event/area`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/area_events.dm#L11), [`beauty_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/beauty_events.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Муд-события потребностей
Голод/сытость, ожирение, гигиена, отсутствие одежды и т.п. (`needs_events.dm`); муд от питания пересчитывается из уровня сытости.

- **Код:** [`proc/update_nutrition_moodlets()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood.dm#L117), [`/datum/mood_event/hungry`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/needs_events.dm#L18)
- **Статус в ss13-rs:** ❌ нет

### Муд-события еды, напитков и наркотиков
Любимая/отвратительная еда, качество блюд, опьянение, кайф и ломка от наркотиков.

- **Код:** [`food_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/food_events.dm#L1), [`drink_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/drink_events.dm#L1), [`drug_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/drug_events.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Муд-события смерти и морбидные
Увидеть смерть (с учётом питомцев, гиба, пыли), мрачные события для морбидных персонажей (расчленение, вскрытие).

- **Код:** [`/datum/mood_event/conditional/see_death`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/death.dm#L14), [`morbid_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/morbid_events.dm#L10)
- **Статус в ss13-rs:** ❌ нет

### Общие положительные и отрицательные муд-события
Около 250 событий: объятия, подарки, музыка, наручники, удары, мокрая одежда, жалобы на хвост и т.д.

- **Код:** [`generic_positive_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/generic_positive_events.dm#L1), [`generic_negative_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/generic_negative_events.dm#L1)
- **BandaStation:** [`generic_positive_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/generic_positive_events.dm#L80) — дёрнутый хвост даёт −1 вместо +1; кокаин, обезболивающие, напитки и кальяны форка добавляют свои события (`modular_bandastation/objects`, `bar_hookahs`)
- **Статус в ss13-rs:** ❌ нет

### Особые муд-события
Потеря топора (пожарный топор), еретические картины, ДНК-инфузер («это было в мыши»).

- **Код:** [`axe_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/axe_events.dm#L2), [`eldritch_painting_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/eldritch_painting_events.dm#L5), [`dna_infuser_events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mood_events/dna_infuser_events.dm#L1)
- **Статус в ss13-rs:** ❌ нет

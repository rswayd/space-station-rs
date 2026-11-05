# События аномалий

События, которые ставят аномалию в случайной подходящей зоне станции. Сами аномалии (`/obj/effect/anomaly`, ядра, исследование) — область науки, см. ../science/; здесь только событийная часть: объявление, выбор места (`anomaly_placer`), таймер до взрыва.

### Каркас событий аномалий
Базовое событие выбирает безопасную зону через `/datum/anomaly_placer`, объявляет тип аномалии и место, спавнит её; если аномалию не нейтрализовать сигналером за время жизни, она срабатывает.

- **Код:** [`/datum/round_event_control/anomaly`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/anomaly/_anomaly.dm#L1), [`/datum/anomaly_placer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/anomaly/anomaly_placer.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/anomalies.rs`: выбор зоны станции вне «безопасных» (`anomaly_placer`, кроме комнаты отдыха инженеров), объявление с именем зоны, аномалия на свободном не-космическом тайле, процесс раз в 2 с (дрейф 45%/с), детонация через 120 с; нейтрализация сигналером, ядра и анализатор — наука (../science/anomalies.md); нет админского выбора места, оповещения призраков и отсчёта

### Anomaly: Hyper-Energetic Flux
Энергетическая аномалия: бьёт током подошедших, в конце взрывается.

- **Код:** [`/datum/round_event_control/anomaly/anomaly_flux`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/anomaly/anomaly_flux.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/anomalies.rs`: событие (min_players 10, вес 20, до 5 раз), объявление через 15 тиков; сама аномалия — ../science/anomalies.md

### Anomaly: Bluespace
Телепортирует всё вокруг, в конце — массовая телепортация зоны.

- **Код:** [`/datum/round_event_control/anomaly/anomaly_bluespace`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/anomaly/anomaly_bluespace.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/anomalies.rs`: событие (вес 15, 1 раз), объявление сразу, аномалия на 3-м тике; сама аномалия — ../science/anomalies.md

### Anomaly: Gravitational (обычная и High Intensity)
Притягивает и швыряет предметы и мобов; высокоинтенсивный вариант создаёт вокруг поле изменённой гравитации радиусом 7.

- **Код:** [`/datum/round_event_control/anomaly/anomaly_grav`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/anomaly/anomaly_grav.dm#L1), [`/datum/round_event_control/anomaly/anomaly_grav/high`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/anomaly/anomaly_grav.dm#L16)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/anomalies.rs`: обычная (вес 25, до 5 раз, звук `ANNOUNCER_GRANOMALIES`); нет High Intensity (нет поля гравитации и генератора гравитации); сама аномалия — ../science/anomalies.md

### Anomaly: Pyroclastic
Выпускает горячую плазму вокруг, в конце порождает огненного слайма.

- **Код:** [`/datum/round_event_control/anomaly/anomaly_pyro`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/anomaly/anomaly_pyro.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/anomalies.rs`: событие (вес 20, до 5 раз); сама аномалия — ../science/anomalies.md

### Anomaly: Vortex
Блуждающий вихрь, разрушающий объекты на пути, в конце взрывается.

- **Код:** [`/datum/round_event_control/anomaly/anomaly_vortex`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/anomaly/anomaly_vortex.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет аномалий

### Anomaly: Bioscrambler
Подменяет органы и конечности ближайших существ чужими.

- **Код:** [`/datum/round_event_control/anomaly/anomaly_bioscrambler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/anomaly/anomaly_bioscrambler.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет аномалий

### Anomaly: Dimensional
Перестраивает окружающие турфы и объекты в случайную «тему» (материал стен, пола, окон).

- **Код:** [`/datum/round_event_control/anomaly/anomaly_dimensional`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/anomaly/anomaly_dimensional.dm#L1)
- **BandaStation:** [`anomalies_dimensional_themes.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_dimensional_themes.dm#L196) — темы перекрашивают и обводку окон (`edge_overlay_color`)
- **Статус в ss13-rs:** ❌ нет — нет аномалий

### Anomaly: Ectoplasmic Outburst
Реагирует на число призраков рядом: от сглаза предметов до призыва призрачной орды.

- **Код:** [`/datum/round_event_control/anomaly/anomaly_ectoplasm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/anomaly/anomaly_ectoplasm.dm#L5)
- **Статус в ss13-rs:** ❌ нет — нет аномалий

### Anomaly: Hallucination
Вызывает галлюцинации у всех вокруг, в конце — сильная волна галлюцинаций.

- **Код:** [`/datum/round_event_control/anomaly/anomaly_hallucination`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/anomaly/anomaly_hallucination.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет аномалии галлюцинаций (импульс есть: `crate::hallucination::hallucination_pulse`)

### Anomaly: Weather / Thundering Weather
Погодная аномалия: локальный дождь (или гроза с молниями) в зоне вокруг себя.

- **Код:** [`/datum/round_event_control/anomaly/anomaly_weather`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/anomaly/anomaly_weather.dm#L1), [`/datum/round_event_control/anomaly/anomaly_weather/thundering`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/anomaly/anomaly_weather.dm#L38)
- **Статус в ss13-rs:** ❌ нет — нет аномалий и погоды

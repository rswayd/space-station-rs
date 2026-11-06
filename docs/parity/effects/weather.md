# Погода

Подсистема погоды и все типы погоды: штормы на планетах (Лаваленд, Ледяная Луна), станционные погодные явления и связанные приборы.

### Подсистема погоды SSweather
Для каждого z-уровня с погодным трейтом (`ZTRAIT_ASHSTORM`, `ZTRAIT_SNOWSTORM` и т.д.) по таймеру выбирает погоду по `probability` и запускает её; админы и события запускают погоду вручную (`run_weather`).

- **Код:** [`SUBSYSTEM_DEF(weather)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/weather.dm#L2), [`proc/run_weather()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/weather.dm#L132)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/weather.rs`: `run_weather` (по типу или имени, уровни по умолчанию — станционные), обработка запущенных погод, раз в секунду `weather_act_mob`; планировщик: уровень, получивший погоды (`make_eligible`, tg — уровни с погодным трейтом), получает погоду по весу `probability` и снова доступен через 5–10 минут после верхней длительности; у станционных погод `probability` 0, как в tg; уровень шахты (`ZTRAIT_ASHSTORM`, `ss13_mapload::mining_z`) получает пепельную бурю (90) и emberfall (10); снежных и песчаных уровней нет

### Жизненный цикл погоды
`/datum/weather`: предупреждение (`telegraph`, сообщение и звук), начало (`start`, оверлеи на зоны), затухание (`wind_down`), конец (`end`); каждый тик действует на мобов и турфы в затронутых зонах, кроме защищённых (`protected_areas`, укрытия, иммунитеты). Поддерживаются грозы (`thunder_act_turf`) и частицы.

- **Код:** [`/datum/weather`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather.dm#L15), [`proc/telegraph()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather.dm#L186), [`proc/weather_act_mob()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather.dm#L465), [`proc/thunder_act_turf()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather.dm#L516), [`/datum/weather/particle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/particle_weather.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/weather.rs`: стадии `telegraph`/`start`/`wind_down`/`end` с длительностями tg (`rand(lower, upper)`, бесконечные — до `end_weather`), сообщения и звуки стадий всем игрокам затронутых уровней (`send_alert`), выбор зон (`area_type`, `protected_areas`, `outdoors` без `WEATHER_INDOORS`), `can_weather_act_mob` (уровень, зона, защищающие контейнеры — герметичные мешки для тел, иммунитеты типов); `// shortcut:` нет оверлеев зон (нужен контракт клиента: значок стадии поверх всех тайлов зоны), частиц, действий по турфам и грозы (`thunder_act_turf`, только у дождей), подтягивания температуры тела к 20 °C и `area_sound_manager`

### Пепельная буря и Emberfall (Лаваленд)
Пепельная буря обжигает всех снаружи без защиты (шахтёрские костюмы, укрытия); Emberfall — безвредный пепел. Звук бури слышен в укрытиях.

- **Код:** [`/datum/weather/particle/ash_storm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather_types/ash_storm.dm#L2), [`/datum/weather/particle/ash_storm/emberfall`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather_types/ash_storm.dm#L128)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/weather.rs` (`WeatherKind::AshStorm`, `Emberfall`): по умолчанию на уровне шахты, планировщик 90/10; 30 с предупреждения, 1–2 минуты, 30 с затухания, сообщения tg; только открытые (`outdoors`) зоны — база, капсулы-укрытия и руины защищают; раз в секунду 4 ожога всем, кроме иммунных к пепельной буре (силиконы, шахтёрская фауна и мегафауна, рапторы, пилоты мехов) и людей с термозащитой костюма и шлема ≥ `FIRE_IMMUNITY_MAX_TEMP_PROTECT`; emberfall безвреден. Оверлеи, частицы, гроза, звуки бури в зонах (`ash_storm_sounds`) — в «Жизненном цикле погоды» и записи о мониторах погоды; засыпание раскопанного базальта (`refill_dug`) — у группы шахты; `required_bodytype` урона не учитывается

### Снежная буря (Ледяная Луна)
Метель охлаждает тела снаружи; вариант `forever_storm` не прекращается.

- **Код:** [`/datum/weather/snow_storm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather_types/snow_storm.dm#L1), [`/datum/weather/snow_storm/forever_storm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather_types/snow_storm.dm#L47)
- **Статус в ss13-rs:** ❌ нет — каркас погоды есть (`crates/ss13_game/src/weather.rs`), но нет Ледяной Луны и уровней с `ZTRAIT_SNOWSTORM`

### Песчаная буря
Песчаный шторм на песчаных планетах ранит незащищённых; безвредный вариант только засыпает местность.

- **Код:** [`/datum/weather/sand_storm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather_types/sand_storm.dm#L2), [`/datum/weather/sand_storm/harmless`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather_types/sand_storm.dm#L52)
- **Статус в ss13-rs:** ❌ нет — каркас погоды есть (`crates/ss13_game/src/weather.rs`), но нет песчаных планет

### Дождь и его варианты
Ливень с частицами мочит всех снаружи; варианты: кровавый, плазменный, «пережаренный», кислотный и волшебный (событие мага). Может сопровождаться грозой с молниями.

- **Код:** [`/datum/weather/particle/rain_storm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather_types/rain_storm.dm#L1), [`/datum/weather/particle/rain_storm/blood`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather_types/rain_storm.dm#L92), [`/datum/weather/particle/rain_storm/plasma`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather_types/rain_storm.dm#L98), [`/datum/weather/particle/rain_storm/acid`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather_types/rain_storm.dm#L108), [`/datum/weather/particle/rain_storm/wizard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather_types/rain_storm.dm#L130)
- **Статус в ss13-rs:** ❌ нет — каркас погоды есть (`crates/ss13_game/src/weather.rs`), но нет уровней с дождём, погодных реагентов и грозы; волшебный дождь — событие мага

### Радиационный шторм
Станционная погода: тревога, все прячутся в техтоннелях; в открытых зонах облучает (мутации, токсины). Вариант `nebula` для туманностей станционного трейта.

- **Код:** [`/datum/weather/rad_storm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather_types/radiation_storm.dm#L2), [`/datum/weather/rad_storm/nebula`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather_types/radiation_storm.dm#L103)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/weather.rs` (`WeatherKind::RadStorm`, `RadStormNebula`), событие — `crates/ss13_game/src/events/radiation_storm.rs`: стадии и сообщения tg, звук тревоги, зоны `/area` кроме укрытий, раз в секунду бросок мутации 40% (иммунитет вида, радзащитная одежда), объявление об окончании; `nebula` бесконечна, свои укрытия, мутация 0.1% и 5% `radiation_pulse`; оверлей зон — в «Жизненном цикле погоды», тревога на статус-дисплеях — у группы статус-дисплеев (нет экстренного режима), `random_mutate_unique_features` — у генетики

### Пустотная буря (void storm)
Буря еретика Пустоты (вознесение по пути Void): чёрная метель по всей станции, бесконечная, пока жив еретик; сама по себе почти только визуальная, силу даёт еретику.

- **Код:** [`/datum/weather/void_storm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather_types/void_storm.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/weather.rs` (`WeatherKind::VoidStorm`): 2 с предупреждения, сообщение, бесконечна до `end_weather`, все зоны, без действия на мобов; запуск при вознесении еретика Пустоты и усиление еретика — у группы еретика, оверлей — в «Жизненном цикле погоды»

### Пол — лава (floor_is_lava)
Пол станции на время становится «холодной лавой», слегка обжигающей всех, кто стоит на полу, а не на мебели (событие мага).

- **Код:** [`/datum/weather/floor_is_lava`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather_types/floor_is_lava.dm#L2)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/weather.rs` (`WeatherKind::FloorIsLava`): 15 с предупреждения, 30–60 с, без затухания, все зоны кроме космоса; раз в секунду 3 ожога игроку на полу — не силикону, не в кровати, не на плотной структуре (столы), не летящему, без `TRAIT_LAVA_IMMUNE`; вызов событием мага — у группы мага

### Предупреждение о погоде: метеорадио и мониторы
Компонент `weather_announcer` показывает время до бури на шахтёрской рации и настенных погодных мониторах; элемент `weather_listener` проигрывает звук бури тем, кто в укрытии.

- **Код:** [`/datum/component/weather_announcer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/weatherannouncer.dm#L9), [`/datum/element/weather_listener`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/weather_listener.dm#L2), [`/obj/item/radio/weather_monitor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mining/equipment/miningradio.dm#L2)
- **Статус в ss13-rs:** ❌ нет — каркас погоды есть (`crates/ss13_game/src/weather.rs`), но нет шахтёрских раций, мониторов и планетарных бурь

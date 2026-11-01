# Инженерные и станционные события

Случайные события, бьющие по оборудованию станции: питание, связь, ИИ, вентиляция, гравитация, двигатель.

### Grid Check
Отключает все APC станции на время (объявление «перегрузка энергосети»); свет гаснет, оборудование обесточено, пока питание не вернётся само.

- **Код:** [`/datum/round_event_control/grid_check`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/grid_check.dm#L1), [`/datum/round_event/grid_check`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/grid_check.dm#L14)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/grid_check.rs`: объявление (кулдаун 30 с), `power_fail(60, 240)`: `failure_timer` APC с ячейкой на уровнях станции, питание возвращается само (`crates/ss13_game/src/power/model.rs` `late_process`); нет элемента `block_area_power_fail`

### Electrical Storm
В случайной точке станции лопаются лампы в радиусе (`/obj/machinery/light/proc/flicker`/`break_light_tube`); предупреждение экипажу.

- **Код:** [`/datum/round_event_control/electrical_storm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/electrical_storm.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/electrical_storm.rs`: эпицентр — случайный открытый турф станции, APC в радиусе 25 платят 20 кДж и бьют все лампы зоны (`power::break_light_tube`: лампа гаснет навсегда, иконка `-broken`); нет искр, звука, осколков и замены ламп

### Communications Blackout
ЭМИ по всем машинам телекоммуникаций: радио глохнет до перезапуска, ИИ получает искажённое сообщение.

- **Код:** [`/datum/round_event_control/communications_blackout`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/communications_blackout.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/comms_blackout.rs` + `crates/ss13_game/src/radio.rs` `TelecommsOutage`: на 298–302 с гарнитуры не передают и не принимают, интеркомы и прочие не-subspace радио вещают только друг другу на уровне (tg `backup_transmission`), объявление в 30% случаев с искажённым текстом; телекомы как машины не моделируются, резервная передача без задержки 2 с, нет ИИ и трамвая

### Processor Overload
ЭМИ процессоров телекомов: речь по радио искажается, часть процессоров может взорваться.

- **Код:** [`/datum/round_event_control/processor_overload`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/processor_overload.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет телекомов

### Camera Failure
Отключает одну или несколько случайных камер наблюдения (частое событие, вес 100).

- **Код:** [`/datum/round_event_control/camera_failure`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/camerafailure.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет камер

### Grey Tide
В нескольких отделах сбой открывает все шлюзы, болтирует их и выключает свет; двери тюрьмы и шкафчики брига отпираются.

- **Код:** [`/datum/round_event_control/grey_tide`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/grey_tide.dm#L1)
- **Статус в ss13-rs:** ❌ нет — шлюзы и болты есть (`crates/ss13_game/src/doors.rs`), события нет

### Gravity Generator Blackout
Генератор гравитации отключается: на станции пропадает гравитация, пока инженеры его не перезапустят.

- **Код:** [`/datum/round_event_control/gravity_generator_blackout`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/gravity_generator_blackout.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет генератора гравитации и невесомости

### Radiation Leak
Машина в случайной зоне начинает излучать радиацию и газ (часто плазму/трития), пока её не починят инструментами.

- **Код:** [`/datum/round_event_control/radiation_leak`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/radiation_leak.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет радиации

### Supermatter Surge
Суперматерия на время получает прибавку к мощности и нагреву; объявление в инженерию. Вариант «Poly's Revenge» — при жертве попугая Поли.

- **Код:** [`/datum/round_event_control/supermatter_surge`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/supermatter_surge.dm#L26), [`/datum/round_event_control/supermatter_surge/poly`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/supermatter_surge.dm#L112)
- **Статус в ss13-rs:** ❌ нет — событий нет (сама суперматерия — `crates/ss13_game/src/power/supermatter.rs`)

### Tram Malfunction
Трамвай (на картах с трамваем) идёт с ошибками: пропускает станции, сигналы переходов сбоят, шанс аварий растёт.

- **Код:** [`/datum/round_event_control/tram_malfunction`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/tram_malfunction.dm#L4)
- **Статус в ss13-rs:** ❌ нет — нет трамвая

### Chasmic Earthquake
По станции проходит разлом: после предупреждения с тряской участок турфов проваливается в пропасть, разрушая всё на линии.

- **Код:** [`/datum/round_event_control/earthquake`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/earthquake.dm#L5)
- **Статус в ss13-rs:** ❌ нет — нет пропастей и события

### Brand Intelligence
Один торговый автомат становится «разумным» и заражает остальные автоматы своего отдела: они кричат рекламу, стреляют товаром и в конце могут ожить; остановка — уничтожить первоисточник.

- **Код:** [`/datum/round_event_control/brand_intelligence`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/brand_intelligence.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/brand_intelligence.rs`: источник среди автоматов станции стреляет товаром (`VendorHack.shoot_inventory`, `crates/ss13_game/src/wires.rs`), каждые 4 с заражает следующий, речь автомата, конец при выключенном динамике или всех перерезанных проводах источника; нет отказа бедным покупателям и оживающих автоматов (ИИ мобов)

### Ion Storm
Ионный шторм добавляет ИИ и киборгам случайный закон (или меняет/удаляет законы), шанс ботам сойти с ума, сообщение «ионный шторм».

- **Код:** [`/datum/round_event_control/ion_storm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/ion_storm.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет ИИ и законов

### Radiation Storm
Погода `rad_storm` на станции: тревога, экипаж прячется в техтоннелях, открытые зоны облучаются (мутации, ожоги).

- **Код:** [`/datum/round_event_control/radiation_storm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/radiation_storm.dm#L1), [`/datum/weather/rad_storm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather_types/radiation_storm.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/radiation_storm.rs`: объявление, погода `rad_storm` (40 с предупреждения, 1–2,5 мин шторма, 10 с затихания, отбой), незащищённые зоны (кроме техтоннелей и убежищ из `protected_areas`) на уровнях станции; `// shortcut:` без генетики бросок мутации (40%/с) даёт 1 токсин и половина из них 1 ожог; нет оверлея зоны, статус-дисплеев и звуков

### Ventilation Clog (Minor / Major / Critical / Strange)
Вентиляция засоряется: из вентов вылезают мобы (тараканы, мыши, слаймы; на «Strange» — редкие твари) или идёт грязь, пока вент не прочистят вантузом. Тяжесть определяет типы и число мобов.

- **Код:** [`/datum/round_event_control/vent_clog`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/vent_clog.dm#L3), [`/datum/round_event_control/vent_clog/major`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/vent_clog.dm#L212), [`/datum/round_event_control/vent_clog/critical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/vent_clog.dm#L246), [`/datum/round_event_control/vent_clog/strange`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/vent_clog.dm#L278)
- **Статус в ss13-rs:** ❌ нет — нет вентов и мобов

### Scrubber Overflow (Normal / Threatening / Catastrophic / Every Vent)
Скрубберы выбрасывают пену со случайными реагентами (на тяжёлых уровнях — опасными); «Every Vent» — только для админов, все скрубберы сразу.

- **Код:** [`/datum/round_event_control/scrubber_overflow`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/scrubber_overflow.dm#L1), [`/datum/round_event_control/scrubber_overflow/threatening`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/scrubber_overflow.dm#L125), [`/datum/round_event_control/scrubber_overflow/catastrophic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/scrubber_overflow.dm#L140), [`/datum/round_event_control/scrubber_overflow/every_vent`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/scrubber_overflow.dm#L155)
- **BandaStation:** [`code/modules/events/scrubber_overflow.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/scrubber_overflow.dm#L54) — убран случайный блеск (`glitter/random`) из списка реагентов
- **Статус в ss13-rs:** ❌ нет — нет скрубберов и пены

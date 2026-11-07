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
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/comms_blackout.rs`: `EMP_HEAVY` каждой машине телекомов (`crate::emp::machines::telecomms_emp` → `crate::telecomms::emp_machine`: 50% на машину, 148–152 с), сигнал не проходит цепочку — гарнитуры молчат, интеркомы и прочие не-subspace радио вещают друг другу (tg `backup_transmission`); на карте без телекомов — заглушка `TelecommsOutage`; объявление в 30% случаев с искажённым текстом; резервная передача без задержки 2 с, нет ИИ и трамвая

### Processor Overload
ЭМИ процессоров телекомов: речь по радио искажается, часть процессоров может взорваться.

- **Код:** [`/datum/round_event_control/processor_overload`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/processor_overload.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/processor_overload.rs`: вес 15, от 20 игроков; объявление в 80% случаев (искажённый текст), каждый процессор `EMP_HEAVY` (`crate::telecomms::emp_machine`), с шансом 10% — лёгкий взрыв радиуса 2 и разрушительный удар по самому процессору; выключенный процессор оставляет речь сжатой — `Gibberish` (`crate::telecomms::broadcast_text`); нет оповещения ИИ и призраков (нет ИИ-списка и орбит-алертов)

### Camera Failure
Отключает одну или несколько случайных камер наблюдения (частое событие, вес 100).

- **Код:** [`/datum/round_event_control/camera_failure`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/camerafailure.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/camera_failure.rs`: камеры сети `ss13` гаснут с вероятностью `round(100 / iterations)`, `iterations ×= 2,5` (вес 100, до 20 раз, не фейкается); камера выключается через общий `crate::emp::machines::set_camera`; включить её обратно руками пока нельзя (камеры не переключаются вручную — силиконы/камеры)

### Grey Tide
В нескольких отделах сбой открывает все шлюзы, болтирует их и выключает свет; двери тюрьмы и шкафчики брига отпираются.

- **Код:** [`/datum/round_event_control/grey_tide`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/grey_tide.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/grey_tide.rs`: 1–3 отдела из шести, на 20–30 тике шлюзы в них отбалтываются, открываются (если есть питание) и болтируются, секьюрные шкафы отпираются, APC выключают освещение; объявление на 50–60 тике с уровнем угрозы; нет мерцания ламп (лампы не мерцают — питание), таймеров камер брига (нет), эмаг-проверки шлюза (нет эмага)

### Gravity Generator Blackout
Генератор гравитации отключается: на станции пропадает гравитация, пока инженеры его не перезапустят.

- **Код:** [`/datum/round_event_control/gravity_generator_blackout`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/gravity_generator_blackout.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет генератора гравитации (`docs/parity/power/generators.md`: машина, зарядка, окно `GravityGenerator` с рубильником) и гравитации по z-уровням (`crate::multiz::has_gravity` знает только космос); событие — один вызов `blackout()` на генераторах станции, без перезапуска оно лишило бы станцию гравитации навсегда

### Radiation Leak
Машина в случайной зоне начинает излучать радиацию и газ (часто плазму/трития), пока её не починят инструментами.

- **Код:** [`/datum/round_event_control/radiation_leak`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/radiation_leak.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/radiation_leak.rs`: вес 15, максимум 3; плотная машина станции с `use_power` (или вент/скруббер), видимая, не под полом и пропускающая воздух; `radioactive_emitter` (`crate::radiation::RadioactiveEmitter`: импульс раз в 2 с, радиус 5, порог `RAD_MEDIUM_INSULATION`, строка осмотра), дым полония и мутагена (радиус 2, по 10 u) со звуком три раза за 5 минут, объявление с зоной; 1–3 способа из восьми — `use_tool` 30 с (сварка тратит 2 топлива) нужным инструментом останавливает утечку и событие; машина берётся из всех подходящих на станции, а не у ландмарок `generic_event_spawns` (не загружаются), нет оповещения призраков

### Supermatter Surge
Суперматерия на время получает прибавку к мощности и нагреву; объявление в инженерию. Вариант «Poly's Revenge» — при жертве попугая Поли.

- **Код:** [`/datum/round_event_control/supermatter_surge`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/supermatter_surge.dm#L26), [`/datum/round_event_control/supermatter_surge/poly`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/supermatter_surge.dm#L112)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/supermatter_surge.rs`: вес 15, максимум 1, только при СИ или больше трёх инженеров/атмосферников в манифесте; главный кристалл, класс 1–4 (14/20/35/31%), 120–135 тиков событий; `bullet_energy` = класс + 5 (лучи эмиттера), азот кристаллов — `powerloss_inhibition`/`heat_power_generation`/`heat_modifier` tg (`Crystal::nitrogen`), в конце — исходные значения и объявление; нет админской настройки класса и «Poly's Revenge» (нет Поли)

### Tram Malfunction
Трамвай (на картах с трамваем) идёт с ошибками: пропускает станции, сигналы переходов сбоят, шанс аварий растёт.

- **Код:** [`/datum/round_event_control/tram_malfunction`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/tram_malfunction.dm#L4)
- **Статус в ss13-rs:** ❌ нет — нет трамвая (`SStransport`, `/obj/structure/transport/linear/tram`, контроллеры и сигналы переходов); ни одна карта порта его не загружает

### Chasmic Earthquake
По станции проходит разлом: после предупреждения с тряской участок турфов проваливается в пропасть, разрушая всё на линии.

- **Код:** [`/datum/round_event_control/earthquake`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/earthquake.dm#L5)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/earthquake.rs`: вес 7, от 20 игроков, с 35 минуты, до 3 раз, только на планетарной карте (`"planetary"` в json карты); эпицентр — случайный тайл станции ±10, разлом между двумя противоположными точками излома (прямоугольник, оставлены тайлы в пределах 2 от линии tg, кромка — ровно 3), объявление в 25%; каждые 10 тиков у игроков станции тряска камеры и далёкий гул, на 23 тике живые на разломе сбиты (7 с) и парализованы (5 с), на 24 — лёгкий взрыв 90% разлома, порода под ним выбурена, скрип металла; в конце разлом бомбится (опустошение, 10% — тяжёлый), кромка — лёгкий/25% тяжёлый через очередь взрывов (`crate::explosions::queue_turfs`) без самого взрыва; эпицентр не от ландмарок `generic_event_spawns` (не загружаются), нет анимации тряски турфов, оповещения призраков и ачивки; тряска в конце — игрокам станции (в tg из-за `||` почти никому)

### Brand Intelligence
Один торговый автомат становится «разумным» и заражает остальные автоматы своего отдела: они кричат рекламу, стреляют товаром и в конце могут ожить; остановка — уничтожить первоисточник.

- **Код:** [`/datum/round_event_control/brand_intelligence`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/brand_intelligence.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/brand_intelligence.rs`: источник среди автоматов станции стреляет товаром (`VendorHack.shoot_inventory`, `crates/ss13_game/src/wires.rs`), каждые 4 с заражает следующий, речь автомата, конец при выключенном динамике или всех перерезанных проводах источника; нет отказа бедным покупателям и оживающих автоматов (ИИ мобов)

### Ion Storm
Ионный шторм добавляет ИИ и киборгам случайный закон (или меняет/удаляет законы), шанс ботам сойти с ума, сообщение «ионный шторм».

- **Код:** [`/datum/round_event_control/ion_storm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/ion_storm.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/ion_storm.rs`: вес 15, от 2 игроков; объявление в 33% (`ANNOUNCER_IONSTORM`); BandaStation `scramble_ai_rack` на каждой стойке законов (`crate::silicon::law_rack`): ядровой модуль — новый набор по `LAW_WEIGHT` конфига (25%), удаление закона (10%), перемешивание (10%), ионный закон сверху, иначе замена (10%); модули законов — замена ионным (до двух на стойку) или стирание дополнительного; модули ионизируются (чинятся мультитулом), стойка обновляет законы ИИ/киборгов; `generate_ion_law` — 165 шаблонов с весами tg по `strings/ion_laws.json`; 1% ботам — `emag_act`; без лога законов, станционной черты Unique AI и двойного броска шанса объявления tg

### Radiation Storm
Погода `rad_storm` на станции: тревога, экипаж прячется в техтоннелях, открытые зоны облучаются (мутации, ожоги).

- **Код:** [`/datum/round_event_control/radiation_storm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/radiation_storm.dm#L1), [`/datum/weather/rad_storm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather_types/radiation_storm.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/radiation_storm.rs`: объявление, погода `rad_storm` (40 с предупреждения, 1–2,5 мин шторма, 10 с затихания, отбой), незащищённые зоны (кроме техтоннелей и убежищ из `protected_areas`) на уровнях станции; мутации незащищённых (иммунитет вида и радзащитная одежда, `crates/ss13_game/src/radiation.rs`); нет оверлея зоны, статус-дисплеев и звуков

### Ventilation Clog (Minor / Major / Critical / Strange)
Вентиляция засоряется: из вентов вылезают мобы (тараканы, мыши, слаймы; на «Strange» — редкие твари) или идёт грязь, пока вент не прочистят вантузом. Тяжесть определяет типы и число мобов.

- **Код:** [`/datum/round_event_control/vent_clog`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/vent_clog.dm#L3), [`/datum/round_event_control/vent_clog/major`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/vent_clog.dm#L212), [`/datum/round_event_control/vent_clog/critical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/vent_clog.dm#L246), [`/datum/round_event_control/vent_clog/strange`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/vent_clog.dm#L278)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/vent_clog.rs`: четыре события с весами/порогами tg, случайный незаваренный вент станции на свободном тайле (столы не мешают), грязь вокруг (85%, по радиусу 2, без `view`), моб из списка тяжести каждые `spawn_delay` тиков пока живых меньше `maximum_spawns` и вент не заварен, с грязью и звуком, переезд засора при уничтожении вента с объявлением, объявления (Strange — `ANNOUNCER_ALIENS`); нет прочистки вантузом (нет вантуза), осмотра вента; без вента событие гибнет в `setup`, а не исключается из ролла; мобы без прототипа/ИИ не появляются или стоят

### Scrubber Overflow (Normal / Threatening / Catastrophic / Every Vent)
Скрубберы выбрасывают пену со случайными реагентами (на тяжёлых уровнях — опасными); «Every Vent» — только для админов, все скрубберы сразу.

- **Код:** [`/datum/round_event_control/scrubber_overflow`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/scrubber_overflow.dm#L1), [`/datum/round_event_control/scrubber_overflow/threatening`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/scrubber_overflow.dm#L125), [`/datum/round_event_control/scrubber_overflow/catastrophic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/scrubber_overflow.dm#L140), [`/datum/round_event_control/scrubber_overflow/every_vent`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/scrubber_overflow.dm#L155)
- **BandaStation:** [`code/modules/events/scrubber_overflow.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/scrubber_overflow.dm#L54) — убран случайный блеск (`glitter/random`) из списка реагентов
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/scrubber_overflow.rs`: четыре события с весами/порогами tg (Every Vent — вес 0, только принудительно), только при незаваренном скруббере станции; объявление «Инженерный отдел», через 5 тиков каждый отобранный скруббер (50%, Every Vent — 100%) выпускает короткоживущую пену (`crate::effects::foam::do_foam`, `FoamKind::ShortLife`) на `reagents_amount` (50/100/150/100) одного реагента: безопасного из списка BandaStation (без блёсток) или, с `danger_chance` 1/10/30%, случайного (`get_random_reagent_id`) с тараканом и кровавым тараканом; нет админской настройки реагента

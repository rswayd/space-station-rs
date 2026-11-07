# Провода и взлом
Механизм `/datum/wires`: цветные провода в машинах, окно `Wires`, резка/пульс/сигналеры и все наборы проводов.

### Механизм проводов и окно Wires
У машины с открытой панелью есть набор проводов (цвета перемешиваются раз в раунд на набор, есть «пустышки»); кусачки режут/чинят, мультитул пульсирует; tgui-окно `Wires` открывается инструментом или рукой; провода доступны, пока панель открыта.

- **Код:** [`/datum/wires`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/_wires.dm#L32), [`proc/randomize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/_wires.dm#L126), [`proc/cut()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/_wires.dm#L179), [`proc/pulse()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/_wires.dm#L199), [`proc/interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/_wires.dm#L258); tgui: [`Wires.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Wires.tsx)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/wires.rs`, `crates/ss13_tgui/src/interfaces/wires.rs`: перемешивание по сиду карты, пустышки, резка/починка/пульс с проверками в момент действия, окно

### Сигналеры на проводах
К проводу крепится сборка (сигналер, таймер, датчик), которая пульсирует его по сигналу — удалённый взлом.

- **Код:** [`proc/attach_assembly()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/_wires.dm#L214), [`proc/pulse_assembly()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/_wires.dm#L208)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/wires.rs` (`attach_act`, `attach_assembly`, `detach_assembly`, `pulse_assembly`, `WireAssemblies`): кнопка «attach» в окне `Wires` вешает сборку из активной руки на провод (только сборки с `ASSEMBLY_INPUT`: сигналер, таймер, датчики, голосовой анализатор; иначе «You cannot attach this assembly to these wires!») или снимает висящую в руки; окно показывает подключённые; импульс сборки (`crate::assemblies::server::pulse`) пульсирует свой провод, даже перерезанный (`force`). Сами сборки — [../science/assemblies.md](../science/assemblies.md)

### Раскрытие проводов и ЭМИ
Чертежи, абдукторский мультитул, черта инженера (`TRAIT_KNOW_ENGI_WIRES`) и фото проводки показывают назначение проводов; дальтонизм путает цвета; ЭМИ пульсирует случайные провода.

- **Код:** [`proc/can_reveal_wires()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/_wires.dm#L276), [`proc/try_study_photo()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/_wires.dm#L312), [`proc/emp_pulse()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/_wires.dm#L230)
- **Статус в ss13-rs:** 🟡 частично — ЭМИ пульсирует до 3 случайных проводов по 33% (`crates/ss13_game/src/wires.rs` `emp_pulse`, если нет `EMP_PROTECT_WIRES`); нет раскрытия проводов и дальтонизма

### Провода шлюза
Основное/резервное питание, болты, ID-сканер, открытие, безопасность, тайминг, обратная связь (огни), высокое напряжение, связь с ИИ, зап-провода; набор по области шлюза; защищённый вариант.

- **Код:** [`/datum/wires/airlock`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/airlock.dm#L1), [`/datum/wires/airlock/secure`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/airlock.dm#L5)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/wires.rs`: все провода с эффектами tg, наборы по областям; провод ИИ лишь меняет состояние (нет ИИ), нет защитной обшивки

### Провода APC
Питание, отключение ИИ, разблокировка интерфейса, короткое замыкание (APC перестаёт работать 120 с).

- **Код:** [`/datum/wires/apc`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/apc.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/wires.rs`: переключение каналов, замыкание, разблокировка; крышка APC всегда закрыта (shortcut), нет ИИ

_«Провода торгового автомата» описано в [../cargo/vending.md](../cargo/vending.md)._

### Провода атмосферных машин и тревог
Воздушная тревога (сброс, блокировка, панель, режимы), пожарная тревога (тревога, сброс, отключение датчика), генератор атмосферного щита.

- **Код:** [`/datum/wires/airalarm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/airalarm.dm#L1), [`/datum/wires/firealarm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/firealarm.dm#L1), [`/datum/wires/atmosshieldgen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/atmosshieldgen.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/machine_wires.rs` (`WireHolder::Machine`): провода ставятся машине при появлении (карта или сборка), панель воздушной и пожарной тревоги — отвёрткой, с открытой панелью рука/кусачки/мультитул открывают окно `Wires` раньше собственного клика машины. Воздушная тревога: питание (пульс — замыкание на 2 мин, резка — навсегда и удар током 50%; замкнутая не оценивает воздух, её окно закрывается), ID (пульс переключает замок, резка запирает), ИИ (пульс — на 10 с), паника (пульс переключает фильтрацию/панический отсос, резка включает отсос), тревога (резка поднимает, пульс снимает), динамик (резка глушит предупреждения); статус-строки tg. Пожарная тревога: датчик (пульс переключает `fire_detect` зоны, резка блокирует переключение и выключает детекцию, когда перерезан у всех тревог зоны; починка возвращает), триггер/сброс (пульс — `alarm()`/`reset()`, резка — залипание); статус «thermal sensor». Атмосферный щит: пульс `Activate` переключает закреплённый генератор. В tg резка провода ИИ ставит `aidisabled = mend` (ИИ получает доступ обратно) — баг не перенесён, резка отключает ИИ. Доступ ИИ к тревогам — группа ИИ (../silicon/)

### Провода инженерных машин
Автолат (взлом — запрещённые чертежи, удар током, блокировка), эмиттер, катушка теслы, генератор щитовой стены, массдрайвер, конвейер, большой манипулятор.

- **Код:** [`/datum/wires/autolathe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/autolathe.dm#L1), [`/datum/wires/emitter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/emitter.dm#L2), [`/datum/wires/tesla_coil`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/tesla_coil.dm#L1), [`/datum/wires/shieldwallgen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/shieldwallgen.dm#L1), [`/datum/wires/mass_driver`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/mass_driver.dm#L1), [`/datum/wires/conveyor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/conveyor.dm#L1), [`/datum/wires/big_manipulator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/big_manipulator.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — автолат: `crates/ss13_game/src/wires.rs` (взлом, удар током, блокировка, пульс на 6 с); `crates/ss13_game/src/machine_wires.rs`: эмиттер (`Zap` — внеочередной выстрел при прошедшей задержке, приварке и избытке сети, `power/emitter.rs` `fire_beam_pulse`; `Hack` переключает `mode`, который в tg меняет снаряд только со встроенным оружием — его нет, провод инертен), генератор щитовой стены (один случайный провод на экземпляр; пульс включает/выключает закреплённый генератор в сети), массдрайвер (`Launch` — `drive()`, `Safeties` пульс — мощность 3, резка — обратно 1), катушка теслы (один случайный провод; `Zap` — пятая часть мощности сети × множитель, не больше избытка, разряд `tesla_zap` на 10 тайлов, откат 10 с; без звука `lightningshock.ogg`); панели эмиттера, массдрайвера и катушки — отвёрткой. Провода конвейера и большого манипулятора — вместе с машинами (→ [recycling.md](recycling.md), [machines.md](machines.md#большой-манипулятор))

### Провода прочих машин
Микроволновка, хранилище скафандров, сканер-ворота, голопад, факс, рулетка, R&D-машины, экто-нюхач.

- **Код:** [`/datum/wires/microwave`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/microwave.dm#L1), [`/datum/wires/suit_storage_unit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/suit_storage_unit.dm#L1), [`/datum/wires/scanner_gate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/scanner_gate.dm#L1), [`/datum/wires/holopad`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/holopad.dm#L1), [`/datum/wires/fax`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/fax.dm#L1), [`/datum/wires/roulette`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/roulette.dm#L1), [`/datum/wires/rnd`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/r_n_d.dm#L1), [`/datum/wires/ecto_sniffer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/ecto_sniffer.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Провода роботов, мехов, МОДов и мулботов
Борг (законы, камера, блокировка, ИИ-связь), мех, МОД-костюм, мулбот, радио (вкл/передача/приём).

- **Код:** [`/datum/wires/robot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/robot.dm#L1), [`/datum/wires/mecha`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/mecha.dm#L1), [`/datum/wires/mod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/mod.dm#L1), [`/datum/wires/mulebot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/mulebot.dm#L5), [`/datum/wires/radio`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/radio.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Провода взрывчатки
Бомба Синдиката (обезвредить, ускорить, взорвать), гранаты и самодельная взрывчатка, бомба-ошейник.

- **Код:** [`/datum/wires/syndicatebomb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/syndicatebomb.dm#L1), [`/datum/wires/explosive`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/explosive.dm#L1), [`/datum/wires/explosive/chem_grenade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/explosive.dm#L21), [`/datum/wires/collar_bomb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/collar_bomb.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `/datum/wires/explosive/chem_grenade` (`WireHolder::ChemGrenade`, `chem_grenade_set`: один провод-пустышка, окно открывается только у собранного корпуса в досягаемости; пульс/резка у запертой гранаты взводит её, у мины — подрывает; `attach_assembly` таймера ставит взрыватель на его время, датчика движения — режим мины, датчика здоровья — включает скан; `crate::assemblies::chem_grenade`). Нет: бомбы Синдиката, C4 и пиццы-бомбы, бомбы-ошейника

### Провода компонента «пучок проводов»
Провода, добавляемые к предметам компонентом (самодельные устройства).

- **Код:** [`/datum/wires/wire_bundle_component`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/wire_bundle_component.dm#L3)
- **Статус в ss13-rs:** ❌ нет

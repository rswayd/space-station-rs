# Провода и взлом
Механизм `/datum/wires`: цветные провода в машинах, окно `Wires`, резка/пульс/сигналеры и все наборы проводов.

### Механизм проводов и окно Wires
У машины с открытой панелью есть набор проводов (цвета перемешиваются раз в раунд на набор, есть «пустышки»); кусачки режут/чинят, мультитул пульсирует; tgui-окно `Wires` открывается инструментом или рукой; провода доступны, пока панель открыта.

- **Код:** [`/datum/wires`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/_wires.dm#L32), [`proc/randomize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/_wires.dm#L126), [`proc/cut()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/_wires.dm#L179), [`proc/pulse()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/_wires.dm#L199), [`proc/interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/_wires.dm#L258); tgui: [`Wires.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Wires.tsx)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/wires.rs`, `crates/ss13_tgui/src/interfaces/wires.rs`: перемешивание по сиду карты, пустышки, резка/починка/пульс с проверками в момент действия, окно

### Сигналеры на проводах
К проводу крепится сборка (сигналер, таймер, датчик), которая пульсирует его по сигналу — удалённый взлом.

- **Код:** [`proc/attach_assembly()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/_wires.dm#L214), [`proc/pulse_assembly()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/_wires.dm#L208)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/wires.rs` (shortcut: no assemblies)

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
- **Статус в ss13-rs:** ❌ нет

### Провода инженерных машин
Автолат (взлом — запрещённые чертежи, удар током, блокировка), эмиттер, катушка теслы, генератор щитовой стены, массдрайвер, конвейер, большой манипулятор.

- **Код:** [`/datum/wires/autolathe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/autolathe.dm#L1), [`/datum/wires/emitter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/emitter.dm#L2), [`/datum/wires/tesla_coil`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/tesla_coil.dm#L1), [`/datum/wires/shieldwallgen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/shieldwallgen.dm#L1), [`/datum/wires/mass_driver`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/mass_driver.dm#L1), [`/datum/wires/conveyor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/conveyor.dm#L1), [`/datum/wires/big_manipulator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/big_manipulator.dm#L1)
- **Статус в ss13-rs:** ❌ нет

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
- **Статус в ss13-rs:** ❌ нет

### Провода компонента «пучок проводов»
Провода, добавляемые к предметам компонентом (самодельные устройства).

- **Код:** [`/datum/wires/wire_bundle_component`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/wire_bundle_component.dm#L3)
- **Статус в ss13-rs:** ❌ нет

# События экипажа, бюрократии и вторжений

События, действующие на людей и на станционную бюрократию: болезни, травмы, фальшивые тревоги, грузы, шаттлы, разумные животные, мидраунд-антагонисты.

## Здоровье

### Disease Outbreak: Classic / Advanced
Несколько членов экипажа заражаются вирусом: классическим (готовые болезни) или продвинутым (случайные симптомы, тяжесть растёт со временем раунда); объявление приходит с задержкой.

- **Код:** [`/datum/round_event_control/disease_outbreak`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/disease_outbreak.dm#L26), [`/datum/round_event_control/disease_outbreak/advanced`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/disease_outbreak.dm#L168)
- **Статус в ss13-rs:** ❌ нет — нет болезней

### Fake Virus
Несколько человек на время получают ипохондрию: фальшивые симптомы в чате без вреда.

- **Код:** [`/datum/round_event_control/fake_virus`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/fake_virus.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет события

### Random Heart Attack
У случайного члена экипажа (с учётом здоровья сердца) останавливается сердце; спасает дефибриллятор или лекарства.

- **Код:** [`/datum/round_event_control/heart_attack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/heart_attack.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет органов и остановки сердца

### Spontaneous Brain Trauma
Случайный член экипажа получает случайную травму мозга (фобия, амнезия, заикание и т.п.).

- **Код:** [`/datum/round_event_control/brain_trauma`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/brain_trauma.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет травм мозга

### Mass Hallucination
Весь экипаж одновременно видит одну и ту же галлюцинацию.

- **Код:** [`/datum/round_event_control/mass_hallucination`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/mass_hallucination.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/mass_hallucination.rs`: вес 10, максимум 2, всем живым людям один тип (обычный/странный звук, объявление, общая галлюцинация); категории бреда берут общую (бред — в hallucinations.md), без админской настройки

## Бюрократия

### False Alarm
Фальшивое объявление одного из реальных событий (без самого события).

- **Код:** [`/datum/round_event_control/falsealarm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/false_alarm.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/false_alarm.rs`: объявляет случайное `fakeable` событие, которое сейчас могло бы случиться, не запуская его; нет фальшивых мидраунд-рулсетов (нет мидраунд-антагонистов)

### Bureaucratic Error
Случайно открывает и закрывает слоты профессий в позднем входе и меняет роль переполнения.

- **Код:** [`/datum/round_event_control/bureaucratic_error`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/bureaucratic_error.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/bureaucratic_error.rs`: объявление, 33% — одна профессия без лимита, остальные закрыты, иначе каждой `rand(-2, 4)` слотов (не ниже 0); роль переполнения, ИИ и профессии станционных трейтов не трогаются

### Market Crash
Цены торговых автоматов временно растут; объявление о крахе рынка.

- **Код:** [`/datum/round_event_control/market_crash`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/market_crash.dm#L6)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/market_crash.rs`: объявления о росте и стабилизации цен (50–100 тиков), инфляция `5.5 * log(activeFor + 1)` цен станционных автоматов (обновление раз в 5 тиков, открытые окна получают новые цены), в конце — обратно (`crates/ss13_game/src/vending/mod.rs` `update_prices`)

### Stray Cargo Pod / Stray Syndicate Cargo Pod
На станцию падает дроп-под со случайным ящиком карго (или с вещами Синдиката); падение повреждает место приземления.

- **Код:** [`/datum/round_event_control/stray_cargo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/stray_cargo.dm#L2), [`/datum/round_event_control/stray_cargo/syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/stray_cargo.dm#L169)
- **BandaStation:** [`modular_bandastation/balance/code/events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/events.dm#L5) — поды при падении взрываются сильнее (`explosionSize = list(1,2,3,3)`)
- **Статус в ss13-rs:** ❌ нет — нет дроп-подов и взрывов

### Shuttle Catastrophe
Аварийный шаттл станции заменяется случайным другим (объявление о «катастрофе»); не срабатывает, если шаттл уже пристыкован, построен игроками или заменён принудительно; страховка оплачивает ЦК компенсацию.

- **Код:** [`/datum/round_event_control/shuttle_catastrophe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/shuttle_catastrophe.dm#L1)
- **Статус в ss13-rs:** ❌ нет — аварийный шаттл один, из json карты (`crates/ss13_game/src/shuttle.rs`)

### Shuttle Insurance
Сомнительная страховая компания предлагает через сообщение консоли связи застраховать аварийный шаттл; если станция платит, Shuttle Catastrophe возмещает потерю шаттла.

- **Код:** [`/datum/round_event_control/shuttle_insurance`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/shuttle_insurance.dm#L3)
- **Статус в ss13-rs:** ❌ нет — событий и сообщений консоли связи нет (бюджеты отделов есть: `crates/ss13_game/src/cargo/economy.rs`)

### Shuttle Loan
ЦК просит одолжить шаттл карго; при согласии через консоль он возвращается с грузом и/или врагами (пираты, пчёлы, бомба и др.).

- **Код:** [`/datum/round_event_control/shuttle_loan`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/shuttle_loan/shuttle_loan_event.dm#L2), [`/datum/shuttle_loan_situation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/shuttle_loan/shuttle_loan_datum.dm#L2)
- **Статус в ss13-rs:** ❌ нет — нет шаттла карго

## Мирные

### Night Shift
Переводит свет станции в ночной режим на 20 минут (приглушённые лампы через APC).

- **Код:** [`/datum/round_event_control/nightshift`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/nightshift.dm#L1)
- **Статус в ss13-rs:** ❌ нет — ночного режима ламп нет (`crates/ss13_mapload/src/light.rs`: shortcut no nightshift)

### Wisdom Cow
На станции появляется корова мудрости, изрекающая мудрости и дающая особое молоко.

- **Код:** [`/datum/round_event_control/wisdomcow`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wisdomcow.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет мобов-животных

### Random / Station-wide Human-level Intelligence
Случайное животное или бот обретает разум и отдаётся призраку-добровольцу; станционный вариант (только админы) — все сразу.

- **Код:** [`/datum/round_event_control/sentience`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/ghost_role/sentience.dm#L25), [`/datum/round_event_control/sentience/all`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/ghost_role/sentience.dm#L125), [`/datum/round_event/ghost_role`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/ghost_role/_ghost_role.dm#L4)
- **Статус в ss13-rs:** ❌ нет — нет животных и опроса призраков

## Вторжения

### Lone Operative
Одиночный ядерный оперативник прилетает за диском ядерной аутентификации; вес растёт, пока диск лежит без присмотра.

- **Код:** [`/datum/round_event_control/operative`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/ghost_role/operative.dm#L1)
- **BandaStation:** [`code/modules/events/ghost_role/operative.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/ghost_role/operative.dm#L18), [`modular_bandastation/balance/code/events.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/events.dm#L1) — взвешенный выбор кандидата; минимум 30 игроков
- **Статус в ss13-rs:** ❌ нет — нет ядерного диска и оперативников

### Dynamic Tweak
Позволяет Dynamic запустить ещё один мидраунд- или латджойн-антагонист (оживляет «зелёные» смены).

- **Код:** [`/datum/round_event_control/dynamic_tweak`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/dynamic_tweak.dm#L3)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/antag/dynamic.rs` выбирает только раундстартового предателя, событий нет

_«Spawn Bitrunning Glitch» описано в [../science/bitrunning.md](../science/bitrunning.md)._

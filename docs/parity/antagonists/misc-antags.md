# Малые и особые антагонисты
Роли, выдаваемые событиями, предметами, админами или эффектами: клоны, валентинки, промывка мозгов, хайлендер, выживальщики, нации и т.д.

### Парадокс-клон и злой клон
Парадокс-клон — копия члена экипажа из другой реальности, цель — убить и заменить оригинал (мидраунд из призраков); злой клон — клон из неисправного клонирующего устройства/эффекта.

- **Код:** [`/datum/antagonist/paradox_clone`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/paradox_clone/paradox_clone.dm#L1), [`/datum/antagonist/evil_clone`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/evil_clone/evil_clone.dm#L2)
- **BandaStation:** [`dynamic_ruleset_midround.dm#L774`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/dynamic/dynamic_ruleset_midround.dm#L774) — сообщение админам при выборе клона
- **Статус в ss13-rs:** 🟡 частично — парадокс-клон готов: `crates/ss13_game/src/antag/paradox_clone.rs` + рулсет `Paradox Clone` в `antag/midround.rs` (призраки с `be_special` «Paradox Clone», 20% второй клон того же оригинала; оригинал — случайный член экипажа в сознании с клиентом; спавн на `generic_maintenance_landmark`, `SpawnPoints::maintenance`): копия имени, вида и тела оригинала, его рабочий аутфит (ID, PDA со скрытым мессенджером, рюкзак), механический ящик в руке, без станционной работы; цели `accept_no_substitutes` и `paradox_clone_replace`, приветствие, HUD `paradox_clone`, панель `AntagInfoGeneric`, отчёт конца раунда, переключатель в лобби; админ может форсировать рулсет; игрок выбирается опросом призраков (`ghost_poll.rs`, оригинал называется в вопросе и выбирается до опроса). Нет выбора оригинала админом, проверки безопасности тайла, звука, копии квирков/памяти, сенсоров костюма. Злой клон ❌ — нужен экспериментальный клонер (группа медицины/руин)

### Промытые мозги и гипноз
Brainwashed — цели, вложенные хирургией/нанитами/вещами; hypnotized — фраза гипноза от гипнофлеша/травмы.

- **Код:** [`/datum/antagonist/brainwashed`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/brainwashing/brainwashing.dm#L30), [`/datum/antagonist/hypnotized`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/hypnotized/hypnotized.dm#L2); tgui: [`AntagInfoBrainwashed.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AntagInfoBrainwashed.tsx)
- **Статус в ss13-rs:** 🟡 частично — промытые мозги готовы: `crates/ss13_game/src/antag/brainwashed.rs` (`brainwash()`: директивы как цели, повтор добавляет их; HUD `brainwashed`, панель `AntagInfoBrainwashed` (рисуется как общая, без темы), снятие операцией на мозге и лоботомией, отчёт конца раунда), источники — операции промывки и «спящего агента» с дисков (`medical/surgery.rs`). Нет deadchat-оповещения, звука, админ-команды промывки. Гипноз 🟡 — травма гипноза с фразой есть (`medical/traumas.rs`, `gain_hypnosis`); нет антагониста `hypnotized` (цель-фраза, HUD), гипнофлеша, транса и перехвата услышанной фразы (группы устройств и речи)

### Валентинки
Событие Дня святого Валентина: пары валентинок защищают друг друга, третий лишний и разбиватель сердец.

- **Код:** [`/datum/antagonist/valentine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/valentines/valentine.dm#L1), [`/datum/antagonist/heartbreaker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/valentines/heartbreaker.dm#L1), [`/datum/antagonist/valentine/third_wheel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/valentines/valentine.dm#L72)
- **Статус в ss13-rs:** ✅ готово — `crates/ss13_game/src/antag/valentine.rs` + событие «Valentines!» (`events/holiday_events.rs`, только в День святого Валентина, срабатывает при первом розыгрыше событий): анонс, все живые игроки разбиваются на пары (сердечная коробка, конфета-сердце и открытка в рюкзак), цель «защитить пару», третий лишний с шансом 4% (защищает одного из пары, букет в руках), оставшийся без пары — Heartbreaker (цель «умереть славной смертью», панель `AntagInfoGeneric`); ИИ и киборги-валентинки получают закон 0 (киборг отключается от ИИ); раздел «Валентинки» в отчёте конца раунда (одна строка на пару, третьи лишние, разбиватели сердец). Перед парами опрос 30 с: все записаны сразу, можно отписаться (`start_signed_up`, `ghost_poll.rs`). Нет статуса «In Love» с сердечками над парой, текстов открытки/конфеты из `valentines.json`, проверки ЦК

### Хайлендер и выживальщики
Хайлендер (админ-событие «There can be only one»: клеймор, остаться последним); выживальщики Summon Guns/Summon Magic и battle royale — собрать оружие/магию и выжить.

- **Код:** [`/datum/antagonist/highlander`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/highlander/highlander.dm#L1), [`/datum/antagonist/survivalist`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/survivalist/survivalist.dm#L1), [`/datum/antagonist/survivalist/guns`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/survivalist/survivalist.dm#L26), [`/datum/antagonist/survivalist/battle_royale`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/survivalist/survivalist.dm#L62)
- **Статус в ss13-rs:** 🟡 частично — выживальщик Summon Guns готов: `crates/ss13_game/src/antag/survivalist.rs` (`summon_guns`: ритуал книги мага и wizard-событие «Summon Guns», случайное оружие из `GLOB.summoned_guns` в руки с обычным бойком каждому человеку экипажа/антагонисту на станции, 10% не-антагонистов становятся выживальщиками; каждое применение — ещё по стволу, опоздавшим — по стволу за каждое применение; цели «украсть 5 настоящего оружия» и «выжить», приветствие, панель `AntagInfoGeneric`, отчёт «Другие антагонисты»). Нет Summon Magic (жезлы/артефакты почти не портированы — группа мага), battle royale, хайлендера (клеймор-хайлендер, админ-событие)

### Сепаратисты (нации)
Режим Nations: каждый департамент — независимая нация со своими целями.

- **Код:** [`/datum/team/nation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/separatist/separatist.dm#L1), [`/datum/antagonist/separatist`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/separatist/separatist.dm#L81); tgui: [`AntagInfoSeparatist.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AntagInfoSeparatist.tsx)
- **Статус в ss13-rs:** ❌ нет

### Подчинённые и разумные существа
Разумное существо (после зелья разума), шейд-слуга, слуга мага (magic servant), обезьяна-агент Синдиката, пиро-слайм, венерина мухоловка, исполнитель желаний (wishgranter), гринтекст-носитель.

- **Код:** [`/datum/antagonist/sentient_creature`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/sentient_creature/sentient_creature.dm#L1), [`/datum/antagonist/shade_minion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/shade/shade_minion.dm#L7), [`/datum/antagonist/magic_servant`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/magic_servant/servant.dm#L1), [`/datum/antagonist/syndicate_monkey`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/syndicate_monkey/syndicate_monkey.dm#L1), [`/datum/antagonist/pyro_slime`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/pyro_slime/pyro_slime.dm#L1), [`/datum/antagonist/venus_human_trap`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/venus_human_trap/venus_human_trap.dm#L1), [`/datum/antagonist/wishgranter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/wishgranter/wishgranter.dm#L1), [`/datum/antagonist/greentext`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/greentext/greentext.dm#L1); tgui: [`AntagInfoSentient.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AntagInfoSentient.jsx)
- **Статус в ss13-rs:** ❌ нет

### Пеплоходцы
Ящеры Лаваленда, команда вокруг гнезда-тендрила: приносят тела для размножения, защищают гнездо.

- **Код:** [`/datum/antagonist/ashwalker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/ashwalker/ashwalker.dm#L1), [`/datum/team/ashwalkers`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/ashwalker/ashwalker.dm#L44)
- **Статус в ss13-rs:** ❌ нет

### Санта
Админ-событие: Санта с мешком подарков и телепортом.

- **Код:** [`/datum/antagonist/santa`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/santa/santa.dm#L1)
- **Статус в ss13-rs:** ❌ нет

# Галлюцинации
Клиентские иллюзии, которые видит и слышит только галлюцинирующий: `/datum/hallucination`, выбираемые случайно из взвешенного списка по тирам, пока действует статус-эффект галлюцинаций.

### Статус-эффект галлюцинаций и выбор по весам
Эффект `hallucination` периодически вызывает случайную галлюцинацию из `random_hallucination_weighted_list` (тиры common/uncommon/rare/special); варианты от низкого рассудка и «перцептоматрицы»; импульсы галлюцинаций по радиусу (`hallucination_pulse`, например от суперматерии).

- **Код:** [`/datum/status_effect/hallucination`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/hallucination.dm#L3), [`/datum/status_effect/hallucination/sanity`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/status_effects/debuffs/hallucination.dm#L121), [`proc/_cause_hallucination()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/hallucinations.dm#L39), [`proc/get_random_hallucination()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/hallucinations.dm#L149), [`proc/hallucination_pulse()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/hallucinations.dm#L112), [`/datum/hallucination`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/_hallucination.dm#L9)
- **Статус в ss13-rs:** ❌ нет

### Ложные звуки
~30 звуков только для жертвы: шлюзы, выстрелы, взрывы, шаги, сирены, голос ИИ и т.п.

- **Код:** [`/datum/hallucination/fake_sound`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/fake_sound.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Ложные сообщения, чат и телепатия
Фальшивые реплики окружающих (в том числе по радио), системные сообщения, «голоса в голове», объявления станции (блоб, шаттл, ИИ-сбой, культ).

- **Код:** [`/datum/hallucination/chat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/fake_chat.dm#L2), [`/datum/hallucination/message`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/fake_message.dm#L1), [`/datum/hallucination/telepathy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/telepathy.dm#L1), [`/datum/hallucination/station_message`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/station_message.dm#L3)
- **BandaStation:** [`station_message.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/station_message.dm#L111) — звук призыва культа заменён звуком форка
- **Статус в ss13-rs:** ❌ нет

### Бред (delusions)
Окружающие люди выглядят как монстры, клоуны, ксеноморфы, обезьяны и т.п.; админский вариант с произвольной внешностью.

- **Код:** [`/datum/hallucination/delusion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/delusions.dm#L2), [`/datum/hallucination/delusion/preset`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/delusions.dm#L222), [`proc/create_delusion()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/__HELPERS/hallucinations.dm#L247)
- **BandaStation:** [`delusions.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/delusions.dm#L73) — свой выбор целей и внешностей бреда
- **Статус в ss13-rs:** ❌ нет

### Ложный HUD и тревоги
Фальшивые алерты (нет кислорода, жара, огонь, взлом), ложный индикатор здоровья (крит, смерть), неправильная кукла здоровья.

- **Код:** [`/datum/hallucination/fake_alert`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/fake_alert.dm#L2), [`/datum/hallucination/screwy_hud`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/hud_screw.dm#L2), [`/datum/hallucination/fake_health_doll`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/screwy_health_doll.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Ложные атаки и угрозы
Невидимая драка (выстрелы, удары, энергомеч), шальная пуля, нападение ксеноморфа, Баблгам («oh yeah»), разряд тока, мнимый пожар на себе, ледяной блок.

- **Код:** [`/datum/hallucination/battle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/battle.dm#L2), [`/datum/hallucination/stray_bullet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/stray_bullet.dm#L2), [`/datum/hallucination/xeno_attack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/xeno_attack.dm#L2), [`/datum/hallucination/oh_yeah`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/bubblegum_attack.dm#L2), [`/datum/hallucination/shock`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/shock.dm#L2), [`/datum/hallucination/fire`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/on_fire.dm#L4), [`/datum/hallucination/ice`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/ice_cube.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Ложные предметы, тела и окружение
Мнимый предмет в руке или у соседа (револьвер, энергомеч), трупы и хаски на полу, лава и пропасти, плазменный потоп, заболченные шлюзы, взломанный АПЦ, глаза в темноте, кровотечение.

- **Код:** [`/datum/hallucination/fake_item`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/inhand_fake_item.dm#L2), [`/datum/hallucination/nearby_fake_item`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/nearby_fake_item.dm#L2), [`/datum/hallucination/body`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/body.dm#L2), [`/datum/hallucination/hazard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/hazard.dm#L2), [`/datum/hallucination/fake_flood`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/fake_plasmaflood.dm#L6), [`/datum/hallucination/bolts`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/bolted_airlocks.dm#L1), [`/datum/hallucination/malf_apc`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/malf_apc.dm#L1), [`/datum/hallucination/eyes_in_dark`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/eyes_in_dark.dm#L1), [`/datum/hallucination/blood_flow`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/blood_flow.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Ложная смерть и «твоя мама»
Жертва видит собственную смерть (сообщения в дедчат, обращение в пыль) или появление своей матери.

- **Код:** [`/datum/hallucination/death`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/fake_death.dm#L2), [`/datum/hallucination/your_mother`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/hallucination/mother.dm#L2)
- **Статус в ss13-rs:** ❌ нет

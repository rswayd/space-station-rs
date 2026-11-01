# Ядерные оперативники
Команда Синдиката с базы-аванпоста: добыть диск ядерной аутентификации и взорвать станцию. Сюда же клоун-опы, одиночный оперативник и боевой крейсер.

### Оперативники и команда
`/datum/antagonist/nukeop` и `/datum/team/nuclear`: позывные, лидер (выбор названия команды, код от бомбы), снаряжение, аплинк на всю команду, цель — взорвать станцию.

- **Код:** [`/datum/antagonist/nukeop`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/datums/operative.dm#L1), [`/datum/antagonist/nukeop/leader`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/datums/operative_leader.dm#L1), [`/datum/team/nuclear`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/datums/operative_team.dm#L4), [`/datum/outfit/syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/outfits.dm#L1)
- **BandaStation:** [`operative_team.dm#L198`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/datums/operative_team.dm#L198) — префы переносятся без модификаций тела
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/nukeop/server.rs` (`spawn_operatives`, `add_nukeop`, `NukeTeam`, `compute_result`, `roundend_report`): рулсет `Roundstart Nukeops` (кап `ceil(pop/18)+1`, мин = макс), без станционной работы, старт на инфильтраторе (`// shortcut:` его уровень заменяет базу Синдиката), позывные «<алиас> <фамилия>», лидер с титулом и бумагой с кодом, снаряжение `/datum/outfit/syndicate` (ID с доступом syndicate, гарнитура на частоте Синдиката, ядерный аплинк 25 + `CEILING(игроки/5, 5)` ТК, пинпоинтер), цель «взорвать станцию», результат `get_result` и отчёт конца раунда; нет выбора названия команды лидером, командных скидок, имплантов, МОДов, командного HUD

### Объявление войны
Лидер может объявить войну (в течение 5 минут): команда получает бонусные ТК, шаттл задерживается до 25 минут, глушатся связи.

- **Код:** [`/obj/item/nuclear_challenge`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/equipment/nuclear_challenge.dm#L13)
- **BandaStation:** [`nuclear_challenge.dm#L5`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/equipment/nuclear_challenge.dm#L5) — бонус ТК считается из числа экипажа (`challenge_telecrystals_per_crewman`, 3.5); [`nuclear_challenge.dm#L99`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/equipment/nuclear_challenge.dm#L5) — через 20 с ставится код Гамма
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/antag/nukeop/server.rs` (`attack_self`): предмет объявления войны выдаётся лидеру, но только сообщает, что объявление недоступно

### Ядерная бомба
Ядерное устройство: ввод кода, диск, таймер, взрыв уничтожает станцию (синематик, конец раунда); бомба Синдиката, самоуничтожение станции, пивная и бананиумовая бомбы.

- **Код:** [`/obj/machinery/nuclearbomb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/equipment/nuclear_bomb/_nuclear_bomb.dm#L6), [`/obj/machinery/nuclearbomb/syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/equipment/nuclear_bomb/syndicate_nuke.dm#L1), [`/obj/machinery/nuclearbomb/selfdestruct`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/equipment/nuclear_bomb/self_destruct.dm#L1), [`/obj/machinery/nuclearbomb/beer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/equipment/nuclear_bomb/beer_nuke.dm#L2), [`/obj/machinery/nuclearbomb/syndicate/bananium`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/clown_ops/bananium_bomb.dm#L1); tgui: [`NuclearBomb.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/NuclearBomb.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/nukeop/bomb.rs` (`NukeState`, `NuclearBomb`), окно `NuclearBomb` в `crates/ss13_game/src/antag/nukeop/client.rs`: диск, код, таймер 90–3600 с, ARM/разминирование, ANCHOR, спрайт по состоянию, взрыв через 10 с (на станции — гибель всех живых на уровнях станции и конец раунда, на базе — Fluke, иначе промах); нет разборки и ядра, синематика, кода Дельта, пивной/бананиумовой бомбы; самоуничтожение станции без кода (`admin`)

### Диск ядерной аутентификации
Единственный диск на станции (у капитана): возрождается при уничтожении или долгом нахождении вне станции; фальшивые диски; пинпоинтер ведёт к диску или бомбе.

- **Код:** [`/obj/item/disk/nuclear`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/equipment/nuclear_authentication_disk.dm#L2), [`/obj/item/disk/nuclear/fake`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/equipment/nuclear_authentication_disk.dm#L115), [`/obj/item/pinpointer/nuke`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/equipment/pinpointer.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/nukeop/server.rs` (`NukeDisk`, `watch_disk`, `Pinpointer`): настоящий диск с карты, вне допустимых уровней (`atom_in_bounds`) или уничтоженный возвращается на случайный пол станции; пинпоинтеры (капитанский и операторов) показывают направление и дальность до диска, синдикатовский после взвода — до инфильтратора, тревога при таймере; нет фальшивых дисков (кроме отказа бомбы), «скверного обращения» с диском для одиночного оперативника

### Подкрепления и поддержка
Подкрепления по маяку (оперативник от разных корпораций, киборги Синдиката: штурмовой, медицинский, саботажник), оператор поддержки (overwatch) с камерами и очками.

- **Код:** [`/datum/antagonist/nukeop/reinforcement`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/datums/operative_reinforcement.dm#L1), [`/datum/antagonist/nukeop/reinforcement/cyborg`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/datums/operative_reinforcement.dm#L8), [`/datum/antagonist/nukeop/support`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/datums/operative_support.dm#L1), [`/obj/machinery/computer/security/overwatch`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/equipment/overwatch_tools.dm#L19), [`/obj/item/borg_chameleon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/equipment/borgchameleon.dm#L4)
- **Статус в ss13-rs:** ❌ нет

### Одиночный оперативник
Событие: одиночный опер появляется, если диск долго лежит без движения.

- **Код:** [`/datum/antagonist/nukeop/lone`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/datums/operative_lone.dm#L1), [`/datum/team/nuclear/loneop`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/datums/operative_team.dm#L348), [`/datum/outfit/syndicate/full/loneop`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/nukeop/outfits.dm#L95)
- **BandaStation:** [`/datum/round_event_control/operative`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/events.dm#L1) — событие только от 30 игроков
- **Статус в ss13-rs:** ❌ нет

### Клоун-оперативники
Вариант нюков от клоунов: бананиумовая бомба, бананиумовые меч и щит, смазочный цветок, бомбананы, усатая граната, клоунский аплинк.

- **Код:** [`/datum/antagonist/nukeop/clownop`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/clown_ops/clownop.dm#L2), [`/datum/outfit/syndicate/clownop`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/clown_ops/outfits.dm#L1), [`/obj/item/melee/energy/sword/bananium`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/clown_ops/clown_weapons.dm#L18), [`/obj/item/shield/energy/bananium`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/clown_ops/clown_weapons.dm#L104), [`/obj/item/seeds/banana/bombanana`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/clown_ops/clown_weapons.dm#L153)
- **Статус в ss13-rs:** ❌ нет

### Боевой крейсер Синдиката
Админ-событие: экипаж крейсера (капитан, союзники) атакует станцию.

- **Код:** [`/datum/team/battlecruiser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/battlecruiser/battlecruiser.dm#L1), [`/datum/antagonist/battlecruiser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/battlecruiser/battlecruiser.dm#L15), [`/datum/antagonist/battlecruiser/captain`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/battlecruiser/battlecruiser.dm#L39)
- **Статус в ss13-rs:** ❌ нет

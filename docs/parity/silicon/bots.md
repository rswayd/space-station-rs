# Боты
Простые роботы `/mob/living/basic/bot` с ИИ-контроллером: патруль, задачи по отделам, сборка из частей.

### Базовый бот
Включение/выключение, панель с замком (доступ), окно настроек `SimpleBot`, патруль по маячкам навигации, вызов ИИ/КПК в точку, емаг (опасный режим), возможность вселения призрака (персональный бот), HUD-диагностика, ремонт сваркой.

- **Код:** [`/mob/living/basic/bot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/_bots.dm#L11), [`/datum/ai_controller/basic_controller/bot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/bot_ai.dm#L1), [`bot_hud.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/bot_hud.dm#L1); tgui: [`SimpleBot.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SimpleBot.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/bots/` (`core.rs`, `ui.rs`, `ai.rs`): `Bot` из переменных `mobs.ron` (`ss13-mapload convert`), вкл/выкл (ИИ бота стоит), панель техобслуживания (отвёртка), замок управления по ID/alt-клику с `req_one_access`, емаг (разблокировка, затем емаг с открытой панелью), ремонт сваркой, окно `SimpleBot` (питание, «в самолёте», патруль, панель, настройки вида; ИИ и киборги тоже открывают, `hack`), патруль по маякам `next_patrol` (`dm_navbeacon`), карта доступа (бот проходит и прокладывает путь через шлюзы со своим доступом, `crate::mobs::ai::movement`), отдание чести командованным ботам, взрыв при смерти с деталями; нет вызова ИИ/КПК (нет программы управления ботами), вселения призрака и pAI, переименования, HUD-диагностики и пути, ЭМИ

### Объявления ботов
Действие с выбором фраз (окно `BotAnnouncement`) и быстрые объявления (медбот сообщает о пациенте в крите).

- **Код:** [`/datum/action/cooldown/bot_announcement`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/bot/bot_announcement.dm#L2), [`/datum/action/cooldown/bot_announcement/medbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/bot/bot_announcement.dm#L164); tgui: [`BotAnnouncement.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/BotAnnouncement.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/bots/core.rs` (`speak`): автоматические фразы видов (переводы BandaStation) вслух и в канал рации бота; нет действия и окна `BotAnnouncement` и звуков голосовых фраз (их `.ogg` не конвертируются)

### Сборка ботов
Сборки из подручных частей: ведро+датчик → клинбот, аптечка+рука → медбот, шлем+сигнализатор+дубинка → секбот, огнетушитель → файрбот, ящик с инструментами → ремботы, хонкбот, гигиенобот, «Vim».

- **Код:** [`/obj/item/bot_assembly`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/bot/construction.dm#L3), [`/obj/item/bot_assembly/medbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/bot/construction.dm#L271), [`/obj/item/bot_assembly/secbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/bot/construction.dm#L354), [`/obj/item/bot_assembly/cleanbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/bot/construction.dm#L38)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/bots/construction.rs`: клинбот (ведро + датчик, рука), медбот (рука на пустую аптечку, анализатор, датчик; скин и тип лечения аптечки), секбот (сигнализатор на шлем, сварка, датчик, рука, дубинка; откат отвёрткой/сваркой), файрбот (рука на огнетушитель, красная каска, датчик); имя сборки переходит боту; нет ED-209, хонкбота, гигиенобота, ремботов, «Vim», генерала Бипски, переименования ручкой

### Клинбот
Ищет и убирает грязь, кровь, мусор; апгрейды (швабра, кислотный режим при емаге); форк учитывает русские названия должностей в «заказанных» целях.

- **Код:** [`/mob/living/basic/bot/cleanbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/cleanbot/cleanbot.dm#L3)
- **BandaStation:** [`cleanbot.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/cleanbot/cleanbot.dm#L287) — перевод названия должности
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/bots/ai.rs` (`cleanbot`): ищет в 5 клетках грязь (кровь, граффити, мусор, вредителей по настройкам окна), достижимую за 15 шагов, моет 2 с, патрулирует через 5 с после уборки, отдаёт честь; нет режима емага (кислота, пена), ножа, швабры-апгрейда, команд питомца, «дружбы» с уборщиком и званий по должностям (в т.ч. русских названий BandaStation)

### Медбот
Лечит раненых своими реагентами по типу урона, сообщает о критах по радио; емаг — травит.

- **Код:** [`/mob/living/basic/bot/medbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/medbot/medbot.dm#L3)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/bots/ai.rs` (`medbot`): лечит людей в 7 клетках (1 в стационарном режиме) с уроном своего типа выше порога (2.5 за 2 с, все типы у «продвинутых»), фразы ожидания/после лечения/у умирающего/холостые, сообщает о критах по радио раз в 3 мин, емаг — хлоралгидрат; нет опрокидывания, улучшений из техвеба, шапок, оверлеев колёс и лица

### Секбот (Бипски и вариации) и ED-209
Арестует по статусу розыска и уровню угрозы: оглушает, надевает стяжки; ED-209 стреляет дизейблером; Бипски, Пингски, супер-Бипски; ED-209 нюкеров.

- **Код:** [`/mob/living/basic/bot/secbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/secbot/secbot.dm#L1), [`/mob/living/basic/bot/secbot/beepsky`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/secbot/secbot_subtypes.dm#L1), [`/mob/living/basic/bot/secbot/ed209`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/ed209/ed209.dm#L1), [`super_beepsky.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/secbot/super_beepsky.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/bots/ai.rs` (`secbot`, `assess_threat`, `stun`): Бипски, Армски, Пингски, Офицер Бипски патрулируют/охраняют, цель — человек с угрозой выше 4 (розыск в записях, проверка ID, емаг) или ударивший бота; оглушение 8 с (откат 10 с), затем стяжки за 4 с, объявление ареста по радио СБ; нет проверки оружия (`NEEDS_PERMIT`), видов и шляпы волшебника, штрафов, подсечки, ED-209, супер-Бипски и Генески-вариаций

### Хонкбот
Клоунский секбот: сигналит, подскальзывает, хонкает нарушителей.

- **Код:** [`/mob/living/basic/bot/secbot/honkbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/honkbots/honkbot.dm#L1)
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/bots/ai.rs`: картированные хонкботы только патрулируют и отдают честь (дерево базового бота)

### Файрбот
Тушит горящих и пожары пеной из огнетушителя.

- **Код:** [`/mob/living/basic/bot/firebot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/firebot/firebot.dm#L3)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/bots/ai.rs` (`firebot`): тушит горящих людей и очаги огня в 5 клетках (вода охлаждает воздух очага), объявляет пожар, холостые фразы; емаг поджигает людей; нет пены при жаре вокруг, анимации распыления, стационарного режима в окне кроме переключателя

### Гигиенобот
Моет грязных людей водой.

- **Код:** [`/mob/living/basic/bot/hygienebot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/hygienebot/hygienebot.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Ремонтный бот (repairbot)
Чинит пробоины: ставит плитки, решётки, окна, стены из своего запаса листов (окно `RepairBot`/`RepairbotResources`).

- **Код:** [`/mob/living/basic/bot/repairbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/repairbot/repairbot.dm#L2); tgui: [`RepairBot.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/RepairBot.tsx), [`RepairbotResources.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/RepairbotResources.tsx)
- **Статус в ss13-rs:** ❌ нет — картированный рембот только патрулирует и отдаёт честь (`crates/ss13_game/src/bots/ai.rs`)

### МУЛбот
Доставочный робот: возит ящики к маякам доставки, домой, по вызову из КПК; провода МУЛа, наезд на людей при емаге.

- **Код:** [`/mob/living/basic/bot/mulebot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/mulebot/mulebot.dm#L1), [`mulebot_delivery.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/mulebot/mulebot_delivery.dm#L1), [`/datum/wires/mulebot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/mulebot.dm#L5); tgui: [`Mule.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Mule.tsx)
- **Статус в ss13-rs:** ❌ нет — картированные МУЛы загружаются как боты (окно `SimpleBot`, ремонт, емаг), но не возят грузы и не патрулируют (`crates/ss13_game/src/bots/ai.rs`)

### Вайббот и дедбот
Вайббот поднимает настроение светом и музыкой; дедбот — враждебный бот-заготовка.

- **Код:** [`/mob/living/basic/bot/vibebot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/vibebot/vibebot.dm#L1), [`/mob/living/basic/bot/dedbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/dedbot.dm#L6)
- **Статус в ss13-rs:** ❌ нет

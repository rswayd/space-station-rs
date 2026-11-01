# Боты
Простые роботы `/mob/living/basic/bot` с ИИ-контроллером: патруль, задачи по отделам, сборка из частей.

### Базовый бот
Включение/выключение, панель с замком (доступ), окно настроек `SimpleBot`, патруль по маячкам навигации, вызов ИИ/КПК в точку, емаг (опасный режим), возможность вселения призрака (персональный бот), HUD-диагностика, ремонт сваркой.

- **Код:** [`/mob/living/basic/bot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/_bots.dm#L11), [`/datum/ai_controller/basic_controller/bot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/bot_ai.dm#L1), [`bot_hud.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/bot_hud.dm#L1); tgui: [`SimpleBot.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SimpleBot.tsx)
- **Статус в ss13-rs:** ❌ нет

### Объявления ботов
Действие с выбором фраз (окно `BotAnnouncement`) и быстрые объявления (медбот сообщает о пациенте в крите).

- **Код:** [`/datum/action/cooldown/bot_announcement`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/bot/bot_announcement.dm#L2), [`/datum/action/cooldown/bot_announcement/medbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/bot/bot_announcement.dm#L164); tgui: [`BotAnnouncement.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/BotAnnouncement.tsx)
- **Статус в ss13-rs:** ❌ нет

### Сборка ботов
Сборки из подручных частей: ведро+датчик → клинбот, аптечка+рука → медбот, шлем+сигнализатор+дубинка → секбот, огнетушитель → файрбот, ящик с инструментами → ремботы, хонкбот, гигиенобот, «Vim».

- **Код:** [`/obj/item/bot_assembly`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/bot/construction.dm#L3), [`/obj/item/bot_assembly/medbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/bot/construction.dm#L271), [`/obj/item/bot_assembly/secbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/bot/construction.dm#L354), [`/obj/item/bot_assembly/cleanbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/bot/construction.dm#L38)
- **Статус в ss13-rs:** ❌ нет

### Клинбот
Ищет и убирает грязь, кровь, мусор; апгрейды (швабра, кислотный режим при емаге); форк учитывает русские названия должностей в «заказанных» целях.

- **Код:** [`/mob/living/basic/bot/cleanbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/cleanbot/cleanbot.dm#L3)
- **BandaStation:** [`cleanbot.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/cleanbot/cleanbot.dm#L287) — перевод названия должности
- **Статус в ss13-rs:** ❌ нет

### Медбот
Лечит раненых своими реагентами по типу урона, сообщает о критах по радио; емаг — травит.

- **Код:** [`/mob/living/basic/bot/medbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/medbot/medbot.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Секбот (Бипски и вариации) и ED-209
Арестует по статусу розыска и уровню угрозы: оглушает, надевает стяжки; ED-209 стреляет дизейблером; Бипски, Пингски, супер-Бипски; ED-209 нюкеров.

- **Код:** [`/mob/living/basic/bot/secbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/secbot/secbot.dm#L1), [`/mob/living/basic/bot/secbot/beepsky`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/secbot/secbot_subtypes.dm#L1), [`/mob/living/basic/bot/secbot/ed209`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/ed209/ed209.dm#L1), [`super_beepsky.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/secbot/super_beepsky.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Хонкбот
Клоунский секбот: сигналит, подскальзывает, хонкает нарушителей.

- **Код:** [`/mob/living/basic/bot/secbot/honkbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/honkbots/honkbot.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Файрбот
Тушит горящих и пожары пеной из огнетушителя.

- **Код:** [`/mob/living/basic/bot/firebot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/firebot/firebot.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Гигиенобот
Моет грязных людей водой.

- **Код:** [`/mob/living/basic/bot/hygienebot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/hygienebot/hygienebot.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Ремонтный бот (repairbot)
Чинит пробоины: ставит плитки, решётки, окна, стены из своего запаса листов (окно `RepairBot`/`RepairbotResources`).

- **Код:** [`/mob/living/basic/bot/repairbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/repairbot/repairbot.dm#L2); tgui: [`RepairBot.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/RepairBot.tsx), [`RepairbotResources.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/RepairbotResources.tsx)
- **Статус в ss13-rs:** ❌ нет

### МУЛбот
Доставочный робот: возит ящики к маякам доставки, домой, по вызову из КПК; провода МУЛа, наезд на людей при емаге.

- **Код:** [`/mob/living/basic/bot/mulebot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/mulebot/mulebot.dm#L1), [`mulebot_delivery.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/mulebot/mulebot_delivery.dm#L1), [`/datum/wires/mulebot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/mulebot.dm#L5); tgui: [`Mule.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Mule.tsx)
- **Статус в ss13-rs:** ❌ нет

### Вайббот и дедбот
Вайббот поднимает настроение светом и музыкой; дедбот — враждебный бот-заготовка.

- **Код:** [`/mob/living/basic/bot/vibebot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/vibebot/vibebot.dm#L1), [`/mob/living/basic/bot/dedbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/dedbot.dm#L6)
- **Статус в ss13-rs:** ❌ нет

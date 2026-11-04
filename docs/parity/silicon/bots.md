# Боты
Простые роботы `/mob/living/basic/bot` с ИИ-контроллером: патруль, задачи по отделам, сборка из частей.

### Базовый бот
Включение/выключение, панель с замком (доступ), окно настроек `SimpleBot`, патруль по маячкам навигации, вызов ИИ/КПК в точку, емаг (опасный режим), возможность вселения призрака (персональный бот), HUD-диагностика, ремонт сваркой.

- **Код:** [`/mob/living/basic/bot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/_bots.dm#L11), [`/datum/ai_controller/basic_controller/bot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/bot_ai.dm#L1), [`bot_hud.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/bot_hud.dm#L1); tgui: [`SimpleBot.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SimpleBot.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/bots/` (`core.rs`, `ui.rs`, `ai.rs`, `voice.rs`): `Bot` из переменных `mobs.ron` (`ss13-mapload convert`), вкл/выкл (ИИ бота стоит), панель техобслуживания (отвёртка), замок управления по ID/alt-клику с `req_one_access`, емаг (разблокировка, затем емаг с открытой панелью), ремонт сваркой, окно `SimpleBot` (питание, «в самолёте», патруль, панель, переименование через `tgui_input_text`, настройки вида; ИИ и киборги тоже открывают, `hack`), патруль по маякам `next_patrol` (`dm_navbeacon`), карта доступа (бот проходит и прокладывает путь через шлюзы со своим доступом, `crate::mobs::ai::movement`), отдание чести командованным ботам, речь глаголами бота (`verb_say` и др., BandaStation `сообщает`), взрыв при смерти с деталями вида; нет вселения призрака, HUD-диагностики и пути. Чужое: вызов ИИ — [ai.md](ai.md) «Вызов шаттла ИИ, вызов ботов» (нет панели управления ботами), вызов КПК — нет программы управления ботами, pAI — [pai.md](pai.md), ЭМИ — [explosions.md](../effects/explosions.md)

### Объявления ботов
Действие с выбором фраз (окно `BotAnnouncement`) и быстрые объявления (медбот сообщает о пациенте в крите).

- **Код:** [`/datum/action/cooldown/bot_announcement`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/bot/bot_announcement.dm#L2), [`/datum/action/cooldown/bot_announcement/medbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/bot/bot_announcement.dm#L164); tgui: [`BotAnnouncement.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/BotAnnouncement.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/bots/core.rs` (`speak`), `voice.rs`: автоматические фразы видов (переводы BandaStation) вслух и в канал рации бота, озвученные фразы играют свою запись (`generate_speak_list`: секбот, хонкбот, медбот, файрбот, гигиенобот; файлы копирует конвертер, `files::BOTS`); нет действия и окна `BotAnnouncement` с кнопками-ярлыками (они у бота под управлением игрока: нужно вселение призрака)

### Сборка ботов
Сборки из подручных частей: ведро+датчик → клинбот, аптечка+рука → медбот, шлем+сигнализатор+дубинка → секбот, огнетушитель → файрбот, ящик с инструментами → ремботы, хонкбот, гигиенобот, «Vim».

- **Код:** [`/obj/item/bot_assembly`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/bot/construction.dm#L3), [`/obj/item/bot_assembly/medbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/bot/construction.dm#L271), [`/obj/item/bot_assembly/secbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/bot/construction.dm#L354), [`/obj/item/bot_assembly/cleanbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/simple_animal/bot/construction.dm#L38)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/bots/construction.rs`: клинбот (ведро + датчик, рука), медбот (рука на пустую аптечку, анализатор, датчик; скин и тип лечения аптечки), секбот (сигнализатор на шлем, сварка, датчик, рука, дубинка; откат отвёрткой/сваркой), генерал Бипски (ключ на сборку секбота, три энергомеча и четвёртый; игрушечные мечи — игрушечный; отвёртка снимает), ED-209 (лист железа на эндоскелет, две ноги, бронежилет, сварка, шлем СБ, датчик, кабель 4 с, дизейблер, отвёртка 4 с), хонкбот (рука на пустую коробку клоуна, датчик, велосипедный гудок), файрбот (рука на огнетушитель, красная каска, датчик); имя сборки переходит боту, умирающий секбот/ED-209/хонкбот оставляет свою сборку; нет гигиенобота (его сборка — рецепт из листов, нет меню крафта), ремботов, «Vim», переименования ручкой

### Клинбот
Ищет и убирает грязь, кровь, мусор; апгрейды (швабра, кислотный режим при емаге); форк учитывает русские названия должностей в «заказанных» целях.

- **Код:** [`/mob/living/basic/bot/cleanbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/cleanbot/cleanbot.dm#L3)
- **BandaStation:** [`cleanbot.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/cleanbot/cleanbot.dm#L287) — перевод названия должности
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/bots/ai.rs` (`cleanbot`), `cleanbot.rs`: ищет в 5 клетках грязь (кровь, граффити, мусор, вредителей по настройкам окна), достижимую за 15 шагов, моет 2 с, патрулирует через 5 с после уборки, отдаёт честь; нож (вне боевого режима 2,5 с) — бьёт в ногу наступившего (половина силы ножа, вся при емаге) и сбивает на 2 с, звания по ID (в т.ч. русские названия должностей BandaStation через `job_ru`), глава отдела делает его «командированным»; емаг — раз в 30 с брызгает плавиковой кислотой на человека в 5 клетках (непокрытые части тела, `acid_act(75, 10)`); нет пены (нет эффектов пены), порчи одежды кислотой, команд питомца, «дружбы» с уборщиком, швабры-апгрейда

### Медбот
Лечит раненых своими реагентами по типу урона, сообщает о критах по радио; емаг — травит.

- **Код:** [`/mob/living/basic/bot/medbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/medbot/medbot.dm#L3)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/bots/ai.rs` (`medbot`): лечит людей в 7 клетках (1 в стационарном режиме) с уроном своего типа выше порога (2.5 за 2 с, все типы у «продвинутых»), фразы ожидания/после лечения/у умирающего/холостые со своими записями, сообщает о критах по радио раз в 3 мин, емаг — хлоралгидрат; нет опрокидывания, улучшений из техвеба, шапок, оверлеев колёс и лица

### Секбот (Бипски и вариации) и ED-209
Арестует по статусу розыска и уровню угрозы: оглушает, надевает стяжки; ED-209 стреляет дизейблером; Бипски, Пингски, супер-Бипски; ED-209 нюкеров.

- **Код:** [`/mob/living/basic/bot/secbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/secbot/secbot.dm#L1), [`/mob/living/basic/bot/secbot/beepsky`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/secbot/secbot_subtypes.dm#L1), [`/mob/living/basic/bot/secbot/ed209`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/ed209/ed209.dm#L1), [`super_beepsky.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/secbot/super_beepsky.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/bots/ai.rs` (`secbot_tree`, `assess_threat`, `stun`, `cuff`), `sec.rs`: Бипски, Армски, Пингски, Офицер Бипски, Генески патрулируют/охраняют, цель — человек с угрозой выше 4 (розыск в записях, проверка ID, проверка оружия: `NEEDS_PERMIT` в руках +4, на поясе/спине +2 без доступа `weapons`; шляпа волшебника +2; емаг) или ударивший бота; оглушение 8 с (откат 10 с, звук перчаток), затем стяжки за 4 с, объявление ареста по радио СБ и звук `law`, «Level N infraction alert» с записью; подсечка наступившего, пока цель далеко; ED-209 стреляет дизейблером (лазером при емаге) раз в 3 с до 9 клеток, держит 2–3 клетки, упавшего (паралич или стамкрит) оглушает и вяжет стяжками; генерал Бипски рубит энергомечами (30), не арестовывает, режет наступившую цель; Бипски роняет картофельную батарейку; переключатели окна у ED-209 и хонкбота по tg; нет штрафов (`price_*` 0 у станционных), модификатора угрозы от трима ID и слагаемого за не-человеческий вид, насмешки пацифиста, ускорения от удара током, блока генерала, рывка ED-209 (умение разумного бота) и шерифских шляп

### Хонкбот
Клоунский секбот: сигналит, подскальзывает, хонкает нарушителей.

- **Код:** [`/mob/living/basic/bot/secbot/honkbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/honkbots/honkbot.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/bots/sec.rs` (`honkbot`), `ai.rs`: охотится как секбот (оценка угрозы, переключатели `slip_people`, `fake_cuff`, `check_ids`, `check_records`), оглушает воздушным гудком, вяжет фальшивыми стяжками, после ареста гудит и зловеще хохочет (при емаге — звуки `honkbot_e`); наступивший на него во время охоты подскальзывается (70%: сбит 6 с, паралич 3 с, «Хонк...»); празднует с клоунами (поиск 5 с, эмоция и гудок); нет затаскивания жертв на бананы (`honkbot_slip`), заикания, дрожи и оглушения звуком

### Файрбот
Тушит горящих и пожары пеной из огнетушителя.

- **Код:** [`/mob/living/basic/bot/firebot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/firebot/firebot.dm#L3)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/bots/ai.rs` (`firebot`): тушит горящих людей и очаги огня в 5 клетках (вода охлаждает воздух очага), объявляет пожар, холостые фразы; емаг поджигает людей; нет пены при жаре вокруг, анимации распыления, стационарного режима в окне кроме переключателя

### Гигиенобот
Моет грязных людей водой.

- **Код:** [`/mob/living/basic/bot/hygienebot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/hygienebot/hygienebot.dm#L3)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/bots/sec.rs` (`hygienebot`): ищет в 5 клетках человека для мытья (5 с), догоняет, с 5-й попытки ругается (раз в 4 с), 3 с поливает свою клетку и цель: вода тушит огонь, емаг — поджигает; фразы с записями, оверлеи пламени и струи; находит только при емаге (на одежде и мобах нет крови) и моет с соседней клетки (боты не проходят сквозь мобов); нет пены при смерти, шляп и сборки

### Ремонтный бот (repairbot)
Чинит пробоины: ставит плитки, решётки, окна, стены из своего запаса листов (окно `RepairBot`/`RepairbotResources`).

- **Код:** [`/mob/living/basic/bot/repairbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/repairbot/repairbot.dm#L2); tgui: [`RepairBot.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/RepairBot.tsx), [`RepairbotResources.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/RepairbotResources.tsx)
- **Статус в ss13-rs:** ❌ нет — картированный рембот только патрулирует и отдаёт честь (`crates/ss13_game/src/bots/ai.rs`)

### МУЛбот
Доставочный робот: возит ящики к маякам доставки, домой, по вызову из КПК; провода МУЛа, наезд на людей при емаге.

- **Код:** [`/mob/living/basic/bot/mulebot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/mulebot/mulebot.dm#L1), [`mulebot_delivery.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/mulebot/mulebot_delivery.dm#L1), [`/datum/wires/mulebot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/mulebot.dm#L5); tgui: [`Mule.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Mule.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/bots/mule.rs`, `ai.rs` (`mulebot`): «вперёд» — к маяку доставки (`codes_txt` `delivery`) с тегом назначения, «домой» — к домашнему (маяк под ним при загрузке карты); на маяке выгружает груз в его `dir` с докладом в канал снабжения или забирает ящик оттуда (автозабор), затем домой (автовозврат) или простой; погрузка перетаскиванием (ящик закрывается и едет на платформе, при перерезанном проводе проверки груза — что угодно; живой моб едет верхом и слезает шагом), провода МУЛа со случайными цветами (питание, обход препятствий, проверка груза, моторы — скорость, приём — ИИ, передатчик, маяк), батарея внутри (5 Дж за шаг, лом/вставка при открытой панели), переезд лежащих людей при емаге или перерезанном обходе (5–15 × веса зон) или притормаживание на 5 с, сбивание с ног при толчке с перерезанным обходом, окно `Mule` (питание, заряд, режим, груз, ID и дом через ввод, назначение из списка маяков, флаги); путь до 150 клеток (A* до 12000 узлов); нет GHOULbot-замены при загрузке, жужжания при застревании. Чужое: управление с КПК — нет программы управления ботами

### Вайббот и дедбот
Вайббот поднимает настроение светом и музыкой; дедбот — враждебный бот-заготовка.

- **Код:** [`/mob/living/basic/bot/vibebot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/vibebot/vibebot.dm#L1), [`/mob/living/basic/bot/dedbot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/bots/dedbot.dm#L6)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/bots/sec.rs` (`dedbot`): дедбот патрулирует, цели — живые не своей фракции в 2 клетках, кромсает всех вокруг (18 урона, раз в 0,5 с); вайббот только патрулирует (дерево базового бота): его вечеринки ищут людей в плохом настроении — нет системы настроения

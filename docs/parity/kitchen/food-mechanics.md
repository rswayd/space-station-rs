# Механика еды
Общие правила, по которым любая `/obj/item/food` съедается, режется, готовится и портится.

### Предмет еды и компонент edible
Базовый тип еды: при `Initialize` получает реагенты (`food_reagents`), объём, размер укуса, вкусы, тип пищи (`foodtypes`) и подключает компонент `edible`, элементы обработки, готовки и мусора.

- **Код:** [`/obj/item/food`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/_food.dm#L2), [`proc/make_edible()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/_food.dm#L130), [`/datum/component/edible`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/food/edible.dm#L17)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/food.rs` (`DmFood` для всех `/obj/item/food`, с `foodtypes`, `crafting_complexity`, данными гниения и сушки), `crates/ss13_game/src/food.rs`: нет кастомных материалов и сигналов компонента для других систем

### Поедание и кормление
Еда в руке, кликнутая на себя вне боевого режима, съедается по укусам (`eat_time`, сообщения о сытости, отказ при переедании); на другого — после 3 с, по одному укусу. Закрытый рот (маска) мешает.

- **Код:** [`proc/TryToEat()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/food/edible.dm#L374), [`proc/TakeBite()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/food/edible.dm#L488), [`proc/On_Consume()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/food/edible.dm#L704)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/food.rs` (`try_to_eat`, `take_bite`, `on_consume`): нет звуков, трейтов voracious/glutton, проверки крышки шлема (любая маска закрывает рот), еды голограмм

_«Вкус» описано в [../mobs/hunger-taste.md](../mobs/hunger-taste.md)._

### Мусор после еды (food_trash)
Доеденная еда оставляет `trash_type` (обёртку, палочку, тарелку) в руке едока.

- **Код:** [`/datum/element/food_trash`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/food/food_trash.dm#L2), [`proc/make_leave_trash()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/_food.dm#L215)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/food.rs` (`on_consume`, trash в руку)

### Нарезка и раскатка (processable)
Инструмент нужного поведения (нож, скалка) на еде на столе или подносе за время элемента превращает её в `amount` результатов, делящих реагенты (хлеб в ломтики, пицца в куски, тесто в лепёшку). При нескольких вариантах — радиальное меню.

- **Код:** [`/datum/element/processable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/food/processable.dm#L2), [`proc/make_processable()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/_food.dm#L192)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/food.rs` (`try_process`, `finish_process`), `crates/ss13_mapload/src/food.rs`: радиальное меню заменено выбором первого варианта

### Результат микроволновки (microwavable)
Еда в микроволновке становится своим `microwaved_type` (яйцо — варёное, донк-покет — тёплый), остальное — «неудачным блюдом» (`badrecipe`); результат может получить доп. реагенты.

- **Код:** [`/datum/element/microwavable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/food/microwavable.dm#L2), [`proc/make_microwaveable()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/_food.dm#L207), [`/obj/item/food/badrecipe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/misc.dm#L196)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/kitchen.rs` (`microwave_act`), `crates/ss13_mapload/src/food.rs`: результат `microwaved_type` с реагентами исходника, остальное — в неудачное блюдо; добавочные реагенты результата (омнизин тёплого донк-покета) — `food.rs` (`warm_bonus`)

### Типы пищи, любимая и нелюбимая еда
У еды есть `foodtypes` (мясо, овощи, жареное, токсичное...), у языка вида — списки любимого, нелюбимого и токсичного; от этого зависят мудлеты, рвота от токсичной еды и качество.

- **Код:** [`/datum/component/edible`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/food/edible.dm#L17) (проверка `foodtypes`, `get_perceived_food_quality`), [`/obj/item/organ/tongue`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/tongue/_tongue.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/food.rs` (`perceived_quality`, `check_liked`, `examine_lines`): `foodtypes` из `DmFood`, язык человека (любит JUNKFOOD|FRIED, не любит GROSS|RAW|CLOTH|BUGS|GORE, токсично TOXIC; без языка — вкус 0), раз в 5 с укусов — сообщения tg («Что это, черт побери, было?!», «Эта пища была ужасна...», «Это … пища.»), в осмотре — состав и как выглядит; нет мудлетов, отвращения (disgust) и рвоты, аллергий, видов кроме человека, `FRIED` от фритюра

### Качество блюда и баффы еды
Скрафченная еда имеет сложность рецепта; качество (с учётом вкусов едока) даёт мудлет от «отвратительно» до «фантастически», а сложные блюда — временные баффы.

- **Код:** [`proc/apply_buff()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/food/edible.dm#L586), [`proc/get_recipe_complexity()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/food/edible.dm#L648), [`/datum/element/quality_food_ingredient`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/quality_food_ingredient.dm#L2), [`/datum/element/love_food_buff`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/food/love_food_buff.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/food.rs`: качество = `crafting_complexity` у `Handmade` (результаты духовки, сковороды, микроволновки, сушилки, мороженого) ± вкусы, потолок 7, ярлыки `food_quality_description`; мудлеты `checkLiked` (`crate::mood::add_food_event`: `ceil(1 + 1.5 × качество)` на 5 мин × 2 × средняя чистота, при повторе — сильнейший и дольший; «gross_food» −2 и «toxic_food» −6 на 4 мин); бафф `haste` на первом укусе (`crates/ss13_game/src/buffs.rs`, см. ../mobs/status-effects-buffs.md); нет отвращения (disgust), аллергий, завтрака, личностей, крафта

### Микробы и гниение еды
Еда, полежавшая на полу (или взятая грязными руками), заражается и через время гниёт: превращается в плесневелую версию, может заразить болезнью.

- **Код:** [`/datum/component/germ_sensitive`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/food/germ_sensitive.dm#L14), [`/datum/component/decomposition`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/food/decomposition.dm#L8), [`proc/make_germ_sensitive()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/_food.dm#L224)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/decay.rs`, данные `crates/ss13_mapload/src/food.rs` (`decomposes`, `decomp_type`, `decomp_secs`, `decomp_req_handle`, `ant_attracting`): еда на открытом полу без стола/шкафа/кухонной машины 5 с — микробы; первый раз лишь «помечает» еду с карты (она защищена, пока не сдвинется), дальше идёт таймер (5/7/10 мин по RAW/GROSS или `decomposition_time`), по истечении — `decomp_type` с реагентами, муравьи, «gets overtaken by mold»; осмотр — «stale/gross/barely edible»; нет болезней с пола (`infective`), частиц вони; проверка раз в секунду

### Тайник в еде (food_storage)
В крупную еду (хлеб, торт) можно спрятать маленький предмет; его находят, откусив или разрезав.

- **Код:** [`/datum/component/food_storage`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/food_storage.dm#L5)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/food_storage.rs`: хлеб, торты, сыр (клин, круг, ломтик), котлеты; правый клик предметом не больше маленького (не еда, не хранилище) вне боевого режима — 1.5 с, один предмет; ctrl-клик (10 с, еду не тащит — `pull.rs`) вынимает в руки или на пол; каждый укус — шанс заметить (`bad − 50`%) или надкусить (`bad`%, `bitecount / (объём / укус) × 100`): острый с силой ≥5 — травма головы (≥15), −2 вдоха, 2 ед. крови в еду; ёмкость с реагентами отдаёт часть в желудок; иначе «something strange»; съеденная или нарезанная еда роняет предмет (в tg нарезка удаляет его вместе с хлебом — явная ошибка, не переносится); нет внедрения острого предмета, материалов (стекло, отвращение), проглатывания крошечных предметов, крика, подсказок

### Макание (dunkable)
Печенье и пончики можно обмакнуть в напиток: они впитывают часть реагентов из кружки.

- **Код:** [`/datum/element/dunkable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/food/dunkable.dm#L3)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/food.rs` (`dunk`): еда с `dunk_amount` (`AddElement(/datum/element/dunkable, n)` в `Initialize()` по цепочке типов: `crates/ss13_dm_extract` сканирует элемент, `crates/ss13_mapload/src/food.rs`) на ёмкости с `DUNKABLE` впитывает до `n` ед. (сообщения «empty»/«full», не сливаемая — отказ); сообщение называет еду и ёмкость (в tg — ёмкость дважды)

### Шипение горячей еды (sizzle)
Только что приготовленная на гриле или в сковороде еда некоторое время шипит (визуальный и звуковой эффект).

- **Код:** [`/datum/component/sizzle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/sizzle.dm#L1)
- **Статус в ss13-rs:** ✅ — `crates/ss13_game/src/grill.rs` (`Sizzle`, `sizzle`): в tg это метки гриля от угольного гриля, а не эффект сковороды; еда на гриле получает наложение `grillmarks` (`icons/obj/machines/kitchen.dmi`) и хранит время жарки, с которым продолжает на гриле; нет вырезания меток по форме спрайта еды (смешивание иконок)

### Еда для призраков
Отдельные блюда (хеллоуинские) призрак может «есть»: предмет постепенно исчезает под действием наблюдателей.

- **Код:** [`/datum/component/ghost_edible`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/food/ghost_edible.dm#L5)
- **Статус в ss13-rs:** ❌ нет

### Обезьяний кубик
Кубик, политый водой или съеденный, раскрывается в живую обезьяну (у едока — из груди); есть варианты других видов.

- **Код:** [`/obj/item/food/monkeycube`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/monkeycube.dm#L1), [`proc/Expand()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/monkeycube.dm#L25)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/food.rs` (`expand_cube`): кубик, облитый водой (выплеск, душ, вода внутрь шприцем), превращается в мартышку с ИИ на месте (потолок 64 обезьян из кубиков); кубики в консоль ксенобиологии — `science/xenobio_console.rs`. Нет: съеденный кубик не вырывается из едока, прочие кубики (горилла, курица, пчела и др.) не раскрываются

### Наживка для рыбалки
Червяки и наживка (`bait.dm`) — еда, используемая как приманка на удочке; сама рыбалка — см. ../service/.

- **Код:** [`/obj/item/food/bait`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/food/bait.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/fishing/server.rs`: любая еда — наживка (`TRAIT_FISHING_BAIT`), у подтипов `/obj/item/food/bait` своё качество (`bait_quality`), наживка тратится при поклёвке; сама рыбалка — ../service/

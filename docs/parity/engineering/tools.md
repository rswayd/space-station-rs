# Инструменты
Шесть базовых инструментов (`tool_behaviour`), таймерные действия `do_after`, сварка с топливом, силовые и особые инструменты, краскопульты, огнетушитель, индуктор.

### Инструментальные действия и do_after
Предмет с `tool_behaviour` вне боевого режима сначала вызывает `tool_act` цели; долгие шаги — `do_after` (задержка × `toolspeed`), прерывается движением, сменой предмета или цели; повторная проверка состояния в конце.

- **Код:** [`proc/use_tool()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items.dm#L1216), [`proc/tool_use_check()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items.dm#L1269), [`proc/do_after()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/timed_actions.dm#L211)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/tools.rs` (`DoAfter`, `use_tool`), `crates/ss13_game/src/construction.rs`: шесть поведений, `toolspeed`, проверки в конце; нет полосы прогресса, `extra_checks` и прочих поведений (анализатор, кирка, пила, ...)

### Лом
Поддевает плитку, вскрывает обесточенные двери, разбирает окна/шлюзы; большой и аварийный ломы, абдукторский, лом-молот.

- **Код:** [`/obj/item/crowbar`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/crowbar.dm#L1), [`/obj/item/crowbar/large`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/crowbar.dm#L57), [`/obj/item/crowbar/abductor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/crowbar.dm#L47)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/tools.rs`, `crates/ss13_game/src/construction.rs`: плитка и окна; дверей не вскрывает

### Отвёртка
Открывает панели машин, крепит окна и решётки, разбирает балки; электроотвёртка, абдукторская.

- **Код:** [`/obj/item/screwdriver`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/screwdriver.dm#L1), [`/obj/item/screwdriver/power`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/screwdriver.dm#L85)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/construction.rs`, `crates/ss13_game/src/wires.rs`: панели шлюзов, APC и автоматов, окна, решётки, балки

### Гаечный ключ
Закрепляет и откручивает машины и структуры (`default_unfasten_wrench`), разбирает окна; медицинский, боевой, болтовёрт.

- **Код:** [`/obj/item/wrench`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/wrench.dm#L1), [`/obj/item/wrench/combat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/wrench.dm#L97), [`/obj/item/wrench/bolter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/wrench.dm#L134)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/construction.rs`: балки, окна, торговые автоматы; другие машины не откручиваются

### Кусачки
Режут провода, решётки, кабели, наручники-стяжки; абдукторские.

- **Код:** [`/obj/item/wirecutters`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/wirecutters.dm#L1), [`/obj/item/wirecutters/abductor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/wirecutters.dm#L67)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/wires.rs`, `crates/ss13_game/src/construction.rs`: провода и решётки; кабели не режутся

### Сварочный аппарат
Включается в руке, жжёт топливо (1 ед./5 с, расход на работу), нагревает до 1500+ K: режет стены, заваривает двери, чинит окна и роботов, поджигает плазму; заправка из бака топлива; без защиты глаз повреждает зрение; варианты: большой, мини, экспериментальный (самозаправка), промышленный.

- **Код:** [`/obj/item/weldingtool`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/weldingtool.dm#L1), [`proc/switched_on()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/weldingtool.dm#L249), [`use()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/weldingtool.dm#L215), [`proc/try_heal_loop()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/weldingtool.dm#L162), [`/obj/item/weldingtool/experimental`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/weldingtool.dm#L403)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/tools.rs`, `crates/ss13_game/src/fire.rs`: включение кликом по слоту, расход топлива, урон 15 ожогом, поджог газа, резка стен; нет заправки, вреда глазам, ремонта роботов, самозаправки

### Мультитул
Пульсирует провода, хранит буфер для связывания машин (`multitool_act`), показывает сведения о сети; абдукторский (видит назначение проводов), ИИ-детектор.

- **Код:** [`/obj/item/multitool`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/multitool.dm#L13)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/wires.rs`: пульс проводов; нет буфера и связывания машин

### Силовые инструменты
Челюсти жизни (лом/кусачки, вскрывают запитанные шлюзы), электродрель (отвёртка/ключ), складной космический нож-мультиинструмент.

- **Код:** [`/obj/item/crowbar/power`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/crowbar.dm#L115), [`/obj/item/screwdriver/power`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/screwdriver.dm#L85), [`/obj/item/spess_knife`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/spess_knife.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Щётка от ржавчины
Счищает ржавчину со стен и полов.

- **Код:** [`/obj/item/wirebrush`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/wirebrush.dm#L5)
- **Статус в ss13-rs:** ❌ нет

### Краскопульты декалей и труб
Краскопульт декалей рисует разметку на полу (tgui с выбором цвета и направления), краскопульт труб перекрашивает трубы.

- **Код:** [`/obj/item/airlock_painter/decal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/painter/decal_painter.dm#L1), [`paintable_decals.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/engineering/painter/paintable_decals.dm), [`/obj/item/pipe_painter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/pipe_painter.dm#L1); tgui: [`DecalPainter.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/DecalPainter.tsx)
- **Статус в ss13-rs:** ❌ нет

### Огнетушитель
Распыляет воду/пену по конусу, тушит огонь и людей, толкает стрелка в невесомости; заправка из бака; мини, самодельный, продвинутый.

- **Код:** [`/obj/item/extinguisher`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/extinguisher.dm#L1), [`/obj/item/extinguisher/advanced`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/extinguisher.dm#L193)
- **Статус в ss13-rs:** ❌ нет

### Индуктор
Заряжает батареи APC, мехов и боргов на расстоянии от своей батареи.

- **Код:** [`/obj/item/inducer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/inducer.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Т-сканер
Показывает трубы и кабели под полом вокруг владельца.

- **Код:** [`/obj/item/t_scanner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/scanners/t_scanner.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Инструменты для кражи ядра
Специальная отвёртка и контейнер для извлечения плутониевого ядра из ядерной бомбы (цель предателя).

- **Код:** [`/obj/item/screwdriver/nuke`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/theft_tools.dm#L88), [`/obj/item/nuke_core_container`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/tools/theft_tools.dm#L43)
- **Статус в ss13-rs:** ❌ нет

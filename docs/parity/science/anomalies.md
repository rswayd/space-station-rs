# Аномалии

Аномалии на станции, их ядра, нейтрализация сигналером, искусственное получение ядер в рефайнери и применение ядер.

### Базовая аномалия и нейтрализация
`/obj/effect/anomaly` — движущийся по станции эффект с таймером: по истечении детонирует; сигналер, настроенный на частоту/код ядра (видны анализатором), нейтрализует её и оставляет ядро. Неисследованные ядра ограничены числом на раунд.

- **Код:** [`/obj/effect/anomaly`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/_anomalies.dm#L2), [`proc/anomalyEffect()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/_anomalies.dm#L80), [`proc/detonate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/_anomalies.dm#L90), [`proc/anomalyNeutralize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/_anomalies.dm#L100)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/anomalies.rs` `Anomaly`: дрейф 45%/с, `anomalyEffect` раз в 2 с (`SSobj`), детонация через 120 с; ядро-сигналер со случайной свободной частотой (нечётной) и кодом, анализатор показывает их на аномалии и ядре (`analyzer_act`), сигнал с этими частотой и кодом (`assemblies::server::signal`, с учётом дальности) нейтрализует её (`anomalyNeutralize`): выпадает ядро её типа на той же частоте, пока не исчерпан лимит раунда `MAX_CORES_*` (`CoresMade`), иначе инертное ядро с предупреждением; стабилизация (`stabilize`: `immortal`, якорь, «hollow» без ядра) — для выпускателя; нет дыма при нейтрализации, отсчёта и света

_«События появления аномалий» описано в [../events/anomaly-events.md](../events/anomaly-events.md)._

### Пирокластическая аномалия
Выбрасывает горячую плазму и поджигает окружение; при детонации — слайм-пиромонстр.

- **Код:** [`/obj/effect/anomaly/pyro`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_pyroclastic.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/anomalies.rs`: 5 моль O2 и плазмы при 1000 K каждые 10 с, при детонации по 500 моль; нет пирослайма (слаймы есть — `crate::science::xenobio`, нет опроса призраков)

### Гравитационная аномалия
Притягивает и швыряет предметы и людей; высокогравитационный вариант меняет гравитацию вокруг.

- **Код:** [`/obj/effect/anomaly/grav`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_gravity.dm#L2), [`grav/high`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_gravity.dm#L59)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/anomalies.rs`: тянет незакреплённые объекты и мобов в радиусе 4 на шаг, сбивает с ног (4 с) вставшего на неё, бросает предметы со своего тайла в живого в радиусе 4 (20%), звук при детонации; мобов не отбрасывает (броска мобов нет), нет магбутсов, нет High Intensity

### Флюкс-аномалия
Бьёт током при касании, при детонации взрывается.

- **Код:** [`/obj/effect/anomaly/flux`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_flux.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/anomalies.rs`: плотная, после каждого процесса бьёт током 20 (без учёта перчаток) первого коснувшегося (столкновение, шаг на неё, врезавшись при дрейфе), детонация — `empulse(4, 16)` и взрыв 0/1/4 со вспышкой 6; нет варианта `big`/`minor` (не используются событиями)

### Блюспейс-аномалия
Телепортирует касающихся; при детонации — массовая телепортация по станции.

- **Код:** [`/obj/effect/anomaly/bluespace`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_bluespace.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/anomalies.rs`: телепорт живых в радиусе 1 на до 4 тайлов, врезавшегося — до 8; детонация переносит всё незакреплённое в радиусе 12 от случайного тайла зоны к случайному маяку (`/obj/item/beacon`) со сдвигом, с объявлением; маяки только на уровнях станции, нет вспышки «bluespace sparkle»

### Вихревая аномалия (чёрная дыра)
Засасывает и уничтожает объекты, рушит конструкции вокруг.

- **Код:** [`/obj/effect/anomaly/bhole`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_vortex.dm#L2)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/anomalies.rs` (`vortex_effect`): каждые 2 с кольцо радиуса 0–3 — 50% подтянуть незакреплённое и мобов на шаг, 25% тайлов в очередь взрыва (тяжёлый/опустошающий, `explosions::queue_turfs`); незакреплённые объекты в радиусе 2 бросаются в ближайшего живого в радиусе 4, закреплённые — под тяжёлый взрыв; детонация — звук `far_noise`; событие Vortex (вес 10, мин. 20 игроков, до 2 раз); нет визуальной волны `circle_wave`

### Биоскрамблер
Заменяет конечности людей рядом на чужие.

- **Код:** [`/obj/effect/anomaly/bioscrambler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_bioscrambler.dm#L2)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/anomalies/exotic.rs` (`bioscrambler_move`, `bioscrambler_effect`, `bioscramble`): живёт 240 с, преследует ближайшего игрока-карбона в сознании на своём уровне сквозь всё (80% держит цель, новая цель получает строку о кровотечении из носа), раз в 10 с импульс со звуком в радиусе 2: органическому карбону без гена-запрета и без полной био-брони заменяет один орган на случайный орган другого вида (старый удаляется; роботизированный или отсутствующая часть тела — «аугментированное тело защищает»), детонация — ещё один импульс; событие Bioscrambler (вес 20, мин. 10 игроков, до 5 раз); ядро и лимит 8. Замена конечностей перенесена в группу частей тела 🟡: у конечности нет собственного вида (`HumanLook` рисует один вид на всё тело); пул органов — модельные слоты портированных видов; кровотечение — только строка

### Галлюциногенная аномалия
Вызывает галлюцинации у находящихся рядом; есть ложные копии.

- **Код:** [`/obj/effect/anomaly/hallucination`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_hallucination.dm#L2)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/anomalies.rs`: импульс галлюцинаций (радиус 5, 50 с, до 300 с) каждые 5 с, при детонации радиус 15, с фразами; 35% соседних тайлов — ложные копии без ядра, которые только дрейфуют и выдают анализатору бессмыслицу; событие Hallucination (вес 20, до 5 раз); импульс без проверки прямой видимости (`visible_hallucination_pulse`), нет волнистого фильтра

### Пространственная (dimensional) аномалия
Перекрашивает и трансмутирует тайлы вокруг в одну из тем (золото, плазма, клоун, ...).

- **Код:** [`/obj/effect/anomaly/dimensional`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_dimensional.dm#L2), [`/datum/dimension_theme`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_dimensional_themes.dm#L4)
- **BandaStation:** [`anomalies_dimensional_themes.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_dimensional_themes.dm#L196) — цвет оверлея краёв окон
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/anomalies/exotic.rs` (`THEMES`, `prepare_area`, `dimensional_effect`, `apply_theme`): не дрейфует, 18 тем (золото, плазма, клоун, радиоактивная, мясо, пицца, природа, бамбук, зима, зимняя хижина, лаваленд, космос, стекло, роскошь, диско, джунгли, пришельцы, бронза); каждый процесс превращает следующий тайл спирали радиуса 3 (полы и стены по весам темы с сохранением воздуха и baseturfs, стулья/столы/туалеты/платформы/решётки, полноразмерные окна заменяются или перекрашиваются, случайная флора), материальные темы перекрашивают тайлы и объекты в цвет материала; без тайлов — перемещение в новую зону с объявлением (1–4 раза), затем исчезает (бессмертная остаётся); событие Dimensional (вес 20, мин. 10, до 5); нет замены шлюзов (копирование доступа), мобов из тем, оверлея темы и вспышки; ковёр «роскоши» — на зону, а не раз в 90 с

### Эктоплазменная аномалия
Сила зависит от числа призраков, которые «вкладываются» в неё; при детонации — портал призраков.

- **Код:** [`/obj/effect/anomaly/ectoplasm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_ectoplasm.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/anomalies/exotic.rs` (`ectoplasm_effect`, `ectoplasm_detonate`, `GhostPortal`): не дрейфует, живёт 122 с, сила — доля «орбитирующих» среди мёртвых игроков и наблюдателей (спрайт `ectoplasm_heavy` от 50%), админский оверрайд силы; детонация от 10% оскверняет тайлы в радиусе `орбитирующие + 3` (кровь 5%, эктоплазма 10%, пыль 10%, 20% сорванная плитка со сломанным и обожжённым полом, окна получают 60–90 урона), от 50% — портал призраков на 2 минуты со звуками и объявлением; событие Ectoplasmic Outburst (вес 4, мин. 30, до 2). Чужие части 🟡: орбиты призраков нет (считаются наблюдатели на её тайле), опрос призрачного роя (polling), `haunt_outburst` (одержимые предметы), revblight (болезни), deadchat-управление

### Погодная аномалия
Создаёт локальную погоду (дождь, гроза с молниями).

- **Код:** [`/obj/effect/anomaly/weather`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_weather.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Ядра аномалий
Ядро — сигналер особого типа (10 видов): используется в рефракторах/модулях MOD, оружии и броне; может быть активировано сигналом.

- **Код:** [`/obj/item/assembly/signaler/anomaly`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/anomaly/anomaly_core.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/anomalies.rs` (`neutralize`, `analyzer_act`): ядро — сигналер `/obj/item/assembly/signaler/anomaly/<тип>` на частоте и коде своей аномалии, анализатор показывает их; лимит ядер на раунд; нет собственных эффектов ядер при сигнале (`signal()`: огонь, притяжение, разряд, телепорт, вихрь), рефракторов, модулей и оружия с ядрами

### Сырые ядра и рефайнери аномалий
Сырое ядро (заказывается в карго) помещается в рефайнери вместе с ТТВ-бомбой; при достаточном радиусе взрыва получается готовое ядро.

- **Код:** [`/obj/item/raw_anomaly_core`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/anomaly/raw_anomaly.dm#L9), [`/obj/machinery/research/anomaly_refinery`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/anomaly/anomaly_refinery.dm#L13), [`start_test()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/anomaly/anomaly_refinery.dm#L149); tgui: [`AnomalyRefinery.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AnomalyRefinery.jsx)
- **Статус в ss13-rs:** ❌ нет

### Нейтрализатор и выпускатель аномалий
Одноразовый нейтрализатор мгновенно гасит аномалию; выпускатель высвобождает аномалию из ядра.

- **Код:** [`/obj/item/anomaly_neutralizer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/anomaly_neutralizer.dm#L1), [`/obj/item/anomaly_releaser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/anomaly_releaser.dm#L2)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/anomalies.rs` (`neutralizer_act`, `releaser_act`, `complete_release`, `stabilize`): нейтрализатор при ударе по аномалии вызывает `anomalyNeutralize` (ядро с учётом лимита раунда), искрит и исчезает, ложную копию галлюциногенной не трогает; выпускатель на ядре (на полу/столе) после `do_after` 3 с выпускает на его тайле аномалию его типа, стабилизированную (`immortal`, «stable», со свежим ядром), ядро уходит, выпускатель становится «used» и больше не работает; вариант `infinite` (не используется) и закрытие разломов еретика нейтрализатором не перенесены; ядро в другой руке — не цель (`do_after` по тайлу)

### Модули с ядром аномалии
Компонент для модулей (MOD, импланты), которые работают только со вставленным ядром нужного типа (кинезис, антиграв, телепорт).

- **Код:** [`/datum/component/anomaly_locked_module`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/anomalock_module.dm#L1), [`/obj/item/mod/module/anomaly_locked`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/modules/_module.dm#L474)
- **Статус в ss13-rs:** ❌ нет

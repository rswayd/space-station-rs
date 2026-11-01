# Аномалии

Аномалии на станции, их ядра, нейтрализация сигналером, искусственное получение ядер в рефайнери и применение ядер.

### Базовая аномалия и нейтрализация
`/obj/effect/anomaly` — движущийся по станции эффект с таймером: по истечении детонирует; сигналер, настроенный на частоту/код ядра (видны анализатором), нейтрализует её и оставляет ядро. Неисследованные ядра ограничены числом на раунд.

- **Код:** [`/obj/effect/anomaly`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/_anomalies.dm#L2), [`proc/anomalyEffect()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/_anomalies.dm#L80), [`proc/detonate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/_anomalies.dm#L90), [`proc/anomalyNeutralize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/_anomalies.dm#L100)
- **Статус в ss13-rs:** ❌ нет

_«События появления аномалий» описано в [../events/anomaly-events.md](../events/anomaly-events.md)._

### Пирокластическая аномалия
Выбрасывает горячую плазму и поджигает окружение; при детонации — слайм-пиромонстр.

- **Код:** [`/obj/effect/anomaly/pyro`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_pyroclastic.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Гравитационная аномалия
Притягивает и швыряет предметы и людей; высокогравитационный вариант меняет гравитацию вокруг.

- **Код:** [`/obj/effect/anomaly/grav`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_gravity.dm#L2), [`grav/high`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_gravity.dm#L59)
- **Статус в ss13-rs:** ❌ нет

### Флюкс-аномалия
Бьёт током при касании, при детонации взрывается.

- **Код:** [`/obj/effect/anomaly/flux`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_flux.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Блюспейс-аномалия
Телепортирует касающихся; при детонации — массовая телепортация по станции.

- **Код:** [`/obj/effect/anomaly/bluespace`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_bluespace.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Вихревая аномалия (чёрная дыра)
Засасывает и уничтожает объекты, рушит конструкции вокруг.

- **Код:** [`/obj/effect/anomaly/bhole`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_vortex.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Биоскрамблер
Заменяет конечности людей рядом на чужие.

- **Код:** [`/obj/effect/anomaly/bioscrambler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_bioscrambler.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Галлюциногенная аномалия
Вызывает галлюцинации у находящихся рядом; есть ложные копии.

- **Код:** [`/obj/effect/anomaly/hallucination`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_hallucination.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Пространственная (dimensional) аномалия
Перекрашивает и трансмутирует тайлы вокруг в одну из тем (золото, плазма, клоун, ...).

- **Код:** [`/obj/effect/anomaly/dimensional`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_dimensional.dm#L2), [`/datum/dimension_theme`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_dimensional_themes.dm#L4)
- **BandaStation:** [`anomalies_dimensional_themes.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_dimensional_themes.dm#L196) — цвет оверлея краёв окон
- **Статус в ss13-rs:** ❌ нет

### Эктоплазменная аномалия
Сила зависит от числа призраков, которые «вкладываются» в неё; при детонации — портал призраков.

- **Код:** [`/obj/effect/anomaly/ectoplasm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_ectoplasm.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Погодная аномалия
Создаёт локальную погоду (дождь, гроза с молниями).

- **Код:** [`/obj/effect/anomaly/weather`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/anomalies/anomalies_weather.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Ядра аномалий
Ядро — сигналер особого типа (10 видов): используется в рефракторах/модулях MOD, оружии и броне; может быть активировано сигналом.

- **Код:** [`/obj/item/assembly/signaler/anomaly`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/anomaly/anomaly_core.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Сырые ядра и рефайнери аномалий
Сырое ядро (заказывается в карго) помещается в рефайнери вместе с ТТВ-бомбой; при достаточном радиусе взрыва получается готовое ядро.

- **Код:** [`/obj/item/raw_anomaly_core`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/anomaly/raw_anomaly.dm#L9), [`/obj/machinery/research/anomaly_refinery`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/anomaly/anomaly_refinery.dm#L13), [`start_test()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/anomaly/anomaly_refinery.dm#L149); tgui: [`AnomalyRefinery.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AnomalyRefinery.jsx)
- **Статус в ss13-rs:** ❌ нет

### Нейтрализатор и выпускатель аномалий
Одноразовый нейтрализатор мгновенно гасит аномалию; выпускатель высвобождает аномалию из ядра.

- **Код:** [`/obj/item/anomaly_neutralizer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/anomaly_neutralizer.dm#L1), [`/obj/item/anomaly_releaser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/anomaly_releaser.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Модули с ядром аномалии
Компонент для модулей (MOD, импланты), которые работают только со вставленным ядром нужного типа (кинезис, антиграв, телепорт).

- **Код:** [`/datum/component/anomaly_locked_module`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/anomalock_module.dm#L1), [`/obj/item/mod/module/anomaly_locked`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/modules/_module.dm#L474)
- **Статус в ss13-rs:** ❌ нет

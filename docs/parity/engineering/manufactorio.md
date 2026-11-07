# Мануфакторио (производственные линии)
Набор машин `code/modules/manufactorio` для автоматических производственных цепочек: машины передают ресурсы друг другу через стороны ввода/вывода и потребляют питание.

### Основа производственных машин
Общий тип: стороны ввода и вывода, передача ресурса соседней машине или на пол, слияние стаков, оверлеи направлений, питание от кабеля.

- **Код:** [`/obj/machinery/power/manufacturing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/_manufacturing.dm#L6), [`proc/send_resource()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/_manufacturing.dm#L91), [`proc/receive_resource()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/_manufacturing.dm#L87)
- **Статус в ss13-rs:** ✅ `crates/ss13_game/src/eng_machines/manufactorio.rs` (`send_resource`, `receive_resource`, слияние стаков, узел на кабеле, ключ, поворот alt-кликом, тик 1 с; семейство `machine_frame::Family::Manufacturing`). Приём с конвейера (`Bumped`) — вместе с конвейерными лентами ([recycling.md](recycling.md)); оверлеев ввода/вывода нет

### Дробилка и плавильня
Дробилка измельчает предметы и валуны, накапливая материал меньше листа; плавильня переплавляет руду и обломки в листы.

- **Код:** [`/obj/machinery/power/manufacturing/crusher`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/machines/crusher.dm#L1), [`/obj/machinery/power/manufacturing/smelter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/machines/smelter.dm#L1)
- **Статус в ss13-rs:** 🟡 плавильня: руда → листы по 5 за тик, выход вперёд; дробилка: материалы стаков (`ResearchDb::stacks`) в листы с накоплением долей. Нет: `custom_materials` не-стаков (не извлекаются), валунов (горное дело), сжигания проходящих через плавильню предметов (`fire_act`), дробления мобов

### Производственный токарный станок и крафтер
Станок (урезанный автолат) печатает заданный рецепт, пока есть материалы; крафтер собирает предметы по рецептам крафта из поступающих компонентов (вариант-повар).

- **Код:** [`/obj/machinery/power/manufacturing/lathe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/machines/lathe.dm#L1), [`/obj/machinery/power/manufacturing/crafter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/machines/crafter.dm#L1), [`/obj/machinery/power/manufacturing/crafter/cooker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/machines/crafter.dm#L120)
- **Статус в ss13-rs:** 🟡 станок ✅ (`eng_machines/manufactorio.rs`: листы сзади в хранилище по 50 листов на уровень матрицы, мультитул выбирает дизайн автолата, печать за `(construction_time * lathe_time_factor) ** 0.8` дс при 5 кВт, выход вперёд). Крафтер и повар — вместе с меню крафта (рецептов крафта в ss13-rs нет)

### Маршрутизатор и сортировщик
Маршрутизатор делит поток поровну на 3 выхода (выходы переключаются мультитулом), сортировщик отправляет предметы по фильтрам (tgui).

- **Код:** [`/obj/machinery/power/manufacturing/router`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/machines/router.dm#L1), [`/obj/machinery/power/manufacturing/sorter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/machines/sorter.dm#L1), [`/datum/sortrouter_filter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/machines/sorter_filters.dm#L1); tgui: [`ManufacturingSorter.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ManufacturingSorter.tsx)
- **Статус в ss13-rs:** ✅ маршрутизатор (по очереди на 3 стороны, стаки по одному, мультитул переключает выход); сортировщик по фильтрам с окном `ManufacturingSorter` (новый фильтр из списка, стороны, порядок, удаление, правка текста, фильтр по предмету ударом). Нет фильтра `is_tagged` — вместе с метками посылок

### Хранилище и разгрузчик
Ящик-накопитель на линии и разгрузчик, вынимающий содержимое ящиков и контейнеров в линию.

- **Код:** [`/obj/machinery/power/manufacturing/storagebox`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/machines/storagebox.dm#L1), [`/obj/machinery/power/manufacturing/unloader`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/machines/unloader.dm#L1)
- **Статус в ss13-rs:** ✅ хранилище (16 мест, сброс рукой 1.25 с, отвёртка 5 с → 10 листов железа) и разгрузчик (ящик или рудный ящик сзади, содержимое вперёд, пустой ящик вбок и открывается; мультитул переворачивает сторону). Рецепт крафта хранилища — вместе с крафтом

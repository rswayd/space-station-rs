# Мануфакторио (производственные линии)
Набор машин `code/modules/manufactorio` для автоматических производственных цепочек: машины передают ресурсы друг другу через стороны ввода/вывода и потребляют питание.

### Основа производственных машин
Общий тип: стороны ввода и вывода, передача ресурса соседней машине или на пол, слияние стаков, оверлеи направлений, питание от кабеля.

- **Код:** [`/obj/machinery/power/manufacturing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/_manufacturing.dm#L6), [`proc/send_resource()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/_manufacturing.dm#L91), [`proc/receive_resource()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/_manufacturing.dm#L87)
- **Статус в ss13-rs:** ❌ нет

### Дробилка и плавильня
Дробилка измельчает предметы и валуны, накапливая материал меньше листа; плавильня переплавляет руду и обломки в листы.

- **Код:** [`/obj/machinery/power/manufacturing/crusher`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/machines/crusher.dm#L1), [`/obj/machinery/power/manufacturing/smelter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/machines/smelter.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Производственный токарный станок и крафтер
Станок (урезанный автолат) печатает заданный рецепт, пока есть материалы; крафтер собирает предметы по рецептам крафта из поступающих компонентов (вариант-повар).

- **Код:** [`/obj/machinery/power/manufacturing/lathe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/machines/lathe.dm#L1), [`/obj/machinery/power/manufacturing/crafter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/machines/crafter.dm#L1), [`/obj/machinery/power/manufacturing/crafter/cooker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/machines/crafter.dm#L120)
- **Статус в ss13-rs:** ❌ нет

### Маршрутизатор и сортировщик
Маршрутизатор делит поток поровну на 3 выхода (выходы переключаются мультитулом), сортировщик отправляет предметы по фильтрам (tgui).

- **Код:** [`/obj/machinery/power/manufacturing/router`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/machines/router.dm#L1), [`/obj/machinery/power/manufacturing/sorter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/machines/sorter.dm#L1), [`/datum/sortrouter_filter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/machines/sorter_filters.dm#L1); tgui: [`ManufacturingSorter.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ManufacturingSorter.tsx)
- **Статус в ss13-rs:** ❌ нет

### Хранилище и разгрузчик
Ящик-накопитель на линии и разгрузчик, вынимающий содержимое ящиков и контейнеров в линию.

- **Код:** [`/obj/machinery/power/manufacturing/storagebox`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/machines/storagebox.dm#L1), [`/obj/machinery/power/manufacturing/unloader`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/manufactorio/machines/unloader.dm#L1)
- **Статус в ss13-rs:** ❌ нет

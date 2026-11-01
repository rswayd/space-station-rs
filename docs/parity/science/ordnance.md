# Ордонанс и научные публикации

Отдел ордонанса: испытания взрывчатки и газовых смесей, запись результатов и публикация научных работ партнёрам за очки и кредиты.

### Доплеровский массив
Фиксирует взрывы на станции/полигоне (радиус, эпицентр), сохраняет записи на диск, печатает их и засчитывает эксперименты ордонанса.

- **Код:** [`/obj/machinery/doppler_array`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/ordnance/doppler_array.dm#L1), [`proc/sense_explosion()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/ordnance/doppler_array.dm#L148); tgui: [`DopplerArray.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/DopplerArray.jsx)
- **Статус в ss13-rs:** ❌ нет

### Компрессор баков
Атмос-машина: сжимает газ в баке до разрыва, записывает состав утечки; используется для газовых экспериментов.

- **Код:** [`/obj/machinery/atmospherics/components/binary/tank_compressor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/ordnance/tank_compressor.dm#L5); tgui: [`TankCompressor.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/TankCompressor.tsx)
- **Статус в ss13-rs:** ❌ нет

### Эксперименты ордонанса
Эксперименты техвеба на взрыв заданной силы и на газовые смеси; результаты записей доплера/компрессора закрывают их.

- **Код:** [`/datum/experiment/ordnance`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/types/ordnance.dm#L6), [`ordnance/explosive`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/types/ordnance.dm#L31), [`ordnance/gaseous`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/types/ordnance.dm#L46)
- **Статус в ss13-rs:** ❌ нет

### Научные статьи и партнёры
Записи экспериментов публикуются через программу NT Frontier (ПДА/консоль) как статьи (взрывные/газовые) для партнёров (шахта, оборона, медицина, физика и др.); партнёры дают очки, кредиты и открывают узлы.

- **Код:** [`/datum/scientific_paper`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/ordnance/_scipaper.dm#L2), [`/datum/scientific_partner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/ordnance/_scipaper.dm#L272), [`партнёры`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/ordnance/scipaper_partner.dm#L1), [`add_scientific_paper()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/techweb/_techweb.dm#L505), [`/datum/computer_file/program/scipaper_program`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/modular_computers/file_system/programs/frontier.dm#L1); tgui: [`NtosScipaper.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/NtosScipaper.jsx)
- **Статус в ss13-rs:** ❌ нет

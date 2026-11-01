# Экспериментальная наука (experisci)

Система экспериментов техвеба: сканирование предметов, людей, растений, реагентов, машин; результаты дают очки и скидки на узлы.

### Обработчик экспериментов и выбор эксперимента
Компонент `experiment_handler` на сканерах и машинах: привязка к техвебу, выбор активного эксперимента в окне настройки, запуск при сканировании.

- **Код:** [`/datum/component/experiment_handler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/handlers/experiment_handler.dm#L8), [`try_run_handheld_experiment()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/handlers/experiment_handler.dm#L82), [`/datum/experiment`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/types/experiment.dm#L9); tgui: [`ExperimentConfigure.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ExperimentConfigure.tsx)
- **Статус в ss13-rs:** ❌ нет

### Ручной экспериментальный сканер
Предмет учёных: сканирует цели для активного эксперимента.

- **Код:** [`/obj/item/experi_scanner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/handheld_scanner.dm#L6)
- **Статус в ss13-rs:** ❌ нет

### Деструктивный сканер
Машина-камера: закрывается над предметами/существами и сканирует их с уничтожением (для деструктивных экспериментов).

- **Код:** [`/obj/machinery/destructive_scanner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/destructive_scanner.dm#L6), [`proc/activate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/destructive_scanner.dm#L30)
- **Статус в ss13-rs:** ❌ нет

### Эксперименты сканирования
Семейство: сканирование заданных предметов, случайных материалов, растений с мутациями, людей с мутациями/имплантами, реагентов, машин по тиру деталей, рыбы, выращенных в чане клеток, слаймов (очки), аномалий.

- **Код:** [`/datum/experiment/scanning`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/types/scanning.dm#L9), [`scanning/points`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/types/scanning_points.dm#L1), [`scanning/people`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/types/scanning_people.dm#L2), [`scanning/random/plants`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/types/scanning_plants.dm#L1), [`scanning/reagent`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/types/scanning_reagent.dm#L2), [`machinery_tiered_scan`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/types/scanning_machinery.dm#L4), [`scanning/fish`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/types/scanning_fish.dm#L9), [`scanning/cytology`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/types/scanning_vatgrown.dm#L1), список: [`experiments.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/experiments.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Физические эксперименты и вскрытие
Эксперименты по событиям (взрыв мясной стены, победа в аркаде) и вскрытие людей/нелюдей хирургией.

- **Код:** [`/datum/experiment/physical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/types/physical_experiment.dm#L1), [`physical_experiments.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/physical_experiments.dm#L1), [`/datum/experiment/autopsy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/types/autopsy_experiment.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Эксперименты исследования космоса
Сканирование мест эксплоратор-дронами (пояс астероидов, чёрная дыра и т. п.).

- **Код:** [`/datum/experiment/exploration_scan`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/experisci/experiment/types/exploration.dm#L1)
- **Статус в ss13-rs:** ❌ нет

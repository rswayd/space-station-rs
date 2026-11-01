# Продвинутые атмос-машины (электролизёр, кристаллизатор, ГФР)
Машины атмосферной «науки»: производство газов и кристаллов и термоядерный реактор Hypertorus.

### Электролизёр
Портативная машина на батарее: расщепляет водяной пар на водород и кислород, превращает гипер-ноблий в антиноблий, производит галон; скорость от деталей.

- **Код:** [`/obj/machinery/electrolyzer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/electrolyzer/electrolyzer.dm#L4), [`process_atmos()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/electrolyzer/electrolyzer.dm#L89), [`proc/call_reactions()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/electrolyzer/electrolyzer.dm#L132); tgui: [`Electrolyzer.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Electrolyzer.tsx)
- **Статус в ss13-rs:** ❌ нет

### Реакции электролизёра
Отдельный набор реакций `/datum/gas_reaction/electrolyzer`: H2O → H2 + O2, ноблий → антиноблий, генерация галона.

- **Код:** [`/datum/gas_reaction/electrolyzer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/electrolyzer/electrolyzer_reactions.dm#L15), [`h2o_conversion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/electrolyzer/electrolyzer_reactions.dm#L51), [`nob_conversion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/electrolyzer/electrolyzer_reactions.dm#L76), [`halon_generation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/electrolyzer/electrolyzer_reactions.dm#L110)
- **Статус в ss13-rs:** ❌ нет

### Кристаллизатор
Трубная машина, которая из заданного набора газов при нужной температуре выращивает предметы по рецепту: алмазы, листы плазмы, горячий лёд, металлический водород, топливные пеллеты ГФР, гранаты хилиума и прото-нитрата, кристаллы гипер-ноблия и нитрия и др.

- **Код:** [`/obj/machinery/atmospherics/components/binary/crystallizer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/gas_recipe_machines/crystallizer.dm#L6), [`proc/heat_calculations()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/gas_recipe_machines/crystallizer.dm#L128), [`/datum/gas_recipe/crystallizer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/gas_recipe_machines/atmos_machines_recipes.dm#L36); tgui: [`Crystallizer.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Crystallizer.tsx)
- **Статус в ss13-rs:** ❌ нет

### Кристаллы гипер-ноблия и нитрия
Продукты кристаллизатора: кристалл гипер-ноблия вставляется в канистру или баллон и подавляет реакции, кристалл нитрия — стимулятор.

- **Код:** [`/obj/item/hypernoblium_crystal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/gas_recipe_machines/crystallizer_items.dm#L1), [`/obj/item/nitrium_crystal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/gas_recipe_machines/crystallizer_items.dm#L42)
- **Статус в ss13-rs:** ❌ нет

### Hypertorus Fusion Reactor: сборка и управление
Многоблочный термоядерный реактор 3x3 (ядро, топливный вход, вход модератора, выход отходов, интерфейс, углы) собирается из коробок-деталей и управляется через окно: подача топлива, модератора, охлаждение, мощность нагрева.

- **Код:** [`/obj/machinery/atmospherics/components/unary/hypertorus/core`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/fusion/hfr_core.dm#L4), [`proc/check_part_connectivity()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/fusion/hfr_procs.dm#L9), [`proc/activate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/fusion/hfr_procs.dm#L86), [`/obj/machinery/hypertorus/interface`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/fusion/hfr_parts.dm#L137), [`/obj/item/hfr_box`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/fusion/hfr_parts.dm#L413); tgui: [`Hypertorus`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Hypertorus)
- **Статус в ss13-rs:** ❌ нет

### Hypertorus: термоядерный процесс и виды топлива
Реакция синтеза по рецептам топлива (H2+O2, H2+тритий, гипер-ноблий+антиноблий и др.) производит энергию, тепло, газы-продукты; модераторы вызывают ядерные частицы, молнии, гравитационные импульсы.

- **Код:** [`proc/fusion_process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/fusion/hfr_main_processes.dm#L45), [`proc/moderator_common_process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/fusion/hfr_main_processes.dm#L289), [`/datum/hfr_fuel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/fusion/hfr_fuel_datums.dm#L14), [`proc/check_lightning_arcs()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/fusion/hfr_main_processes.dm#L469)
- **Статус в ss13-rs:** ❌ нет

### Hypertorus: повреждения и расплавление
Интеграционное повреждение ядра от перегрева и железа в смеси, радио-предупреждения инженерам, отсчёт и катастрофическое расплавление (взрыв, радиация, выброс газа).

- **Код:** [`proc/process_damageheal()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/fusion/hfr_main_processes.dm#L414), [`proc/check_alert()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/fusion/hfr_procs.dm#L343), [`proc/countdown()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/fusion/hfr_procs.dm#L406), [`proc/meltdown()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/fusion/hfr_procs.dm#L451)
- **Статус в ss13-rs:** ❌ нет

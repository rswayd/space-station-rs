# Хирургия: система и инструменты
Современная система операций tg (`/datum/surgery_operation`): состояния тела, инструменты, шанс провала, операционная. Конкретные операции — [surgery-operations.md](surgery-operations.md). В ss13-rs — `crates/ss13_game/src/medical/surgery.rs`.

### Система операций
Операция выбирается радиальным меню при клике инструментом по лежащему пациенту с открытым операционным полем: доступность определяется состоянием части тела (разрез, отведённая кожа, зажатые сосуды, распиленная кость), инструментом (`get_tool_quality`) и зоной; затем `do_after` и успех/провал.

- **Код:** [`/datum/surgery_operation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/_operation.dm#L388), [`proc/try_perform()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/_operation.dm#L833), [`proc/check_availability()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/_operation.dm#L470), [`proc/success()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/_operation.dm#L1144), [`proc/failure()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/_operation.dm#L1176)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/surgery.rs`: состояния частей тела, выбор операции по инструменту и зоне, автовыбор единственной и список tgui `ListInputWindow` вместо радиального меню, повторная проверка, циклы; нет заблокированных операций, дисков, компьютера

### Базовые шаги: разрез, ретракция, зажим, пила, сшивание
Скальпель режет кожу, ретрактор отводит, гемостат зажимает кровотечение, пила/дрель вскрывают кость, каутер закрывает; для толстой кожи и абдукторов свои варианты; сустав и кости вправляются.

- **Код:** [`/datum/surgery_operation/limb/incise_skin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_generic.dm#L3), [`/datum/surgery_operation/limb/retract_skin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_generic.dm#L103), [`/datum/surgery_operation/limb/clamp_bleeders`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_generic.dm#L217), [`/datum/surgery_operation/limb/saw_bones`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_generic.dm#L308), [`/datum/surgery_operation/limb/drill_bones`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_generic.dm#L429), [`/datum/surgery_operation/limb/close_skin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_generic.dm#L148), [`/datum/surgery_operation/limb/incise_organs`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_generic.dm#L481)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/surgery.rs`: разрез, ретракция, зажим/разжим, пила, дрель, сшивание/прижигание, фиксация костей, разрез органа с временем и инструментами tg; нет вариантов для толстой кожи, абдукторов, механических частей

### Шанс провала и скорость
Скорость и шанс провала зависят от инструмента (замены — ручка, нож, пила-бензопила), стола (операционный, обычный, кровать, пол), анестезии/сна пациента, навыков, чистоты; провал наносит урон.

- **Код:** [`proc/get_time_modifiers()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/_operation.dm#L738)
- **BandaStation:** [`modular_bandastation/medical/code/surgery/fail_chance.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/medical/code/surgery/fail_chance.dm#L8) — собственная формула шанса провала
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/surgery.rs`: качество инструмента × поверхность × пациент без сознания, формула провала BandaStation (+40% в сознании, темнота, класс инструмента); нет навыков, опьянения хирурга, кольца на перчатках

### Хирургические инструменты
Скальпель, гемостат, ретрактор, каутер, дрель, циркулярная пила, хирургическая простыня, фильтр крови; продвинутые (лазерный скальпель, механические ретракторы), инопланетные (абдукторы), лоток и стойка.

- **Код:** [`/obj/item/scalpel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/surgery_tools.dm#L208), [`/obj/item/hemostat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/surgery_tools.dm#L32), [`/obj/item/retractor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/surgery_tools.dm#L1), [`/obj/item/cautery`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/surgery_tools.dm#L65), [`/obj/item/surgicaldrill`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/surgery_tools.dm#L158), [`/obj/item/circular_saw`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/surgery_tools.dm#L268), [`/obj/item/surgical_drapes`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/surgery_tools.dm#L327), [`/obj/item/blood_filter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/surgery_tools.dm#L637), [`/obj/item/surgery_tray`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/surgery_tray.dm#L7)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/surgery.rs`: скальпель, гемостат, ретрактор, каутер, дрель, пила, простыня, костоправ, гель, лента и импровизированные замены по `implements`; нет фильтра крови, лотка, продвинутых инструментов как отдельных механик

### Операционный стол и компьютер
Операционный стол ускоряет операции и позволяет пристегнуть пациента; операционный компьютер (tgui) показывает состояние пациента, доступные процедуры и загруженные с дисков продвинутые операции, эксперименты.

- **Код:** [`/obj/structure/table/optable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/tables_racks.dm#L898), [`/obj/machinery/computer/operating`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/operating_computer.dm#L4), [`/obj/item/disk/surgery`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/surgery_disks.dm#L1); tgui: [`index.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/OperatingComputer/index.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/surgery.rs`: на операционный стол и кровать кладут перетаскиванием (пристёгивание лёжа), модификаторы поверхности; нет операционного компьютера, баллона и маски стола

### Помощь при операции и обезболивание
Простыня/предметы с `surgery_aid` подготавливают зону; анестетики и сон снижают боль и шанс провала.

- **Код:** [`/datum/element/surgery_aid`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/surgery_aid.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/surgery.rs`: простыня готовит зону и спадает после 4 шагов стоя, бессознательный пациент ускоряет и снижает провал; нет анестетиков (`TRAIT_ANALGESIA`)

### Вскрытие и диссекция
Аутопсия (сканер вскрытия) определяет причину смерти и болезни для коронера; диссекция трупов даёт очки исследований.

- **Код:** [`/datum/surgery_operation/limb/autopsy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_autopsy.dm#L1), [`/datum/surgery_operation/basic/dissection`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_dissection.dm#L1), [`/obj/item/autopsy_scanner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/devices/scanners/autopsy_scanner.dm#L1)
- **Статус в ss13-rs:** ❌ нет

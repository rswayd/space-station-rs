# Хирургические операции
Операции поверх системы из [surgery.md](surgery.md). У большинства есть «механический» вариант для роботизированных частей тела.

### Обработка ран (tend wounds)
Лечение brute/burn через операцию (обычная, улучшенная, мастерская и комбинированная) — тем сильнее, чем больше урона.

- **Код:** [`/datum/surgery_operation/basic/tend_wounds`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_healing.dm#L8), [`/datum/surgery_operation/basic/tend_wounds/combo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_healing.dm#L248)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/medical/surgery.rs`: ушибы/ожоги по формуле tg, урон при провале, повтор, улучшенные версии и комбо (`Tend`, заблокированы, открываются дисками и исследованием) с заменой по `replaced_by`, точные остатки урона с анализатором в руке или у операционного компьютера; настроения морбидных — группа настроения

### Манипуляции с органами
Извлечение и вставка внутренних и внешних органов (в том числе кибернетики и имплантов-органов) в открытой полости.

- **Код:** [`/datum/surgery_operation/limb/organ_manipulation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_organ_manip.dm#L4), [`/datum/surgery_operation/limb/organ_manipulation/internal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_organ_manip.dm#L271), [`/datum/surgery_operation/limb/organ_manipulation/external`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_organ_manip.dm#L315)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/surgery.rs`, `crates/ss13_game/src/medical/organs.rs`: внутренние органы операционной зоны (глаза — `eyes`, язык — `mouth`, аппендикс — `groin`) извлекаются зажимом/ломом/вилкой на пол и вставляются из руки в пустой слот, органы груди и мозг — только при распиленной кости; нет еды и растений как муляжей органов (`random_item_organ_pool`); внешние органы — organs.md, импланты — cybernetics.md, механические и абдукторские варианты — свои группы

### Восстановление органов
Лобэктомия (лёгкие), гепатэктомия (печень), коронарное шунтирование (сердце), гастрэктомия (желудок), лечение глаз, ушей, мозга (снимает травмы мозга).

- **Код:** [`/datum/surgery_operation/organ/repair`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_organ_repair.dm#L2), [`/datum/surgery_operation/organ/repair/lobectomy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_organ_repair.dm#L47), [`/datum/surgery_operation/organ/repair/hepatectomy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_organ_repair.dm#L111), [`/datum/surgery_operation/organ/repair/coronary_bypass`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_organ_repair.dm#L176), [`/datum/surgery_operation/organ/repair/gastrectomy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_organ_repair.dm#L241), [`/datum/surgery_operation/organ/repair/brain`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_organ_repair.dm#L461)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/medical/surgery.rs`: коронарное шунтирование, лобэктомия (при провале +4 `losebreath`), гепатэктомия, гастрэктомия (любой острый предмет) — один раз на орган, ставят урон органа в долю tg, при провале урон; операции на ушах и глазах (урон 0, 40 с глухоты; при провале 70 мозгу — tg бьёт сам орган вопреки своим строкам) и на мозге (цикл, −50, снимает травмы до хирургических, при провале — тяжёлая травма до лоботомии — `medical/traumas.rs`); снятие промывки — антагонисты, кровотечение при провале шунтирования (`adjustBleedStacks`) — blood.md, слепота/размытие после операции на глазах — зрение (organs.md)

### Лечение костей и ран операцией
Вправление вывиха, лечение трещины и открытого перелома, восстановление черепа, санация ожогов (debride), зашивание проколов и вен.

- **Код:** [`/datum/surgery_operation/limb/repair_dislocation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_bone_repair.dm#L2), [`/datum/surgery_operation/limb/repair_hairline`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_bone_repair.dm#L68), [`/datum/surgery_operation/limb/repair_compound`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_bone_repair.dm#L175), [`/datum/surgery_operation/limb/repair_cranium`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_bone_repair.dm#L272), [`/datum/surgery_operation/limb/debride`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_debride.dm#L1), [`/datum/surgery_operation/limb/repair_puncture`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_puncture.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/surgery.rs`: вправление вывиха, сращивание трещины, вправление и заживление открытого перелома, санация инфицированного ожога (−4 инфекции, +0.5 санации за шаг), соединение и прижигание сосудов колотой раны (`mend_state`); нет черепной трещины (рана — wounds.md), ускорения отсканированных ран

### Ампутация и замена конечности
Отрезание конечности (в том числе замена ноги на деревянную), пришивание новой или протеза, крепление произвольного предмета как протеза.

- **Код:** [`/datum/surgery_operation/limb/amputate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_amputation.dm#L1), [`/datum/surgery_operation/limb/replace_limb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_replace_limb.dm#L1), [`/datum/surgery_operation/limb/prosthetic_replacement`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_add_limb.dm#L4)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/surgery.rs`, `crates/ss13_game/src/medical/bodyparts.rs`: ампутация руки или ноги (часть тела падает предметом с уроном), прикрепление конечности-предмета к обрубку (`prosthetic_replacement`, 10 токсина); `// shortcut:` нет обезглавливания, деревянных ног, произвольных предметов как протеза и его закрепления; аугментация роботизированными — роботизированные конечности (bodyparts.md)

### Реанимация операцией
Оживление мёртвого пациента разрядом прямо в мозг (дефибриллятор или шоковый инструмент) при допустимом уроне.

- **Код:** [`/datum/surgery_operation/basic/revival`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_revival.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/medical/surgery.rs` (`Op::Revival`, `revive_by_shock`): мёртвый пациент с открытым черепом, лопасти включённого дефибриллятора в руках (заряд, без перезарядки) или активная дубинка; призрак возвращается, −50 удушья, `revive()`, мозгу +15 (до 180) при успехе и +50 (до 199) при неудаче; механический вариант — «Операции на роботизированных частях», запреты для самоубийц, хасков и DNR — cpr-revival.md, разряд из энергооружия — оружие

### Мозговые операции: лоботомия, промывка мозгов, усмирение
Лоботомия лечит глубокие травмы ценой новой, промывка мозгов вписывает директиву (синдикат), «спящий агент», усмирение (пацифизм).

- **Код:** [`/datum/surgery_operation/organ/lobotomy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_lobotomy.dm#L1), [`/datum/surgery_operation/organ/brainwash`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_brainwash.dm#L3), [`/datum/surgery_operation/organ/pacify`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_pacify.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/surgery.rs` (`Op::Lobotomy`, `Op::Pacify`), `crates/ss13_game/src/medical/operating.rs` (дизайны исследований): лоботомия (любой острый предмет; снимает травмы до глубоких, 75% новая лёгкая/тяжёлая/особая только магией; при провале +80 мозгу и такая же травма) и пацификация (пацифизм до лоботомии; при провале тяжёлая травма до лоботомии), обе заблокированы до диска/исследования; нет промывки мозгов и «спящего агента» (антагонист `brainwashed` с окном директив — группа антагонистов), механических вариантов (роботизированные органы)

### Биоапгрейды (bioware)
Экспериментальные операции: прошивка вен (меньше кровотечение), мышечные вены, сращивание и заземление нервов, усиление связок или крючки, складывание и отпечаток коры.

- **Код:** [`/datum/surgery_operation/limb/bioware`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_bioware.dm#L1), [`/datum/surgery_operation/limb/bioware/vein_threading`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_bioware.dm#L45), [`/datum/surgery_operation/limb/bioware/nerve_splicing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_bioware.dm#L111), [`/datum/surgery_operation/limb/bioware/cortex_folding`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_bioware.dm#L246)
- **Статус в ss13-rs:** ❌ нет

### Косметические и бытовые операции
Пластическая хирургия (смена лица/имени), липосакция, полостной имплант (тайник в груди), промывание желудка, фильтрация крови, зубной имплант (таблетка), восстановление крыльев, обход астмы, удаление имплантов, извлечение ядра слайма.

- **Код:** [`/datum/surgery_operation/limb/plastic_surgery`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_plastic_surgery.dm#L3), [`/datum/surgery_operation/limb/lipoplasty`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_lipo.dm#L1), [`/datum/surgery_operation/limb/cavity_implant`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_cavity_implant.dm#L72), [`/datum/surgery_operation/organ/stomach_pump`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_pump.dm#L1), [`/datum/surgery_operation/limb/filter_blood`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_filter.dm#L1), [`/datum/surgery_operation/limb/add_dental_implant`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_dental.dm#L1), [`/datum/surgery_operation/organ/fix_wings`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_wing_repair.dm#L1), [`/datum/surgery_operation/basic/implant_removal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_implant_removal.dm#L1), [`/datum/surgery_operation/basic/core_removal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_core.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Вирусные и некротические операции
Вирусное связывание (болезнь становится переносимой без симптомов), бионекроз (зомби-опухоль).

- **Код:** [`/datum/surgery_operation/basic/viral_bonding`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_virus.dm#L1), [`/datum/surgery_operation/limb/bionecrosis`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_zombie.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Операции на роботизированных частях
Вскрытие и закрытие механических конечностей (отвёртка, ключ), подготовка электроники.

- **Код:** [`/datum/surgery_operation/limb/mechanical_incision`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_generic_mechanic.dm#L3), [`/datum/surgery_operation/limb/mechanic_unwrench`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_generic_mechanic.dm#L163), [`/datum/surgery_operation/limb/prepare_electronics`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/operations/operation_generic_mechanic.dm#L128)
- **Статус в ss13-rs:** ❌ нет

### Настройка голосовых связок (BandaStation)
Операция форка: изменение голоса (TTS) пациента через голосовые связки.

- **Код:** [`/datum/surgery_operation/limb/tune_vocal_cords`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/medical/code/surgery/vocal_cords.dm#L1)
- **BandaStation:** [`modular_bandastation/medical/code/surgery/vocal_cords.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/medical/code/surgery/vocal_cords.dm#L1) — новая операция
- **Статус в ss13-rs:** ❌ нет

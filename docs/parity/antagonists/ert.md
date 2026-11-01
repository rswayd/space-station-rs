# ОБР и отряды ЦК
Отряды быстрого реагирования, вызываемые админами: синие/янтарные/красные/гамма ОБР, эскадрон смерти, инквизиция, уборщики, клоуны, интерны, официальные лица ЦК.

### Отряды ОБР
`/datum/ert` задаёт состав, миссию, оружие и роли; антаг-датумы ролей (командир, СБ, инженер, медик, уборщик, капеллан, клоун, интерн); команда с целью-миссией.

- **Код:** [`/datum/ert`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ert.dm#L1), [`/datum/team/ert`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/ert/ert.dm#L2), [`/datum/antagonist/ert`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/ert/ert.dm#L6), [`/datum/antagonist/ert/commander`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/ert/ert.dm#L100), [`/datum/antagonist/ert/deathsquad`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/ert/ert.dm#L118), [`/datum/ert/inquisition`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/ert.dm#L80)
- **BandaStation:** [`modular_bandastation/antagonists/code/ert.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/antagonists/code/ert.dm#L1) — у эскадрона смерти появился лидер, миссия содержит коды самоуничтожения
- **Статус в ss13-rs:** ❌ нет

### Вызов ОБР админом
Админ-окно настройки ОБР: тип, число, призыв призраков с опросом, превью внешности, телепорт к станции/на шаттл.

- **Код:** [`/datum/admins/proc/makeERTTemplateModified()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/ert.dm#L9), [`/datum/admins/proc/makeERTPreviewIcon()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/ert.dm#L34)
- **Статус в ss13-rs:** ❌ нет

### Менеджер ОБР (BandaStation)
TGUI `ErtManager`: число слотов по специальностям (СБ, медики, инженеры, уборщики, капелланы, клоуны), тип отряда, выбор опытного лидера по наигранному времени, точка брифинга.

- **Код:** [`/datum/ert_manager`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/ert/code/ert_manager.dm#L15), [`/datum/ert_manager/proc/makeERTFromSlots()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/ert/code/ert_manager.dm#L219); tgui: [`ErtManager.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ErtManager.tsx)
- **BandaStation:** [`_ert.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/ert/_ert.dm#L1), [`ert_presets.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/ert/code/ert_presets.dm#L1) — пресеты amber/red/gamma/janitor/clown/inquisition
- **Статус в ss13-rs:** ❌ нет

### Снаряжение ОБР
Аутфиты ОБР и ЦК (броня, MODsuit, оружие по коду угрозы); BandaStation добавляет свои аутфиты ОБР.

- **Код:** [`modular_bandastation/ert/code/ert_outfits.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/ert/code/ert_outfits.dm#L1), [`/obj/item/mod/control/pre_equipped/responsory`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_types.dm#L469)
- **Статус в ss13-rs:** ❌ нет

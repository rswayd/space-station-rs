# Сбойный ИИ (Malf AI)
ИИ станции с взломанными законами: покупает модули за процессорное время от взломанных APC, стремится уничтожить органиков или станцию. Законы и базовый ИИ — см. ../silicon/.

### Антагонист «Сбойный ИИ»
`/datum/antagonist/malf_ai`: закон 0, цели (блок органиков/выживание/кражи), взлом APC даёт процессорное время, перенос ядра в APC (shunt); заражённый вариант (`infected`) от вирусной платы.

- **Код:** [`/datum/antagonist/malf_ai`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai.dm#L5), [`/datum/antagonist/malf_ai/infected`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai.dm#L213)
- **BandaStation:** [`malf_ai.dm#L82`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai.dm#L82) — переработаны особые цели, убран шанс особой цели
- **Статус в ss13-rs:** ❌ нет — ИИ нет; рулсет `Roundstart Malfunctioning AI` в `crates/ss13_game/src/antag/dynamic.rs` никого не спавнит

### Выбор модулей
TGUI-магазин модулей за процессорное время.

- **Код:** [`/datum/module_picker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_module_picker.dm#L2), [`/datum/ai_module`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L122); tgui: [`MalfunctionModulePicker.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/MalfunctionModulePicker.tsx)
- **Статус в ss13-rs:** ❌ нет

### Устройство судного дня
Модуль: ИИ активирует ядерное устройство в ядре, 7.5 минут обратного отсчёта, затем уничтожение станции.

- **Код:** [`/datum/ai_module/malf/destructive/nuke_station`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L160), [`/obj/machinery/doomsday_device`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L296)
- **BandaStation:** [`malf_ai_modules.dm#L373`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L373) — помечает станцию уничтоженной (для цели block)
- **Статус в ss13-rs:** ❌ нет

### Разрушительные модули
Блокировка станции, перехват машины (оживление), перегрузка машины (взрыв), уничтожение RCD, блэкаут, «мега-хонк» по интеркомам.

- **Код:** [`/datum/ai_module/malf/destructive/lockdown`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L386), [`/datum/ai_module/malf/destructive/override_machine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L441), [`/datum/ai_module/malf/destructive/overload_machine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L524), [`/datum/ai_module/malf/destructive/destroy_rcd`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L498), [`/datum/ai_module/malf/destructive/blackout`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L585), [`/datum/ai_module/malf/destructive/megahonk`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L619)
- **Статус в ss13-rs:** ❌ нет

### Утилитарные модули
Фабрика киборгов (запрещает shunt), отключение безопасности воздушных и пожарных тревог, отключение аварийного света, реактивация камер, emag на расстоянии, катящееся ядро (core tilt), опрокидывание торгоматов.

- **Код:** [`/datum/ai_module/malf/utility/place_cyborg_transformer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L652), [`/datum/ai_module/malf/utility/break_air_alarms`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L726), [`/datum/ai_module/malf/utility/break_fire_alarms`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L752), [`/datum/ai_module/malf/utility/emergency_lights`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L783), [`/datum/ai_module/malf/utility/reactivate_cameras`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L810), [`/datum/ai_module/malf/utility/emag`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L1079), [`/datum/ai_module/malf/utility/core_tilt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L1173), [`/datum/ai_module/malf/utility/remote_vendor_tilt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L1272)
- **Статус в ss13-rs:** ❌ нет

### Модули улучшений
Улучшение камер (рентген, ЭМИ-защита), турелей, подслушивание, доминирование над мехами, смена голоса.

- **Код:** [`/datum/ai_module/malf/upgrade/upgrade_cameras`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L851), [`/datum/ai_module/malf/upgrade/upgrade_turrets`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L890), [`/datum/ai_module/malf/upgrade/eavesdrop`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L909), [`/datum/ai_module/malf/upgrade/mecha_domination`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L925), [`/datum/ai_module/malf/upgrade/voice_changer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L941)
- **BandaStation:** [`malf_ai_modules.dm#L854`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/malf_ai/malf_ai_modules.dm#L854) — свои иконки камер
- **Статус в ss13-rs:** ❌ нет

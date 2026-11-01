# События, «секреты», смайты и билдмод
Инструменты админов для вмешательства в раунд: панель секретов, события, погода, звуки, смайты, билдмод.

### Панель секретов
TGUI `Secrets`: массовые действия (полный доступ, все в обезьян/случайные, отключить гравитацию, ионный шторм, массовое лечение, «чёрная дыра», сбросить имена...), отчёты (бомберы, сигнальщики, ДНК, отпечатки, манифест).

- **Код:** [`/datum/secrets_menu`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/secrets.dm#L8); tgui: [`Secrets.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Secrets.jsx)
- **BandaStation:** [`admin_secrets.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/admin/code/admin_secrets.dm#L1) — свои секреты
- **Статус в ss13-rs:** ❌ нет — админ-системы нет

_«Запуск событий и погоды» описано в [../events/framework.md](../events/framework.md)._

### Развлекательные вербы
Взрывы и бомбы (DynEx), ЭМИ, полиморф всех, массовые зомби-инфекция/излечение, массовые трейты, ERT, «Global Man Up» (BandaStation).

- **Код:** [`adminfun.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/adminfun.dm#L1), [`adminevents.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/adminevents.dm#L1)
- **Статус в ss13-rs:** ❌ нет — админ-системы нет

### Звуки админов
Проиграть глобальный/локальный звук, звук от моба, интернет-звук (YouTube через yt-dlp), запросы интернет-звука, остановить все звуки; BandaStation — на z-уровне и в поле зрения.

- **Код:** [`playsound.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/playsound.dm#L1), [`request_internet_sound.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/request_internet_sound.dm#L1), [`sound_emitter.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/sound_emitter.dm#L1)
- **Статус в ss13-rs:** ❌ нет — админ-системы нет

### Смайты
31 наказание: гиб, пыль, молния, огненный шар, BSA, капсула снабжения, неподвижный жезл, окаменение, «Boneless/Bloodless/Berforate», проклятие Вавилона, невезение, мозговые травмы, сломанные шнурки, наггетс, фейковый bwoink, ретекон и др.; BandaStation — сломанная конечность.

- **Код:** [`/datum/smite`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/smites/_smite.dm#L2), [`code/modules/admin/smites`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/smites)
- **BandaStation:** [`/datum/smite/broken_limb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/admin/code/smites/broken_limb.dm#L7)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_admin`: только «Lightning Bolt» (75 ожога, «Боги покарали вас за ваши грехи!»), без луча, звука и анимации

### Билдмод
Режим строительства кликами: базовый, продвинутый (спавн по типу), заливка, копирование, удаление, правка переменных, бросок, редактирование зон, взрыв, генератор карт, экспорт карты, аутфит, смайт, проколл; BandaStation — «crush» и «offer».

- **Код:** [`/datum/buildmode`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/buildmode/buildmode.dm#L5), [`/datum/buildmode_mode`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/buildmode/bm_mode.dm#L6), [`code/modules/buildmode/submodes`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/buildmode/submodes)
- **BandaStation:** [`/datum/buildmode_mode/crush`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/admin/code/buildmode/crush.dm#L8), [`/datum/buildmode_mode/offer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/admin/code/buildmode/offer.dm#L1)
- **Статус в ss13-rs:** ❌ нет — админ-системы нет

### Спавн объектов и мобов
TGUI-панель спавна (поиск типа, количество, место, капсула), создание моба, spawn object-as-mob, подсунуть в капсулу, выдача экипировки (Select Equipment), реагентные контейнеры, меню перекраски greyscale.

- **Код:** [`/datum/spawnpanel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/spawn_panel/spawn_panel.dm#L21), [`create_mob.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/create_mob.dm#L1), [`selectequipment.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/verbs/selectequipment.dm#L1), [`/datum/greyscale_modify_menu`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/greyscale_modify_menu.dm#L3); tgui: [`SpawnPanel.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SpawnPanel/index.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_admin`: поиск прототипа `/obj` и спавн под собой (`+SPAWN`). Нет мобов, количества, капсул, Select Equipment

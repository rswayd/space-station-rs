# Медицинские машины
Машины медотсека: слиперы, стазис, криокапсулы, капельницы, киоск, лимбгроуэр и прочее. В ss13-rs медицинских машин нет.

### Слипер
Пациента помещают внутрь: машина показывает состояние и вводит выбранные химикаты (набор зависит от деталей), синдикатская версия управляется изнутри.

- **Код:** [`/obj/machinery/sleeper`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/sleepers.dm#L1), [`/obj/machinery/sleeper/syndie`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/sleepers.dm#L296); tgui: [`Sleeper.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Sleeper.jsx)
- **Статус в ss13-rs:** ❌ нет

### Стазис-кровать
Замораживает процессы лежащего пациента (статус-эффект стазиса): раны и яды не прогрессируют, операции идут на ней.

- **Код:** [`/obj/machinery/stasis`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/stasis.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Криокапсула
Атмосферная машина: охлаждает пациента холодным газом, криоксадон и пирокс лечат; автоматическое извлечение по выздоровлении.

- **Код:** [`/obj/machinery/cryo_cell`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/unary_devices/cryo.dm#L68); tgui: [`Cryo.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Cryo.tsx)
- **Статус в ss13-rs:** ❌ нет

### Капельница
Подключается к пациенту и вводит или забирает кровь/реагенты из пакета или ёмкости с выбранной скоростью; физраствор встроенный.

- **Код:** [`/obj/machinery/iv_drip`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/iv_drip.dm#L13), [`/obj/machinery/iv_drip/saline`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/iv_drip.dm#L432); tgui: [`IVDrip.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/IVDrip.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/blood.rs`: пакет или стакан на капельницу, подключение перетаскиванием, вливание/забор 5 ед/с, вырывание иглы при отходе, быстрый переключатель ПКМ; нет окна tgui `IVDrip` (скорость), луча

### Медицинский киоск
Платное самообслуживание: скан состояния, ран, химии и болезней за кредиты.

- **Код:** [`/obj/machinery/medical_kiosk`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/medical_kiosk.dm#L13); tgui: [`MedicalKiosk.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/MedicalKiosk.jsx)
- **Статус в ss13-rs:** ❌ нет

### Лимбгроуэр
Выращивает конечности и органы разных видов из синтплоти; рецепты с дисков.

- **Код:** [`/obj/machinery/limbgrower`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/limbgrower.dm#L3); tgui: [`Limbgrower.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Limbgrower.jsx)
- **BandaStation:** [`modular_bandastation/species/code/surgery/organs/limbgrower_designs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/surgery/organs/limbgrower_designs.dm#L1) — рецепты органов видов форка
- **Статус в ss13-rs:** ❌ нет

### Липосактор (fat sucker)
Высасывает жир из пациента, превращая его в кусочки жира, снижает ожирение.

- **Код:** [`/obj/machinery/fat_sucker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/fat_sucker.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Органный холодильник и харвестер
Смартфридж для органов сохраняет их от гниения; харвестер (синдикатский/РнД) извлекает все органы из трупа.

- **Код:** [`/obj/machinery/smartfridge/organ`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/smartfridge.dm#L687), [`/obj/machinery/harvester`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/harvester.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Консоль медицинских записей
Вход по ID с медицинским доступом (или детектива, генетика); физический и психический статус, причина смерти, группа крови, поля записи, до трёх заметок врача; удаление медицинских данных записи. Ноутбук и синдикатский вариант.

- **Код:** [`/obj/machinery/computer/records/medical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/records/medical.dm#L1); tgui: [`MedicalRecords`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/MedicalRecords)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/records/console.rs`, `records/client.rs`: вход по ID, статусы, причина смерти, группа крови, поля, заметки (не больше трёх), удаление; нет превью персонажа и новых записей по фото

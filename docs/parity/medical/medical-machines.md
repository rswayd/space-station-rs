# Медицинские машины
Машины медотсека: слиперы, стазис, криокапсулы, капельницы, киоск, лимбгроуэр и прочее. В ss13-rs есть слипер, стазис-кровать, капельница, липосактор, органный холодильник и харвестер (`crates/ss13_game/src/medical/{sleeper,stasis,pods,iv_drip,blood}.rs`).

### Слипер
Пациента помещают внутрь: машина показывает состояние и вводит выбранные химикаты (набор зависит от деталей), синдикатская версия управляется изнутри.

- **Код:** [`/obj/machinery/sleeper`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/sleepers.dm#L1), [`/obj/machinery/sleeper/syndie`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/sleepers.dm#L296); tgui: [`Sleeper.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Sleeper.jsx)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/medical/sleeper.rs`, `medical/pods.rs`, окно `crates/ss13_tgui/src/interfaces/sleeper.rs`: дверь (alt-клик, кнопка окна, шаг изнутри), перетаскивание карбона в открытый слипер, окно с состоянием, здоровьем, уроном и мозгом, инъекция 10 ед. с проверками `chem_allowed` (матбины — эффективность и `min_health`, сервоприводы — группы препаратов), подтипы (синдикатский, NT, self-control, old, party с ацетатом свинца), емаг перемешивает кнопки; `// shortcut:` изнутри нельзя кликнуть (общее ограничение `crate::click`), спрей-реагенты party-пода вводятся инъекцией, нет звуков и анимаций

### Стазис-кровать
Замораживает процессы лежащего пациента (статус-эффект стазиса): раны и яды не прогрессируют, операции идут на ней.

- **Код:** [`/obj/machinery/stasis`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/stasis.dm#L2)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/medical/stasis.rs`: пристёгивание лёжа, `free_operation` (операции без простыни), стазис работающей кровати (`InStasis`: обездвижен и без рук, без дыхания и давления, ран, органов, крови, болезней, метаболизма, голода, SSD-сна и эффектов радиации, операции без боли), тушение огня, активное потребление, alt-клик с кулдауном 5 с, матрас-оверлей, пересчёт каждый тик машин; `// shortcut:` нет звуков, ряби, `REAGENT_IGNORE_STASIS`, `on_stasis` ожогов, масштабирования мощности деталями

### Криокапсула
Атмосферная машина: охлаждает пациента холодным газом, криоксадон и пирокс лечат; автоматическое извлечение по выздоровлении.

- **Код:** [`/obj/machinery/cryo_cell`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/atmospherics/machinery/components/unary_devices/cryo.dm#L68); tgui: [`Cryo.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/Cryo.tsx)
- **Статус в ss13-rs:** ❌ нет

### Капельница
Подключается к пациенту и вводит или забирает кровь/реагенты из пакета или ёмкости с выбранной скоростью; физраствор встроенный.

- **Код:** [`/obj/machinery/iv_drip`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/iv_drip.dm#L13), [`/obj/machinery/iv_drip/saline`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/iv_drip.dm#L432); tgui: [`IVDrip.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/IVDrip.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/blood.rs`: пакет или стакан на капельницу, подключение перетаскиванием, вливание/забор, вырывание иглы при отходе, быстрый переключатель ПКМ; окно `IVDrip` (`medical/iv_drip.rs`, `crates/ss13_tgui/src/interfaces/iv_drip.rs`): скорость 0–5 ед/с, режим, извлечение ёмкости, отключение; нет луча и встроенного физраствора (`/obj/machinery/iv_drip/saline`, `use_internal_storage`)

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
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/medical/pods.rs`: дверь, защитный люк (alt-клик, доступ кухни или емаг), старт через 2 с для толстого карбона, отсасывание `bite_size` за секунду с фактами о жирах, остановка на `stop_at`, мясо, котлеты и печенье при открытии, детали (микролазеры), емаг понижает пороги; `// shortcut:` нет звуков и света, выбраться из запертого нельзя (tg — 60 с `do_after`), печенье на пол, мясо всегда человеческое

### Органный холодильник и харвестер
Смартфридж для органов сохраняет их от гниения; харвестер (синдикатский/РнД) извлекает все органы из трупа.

- **Код:** [`/obj/machinery/smartfridge/organ`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/food_and_drinks/machinery/smartfridge.dm#L687), [`/obj/machinery/harvester`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/harvester.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/smartfridge.rs` (органный холодильник принимает органы и части тела) и `medical/organs.rs`: органы в нём не гниют, при матбинах выше первого уровня лечатся (`repair_rate`); харвестер — `medical/pods.rs`: дверь, проверки (питание, карбон, без одежды, мёртв; емаг снимает), разогрев и по части тела за `interval` от ног к голове и органам груди на выходной тайл, поворот выхода alt-кликом при открытой панели, прерывание при открытии/без питания; `// shortcut:` нет звуков и оповещения призраков

### Консоль медицинских записей
Вход по ID с медицинским доступом (или детектива, генетика); физический и психический статус, причина смерти, группа крови, поля записи, до трёх заметок врача; удаление медицинских данных записи. Ноутбук и синдикатский вариант.

- **Код:** [`/obj/machinery/computer/records/medical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/records/medical.dm#L1); tgui: [`MedicalRecords`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/MedicalRecords)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/records/console.rs`, `records/client.rs`: вход по ID, статусы, причина смерти, группа крови, поля, заметки (не больше трёх), удаление; нет превью персонажа и новых записей по фото

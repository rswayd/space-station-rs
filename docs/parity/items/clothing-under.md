# Одежда: униформа и аксессуары
Комбинезоны (`/obj/item/clothing/under`), датчики костюма и прикрепляемые аксессуары.

### Униформы отделов и должностей
Форма каждой должности (командование, СБ, инженерия, медицина, наука, карго, сервис, ЦК, заключённые) с вариантами юбок и альтернатив; часть даёт небольшую броню (СБ, капитан).

- **Код:** [`/obj/item/clothing/under`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/_under.dm#L1), [`/obj/item/clothing/under/rank/captain`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/jobs/command.dm#L1), [`/obj/item/clothing/under/rank/security`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/jobs/security.dm#L12), [`/obj/item/clothing/under/rank/engineering`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/jobs/engineering.dm#L3), [`/obj/item/clothing/under/rank/medical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/jobs/medical.dm#L1), [`/obj/item/clothing/under/rank/rnd`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/jobs/rnd.dm#L1), [`/obj/item/clothing/under/rank/cargo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/jobs/cargo.dm#L1), [`/obj/item/clothing/under/rank/civilian`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/jobs/civilian/civilian.dm#L3), [`/obj/item/clothing/under/rank/centcom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/jobs/centcom.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/outfit.rs`, `crates/ss13_game/src/inventory.rs`: униформы выдаются с нарядом работы и надеваются, worn-спрайт рисуется; нет брони, GAGS-вариантов, женской обрезки

### Цветные, гражданские и костюмные комбинезоны
Цветные комбинезоны и юбки (включая случайный цвет), брюки, шорты, платья, деловые костюмы, пижамы, костюмы для вечеринок, синдикатовские водолазки, униформа Звёздного пути, туники эфириалов.

- **Код:** [`/obj/item/clothing/under/color`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/color.dm#L1), [`/obj/item/clothing/under/color/random`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/color.dm#L38), [`/obj/item/clothing/under/pants`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/pants.dm#L1), [`/obj/item/clothing/under/dress`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/skirt_dress.dm#L1), [`/obj/item/clothing/under/suit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/suits.dm#L1), [`/obj/item/clothing/under/costume`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/costume.dm#L1), [`/obj/item/clothing/under/syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/syndicate.dm#L1), [`/obj/item/clothing/under/trek`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/trek.dm#L3), [`/obj/item/clothing/under/misc`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/miscellaneous.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/inventory.rs`: надеваются как любая униформа; нет случайного цвета (`/random` выбирает подтип в `Initialize`), GAGS-спрайтов на мобе

### Защитные комбинезоны плазменов
Герметичный комбинезон плазмамена: не даёт им гореть в кислороде, сам тушит владельца с перезарядкой, пополняется картриджами.

- **Код:** [`/obj/item/clothing/under/plasmaman`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/jobs/Plasmaman/civilian_service.dm#L2), [`code/modules/clothing/under/jobs/Plasmaman`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/jobs/Plasmaman)
- **Статус в ss13-rs:** ❌ нет — нет вида плазменов и самотушения

### Подкатывание комбинезона
Alt-клик сворачивает комбинезон (`can_adjust`, меняет спрайт и открывает торс), правый alt-клик — альтернативный стиль.

- **Код:** [`click_alt()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/_under.dm#L529), [`click_alt_secondary()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/_under.dm#L538)
- **Статус в ss13-rs:** ❌ нет

### Датчики костюма
Режимы датчиков (выкл., жив/мёртв, жизненные показатели, координаты) задаются на униформе и видны на консоли наблюдения за экипажем; ЭМИ ломает датчики, кабель с кусачками чинит; у заключённых датчики заблокированы. Отдельный модуль датчика переносится между униформами.

- **Код:** [`set_sensor_mode()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/_under.dm#L281), [`break_sensors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/_under.dm#L241), [`repair_sensors()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/_under.dm#L253), [`/obj/item/suit_sensor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/_suit_sensor.dm#L2)
- **Статус в ss13-rs:** ❌ нет — нет датчиков и консоли экипажа (консоль экипажа — вне этого раздела)

### Прикрепление аксессуаров
На униформу крепится один аксессуар (повязка, медаль, жилет, кобура): клик аксессуаром по униформе, снятие alt-кликом; аксессуар рисуется поверх формы и может давать броню или хранилище.

- **Код:** [`/obj/item/clothing/accessory`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/accessories/_accessories.dm#L11), [`attach_accessory()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/_under.dm#L354), [`remove_accessory()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/_under.dm#L388)
- **BandaStation:** [`_under.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/_under.dm#L452) — аксессуары прячутся под верхней одеждой; [`_under.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/_under.dm#L462) — кобуры-аксессуары меняют вес формы
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/outfit.rs` не крепит `accessory` наряда

### Повязки, значки и жилеты
Нарукавные повязки отделов, значок адвоката (хлопок по столу), значки фанатов клоуна/мима, карманный протектор, жетоны (аллергии), жилеты, фартук горничной, трибальные талисманы.

- **Код:** [`/obj/item/clothing/accessory/armband`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/accessories/armbands.dm#L2), [`/obj/item/clothing/accessory/lawyers_badge`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/accessories/badges.dm#L2), [`/obj/item/clothing/accessory/dogtag`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/accessories/badges.dm#L120), [`/obj/item/clothing/accessory/waistcoat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/accessories/vests.dm#L2), [`/obj/item/clothing/accessory/talisman`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/accessories/tribal.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Медали
Медали и ленты, которые капитан вручает с указанием причины (записывается в журнал наград).

- **Код:** [`/obj/item/clothing/accessory/medal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/accessories/medals.dm#L1), [`provide_reason()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/clothing/under/accessories/medals.dm#L21)
- **Статус в ss13-rs:** ❌ нет

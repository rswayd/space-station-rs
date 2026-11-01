# MODsuits (модульные костюмы)

Модульный костюм: блок управления на спине разворачивает шлем, нагрудник, перчатки и ботинки; питается от ядра, несёт модули с лимитом сложности.

### Блок управления MOD и развёртывание
`/obj/item/mod/control` в слоте спины: развёртывание/сворачивание частей (по одной или всех), активация с запечатыванием частей (даёт защиту, герметичность, слоудаун по теме), расход заряда в `process()`, отключение при разрядке, ЭМИ, емаг снимает блокировку доступа.

- **Код:** [`/obj/item/mod/control`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_control.dm#L8), [`process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_control.dm#L175), [`deploy()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_activation.dm#L70), [`toggle_activate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_activation.dm#L171), [`seal_part()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_activation.dm#L268), [`/datum/action/item_action/mod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_actions.dm#L1)
- **BandaStation:** [`mod_activation.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_activation.dm#L234) — свой звук активации (переменная темы в [`mod_theme.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_theme.dm#L89))
- **Статус в ss13-rs:** ❌ нет

### Части костюма
Шлем, нагрудник, перчатки, ботинки (и опц. очки/шея) — одежда, создаваемая блоком управления; повреждение части сворачивает её.

- **Код:** [`/obj/item/clothing/head/mod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_clothes.dm#L1), [`/obj/item/clothing/suit/mod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_clothes.dm#L21), [`/obj/item/clothing/gloves/mod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_clothes.dm#L103), [`/obj/item/clothing/shoes/mod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_clothes.dm#L123), [`/datum/mod_part`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_part.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Окно MODsuit
tgui-окно костюма: заряд, статус, части, список модулей с включением/настройкой/закреплением на панели, блокировка доступа; ПДА-программа NtosMODsuit дублирует его.

- **Код:** [`ui_data()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_ui.dm#L7), [`ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_ui.dm#L85); tgui: [`MODsuit.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/MODsuit.tsx), [`NtosMODsuit.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/NtosMODsuit.jsx)
- **Статус в ss13-rs:** ❌ нет

### Обслуживание: панель, модули, ядро
Отвёртка открывает панель, лом извлекает модули/ядро; установка модуля проверяет сложность, конфликты и требуемые части.

- **Код:** [`screwdriver_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_control.dm#L255), [`crowbar_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_control.dm#L273), [`install()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_control.dm#L619), [`uninstall()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_control.dm#L664)
- **Статус в ss13-rs:** ❌ нет

### Ядра MOD
Стандартное (батарея), бесконечное, эфириальное (от заряда эфириала), плазменное (заправляется плазмой/рудой; лаваленд-версия).

- **Код:** [`/obj/item/mod/core`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_core.dm#L1), [`core/standard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_core.dm#L101), [`core/ethereal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_core.dm#L297), [`core/plasma`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_core.dm#L362)
- **Статус в ss13-rs:** ❌ нет

### Сборка MOD
Сборка в робототехнике: оболочка, ядро, части (шлем/нагрудник/перчатки/ботинки), плата, броня-пластины темы; отдельные этапы инструментами.

- **Код:** [`/obj/item/mod/construction`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_construction.dm#L1), узлы техвеба: [`modsuit_nodes.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/research/techweb/nodes/modsuit_nodes.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Темы и готовые костюмы
29 тем (стандарт, гражданский, инженерный, атмосферный, продвинутый, шахтёрский, погрузчик, медицинский, спасательный, научный, СБ, safeguard, magnate, cosmohonk, синдикат, elite, infiltrator, interdyne, ниндзя, хроно, ERT и др.): броня, слоудаун, ёмкость сложности, разрешённые модули, скины; `pre_equipped` — готовые костюмы с модулями.

- **Код:** [`/datum/mod_theme`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_theme.dm#L16), [`/obj/item/mod/control/pre_equipped`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_types.dm#L1)
- **BandaStation:** [`modular_bandastation/objects/code/items/clothing/mod/types/`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/mod/types) — темы и костюмы Скрелл (Raskinta, гвардия императора), СССП, ТСФ, NT (Praetorian, Specops); [`modular_bandastation/balance/code/mod_types.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/mod_types.dm#L1) — тема и костюмы Responsory/Inquisitory
- **Статус в ss13-rs:** ❌ нет

### Краска и скины MOD
Набор краски перекрашивает костюм матрицей цветов (tgui) или меняет скин темы; аппликатор скина (например, honkerative).

- **Код:** [`/obj/item/mod/paint`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_paint.dm#L8), [`/obj/item/mod/skin_applier`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_paint.dm#L142); tgui: [`MODpaint.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/MODpaint.jsx)
- **Статус в ss13-rs:** ❌ нет

### ИИ в MOD
ИИ (с интеллекарты или мини-карты) может вселиться в костюм и управлять движением и модулями носителя.

- **Код:** [`transfer_ai()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_ai.dm#L1), [`relaymove()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_ai.dm#L136), [`/obj/item/mod/ai_minicard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_ai.dm#L168)
- **Статус в ss13-rs:** ❌ нет

### MODlink и Link Scryer
Видеосвязь между костюмами/скраерами по частоте: голограмма собеседника рядом.

- **Код:** [`/datum/mod_link`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_link.dm#L336), [`call_link()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_link.dm#L494), [`/obj/item/clothing/neck/link_scryer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mod/mod_link.dm#L142)
- **Статус в ss13-rs:** ❌ нет

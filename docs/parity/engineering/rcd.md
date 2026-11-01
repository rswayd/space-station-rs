# Быстрые строительные устройства (RCD и семейство)
Ручные устройства на сжатой материи: RCD, RLD, RTD, RPD, RWD/RCL, RDD, RSF, RPLD и их улучшения.

### Общая механика устройств на материи
`/obj/item/construction`: запас материи заправляется картриджами и листами (или из рудного силоса), дальность и задержка постройки, эффект «строительного голограммы», диски улучшений.

- **Код:** [`/obj/item/construction`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RHD.dm#L5), [`proc/useResource()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RHD.dm#L190), [`proc/insert_matter()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RHD.dm#L133), [`proc/toggle_silo()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RHD.dm#L235), [`/obj/item/rcd_upgrade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RHD.dm#L293), [`/obj/item/rcd_ammo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RCD.dm#L660), [`/obj/effect/constructing_effect`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/temporary_visuals/miscellaneous.dm#L518)
- **Статус в ss13-rs:** ❌ нет

### RCD (быстрое строительство)
Строит полы, стены, шлюзы (с выбором электроники и типа), окна и решётки, рамы машин и компьютеров, мебель; разбирает (кроме укреплённых без улучшения); tgui-меню категорий; взрывается от ЭМИ при емаге; борговый и синдикатский варианты.

- **Код:** [`/obj/item/construction/rcd`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RCD.dm#L6), [`proc/rcd_create()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RCD.dm#L258), [`proc/can_place()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RCD.dm#L147), [`wall/rcd_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/turfs/closed/walls.dm#L298); tgui: [`RapidConstructionDevice.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/RapidConstructionDevice.tsx)
- **BandaStation:** [`RCD.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RCD.dm#L156) — хуки скиллчипа инженера (сам модуль `skillchip_rcd` — вне этого раздела)
- **Статус в ss13-rs:** ❌ нет

### RLD (быстрое освещение)
Ставит настенные лампы, напольные лампы и светящиеся палочки нужного цвета, разбирает освещение.

- **Код:** [`/obj/item/construction/rld`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RLD.dm#L18), [`/obj/item/construction/rld/mini`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RLD.dm#L220)
- **BandaStation:** [`modular_bandastation/balance/code/tools.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/tools.dm#L2) — премиальная цена RLD в автоматах
- **Статус в ss13-rs:** ❌ нет

### RTD (быстрая плитка)
Кладёт и снимает напольную плитку любого вида и направления из меню.

- **Код:** [`/obj/item/construction/rtd`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RTD.dm#L13); tgui: [`RapidTilingDevice.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/RapidTilingDevice.tsx)
- **Статус в ss13-rs:** ❌ нет

### RPD (быстрая прокладка труб)
Создаёт атмосферные трубы и устройства, трубы утилизации и транзитные трубы нужного слоя и цвета; разбирает трубы; улучшение — откручивание. Сами атмосферные трубы — см. ../atmos/.

- **Код:** [`/obj/item/pipe_dispenser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RPD.dm#L21), [`proc/do_pipe_build()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RPD.dm#L527), [`/obj/item/rpd_upgrade/unwrench`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RPD.dm#L607); tgui: [`RapidPipeDispenser.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/RapidPipeDispenser.tsx)
- **Статус в ss13-rs:** ❌ нет

### RWD и RCL (прокладка кабеля)
Катушка-укладчик кабеля: прокладывает кабель за идущим, выбирает слой; RWD — быстрая проводка.

- **Код:** [`/obj/item/rwd`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RWD.dm#L5), [`/obj/item/rcl`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RCL.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### RDD (быстрая декорация)
Ставит декоративные объекты (растения, статуи и т. п.) из меню за материю.

- **Код:** [`/obj/item/construction/rdd`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RDD.dm#L167); tgui: [`RapidDecorationDevice.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/RapidDecorationDevice.tsx)
- **Статус в ss13-rs:** ❌ нет

### RSF и RPLD
RSF создаёт посуду, бумагу, сигареты (сервис, у боргов — печенье); RPLD — сантехнические машины для химии и гидропоники (см. ../service/, ../chemistry/).

- **Код:** [`/obj/item/rsf`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RSF.dm#L8), [`/obj/item/construction/plumbing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/rcd/RPLD.dm#L2); tgui: [`RapidPlumbingDevice.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/RapidPlumbingDevice.tsx)
- **Статус в ss13-rs:** ❌ нет

# BandaStation: модпак aesthetics
Перерисовки станции форка (`modular_bandastation/aesthetics`): новые спрайты и звуки для шлюзов, стен, окон, настенных машин, предметов. Механик почти не добавляет; атрибуция ресурсов — в [`attributions.txt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/attributions.txt).

### Шлюзы и сборки шлюзов
Новые спрайты всех станционных шлюзов, их сборок и GAGS-материальных шлюзов, свои звуки открытия и болтов; индикатор «открыто» на запитанном шлюзе (оверлей `poweron`, не горит при аварийном доступе/РЕТА).

- **Код:** [`/obj/machinery/door/airlock`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/airlocks/code/airlock.dm#L1), [`update_overlays()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/airlocks/code/airlock.dm#L12), [`airlock_types.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/airlocks/code/airlock_types.dm), [`airlock_assembly_types.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/airlocks/code/airlock_assembly_types.dm)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_dm_extract` (весь `tgstation.dme` с модпаками), `crates/ss13_game/src/doors.rs`: спрайты форка; нет оверлея `poweron` и звуков (звука в ss13-rs нет вовсе)

### Стены, скалы и сглаживание
Новые спрайты стен (обычных, усиленных, ржавых и т.д.) и минеральных скал; форк переопределяет группы сглаживания окон, стен и шлюзов, чтобы окна стыковались со стенами и шлюзами.

- **Код:** [`/turf/closed/wall`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/walls/code/walls.dm#L2), [`/turf/closed/mineral`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/walls/code/rocks.dm#L16), [`/obj/structure/window/fulltile`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/smoothing/code/smoothing.dm#L9)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_mapload/src/smooth.rs`: битмаск-сглаживание по `smoothing_groups`/`canSmoothWith` из DM-данных с модпаками; `MAP_SWITCH` берёт рантайм-иконку (`crates/ss13_dm_extract/src/lib.rs`)

### Окна с окантовкой
Окна перекрашены цветом (обычные, плазменные, тонированные) и получают отдельный оверлей окантовки `edge-<junction>` своего цвета.

- **Код:** [`/obj/structure/window`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/windows/code/windows.dm#L7), [`update_overlays()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/windows/code/windows.dm#L19)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_mapload/src/look.rs`: `edge_overlay_file`/`edge_overlay_color` рисуются поверх сглаженного окна

### Освещение
Новые спрайты ламп и оверлей, тёплый цвет лампы `#FFF7F2`, мощность 0.8, ночной режим 0.6; звук выключателя.

- **Код:** [`/obj/machinery/light`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/lights/code/lights.dm#L1), [`interact()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/lightswitch/code/lightswitch.dm#L4)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload/src/light.rs`: `bulb_colour`/`bulb_power` форка; нет ночного режима и звука выключателя

### Огонь
Оверлей горения и спрайты огня форка.

- **Код:** [`aesthetics/fire`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/fire)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/fire.rs` (`BURNING_DMI`), `crates/ss13_mapload/src/atmos.rs`: используется иконка `modular_bandastation/aesthetics/fire/icons/fire.dmi`

### Настенные машины и двери
Перерисовки воздушной тревоги, APC, пожарной тревоги, выключателя, камер, авторизатора ключ-карт, шкафчика огнетушителя, гермодверей (blast doors), окон-шлюзов, мусоропровода, СМЕС, микроволновки.

- **Код:** [`airalarm.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/airalarm/code/airalarm.dm), [`apc.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/apc/code/apc.dm), [`firealarm.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/firealarm/code/firealarm.dm), [`cameras.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/cameras/code/cameras.dm), [`keycard_auth.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/keycard_auth/code/keycard_auth.dm), [`extinguisher_cabinet.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/extinguisher_cabinet/code/extinguisher_cabinet.dm), [`blastdoor.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/blastdoor/code/blastdoor.dm), [`windoors.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/windoors/code/windoors.dm), [`disposal.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/disposal/code/disposal.dm), [`smes.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/smes/code/smes.dm), [`microwave.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/kitchen/code/microwave.dm)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_dm_extract`, `crates/ss13_assets`: `icon` переопределены в DM-данных и импортируются; сами машины — в своих разделах

### Перерисовки предметов и мебели
Коробки (373 строки иконок), мыло (в т.ч. мыло НТ), простыни ЦК и революции, комод, стеллаж, перила, наушники ТСФ, торговые автоматы и пополнения, полы, снаряды.

- **Код:** [`/obj/item/storage/box`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/boxes/code/boxes.dm#L1), [`/obj/item/soap`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/soap/code/soap.dm#L1), [`bedsheet.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/bedsheet/code/bedsheet.dm), [`dresser.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/dresser/code/dresser.dm), [`rack.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/rack/code/rack.dm), [`railings.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/railings/code/railings.dm), [`radio.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/radio/code/radio.dm), [`vendors_restock.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/vendors_restock/code/vendors_restock.dm), [`floors.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/floors/code/floors.dm), [`projectiles.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/projectiles/code/projectiles.dm)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_dm_extract`, `crates/ss13_assets`: иконки берутся из DM-данных с модпаками

### Гардеробы ЦК и адвоката
Автоматы-гардеробы ЦК и юристов с наборами формы.

- **Код:** [`/obj/machinery/vending/wardrobe/cent_wardrobe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/vending/code/vending.dm#L1), [`/obj/machinery/vending/wardrobe/law_wardrobe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/vending/code/vending.dm#L44)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/vending.rs`: работают как автоматы с ассортиментом; без оплаты

### Телепорт-поды
Поды доставки в стиле телепорта: портал вместо капсулы, без взрыва и дыма при посадке; используются маяками выбора и королевской битвой.

- **Код:** [`/obj/structure/closet/supplypod/teleporter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/aesthetics/supply_pods/code/supply_pods.dm#L1)
- **Статус в ss13-rs:** ❌ нет — подов нет (см. ../cargo/)

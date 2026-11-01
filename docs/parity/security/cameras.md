# Камеры наблюдения
Сеть камер станции, их обслуживание, консоли и телеэкраны для просмотра, нательные камеры.

### Камера наблюдения
Настенная камера с тегом (`c_tag`) и сетями (`network`: SS13, RD, minisat, ...); видимые ей тайлы доступны консолям и ИИ. Отвёрткой открывается панель, кусачками выключается/включается, ЭМИ отключает на 90 с, разрушается от урона; может светить подсветкой ИИ.

- **Код:** [`/obj/machinery/camera`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/camera/camera.dm#L26), [`emp_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/camera/camera.dm#L228), [`proc/toggle_cam()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/camera/camera.dm#L359), [`proc/can_use()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/camera/camera.dm#L407)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/silicon/cameras.rs`: камеры из карты (`c_tag`, `network`, `view_range`, `camera_enabled`; `ss13_mapload::silicon::DmCamera` с правками карты), видимое ими — в сети камер для ИИ и консолей; нет панели, кусачек, ЭМИ, разрушения, питания и подсветки

### Сборка и разборка камеры
Настенный каркас камеры (`wallframe`), сварка, кабель, отвёртка; инструменты на установленной камере (сварка снимает её, гаечный ключ, кусачки), установка улучшений в панель.

- **Код:** [`/obj/item/wallframe/camera`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/camera/camera.dm#L14), [`welder_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/camera/camera_construction.dm#L1), [`item_interaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/camera/camera_construction.dm#L226)
- **Статус в ss13-rs:** ❌ нет

### Улучшения и пресеты камер
Защита от ЭМИ (пласталь), рентген (X-ray, через анализатор), датчик движения (модуль-датчик) — ставятся в открытую камеру; пресеты карты: `emp_proof`, `xray`, `motion`, `all`, `autoname` (автоимя по зоне), камеры экзокостюмов, ордананса и т. п.

- **Код:** [`proc/upgradeEmpProof()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/camera/presets.dm#L158), [`proc/upgradeXRay()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/camera/presets.dm#L185), [`proc/upgradeMotion()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/camera/presets.dm#L211), [`/obj/machinery/camera/autoname`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/camera/presets.dm#L72), [`/obj/machinery/camera/exosuit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/camera/presets.dm#L120)
- **Статус в ss13-rs:** 🟡 частично — пресеты карты переносятся как типы (сети, `c_tag`), `autoname` даёт «<зона> #<n>» (`crates/ss13_game/src/silicon/cameras.rs`); улучшений (ЭМИ-защита, рентген, движение) нет

### Камеры с датчиком движения
Камера с модулем движения отслеживает мобов в поле зрения и поднимает тревогу камеры для ИИ и консолей (тревоги «Camera»), сбрасывает её, когда цель ушла.

- **Код:** [`/obj/machinery/camera/motion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/camera/presets.dm#L45), [`process()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/camera/motion.dm#L1), [`proc/triggerAlarm()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/camera/motion.dm#L49)
- **Статус в ss13-rs:** ❌ нет

### Сеть камер и статика (SScameras)
Подсистема делит карту на чанки 16×16, считает, какие тайлы видят камеры, и закрывает остальное «статикой» для глаз ИИ и консолей; обновляет видимость при изменении стен и перемещении камер.

- **Код:** [`SScameras`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/cameras.dm#L28), [`/datum/camerachunk`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/silicon/ai/freelook/chunk.dm#L6), [`/mob/eye/camera`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/eye/camera/camera.dm#L5), [`/obj/effect/overlay/camera_static`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/cameras.dm#L329)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/silicon/cameras.rs` `CameraNet`: чанки 8×8 (текущий tg `CHUNK_SIZE`), камеры в радиусе `MAX_CAMERA_RANGE`, `view()` с непрозрачностью (симметричный shadowcasting), пересчёт при смене непрозрачности (двери, стены) и камер; статика у ИИ и консолей — тёмные тайлы (`silicon/client.rs`), а не анимированная `cameravis.dmi`; чанки строятся при старте, а не лениво; сущности за статикой всё равно реплицируются клиенту (рисуются закрытыми)

### Консоль камер наблюдения
Компьютер со списком камер своих сетей; выбранная камера показывается в окне tgui (отдельная карта-вид), без доступных камер — статика. Подвиды: деревянный ТВ, горнодобывающая, научная, ГСБ, трудовой лагерь, КМ.

- **Код:** [`/obj/machinery/computer/security`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/camera.dm#L3), [`ui_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/camera.dm#L97); tgui: [`CameraConsole220.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/CameraConsole220.tsx), [`CameraConsole.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/CameraConsole.tsx)
- **BandaStation:** [`code/game/machinery/computer/camera.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/camera.dm#L67) — свой интерфейс `CameraConsole220` с наномапой (координаты и статус камер в [`SScameras`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/cameras.dm#L273)), без автообновления
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/silicon/cameras.rs`, окно `CameraConsole220` (`silicon/client.rs`): список камер своих сетей (сортировка по `c_tag`), `switch_camera` с проверкой ссылки на сервере, назад/вперёд; вид карты игрока переносится к камере (вьюер репликации) со статикой вне её обзора, закрытие окна возвращает вид; нет наномапы и поиска, отдельной карты-вида в окне нет (используется основной вид)

### Телеэкраны
Настенные экраны, привязанные к одной сети камер (РД, ГИ, СМО, хранилище, двигатель, тюрьма, изолятор, ...); развлекательный экран показывает трансляции (шоу, ТВ-камера) и уведомляет о начале эфира.

- **Код:** [`/obj/machinery/computer/security/telescreen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/telescreen.dm#L1), [`/obj/machinery/computer/security/telescreen/entertainment`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/telescreen.dm#L41)
- **Статус в ss13-rs:** 🟡 частично — подтипы `/obj/machinery/computer/security/telescreen` получают ту же консоль со своими сетями (`crates/ss13_game/src/silicon/cameras.rs`); трансляций развлекательного экрана нет

### Продвинутая консоль камер
Консоль, дающая игроку «глаз» камеры, свободно летающий по видимым камерам зонам (с переключением уровней), как у ИИ; синдикатский вариант; компоненты схем для неё.

- **Код:** [`/obj/machinery/computer/camera_advanced`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/camera_advanced.dm#L1), [`/datum/action/innate/camera_jump`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/camera_advanced.dm#L224)
- **Статус в ss13-rs:** ❌ нет

### Нательные камеры (bodycam)
Компонент, превращающий предмет в движущуюся камеру сети; в форке — аксессуар-бодикам на форме с переключением, ставящийся на паузу, отдельный тип камеры, отключение просмотра на консоли, продажа в вендоре для подписчиков.

- **Код:** [`/datum/component/simple_bodycam`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/simple_bodycam.dm#L2)
- **BandaStation:** [`modular_bandastation/~components/code/pausable_bodycam.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/~components/code/pausable_bodycam.dm#L2), [`modular_bandastation/~components/code/bodycam_camera.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/~components/code/bodycam_camera.dm#L2), [`modular_bandastation/objects/code/items/clothing/accessories/bodycam.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/clothing/accessories/bodycam.dm#L8), [`code/game/machinery/computer/camera.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/computer/camera.dm#L116) — бодикам-аксессуар и реакция консоли на отключение камеры
- **Статус в ss13-rs:** ❌ нет

### Отслеживание через камеры (trackable)
Датум, позволяющий ИИ и консолям следовать за мобом по камерам, пока его видно (с учётом блокирующих отслеживание вещей).

- **Код:** [`/datum/trackable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/camera/trackable.dm#L4)
- **Статус в ss13-rs:** ❌ нет

# Турели, сканер-ворота и баррикады
Автоматическая защита (турели и их пульты), досмотровые ворота и переносные заграждения.

### Портативная турель
Скрывается под крышкой и выдвигается при цели в радиусе 7; стреляет оглушающими или летальными снарядами (`stun_projectile`/`lethal_projectile`, задержка 1.5 с). Оценка угрозы (`assess_perp`): статус розыска в записях, неавторизованное оружие без доступа, отсутствие ID, отсутствие майндщита, киборги, главы — по флагам; ЭМИ/емаг сбивают настройки. Настраивается ID с доступом через окно tgui.

- **Код:** [`/obj/machinery/porta_turret`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/porta_turret/portable_turret.dm#L27), [`proc/assess_perp()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/porta_turret/portable_turret.dm#L587), [`proc/shootAt()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/porta_turret/portable_turret.dm#L637), [`proc/popUp()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/porta_turret/portable_turret.dm#L548); tgui: [`PortableTurret.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PortableTurret.jsx)
- **Статус в ss13-rs:** ❌ нет

### Варианты турелей
Турели ИИ (ядро и минисат), синдиката (баллистические, энергетические, тяжёлые, на подах и шаттлах), ЦК-шаттла, вспомогательной базы шахтёров, лазертага (красная/синяя); ручное управление турелью (`turret_toggle`).

- **Код:** [`/obj/machinery/porta_turret/ai`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/porta_turret/portable_turret.dm#L888), [`/obj/machinery/porta_turret/syndicate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/porta_turret/portable_turret.dm#L766), [`/obj/machinery/porta_turret/centcom_shuttle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/porta_turret/portable_turret.dm#L919), [`/obj/machinery/porta_turret/aux_base`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/porta_turret/portable_turret.dm#L895), [`/obj/machinery/porta_turret/lasertag`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/porta_turret/portable_turret.dm#L1016)
- **Статус в ss13-rs:** ❌ нет

### Пульт управления турелями
Настенный пульт зоны: включает/выключает все турели зоны и переключает оглушение/убийство; блокируется ID, ИИ может быть отключён от него; Alt-клик — смена режима.

- **Код:** [`/obj/machinery/turretid`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/porta_turret/turret_control.dm#L1), [`/obj/item/wallframe/turret_control`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/porta_turret/turret_control.dm#L233); tgui: [`TurretControl.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/TurretControl.tsx)
- **Статус в ss13-rs:** ❌ нет

### Сборка турели
Пошаговая сборка рамы: металл, гаечный ключ, энергооружие внутрь (определяет снаряды), датчик движения, сварка; крышка турели с замком.

- **Код:** [`/obj/machinery/porta_turret_construct`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/porta_turret/portable_turret_construct.dm#L11), [`/obj/machinery/porta_turret_cover`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/porta_turret/portable_turret_cover.dm#L6)
- **Статус в ss13-rs:** ❌ нет

### Сканер-ворота (scanner gate)
Рамка, сканирующая проходящих в выбранном режиме: майндщит, болезни (порог тяжести), оружие, розыск, вид, голод/полнота; при срабатывании — тревога и красная полоса, можно инвертировать; настройка по ID, панель с проводами, емаг ломает. Пресет на оружие.

- **Код:** [`/obj/machinery/scanner_gate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/scanner_gate.dm#L9), [`proc/perform_scan()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/scanner_gate.dm#L192), [`/datum/wires/scanner_gate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/wires/scanner_gate.dm#L1), [`/obj/machinery/scanner_gate/preset_guns`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/scanner_gate.dm#L372); tgui: [`ScannerGate.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ScannerGate.jsx)
- **BandaStation:** [`modular_bandastation/tts/code/base_seeds/objs/objs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/tts/code/base_seeds/objs/objs.dm#L189) — голос TTS у сканер-ворот
- **Статус в ss13-rs:** ❌ нет

### Баррикады
Блокирующие проход структуры: деревянные (из досок, грубые, снежные), мешки с песком, охранная баррикада; ломаются уроном, частично защищают от выстрелов.

- **Код:** [`/obj/structure/barricade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/deployable.dm#L11), [`/obj/structure/barricade/wooden`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/deployable.dm#L66), [`/obj/structure/barricade/sandbags`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/deployable.dm#L113), [`/obj/structure/barricade/security`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/deployable.dm#L132)
- **Статус в ss13-rs:** ❌ нет

### Граната-баррикада и складной пулемёт
Граната охраны разворачивает баррикаду выбранного типа (стена, коридор, ...); складной тяжёлый пулемёт разворачивается в стационарную турель.

- **Код:** [`/obj/item/grenade/barrier`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/deployable.dm#L149), [`/obj/item/deployable_turret_folded`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/machinery/deployable.dm#L205)
- **Статус в ss13-rs:** ❌ нет

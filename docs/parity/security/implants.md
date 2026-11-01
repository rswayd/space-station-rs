# Импланты
Подкожные импланты: базовая механика внедрения, инструменты, охранные импланты и прочие разновидности. Удаление хирургией — см. ../medicine/.

### Механика имплантов
Имплант внедряется в моба (`implant()`), может давать кнопку действия (`activate()`), удаляется при извлечении (`removed()`); виден охранным HUD и консолью заключённых; часть имплантов несовместима друг с другом или имеет ограниченное число использований.

- **Код:** [`/obj/item/implant`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant.dm#L4), [`proc/implant()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant.dm#L64), [`proc/removed()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant.dm#L128), [`proc/activate()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant.dm#L32)
- **Статус в ss13-rs:** ❌ нет — имплантов нет (`crates/ss13_mapload/src/antag.rs` прямо исключает их из каталога аплинка)

### Имплантер, футляр и имплант-пад
Имплантер вводит заряженный имплант в цель (с задержкой, если не в себя); футляр хранит имплант и заряжает имплантер; имплант-пад показывает описание импланта из футляра.

- **Код:** [`/obj/item/implanter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implanter.dm#L4), [`/obj/item/implantcase`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implantcase.dm#L4), [`/obj/item/implantpad`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implantpad.dm#L1); tgui: [`ImplantPad.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ImplantPad.tsx)
- **Статус в ss13-rs:** ❌ нет

### Кресло имплантации
Машина: пристёгнутому вводит импланты из своего запаса (майндщит), варианты генной очистки и промывки мозгов.

- **Код:** [`/obj/machinery/implantchair`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implantchair.dm#L1), [`/obj/machinery/implantchair/brainwash`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implantchair.dm#L173); tgui: [`ImplantChair.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/ImplantChair.jsx)
- **Статус в ss13-rs:** ❌ нет

### Майндщит (mindshield)
Защищает от промывки мозгов и конверсии (революция, культы), снимает статус революционера; значок на охранном HUD; выдаётся главам и охране при появлении.

- **Код:** [`/obj/item/implant/mindshield`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant_mindshield.dm#L1), [`implant()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant_mindshield.dm#L12)
- **Статус в ss13-rs:** ❌ нет

### Трекер
Показывает местоположение носителя на консоли заключённых и служит целью для телепортера; вариант от пули .38 и трекер-GPS.

- **Код:** [`/obj/item/implant/tracking`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/security/implant_track.dm#L1), [`/obj/item/implant/tracking/c38`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/security/implant_track.dm#L60)
- **Статус в ss13-rs:** ❌ нет

### Химический имплант
Хранит реагенты из шприца и вводит их носителю по команде с консоли заключённых или при смерти.

- **Код:** [`/obj/item/implant/chem`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/security/implant_chem.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Импланты изгнания, блокировки телепорта и маяк
Изгнание не даёт вернуться на станцию через врата; блокировщик телепорта запрещает телепортацию; маяк-имплант служит меткой для телепорта.

- **Код:** [`/obj/item/implant/exile`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/security/implant_exile.dm#L4), [`/obj/item/implant/teleport_blocker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/security/implant_noteleport.dm#L2), [`/obj/item/implant/beacon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/security/implant_beacon.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Взрывные импланты
Микробомба (взрыв при смерти или по команде, сила от числа имплантов), макробомба, «отрицание» (без следов), бомба дедматча и королевской битвы.

- **Код:** [`/obj/item/implant/explosive`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant_explosive.dm#L13), [`/obj/item/implant/explosive/macro`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant_explosive.dm#L177), [`/obj/item/implant/explosive/deniability`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant_explosive.dm#L208), [`/obj/item/implant/explosive/battle_royale`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant_battle_royale.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Импланты побега и скрытности
Свобода (снимает наручники/путы, несколько использований), ЭМИ-имплант, дымовой, хранилище (блюспейс-карман), стелс (картонная коробка агента).

- **Код:** [`/obj/item/implant/freedom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant_freedom.dm#L1), [`/obj/item/implant/emp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant_misc.dm#L17), [`/obj/item/implant/smoke`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant_misc.dm#L42), [`/obj/item/implant/storage`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant_storage.dm#L1), [`/obj/item/implant/stealth`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant_stealth.dm#L1)
- **BandaStation:** [`modular_bandastation/contraband/code/implant_misc.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/contraband/code/implant_misc.dm#L1) — имплантер ЭМИ помечен контрабандой
- **Статус в ss13-rs:** ❌ нет

### Служебные импланты
Радио-имплант (шахтёрский, синдиката, слаймов), авторизация оружия, «предсмертный хрип» (deathrattle: сообщает группе о смерти), тактическая карта, грустный тромбон клоуна.

- **Код:** [`/obj/item/implant/radio`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant_misc.dm#L67), [`/obj/item/implant/weapons_auth`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant_misc.dm#L1), [`/obj/item/implant/deathrattle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant_deathrattle.dm#L112), [`/obj/item/implant/tacmap`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant_tacmap.dm#L1), [`/obj/item/implant/sad_trombone`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant_clown.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Импланты боевых искусств и заклинаний
Kaza Ruk (обучает тизирианскому боевому искусству), имплант заклинания, имплант абдуктора; аплинк-имплант — см. ../antagonists/.

- **Код:** [`/obj/item/implant/kaza_ruk`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant_kaza_ruk.dm#L1), [`/obj/item/implant/spell`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant_spell.dm#L1), [`/obj/item/implant/abductor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/implants/implant_abductor.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Имплант CQC (BandaStation)
Обучает боевому искусству Close Quarters Combat; запрещённый имплант.

- **Код:** [`/obj/item/implant/cqc`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/implants/code/implant_cqc.dm#L1)
- **BandaStation:** [`modular_bandastation/implants/code/implant_cqc.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/implants/code/implant_cqc.dm#L1) — новый имплант, имплантер и футляр
- **Статус в ss13-rs:** ❌ нет

### Сигнализация смерти и ЭМИ-поглотитель (BandaStation)
Сигнализация смерти сообщает по радио о смерти носителя и месте; ЭМИ-поглотитель защищает носителя от ЭМИ.

- **Код:** [`/obj/item/implant/death_alarm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/implants/death_alarm.dm#L1), [`/obj/item/implant/empprotection`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/implants/empprotection.dm#L1)
- **BandaStation:** [`modular_bandastation/objects/code/items/implants/death_alarm.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/objects/code/items/implants/death_alarm.dm#L1) — новые импланты форка
- **Статус в ss13-rs:** ❌ нет

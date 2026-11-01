# Баллистическое оружие
Оружие с магазинами и патронами: механика затвора и семейства. Боеприпасы — [ammo-projectiles.md](ammo-projectiles.md).

### Механика баллистики
Патрон досылается в патронник, магазин вставляется/извлекается (тактическая перезарядка), затвор передёргивается (`rack`) или закрывается; виды затворов: обычный, открытый, помповый, болтовой, барабан; обрез ножовкой (`sawoff`), чистка оружия.

- **Код:** [`/obj/item/gun/ballistic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic.dm#L3), [`proc/chamber_round()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic.dm#L462), [`proc/rack()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic.dm#L480), [`proc/insert_magazine()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic.dm#L509), [`proc/eject_magazine()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic.dm#L530)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/guns.rs`: магазины, патронник, затворы (standard/open/locking/no bolt), выброс гильз, тактическая перезарядка, взвод/спуск затвора; нет осечек, обреза, глушителя

### Пистолеты
Макаров-подобный «Стечкин» (синдикат), Деагл и вариации, APS, контрабандные и клоунские пистолеты; в форке — линейки NT (CM-70, PDH, CM-23, CM-357, GP-9, 1911), СССП (Защитник), ТСФ (Wespe), Элизиум (TP-14).

- **Код:** [`/obj/item/gun/ballistic/automatic/pistol`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/pistol.dm#L1), [`/obj/item/gun/ballistic/automatic/pistol/deagle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/pistol.dm#L170), [`/obj/item/gun/ballistic/automatic/pistol/aps`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/pistol.dm#L237)
- **BandaStation:** [`modular_bandastation/weapon/code/ranged/ballistic/nanotrasen/pistols/cm70.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/weapon/code/ranged/ballistic/nanotrasen/pistols/cm70.dm#L1) — пистолеты фракций форка (`nanotrasen/`, `ussp/`, `tsf/`, `elyzium/`)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/guns.rs`: Макаров (9 мм) и прочие пистолеты на общей механике; без обвесов

### Револьверы
.357 (синди), детективный .38, Матеба, Наган, золотой, «русская рулетка», реверсивный (стреляет во владельца); в форке — Бобр и двуствольный «Двойстовл».

- **Код:** [`/obj/item/gun/ballistic/revolver`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/revolver.dm#L1), [`/obj/item/gun/ballistic/revolver/russian`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/revolver.dm#L212), [`/obj/item/gun/ballistic/revolver/reverse`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/revolver.dm#L410)
- **BandaStation:** [`modular_bandastation/weapon/code/ranged/ballistic/ussp/pistols/bobr.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/weapon/code/ranged/ballistic/ussp/pistols/bobr.dm#L1) — револьверы СССП
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/guns.rs`: барабан хранит стреляные гильзы, разрядка в руке, зарядка из спидлоадера/коробки по калибру; без вращения барабана и пустых гнёзд

### Пистолеты-пулемёты и автоматы
Прото-SMG (ПП синдиката C-20r), Узи, Томпсон, автомат NT, боевая винтовка, смартган; в форке — CM-5, CM-15, F4, F90, CM-82, Бизон, Витязь, Ланца, Сабля, Mięcz, Carwo, Sindano, FN-18, FN-4, AS-32.

- **Код:** [`/obj/item/gun/ballistic/automatic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/automatic.dm#L1), [`/obj/item/gun/ballistic/automatic/proto`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/automatic.dm#L17), [`/obj/item/gun/ballistic/automatic/mini_uzi`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/automatic.dm#L120), [`/obj/item/gun/ballistic/automatic/tommygun`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/automatic.dm#L191), [`/obj/item/gun/ballistic/automatic/ar`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/automatic.dm#L227), [`/obj/item/gun/ballistic/automatic/battle_rifle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/automatic.dm#L342)
- **BandaStation:** [`modular_bandastation/weapon/code/ranged/ballistic/tsf/rifles/carwo.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/weapon/code/ranged/ballistic/tsf/rifles/carwo.dm#L1) — автоматическое оружие фракций форка
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/guns.rs`: C-20r и прочие на общей механике с очередью; без переключения режима огня

### Дробовики
Помповые (боевой, охранный riot), автоматические (combat, двухтрубный), Bulldog, двустволка (обрезается), крюк-дробовик, мушкет; в форке — Renoster и одноручный riot.

- **Код:** [`/obj/item/gun/ballistic/shotgun`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/shotgun.dm#L1), [`/obj/item/gun/ballistic/shotgun/riot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/shotgun.dm#L44), [`/obj/item/gun/ballistic/shotgun/automatic/combat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/shotgun.dm#L86), [`/obj/item/gun/ballistic/shotgun/bulldog`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/shotgun.dm#L186), [`/obj/item/gun/ballistic/shotgun/doublebarrel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/shotgun.dm#L338)
- **BandaStation:** [`modular_bandastation/weapon/code/ranged/ballistic/tsf/special/renoster.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/weapon/code/ranged/ballistic/tsf/special/renoster.dm#L1) — дробовики форка
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/guns.rs`: помповые (взвод Z, стреляная гильза остаётся до взвода), автоматические (взвод после выстрела), зарядка по одному патрону, дробь, `WEAPON_HEAVY`; riot shotgun из спавнера оружейной MetaStation; нет обреза, двухтрубного cycler, двустволки (открытие стволов)

### Винтовки: болтовые, снайперские, самодельные
Мосинка (болтовка, обрез), трубчатые самодельные «pipegun», арбалет-арматурник, гарпун; снайперская винтовка синдиката; в форке — Wylom, HLRM, болтовка СССП.

- **Код:** [`/obj/item/gun/ballistic/rifle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/rifle.dm#L1), [`/obj/item/gun/ballistic/rifle/boltaction`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/rifle.dm#L51), [`/obj/item/gun/ballistic/rifle/boltaction/pipegun`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/rifle.dm#L343), [`/obj/item/gun/ballistic/rifle/rebarxbow`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/rifle.dm#L237)
- **BandaStation:** [`modular_bandastation/weapon/code/ranged/ballistic/ussp/rifles/wylom.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/weapon/code/ranged/ballistic/ussp/rifles/wylom.dm#L1) — винтовки форка
- **Статус в ss13-rs:** ❌ нет

### Пулемёты и пусковые установки
L6 SAW, гранатомёты (револьверный, подствольный), гиропистолет, ракетница PML-9; в форке — ДШК, «Волна», РПГ, ПМК, станковый пулемёт M2T9.

- **Код:** [`/obj/item/gun/ballistic/revolver/grenadelauncher`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/launchers.dm#L4), [`/obj/item/gun/ballistic/rocketlauncher`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/launchers.dm#L73), [`/obj/item/gun/ballistic/automatic/gyropistol`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/launchers.dm#L62)
- **BandaStation:** [`modular_bandastation/weapon/code/ranged/ballistic/mounted_machine_gun.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/weapon/code/ranged/ballistic/mounted_machine_gun.dm#L1) — станковый пулемёт и тяжёлое оружие форка
- **Статус в ss13-rs:** ❌ нет

### Луки
Длинный, короткий, божественный (капеллан), пепельный лук; стрелы натягиваются, разные наконечники.

- **Код:** [`/obj/item/gun/ballistic/bow`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/bows/_bow.dm#L2), [`/obj/item/gun/ballistic/bow/longbow`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/ballistic/bows/bow_types.dm#L3)
- **Статус в ss13-rs:** ❌ нет

_«Игрушечное оружие» описано в [../items/toys.md](../items/toys.md)._

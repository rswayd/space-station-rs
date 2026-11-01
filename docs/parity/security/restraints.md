# Наручники и путы
Средства обездвиживания: наручники всех видов, освобождение из них, ловушки и бола для ног.

### Наручники (обычные)
Клик наручниками по соседнему гуманоиду запускает надевание (`handcuff_time`, 4 с у обычных); закованный роняет предметы, не может пользоваться руками, его можно тащить. Освобождение — сопротивление (`breakouttime`: наручники 60 с) или снятие другим игроком через меню раздевания.

- **Код:** [`/obj/item/restraints/handcuffs`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/handcuffs.dm#L33), [`proc/attempt_to_cuff()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/handcuffs.dm#L87), [`/mob/living/carbon/proc/set_handcuffed()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L1119)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/restraints.rs`, `crates/ss13_mapload/src/security.rs`: надевание с `do_after`, оверлей, блок рук, таскание, снятие пустой рукой; нет меню раздевания (клик заменяет строку наручников), нет самоприковывания неуклюжих, нет проверки числа рук, нет кусачек на слабых наручниках, нет выдачи киборгами

### Кабельные стяжки и пластиковые стяжки
Наручники из кабеля (все цвета, а также из жил) и одноразовые стяжки (zipties); быстрее надеваются, слабее держат (кабель 30 с, стяжки 45 с), использованные стяжки повторно не надеваются; есть фальшивые стяжки.

- **Код:** [`/obj/item/restraints/handcuffs/cable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/handcuffs.dm#L201), [`/obj/item/restraints/handcuffs/cable/zipties`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/handcuffs.dm#L345), [`/obj/item/restraints/handcuffs/cable/zipties/used`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/handcuffs.dm#L369), [`/obj/item/restraints/handcuffs/cable/sinew`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/handcuffs.dm#L266)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/restraints.rs`: время надевания/освобождения и одноразовость стяжек есть; нет разрезания кусачками, нет крафта кабельных наручников из кабеля

### Особые наручники
Фальшивые наручники (мгновенный выход), инопланетные, культистские (исчезают при снятии), энергетические абдукторов и войдволкера — отдельные варианты с особым поведением.

- **Код:** [`/obj/item/restraints/handcuffs/fake`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/handcuffs.dm#L189), [`/obj/item/restraints/handcuffs/alien`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/handcuffs.dm#L180), [`/obj/item/restraints/handcuffs/cult`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/handcuffs.dm#L391), [`/obj/item/restraints/handcuffs/energy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/abductor/equipment/gear/abductor_items.dm#L493)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/restraints.rs`: все подтипы `/obj/item/restraints/handcuffs` работают как обычные наручники со своими временами; нет их особых эффектов (исчезновение при снятии и т. п.)

### Сопротивление наручникам
Закованный нажимает «сопротивляться» и после `breakouttime` сбрасывает наручники; движение или оглушение прерывает; некоторые виды могут сломаться (`cuff_break`, у халков — мгновенно).

- **Код:** [`/mob/living/carbon/proc/cuff_resist()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L207)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/restraints.rs` (`resist_restraints`): выход за `breakouttime`; нет ломания наручников (`cuff_break`), нет халка/силы

### Путы для ног (legcuffs)
Закованные ноги: сильное замедление (`slowdown` 7), освобождение сопротивлением за 30 с.

- **Код:** [`/obj/item/restraints/legcuffs`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/handcuffs.dm#L409)
- **Статус в ss13-rs:** ❌ нет — `restraints.rs` явно без legcuffs (shortcut)

### Капкан (beartrap)
Взведённый капкан на полу захлопывается на наступившем: 20 урона ноге и путы на ногах; энергетический капкан (у киборгов и вариант-ловушка) исчезает со временем.

- **Код:** [`/obj/item/restraints/legcuffs/beartrap`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/handcuffs.dm#L432), [`proc/spring_trap()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/handcuffs.dm#L530), [`/obj/item/restraints/legcuffs/beartrap/energy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/handcuffs.dm#L572)
- **Статус в ss13-rs:** ❌ нет

### Бола
Метательное оружие: попав в бегущего, опутывает ноги (выход 3.5 с); тактическая бола ещё и сбивает с ног (3.5 с), энергетическая — от гана/киборга, gonbola — живая и держит 30 с.

- **Код:** [`/obj/item/restraints/legcuffs/bola`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/handcuffs.dm#L607), [`proc/ensnare()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/handcuffs.dm#L649), [`/obj/item/restraints/legcuffs/bola/tactical`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/handcuffs.dm#L663), [`/obj/item/restraints/legcuffs/bola/energy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/handcuffs.dm#L677), [`/obj/item/restraints/legcuffs/bola/gonbola`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/handcuffs.dm#L704)
- **BandaStation:** [`modular_bandastation/contraband/code/legcuffs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/contraband/code/legcuffs.dm#L1) — тактическая бола помечена как контрабанда (`TRAIT_CONTRABAND`)
- **Статус в ss13-rs:** ❌ нет

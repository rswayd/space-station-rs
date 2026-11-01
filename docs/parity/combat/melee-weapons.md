# Холодное оружие и щиты
Семейства оружия ближнего боя. Общая цепочка атаки — [melee-attacks.md](melee-attacks.md). Батоны, наручники и флешки — у агента security (см. ../security/). В ss13-rs любой предмет в руке бьёт своим `force`/`damtype`, поэтому простое оружие работает частично, а оружие с собственной механикой — нет.

### Ножи
Кухонный, мясницкий, охотничий, боевой, выживальщика, заточки (шив), ритуальный, кровопускатель; острые: режут еду, разделывают, вызывают кровотечение, встраиваются при броске.

- **Код:** [`/obj/item/knife`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/knives.dm#L8), [`/obj/item/knife/combat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/knives.dm#L184), [`/obj/item/knife/shiv`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/knives.dm#L273), [`/obj/item/knife/butcher`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/knives.dm#L146)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/combat.rs`: удар по `force`; нет остроты (ран, кровотечения), встраивания, разделки; нарезка еды — `crates/ss13_game/src/food.rs`

### Мечи и сабли
Клеймор (и гладиус, абордажная сабля), катана, сабля капитана (парирование, ножны), высокочастотный клинок, его варианты; в форке — виброклинок, катана ТСФ «Дыхание Ночи», рапиры и катаны ЦК, легендарные сабли ЦК с зачарованиями.

- **Код:** [`/obj/item/claymore`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/claymore.dm#L1), [`/obj/item/katana`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/katana.dm#L1), [`/obj/item/melee/sabre`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/sabre.dm#L1), [`/obj/item/highfrequencyblade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/high_frequency_blade.dm#L1)
- **BandaStation:** [`modular_bandastation/weapon/code/melee/vibroblade.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/weapon/code/melee/vibroblade.dm#L1) — виброклинки, `tsf_katana.dm`, `centcom/` (рапиры, легендарные сабли, бита ЦК)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/combat.rs`: удар по `force`; нет блоков/парирования, остроты, ножен, особых способностей

### Энергетическое оружие ближнего боя
Энергомеч (включается, отражает лазеры, блокирует), двойной энергомеч (двуручный, крутится), энергетический топор, пиратский меч, клинок-рука (генокрад), световой клинок.

- **Код:** [`/obj/item/melee/energy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/energy.dm#L1), [`/obj/item/melee/energy/sword`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/energy.dm#L181), [`/obj/item/dualsaber`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/dualsaber.dm#L4), [`/obj/item/melee/energy/axe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/energy.dm#L135), [`/obj/item/melee/energy/blade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/energy.dm#L343)
- **Статус в ss13-rs:** ❌ нет

### Двуручное оружие: топоры, копья, бензопилы
Пожарный топор (двуручный, рубит окна и двери), абордажный топор, копья (досягаемость, взрывное копьё), бензопила (включается, двуручная), кувалды форка.

- **Код:** [`/obj/item/fireaxe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/fireaxe.dm#L6), [`/obj/item/spear`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/spear.dm#L3), [`/obj/item/spear/explosive`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/spear.dm#L287), [`/obj/item/chainsaw`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/chainsaw.dm#L3)
- **BandaStation:** [`modular_bandastation/weapon/code/melee/sledgehammer.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/weapon/code/melee/sledgehammer.dm#L1) — кувалды (обычная, тактическая для выбивания дверей, синдикатская)
- **Статус в ss13-rs:** ❌ нет — нет двуручного хвата (`two_handed`), без него урон tg не воспроизводится

### Дубинки, биты и кнуты
Бейсбольная бита (хоумран отбрасывает), цепь командования капитана, кнут куратора (выбивает оружие), булава капеллана, посохи, метла, «sord».

- **Код:** [`/obj/item/melee/baseball_bat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/baseball_bat.dm#L1), [`/obj/item/melee/chainofcommand`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/chainofcommand.dm#L1), [`/obj/item/melee/curator_whip`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/misc.dm#L74), [`/obj/item/melee/cleric_mace`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/misc.dm#L231)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/combat.rs`: удар по `force`; нет отбрасывания, выбивания, особых эффектов

### Особое оружие
Силовой кулак (газовый, отбрасывает), меч из суперматерии (дезинтегрирует), Его Милость (голодное оружие, пожирает), коса душ, электропосох форка (вариант батона), трость NT форка.

- **Код:** [`/obj/item/melee/powerfist`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/powerfist.dm#L9), [`/obj/item/melee/supermatter_sword`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/supermatter_sword.dm#L1), [`/obj/item/his_grace`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/his_grace.dm#L7), [`/obj/item/soulscythe`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/soulscythe.dm#L3)
- **BandaStation:** [`modular_bandastation/weapon/code/melee/electrostaff.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/weapon/code/melee/electrostaff.dm#L1) — электропосох, трость NT (`baton.dm`)
- **Статус в ss13-rs:** ❌ нет

### Щиты
Баклер, кайт, римский, противоударный (riot, флеш-щит, телескопический), энергетический, баллистический: `block_chance`, прочность, удар щитом.

- **Код:** [`/obj/item/shield`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/shields.dm#L3), [`/obj/item/shield/riot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/shields.dm#L167), [`/obj/item/shield/energy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/shields.dm#L311), [`/obj/item/shield/ballistic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/shields.dm#L459)
- **Статус в ss13-rs:** ❌ нет

### Заточка оружия
Точильный камень повышает `force` острого оружия один раз.

- **Код:** [`/obj/item/sharpener`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/sharpener.dm#L9)
- **Статус в ss13-rs:** ❌ нет

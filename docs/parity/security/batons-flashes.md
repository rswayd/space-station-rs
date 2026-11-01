# Дубинки и вспышки
Нелетальное снаряжение охраны ближнего боя: дубинки всех видов и ручные вспышки. Тазеры и дизейблеры — это оружие (см. ../combat/ или раздел оружия), флешбанг — граната (см. ../items/).

### Полицейская дубинка (классическая)
Без заряда: левый клик вне боевого режима — оглушающий удар (урон выносливости, сбивание с ног на 1.5 с, кулдаун 4 с), в боевом режиме — обычный удар; неуклюжие бьют себя.

- **Код:** [`/obj/item/melee/baton`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/baton.dm#L1), [`proc/finalize_baton_attack()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/baton.dm#L239), [`proc/baton_effect()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/baton.dm#L248)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/security.rs`, `crates/ss13_mapload/src/security.rs`: удар по выносливости, сбивание, кулдаун на цель и пользователя; нет брони против оглушения, неуклюжести, звуков

### Оглушающая дубинка охраны (stun baton)
Включается в руке; заряд ячейки (`cell_hit_cost`) на каждое оглушение: урон выносливости, спутанность 10 с, отложенное на 2 с сбивание с ног; без заряда выключается; ячейку можно вынуть и заменить отвёрткой; ЭМИ разряжает. Есть вариант ГСБ.

- **Код:** [`/obj/item/melee/baton/security`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/baton.dm#L457), [`/obj/item/melee/baton/security/loaded`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/baton.dm#L752), [`attack_self()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/baton.dm#L625), [`proc/apply_stun_effect_end()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/baton.dm#L704)
- **BandaStation:** [`modular_bandastation/weapon/code/melee/baton.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/weapon/code/melee/baton.dm#L136) — дубинка ОБР (`/obj/item/melee/baton/security/loaded/ert`)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/security.rs`: включение, заряд как число, оглушение, отложенное сбивание, спрайты; нет извлечения/замены ячейки, зарядки в зарядниках, ЭМИ, искр и света

### Телескопическая дубинка
Складывается/раскладывается кликом в руке; разложенная оглушает и даёт «оторопь» (daze). Варианты: бронзовая/серебряная/золотая (командные награды), дубинка контрактника (накладывает стяжки).

- **Код:** [`/obj/item/melee/baton/telescopic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/baton.dm#L316), [`proc/on_transform()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/baton.dm#L391), [`/obj/item/melee/baton/telescopic/contractor_baton`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/baton.dm#L424)
- **BandaStation:** [`modular_bandastation/balance/code/weapons/baton.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/weapons/baton.dm#L1) — `stamina_damage = 35` у телескопической дубинки
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/security.rs`: раскрытие, сила, оторопь; нет особенностей контрактной дубинки

### Прочие оглушающие дубинки
Стан-меч (`stunsword`, сменный скин), электропогонялка (`cattleprod`, крафт), телепорт-прут (телепортирует цель), телекристальный прут, бумеранг-дубинка, нунчаки, набор для переделки дубинки.

- **Код:** [`/obj/item/melee/baton/security/stunsword`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/baton.dm#L774), [`/obj/item/melee/baton/security/cattleprod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/baton.dm#L809), [`/obj/item/melee/baton/security/cattleprod/teleprod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/baton.dm#L912), [`/obj/item/melee/baton/security/boomerang`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/baton.dm#L885), [`/obj/item/melee/baton/nunchaku`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/baton.dm#L957), [`/obj/item/conversion_kit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/melee/baton.dm#L309)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/security.rs`: подтипы `/obj/item/melee/baton/security` оглушают как обычная дубинка; нет их особых эффектов (телепорт, бросок бумеранга, крафт, смена скина)

### Электропосох и трость NT (BandaStation)
Двуручная усиленная оглушающая дубинка для командования (электропосох) и трость-дубинка NT, в том числе со встроенным стрелком алмазами.

- **Код:** [`/obj/item/melee/baton/security/electrostaff`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/weapon/code/melee/electrostaff.dm#L1), [`/obj/item/melee/baton/nt_cane`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/weapon/code/melee/baton.dm#L1)
- **BandaStation:** [`modular_bandastation/weapon/code/melee/electrostaff.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/weapon/code/melee/electrostaff.dm#L1) — новые типы дубинок форка
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/security.rs`: электропосох как подтип `/obj/item/melee/baton/security` оглушает как обычная дубинка; нет двуручности, трости-опоры и стрельбы алмазами

### Ручная вспышка
Клик по мобу, смотрящему на вспышку: спутанность, урон выносливости, сбивание с ног; сбоку эффект вдвое слабее, отвернувшийся или в защите глаз (`flash_protect` ≥ 1) не страдает; повреждение глаз; клик в руке — вспышка по всем в радиусе 3 (только спутанность); перезарядка и шанс перегорания от частого использования.

- **Код:** [`/obj/item/assembly/flash`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/flash.dm#L3), [`/obj/item/assembly/flash/handheld`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/flash.dm#L335), [`proc/flash_mob()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/flash.dm#L145), [`proc/AOE_flash()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/flash.dm#L93), [`proc/try_use_flash()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/flash.dm#L114), [`proc/burn_out()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/flash.dm#L75)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/security.rs`: направленная и круговая вспышка, защита глаз, перегорание; нет повреждения глаз и слепоты, нет анимации вспышки, нет линии видимости (только дистанция), нет использования как сборки (assembly) и вспышки-конверсии революционеров

### Особые вспышки
Вспышка киборга (без перегорания, тратит заряд), «memorizer», имплант-вспышка в руке, гипнотическая вспышка (внушение).

- **Код:** [`/obj/item/assembly/flash/cyborg`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/flash.dm#L309), [`/obj/item/assembly/flash/memorizer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/flash.dm#L327), [`/obj/item/assembly/flash/armimplant`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/flash.dm#L338), [`/obj/item/assembly/flash/hypnotic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/assembly/flash.dm#L374)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/security.rs`: подтипы работают как обычная ручная вспышка; нет их особенностей (гипноз, заряд киборга, бесконечность импланта)

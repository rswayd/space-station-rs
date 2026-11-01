# Кибернетика и аугментации
Импланты-органы (`cyberimp`), кибернетические органы и автохирурги; аугментации в настройках персонажа форка. Импланты-капсулы (`/obj/item/implant`: майндшилд, трекер) — у агента security.

### Импланты рук
Встроенные наборы: инструменты, хирургия, лазер/тазер, энергомеч, флеш, батон, медлуч, бумажная работа, боевой; выдвигаются из руки действием; ЭМИ выводит из строя.

- **Код:** [`/obj/item/organ/cyberimp/arm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/cyberimp/augments_arms.dm#L1), [`/obj/item/organ/cyberimp/arm/toolkit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/cyberimp/augments_arms.dm#L51), [`/obj/item/organ/cyberimp/arm/toolkit/surgery`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/cyberimp/augments_arms.dm#L379), [`/obj/item/organ/cyberimp/arm/toolkit/esword`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/cyberimp/augments_arms.dm#L316)
- **Статус в ss13-rs:** ❌ нет

### Импланты груди
Нутриментный (кормит), ревайвер (лечит и запускает сердце), двигатели (полёт в невесомости), позвоночные усилители (Атлас — переноска тяжёлого).

- **Код:** [`/obj/item/organ/cyberimp/chest`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/cyberimp/augments_chest.dm#L1), [`/obj/item/organ/cyberimp/chest/nutriment`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/cyberimp/augments_chest.dm#L7), [`/obj/item/organ/cyberimp/chest/reviver`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/cyberimp/augments_chest.dm#L66), [`/obj/item/organ/cyberimp/chest/thrusters`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/cyberimp/augments_chest.dm#L183), [`/obj/item/organ/cyberimp/chest/spine`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/cyberimp/augments_chest.dm#L295)
- **Статус в ss13-rs:** ❌ нет

### Импланты глаз и мозга
HUD-импланты (мед, охрана, диагностика), анти-дроп, анти-стан, хирургический процессор, коннектор, дыхательная трубка.

- **Код:** [`/obj/item/organ/cyberimp/eyes/hud`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/cyberimp/augments_eyes.dm#L10), [`/obj/item/organ/cyberimp/brain/anti_drop`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/cyberimp/augments_internal.dm#L103), [`/obj/item/organ/cyberimp/brain/anti_stun`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/cyberimp/augments_internal.dm#L165), [`/obj/item/organ/cyberimp/brain/surgical_processor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/cyberimp/augments_internal.dm#L352), [`/obj/item/organ/cyberimp/mouth/breathing_tube`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/cyberimp/augments_internal.dm#L480)
- **Статус в ss13-rs:** ❌ нет

### Кибернетические органы
Сердце, лёгкие, печень, желудок, уши, глаза трёх уровней (surplus, базовый, улучшенный): прочнее, с бонусами, уязвимы к ЭМИ.

- **Код:** [`/obj/item/organ/heart/cybernetic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/heart/_heart.dm#L205), [`/obj/item/organ/lungs/cybernetic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/lungs/_lungs.dm#L989), [`/obj/item/organ/liver/cybernetic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/liver/_liver.dm#L240), [`/obj/item/organ/stomach/cybernetic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/stomach/_stomach.dm#L494), [`eyes_augments.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/internal/eyes/eyes_augments.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Автохирург
Устройство, вживляющее загруженный орган/имплант без операции.

- **Код:** [`/obj/item/autosurgeon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/surgery/organs/autosurgeon.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Аугментации в настройках персонажа (BandaStation)
Модпак форка: в меню персонажа можно выбрать протезы конечностей (бренды: Bishop, Hephaestus, Xion, Zeng-Hu, Morpheus и др.), роботизированные органы, ампутации; поставки аугментаций в карго.

- **Код:** [`/datum/body_modification`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/augmentation_preferences/code/body_modifications/_body_modifications_datum.dm#L13), [`/datum/preference/body_modifications`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/augmentation_preferences/code/preferences/body_modification_preference.dm#L7), [`/datum/body_modification/bodypart_prosthesis`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/augmentation_preferences/code/body_modifications/body_part/prosthetics.dm#L1), [`/datum/body_modification/limb_amputation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/augmentation_preferences/code/body_modifications/body_part/amputations.dm#L1)
- **BandaStation:** [`modular_bandastation/augmentation_preferences/code/body_modifications/_body_modifications_datum.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/augmentation_preferences/code/body_modifications/_body_modifications_datum.dm#L1) — весь модпак
- **Статус в ss13-rs:** ❌ нет

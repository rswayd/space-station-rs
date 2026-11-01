# Навыки и скиллчипы
Навыки разума (`/datum/skill`, опыт и уровни с бонусами к скорости и наградами) и скиллчипы, вставляемые в мозг.

### Каркас навыков: опыт и уровни
Разум хранит опыт по каждому навыку; уровни от «Novice» до «Legendary» дают модификаторы (скорость действия, шанс) и сообщения о повышении; на высшем уровне выдаётся награда (плащ навыка).

- **Код:** [`/datum/skill`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/skills/_skill.dm#L3), [`proc/level_gained()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/skills/_skill.dm#L50), [`proc/try_skill_reward()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/skills/_skill.dm#L70), [`/datum/mind/proc/adjust_experience()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/skills.dm#L13), [`/datum/mind/proc/get_skill_modifier()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/skills.dm#L49)
- **Статус в ss13-rs:** ❌ нет

### Навык уборки
Опыт за уборку грязи; ускоряет мытьё шваброй и тряпкой.

- **Код:** [`/datum/skill/cleaning`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/skills/cleaning.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Навык атлетики
Опыт за тренажёры; уменьшает усталость и усиливает эффект тренировки.

- **Код:** [`/datum/skill/athletics`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/skills/athletics.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Навык добычи (Mining)
Опыт за копание руды; ускоряет кирку и бур.

- **Код:** [`/datum/skill/mining`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/skills/mining.dm#L1)
- **Статус в ss13-rs:** ❌ нет

_«Навык рыбалки» описано в [../service/fishing.md](../service/fishing.md)._

### Навык игр (Gaming)
Опыт за аркадные автоматы и игры на планшете.

- **Код:** [`/datum/skill/gaming`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/skills/gaming.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Панель навыков (админ) и просмотр уровней
TGUI-панель для просмотра и редактирования навыков разума; печать уровней.

- **Код:** [`/datum/skill_panel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/skill_panel.dm#L1), [`/datum/mind/proc/print_levels()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/skills.dm#L65); tgui: [`SkillPanel.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SkillPanel.jsx)
- **Статус в ss13-rs:** ❌ нет

### Скиллчипы и станция обучения
Чипы в мозг дают умения (акробатика, само-хирургия, профессиональные чипы шеф-повара, клоуна, детектива, шахтёра, инженера и др.); вставляются и извлекаются на станции навыков.

- **Код:** [`/obj/item/skillchip`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/skill_learning/skillchip.dm#L1), [`/obj/machinery/skill_station`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/skill_learning/skill_station.dm#L4), [`proc/implant_skillchip()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/skillchip.dm#L10); tgui: [`SkillStation.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SkillStation.jsx)
- **Статус в ss13-rs:** ❌ нет

### Режим уклонения (BandaStation)
Способность `dodge_mode` включает элемент `dodge_shift`: шанс уклониться от удара или снаряда со сдвигом спрайта и остаточным изображением; выдаётся в prime-экипировке.

- **Код:** [`/datum/element/dodge_shift`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/skills/code/dodge_shift/dodge_shift_element.dm#L8)
- **BandaStation:** [`dodge_shift_spell.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/skills/code/dodge_shift/dodge_shift_spell.dm#L1) — целиком фича форка (`/datum/action/cooldown/spell/dodge_mode`)
- **Статус в ss13-rs:** ❌ нет

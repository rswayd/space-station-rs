# Навыки и скиллчипы
Навыки разума (`/datum/skill`, опыт и уровни с бонусами к скорости и наградами) и скиллчипы, вставляемые в мозг.

### Каркас навыков: опыт и уровни
Разум хранит опыт по каждому навыку; уровни от «Novice» до «Legendary» дают модификаторы (скорость действия, шанс) и сообщения о повышении; на высшем уровне выдаётся награда (плащ навыка).

- **Код:** [`/datum/skill`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/skills/_skill.dm#L3), [`proc/level_gained()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/skills/_skill.dm#L50), [`proc/try_skill_reward()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/skills/_skill.dm#L70), [`/datum/mind/proc/adjust_experience()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/skills.dm#L13), [`/datum/mind/proc/get_skill_modifier()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/skills.dm#L49)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/skills.rs`: `Skills` на мобе (опыт и уровни по `SKILL_EXP_LIST`), `adjust_experience`/`set_level`, сообщения о повышении и понижении, `modifier` (скорость, шанс, разброс, значение); навыки принадлежат телу, а не разуму (не переносятся с разумом); нет множителей опыта и награды за легендарный уровень (её выдаёт приложение skill tracker в КПК — группа КПК)

### Навык уборки
Опыт за уборку грязи; ускоряет мытьё шваброй и тряпкой.

- **Код:** [`/datum/skill/cleaning`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/skills/cleaning.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/skills.rs`, `crates/ss13_game/src/janitor.rs`: швабра и мыло быстрее (`min(модификатор + смещение, 1)`), швабра тратит меньше реагентов, мыло реже теряет заряд; опыт `round(beauty / -15)` за каждое пятно и 1 за отмывание

### Навык атлетики
Опыт за тренажёры; уменьшает усталость и усиливает эффект тренировки.

- **Код:** [`/datum/skill/athletics`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/skills/athletics.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/skills.rs`, `crates/ss13_game/src/tables.rs`: опыт и ускорение за подъём на столы и стеллажи, модификаторы уровней, `exercised` и опыт атлетики во сне (`crates/ss13_game/src/buffs.rs`); тренажёры, бокс, перенос на плечах и осмотр физформы — с их механиками

### Навык добычи (Mining)
Опыт за копание руды; ускоряет кирку и бур.

- **Код:** [`/datum/skill/mining`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/skills/mining.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/skills.rs`, `crates/ss13_game/src/mining/rocks.rs`: кирка/бур и копание руками быстрее по уровню, опыт за стены инструментом и кинетическим выстрелом (`mine_experience` × количество руды, 4 за пустую породу); нет скан-импульса с уровня Journeyman

_«Навык рыбалки» описано в [../service/fishing.md](../service/fishing.md)._

### Навык игр (Gaming)
Опыт за аркадные автоматы и игры на планшете.

- **Код:** [`/datum/skill/gaming`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/skills/gaming.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/skills.rs`: навык и модификаторы; опыт за аркады и игры на КПК — с аркадами (не портированы)

### Панель навыков (админ) и просмотр уровней
TGUI-панель для просмотра и редактирования навыков разума; печать уровней.

- **Код:** [`/datum/skill_panel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/skill_panel.dm#L1), [`/datum/mind/proc/print_levels()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/mind/skills.dm#L65); tgui: [`SkillPanel.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SkillPanel.jsx)
- **Статус в ss13-rs:** ❌ нет

### Скиллчипы и станция обучения
Чипы в мозг дают умения (акробатика, само-хирургия, профессиональные чипы шеф-повара, клоуна, детектива, шахтёра, инженера и др.); вставляются и извлекаются на станции навыков.

- **Код:** [`/obj/item/skillchip`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/skill_learning/skillchip.dm#L1), [`/obj/machinery/skill_station`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/library/skill_learning/skill_station.dm#L4), [`proc/implant_skillchip()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/skillchip.dm#L10); tgui: [`SkillStation.jsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/SkillStation.jsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/skillchips.rs`: все 30 типов чипов (имя навыка, описание, иконка, трейты, категория, сложность, слоты, сообщения), чип в мозге (`ChipIn`/`BrainChips`, уходит и удаляется вместе с мозгом), лимиты мозга (5 слотов, сложность 3 у активных), запрет дубликатов и второго профессионального чипа, активация/деактивация с проверками и откатом 5 мин, трейты активных чипов читаются вживую (`has_trait`), чипы профессий из `skillchips` экипировки при спавне (`crate::outfit`, BandaStation: атмосферник получает чип инженера), отключение чипов при извлечении мозга, извлечение чипов острым предметом из мозга на полу за 15 с, повреждение мозга подозрительным чипом; станция — `crates/ss13_game/src/skill_station.rs` (запись «Станция имплантации скиллчипов» в `../service/library.md`); коннектор — квирк Chip Connector (`../mobs/quirks.md`). Читают трейты: RCD (`TRAIT_ADVANCED_RCD_KNOWLEDGE`, `crates/ss13_game/src/devices/rcd.rs`), рыболовные знания чипа рыбака (`crates/ss13_game/src/skills.rs` `fishing_insight`). Проверено: `skillchips::tests`, `tests/it/skillchips.rs`. Нет: собственных эффектов чипов акробатики, «Taunt 2 Dodge», мюзикла, указки и их действий (совет рыбака, цвет указки), CQC шеф-повара (`under_siege`); трейты остальных чипов выдаются, но их потребители — в группах своих механик (шарики клоуна, вкус детектива, клинботы, минботы, суперматерия, rod suplex, подписи проводов, самохирургия, вино, кусты, лампы, диск и ID, внутренности, сабраж, промывка мозгов, поцелуй шефа, вкус на расстоянии, пьяный бой, бросок); нет `skillchip_complexity_modifier` (мутация biotech compatibility не портирована), failsafe при смене типа мозга, клонирования чипов генокрадом

### Режим уклонения (BandaStation)
Способность `dodge_mode` включает элемент `dodge_shift`: шанс уклониться от удара или снаряда со сдвигом спрайта и остаточным изображением; выдаётся в prime-экипировке.

- **Код:** [`/datum/element/dodge_shift`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/skills/code/dodge_shift/dodge_shift_element.dm#L8)
- **BandaStation:** [`dodge_shift_spell.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/skills/code/dodge_shift/dodge_shift_spell.dm#L1) — целиком фича форка (`/datum/action/cooldown/spell/dodge_mode`)
- **Статус в ss13-rs:** ❌ нет — единственный источник способности в BandaStation — `post_equip` админской экипировки `/datum/outfit/centcom/commander/field/apex` (`modular_bandastation/prime_only`), а выдачи админских экипировок в порте нет; без неё механизм был бы недостижим. Порт требует хуков уклонения в `crate::martial::check_block`, `crate::weapons::block` (брошенное) и попадании снаряда; делать вместе с apex-экипировкой

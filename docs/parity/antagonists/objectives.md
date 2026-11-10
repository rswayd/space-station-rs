# Цели антагонистов
Общие цели (`/datum/objective`) из `code/game/gamemodes/objective.dm` и предметы для кражи. Цели, специфичные для одного антагониста (жертвоприношение культа, поглощение генокрада...), описаны в файлах этих антагонистов.

### Базовая цель и подбор цели
`/datum/objective`: владелец(ы), текст, подбор цели-разума с чёрным списком, проверка выполнения в конце раунда, отображение в окне антага.

- **Код:** [`/datum/objective`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L3), [`/datum/objective/proc/find_target()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L143), [`/datum/objective/proc/check_completion()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L96)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/antag/objectives.rs` (`Objective`, `find_target`, проверка в `RoundEnd`)

### Убийство (assassinate)
Цель должна быть мертва (или не на станции/без мозга) к концу раунда.

- **Код:** [`/datum/objective/assassinate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L220)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/antag/objectives.rs` (`assassinate`)

### Изоляция (maroon)
Цель не должна улететь живой со станции.

- **Код:** [`/datum/objective/maroon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L287)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/antag/objectives.rs` (`maroon`)

### Кража (steal)
Украсть ценный предмет (лазер капитана, ранец, диск ядерной аутентификации, гипоспрей CMO, чертежи, чёрный ящик, осколок суперматерии, ИИ и др.; 70+ вариантов с ограничениями по профессиям).

- **Код:** [`/datum/objective/steal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L610), [`/datum/objective_item`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective_items.dm#L54), [`/datum/objective_item/steal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective_items.dm#L118)
- **BandaStation:** [`objective_items.dm#L374`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective_items.dm#L374) — оружие ГСБ через маяк выбора
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/objectives.rs` (`find_steal_target`), `crates/ss13_mapload/src/antag.rs`: цели с особыми условиями (ИИ в карте, суперматерия, наборы извлечения) пропускаются

### Побег и выживание
Escape — улететь живым на шаттле/капсуле, не в бриге; escape_with_identity — с личностью цели (генокрад); survive — просто выжить; exile — не возвращаться на станцию.

- **Код:** [`/datum/objective/escape`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L488), [`/datum/objective/escape/escape_with_identity`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L502), [`/datum/objective/survive`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L547), [`/datum/objective/exile`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L571)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/objectives.rs` (`considered_escaped`): есть escape; escape_with_identity у генокрада (`crates/ss13_game/src/antag/changeling/server.rs`, `LingObjective::EscapeIdentity`: имя и ID цели); survive у вампира; нет exile; бриг шаттла не отличается

### Мученичество (martyr)
Умереть героически до конца раунда.

- **Код:** [`/datum/objective/martyr`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L584)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/antag/objectives.rs`

### Угон шаттла (hijack) и устранение
Hijack — улететь на шаттле, где нет лояльного экипажа; elimination — остаться единственным живым на шаттле (Highlander).

- **Код:** [`/datum/objective/hijack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L407), [`/datum/objective/elimination`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L425), [`/datum/objective/elimination/highlander`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L440)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/objectives.rs`: hijack выдаётся, но никогда не выполняется; elimination нет

### Цели малфа ИИ: блок органиков и армия роботов
Block — ни одна органическая форма жизни не должна улететь на шаттле (или станция уничтожена); robot_army — иметь армию киборгов.

- **Код:** [`/datum/objective/block`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L453), [`/datum/objective/robot_army`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L471)
- **BandaStation:** [`objective.dm#L452`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L452) — переработка целей малфа
- **Статус в ss13-rs:** ❌ нет

### Защита, удержание и прочие цели на цель
Protect/protect_object — сохранить человека/объект; debrain — украсть мозг цели; jailbreak/detain — освободить или посадить цель; capture — поймать живых существ; destroy — уничтожить ИИ; mutiny — убить глав (революция).

- **Код:** [`/datum/objective/protect`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L343), [`/datum/objective/debrain`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L312), [`/datum/objective/jailbreak`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L374), [`/datum/objective/capture`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L694), [`/datum/objective/destroy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L854), [`/datum/objective/mutiny`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L243)
- **BandaStation:** [`objective.dm#L859`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L859) — исправление чёрного списка для destroy
- **Статус в ss13-rs:** 🟡 частично — protect у вампира (`crates/ss13_game/src/antag/vampire/server.rs`, `VampireObjective::Protect`), mutiny у революции (`crates/ss13_game/src/antag/revolution/server.rs`: все главы — цели); нет protect_object, debrain, jailbreak/detain, capture, destroy (ИИ)

### Кража N предметов типа
Steal_n_of_type: набрать N предметов (оружие при Summon Guns, магия при Summon Magic, органы для абдукторов-подобных).

- **Код:** [`/datum/objective/steal_n_of_type`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L891), [`/datum/objective/steal_n_of_type/summon_guns`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L919), [`/datum/objective/steal_n_of_type/organs`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L955)
- **Статус в ss13-rs:** 🟡 частично — `summon_guns` готов (`crates/ss13_game/src/antag/survivalist.rs`: 5 настоящих стволов на теле, без `NOT_A_REAL_GUN`); `summon_magic` и `organs` ❌

### Особые цели: ядерная, поглощение, своя
Nuclear — взорвать станцию; absorb/absorb_most/absorb_changeling — цели генокрада; custom — произвольный текст от админа.

- **Код:** [`/datum/objective/nuclear`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L598), [`/datum/objective/absorb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L767), [`/datum/objective/custom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L983)
- **Статус в ss13-rs:** 🟡 частично — absorb, absorb_most и absorb_changeling у генокрада (`crates/ss13_game/src/antag/changeling/server.rs`, `LingObjective`); нет nuclear и custom

### Замена целей при уходе цели в крио (BandaStation)
Если цель уходит в криосон, зависимые цели (одержимость, генокрадские личность+изоляция) и индивидуальные цели переназначаются на новую цель или отменяются с уведомлением; цели культа и революции не переназначаются.

- **Код:** [`replace_unavailable_objectives()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/antagonists/code/objective_replacement.dm#L26), [`/datum/objective/proc/replace_unavailable_target()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/antagonists/code/objective_replacement.dm#L60)
- **BandaStation:** [`_antagonists.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/antagonists/_antagonists.dm#L1)
- **Статус в ss13-rs:** ❌ нет — криосон есть (`crates/ss13_game/src/cryosleep.rs`, `despawn_occupant`), но переназначения целей при уходе в крио нет

# Предатель
Агент Синдиката (или иного работодателя) среди экипажа: цели, аплинк, кодовые слова, контракты. Аплинк и цели — в [uplink.md](uplink.md) и [objectives.md](objectives.md).

### Антагонист «Предатель»
`/datum/antagonist/traitor`: при получении роли выдаёт аплинк, генерирует цели, приветствие «Вы - Предатель», окно информации, иконку антаг-HUD, звук-стингер и мудлеты.

- **Код:** [`/datum/antagonist/traitor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/datum_traitor.dm#L6), [`/datum/antagonist/traitor/on_gain()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/datum_traitor.dm#L54)
- **BandaStation:** [`datum_traitor.dm#L138`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/datum_traitor.dm#L138) — защита от null-целей
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/traitor.rs` (`add_traitor`, `gain`): нет стингера, мудлетов, снятия клоунской неуклюжести, переноса роли на новое тело (`on_respawn`)

### Работодатели
Случайный работодатель (Gorlex Marauders, Cybersun, Tiger Cooperative, Corporate Climber и др.) задаёт текст приветствия, тему окна и склонность к угону шаттла.

- **Код:** [`/datum/antagonist/traitor/proc/pick_employer()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/datum_traitor.dm#L101), [`strings/antagonist_flavor/traitor_flavor.json`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/strings/antagonist_flavor/traitor_flavor.json#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/antag/traitor.rs` (`pick_employer`, флейвор из `antag.ron`)

### Генерация целей предателя
Две общие цели (убийство/изоляция/кража, с шансом — уничтожение ИИ или цель профессии ГП), затем финальная: побег, мученичество или угон шаттла.

- **Код:** [`/datum/antagonist/traitor/proc/forge_traitor_objectives()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/datum_traitor.dm#L122), [`/datum/antagonist/traitor/proc/forge_single_generic_objective()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/datum_traitor.dm#L168), [`/datum/antagonist/traitor/proc/forge_ending_objective()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/datum_traitor.dm#L145), [`/datum/job/proc/generate_traitor_objective()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/_job.dm#L658)
- **BandaStation:** [`datum_traitor.dm#L174`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/datum_traitor.dm#L174) — защита от деления на ноль
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/objectives.rs` (`forge_traitor_objectives`): нет целей профессии и уничтожения ИИ

### Окно информации предателя
TGUI `AntagInfoTraitor`: вступление, работодатель, цели, код аплинка и как его открыть, кодовые слова; тема окна зависит от работодателя.

- **Код:** [`/datum/antagonist/traitor/ui_static_data()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/datum_traitor.dm#L223); tgui: [`AntagInfoTraitor.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AntagInfoTraitor.tsx)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/mod.rs`, `crates/ss13_game/src/antag/client.rs`: нет тем окна (`syndicate`, `ntos`) и кодовых слов

### Кодовые слова Синдиката
`SStraitor` генерирует фразы и ответы; у предателя они подсвечиваются в услышанной речи (синим и красным).

- **Код:** [`SUBSYSTEM_DEF(traitor)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/traitor.dm#L1), [`/datum/antagonist/traitor/apply_innate_effects()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/datum_traitor.dm#L201), [`/datum/component/codeword_hearing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/codeword_hearing.dm#L11)
- **Статус в ss13-rs:** ❌ нет — `has_codewords: false` в `crates/ss13_game/src/antag/traitor.rs`

### Собственные цели
Предатель (или админ) может заменить цели своими (`submit_player_objective`), если это разрешено.

- **Код:** [`/datum/antagonist/traitor/submit_player_objective()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/datum_traitor.dm#L217), [`/datum/antagonist/traitor/proc/can_change_objectives()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/datum_traitor.dm#L98)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/antag/traitor.rs` (`ask_custom_objective`, `submit_custom_objective`): кнопка в аплинке (`renegotiate_objectives`), один раз; цели побега остаются, своя — первой, в отчёте без вердикта; выдача целей админом — в группе админки

### Отчёт предателя в конце раунда
Секция отчёта: цели с отметками выполнения, список покупок и потраченные ТК, «лучший предатель» без аплинка, итоги контрактов, кодовые слова в подвале.

- **Код:** [`/datum/antagonist/traitor/roundend_report()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/datum_traitor.dm#L244), [`/datum/antagonist/traitor/roundend_report_footer()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/datum_traitor.dm#L313)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/traitor.rs` (`roundend_report`): нет итогов контрактора и кодовых слов

### Контрактор
Набор «Contract Kit» из аплинка: хаб контрактов (похищение целей с эвакуацией капсулой, выкуп ТК), пинпоинтер, дубинка контрактора, напарник-подкрепление.

- **Код:** [`/datum/contractor_hub`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/contractor/contractor_hub.dm#L1), [`/datum/syndicate_contract`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/contractor/syndicate_contract.dm#L1), [`/datum/objective/contract`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/gamemodes/objective.dm#L1004), [`/datum/antagonist/traitor/contractor_support`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/contractor/contract_teammate.dm#L2), [`/obj/item/pinpointer/crew/contractor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/contractor/contractor_items.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Деморализующие постеры и граффити
Плакаты, граффити и модуль Синдиката вызывают мудлеты у экипажа и антагов в зоне видимости.

- **Код:** [`/datum/proximity_monitor/advanced/demoraliser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/components/demoraliser.dm#L8), [`/datum/demoralise_moods`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/antagonists/traitor/components/demoraliser.dm#L81)
- **Статус в ss13-rs:** ❌ нет

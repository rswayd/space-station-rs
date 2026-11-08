# Клавиши и ввод
Настраиваемые хоткеи (`/datum/keybinding`), подсистема ввода и режимы ввода.

### Система кейбиндингов
`/datum/keybinding`: действия с клавишами по умолчанию (hotkey и classic-режим), переназначение в префах, обработка нажатий через `SSinput` и `client.keyDown`.

- **Код:** [`/datum/keybinding`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/_keybindings.dm#L1), [`/datum/preference_middleware/keybindings`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/middleware/keybindings.dm#L4), [`VERB_MANAGER_SUBSYSTEM_DEF(input)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/input.dm#L1), [`hotkeys.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/client/preferences/hotkeys.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/client.rs`, `crates/ss13_game/src/inventory_client.rs`, `crates/ss13_game/src/interact_client.rs`, `crates/ss13_game/src/health_client.rs`: фиксированные tg-клавиши по умолчанию, переназначения и classic-режима нет

### Движение
WASD/стрелки, вверх/вниз по z-уровням, удержание для блокировки движения.

- **Код:** [`/datum/keybinding/movement`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/movement.dm#L1), [`/datum/keybinding/mob/prevent_movement`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/mob.dm#L125)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/client.rs`: WASD и стрелки; нет z-уровней и блокировки

### Клавиши моба и живых
Отпустить тягу (C/Del), сопротивление (B), отдых, боевой режим (F), бег/ходьба, бросок (R/удержание), передать предмет, отмена взаимодействий, взгляд вверх/вниз, выбор зоны нумпадом.

- **Код:** [`mob.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/mob.dm#L1), [`living.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/living.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/interact_client.rs` (C/Del, B, R), `crates/ss13_game/src/health_client.rs` (F, нумпад): нет отдыха, бега/ходьбы, удержания броска, передачи, взгляда

### Клавиши человека
Быстрая экипировка (E) и в пояс/сумку/хранилище/карманы, вытаскивание аксессуара.

- **Код:** [`human.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/human.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/inventory_client.rs` (E, Shift+E, Shift+V, Shift+Q, Alt+1, Alt+2, Shift+F), `crates/ss13_game/src/inventory.rs` (`quick_equip`, `smart_equip`, `quick_accessory_draw`): нет `equip_delay_self` и особых правил кобур

### Клавиши клиента и общения
Say (T), радио, OOC (O), me (M), молитва, adminhelp (F1), скриншот, полноэкранный режим, минимальный HUD, закрыть все окна.

- **Код:** [`client.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/client.dm#L1), [`communication.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/communication.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_chat/src/client/panel.rs` (T/F3 say, Y рация, O/F2 OOC, M me, P молитва, L LOOC; канал сбрасывается после отправки), `crates/ss13_admin/src/ooc.rs` (молитва), `crates/ss13_game/src/client_keys.rs` (F11 полноэкранный режим, F12 режимы HUD, Home закрыть окна), F1 adminhelp в `crates/ss13_admin/src/client.rs`; скриншот — средствами системы, в BandaStation не назначен

### Клавиши силиконов, существ и админов
Клавиши ИИ, киборгов (модули), dextrous-мобов, админские (aghost, игроки, VV, ...).

- **Код:** [`artificial_intelligence.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/artificial_intelligence.dm#L1), [`robot.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/robot.dm#L1), [`dextrous.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/dextrous.dm#L1), [`admin.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/admin.dm#L1)
- **Статус в ss13-rs:** ❌ нет

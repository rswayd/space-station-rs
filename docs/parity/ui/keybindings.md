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
- **Статус в ss13-rs:** ❌ нет

### Клавиши клиента и общения
Say (T), радио, OOC (O), me (M), молитва, adminhelp (F1), скриншот, полноэкранный режим, минимальный HUD, закрыть все окна.

- **Код:** [`client.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/client.dm#L1), [`communication.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/communication.dm#L1)
- **Статус в ss13-rs:** ❌ нет — ввод речи только через строку чата

### Клавиши силиконов, существ и админов
Клавиши ИИ, киборгов (модули), dextrous-мобов, админские (aghost, игроки, VV, ...).

- **Код:** [`artificial_intelligence.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/artificial_intelligence.dm#L1), [`robot.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/robot.dm#L1), [`dextrous.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/dextrous.dm#L1), [`admin.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/admin.dm#L1)
- **Статус в ss13-rs:** ❌ нет

# LOOC и шёпот (BandaStation)
Модуль форка `communication`: локальный OOC-чат и отдельная клавиша шёпота.

### LOOC
Локальный OOC: сообщение видят игроки в радиусе (с опцией «сквозь стены»), клавиша и канал в tgui-say, настройка «видеть LOOC», админы видят всё.

- **Код:** [`/client/proc/looc_message()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/communication/code/LOOC.dm#L39)
- **BandaStation:** [`modular_bandastation/communication/code/LOOC.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/communication/code/LOOC.dm#L3) — весь модуль
- **Статус в ss13-rs:** ❌ нет

### Клавиша шёпота
Отдельный канал «Шёпот» (клавиша Y) в tgui-say, отправляющий `whisper`.

- **Код:** [`/datum/tgui_say/delegate_speech()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/communication/code/whisper.dm#L20)
- **BandaStation:** [`modular_bandastation/communication/code/whisper.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/communication/code/whisper.dm#L1) — клавиша и канал
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_chat/src/speech.rs`: шёпот префиксом `#`; нет отдельной клавиши/канала

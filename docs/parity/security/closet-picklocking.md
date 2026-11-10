# Взлом замков шкафов (BandaStation)
Форковая механика взлома запертых шкафов и ящиков инструментами без емага.

### Взлом замка шкафа
Вне боевого режима на запертом защищённом шкафу: отвёртка 16 с снимает панель (95%, иначе 5 урона руке), кусачки 16 с готовят проводку (80%, иначе искры и удар током), мультитул 16 с взламывает замок (80%); стадия видна при осмотре; ящики с лутом не поддаются.

- **Код:** [`/obj/structure/closet/screwdriver_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/closet_picklocking/code/closet_picklocking.dm#L18), [`wirecutter_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/closet_picklocking/code/closet_picklocking.dm#L43), [`multitool_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/closet_picklocking/code/closet_picklocking.dm#L60)
- **BandaStation:** [`modular_bandastation/closet_picklocking/code/closet_picklocking.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/closet_picklocking/code/closet_picklocking.dm#L1) — вся механика добавлена форком
- **Статус в ss13-rs:** ✅ `crates/ss13_game/src/closets/picklock.rs` (`tool_click`, `complete`, `examine_lines`; клики из `crates/ss13_game/src/closets.rs`): три шага по 16 с вне боевого режима на запертом защищённом шкафу — отвёртка (95%: замок сломан и не переключается, иначе 5 урона другой руке и крик), кусачки (80%, иначе искры и удар током от питания зоны), мультитул (80%: замок взламывается как емагом, `crates/ss13_game/src/emag.rs`); стадия видна при осмотре; заброшенные ящики (`crate/secure/loot`) не поддаются

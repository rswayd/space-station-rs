# Взлом замков шкафов (BandaStation)
Форковая механика взлома запертых шкафов и ящиков инструментами без емага.

### Взлом замка шкафа
Вне боевого режима на запертом защищённом шкафу: отвёртка 16 с снимает панель (95%, иначе 5 урона руке), кусачки 16 с готовят проводку (80%, иначе искры и удар током), мультитул 16 с взламывает замок (80%); стадия видна при осмотре; ящики с лутом не поддаются.

- **Код:** [`/obj/structure/closet/screwdriver_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/closet_picklocking/code/closet_picklocking.dm#L18), [`wirecutter_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/closet_picklocking/code/closet_picklocking.dm#L43), [`multitool_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/closet_picklocking/code/closet_picklocking.dm#L60)
- **BandaStation:** [`modular_bandastation/closet_picklocking/code/closet_picklocking.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/closet_picklocking/code/closet_picklocking.dm#L1) — вся механика добавлена форком
- **Статус в ss13-rs:** ❌ нет — `crates/ss13_game/src/closets.rs` умеет только открывать защищённые шкафы ID с доступом

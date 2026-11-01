# Внешность: парикмахер, косметика, зеркала
Бритьё, стрижка, краска для волос, помада, зеркала и комод (`code/game/objects/items/cosmetics.dm`, `dyespray.dm`, `code/game/objects/structures/mirror.dm`, `dresser.dm`).

### Бритва
Бритвой на голове или рту цели (с её согласия по `do_after`) сбривают/стригут волосы и бороду, выбирая новую причёску.

- **Код:** [`/obj/item/razor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cosmetics.dm#L189), [`shave()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cosmetics.dm#L209), [`attack()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cosmetics.dm#L216)
- **Статус в ss13-rs:** ❌ нет

### Помада
Помада наносится на губы (себе или другому), цвет выбирается радиальным меню; стирается салфеткой/мытьём. Особые: «смертельная» чёрная, синдикатская.

- **Код:** [`/obj/item/lipstick`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cosmetics.dm#L1), [`display_radial_menu()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cosmetics.dm#L52), [`/obj/item/lipstick/black/death`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cosmetics.dm#L102)
- **Статус в ss13-rs:** ❌ нет

### Спрей-краска для волос
Красит волосы/бороду (и градиенты) цели в выбранный цвет; может окрашивать органы.

- **Код:** [`/obj/item/dyespray`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/dyespray.dm#L1), [`dye()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/dyespray.dm#L22)
- **Статус в ss13-rs:** ❌ нет

### Зеркало
Настенное зеркало: клик меняет причёску и бороду себе; разбивается (плохая примета), снимается/вешается.

- **Код:** [`/obj/structure/mirror`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/mirror.dm#L21), [`change_hair()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/mirror.dm#L177), [`/obj/item/wallframe/mirror`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/mirror.dm#L367)
- **Статус в ss13-rs:** ❌ нет

### Волшебное зеркало
Меняет имя, расу, пол и внешность (волшебники, аванпосты, вариант «гордыни»).

- **Код:** [`/obj/structure/mirror/magic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/mirror.dm#L389), [`change_race()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/mirror.dm#L200), [`change_name()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/mirror.dm#L188)
- **Статус в ss13-rs:** ❌ нет

_«Комод» описано в [../engineering/structures-furniture.md](../engineering/structures-furniture.md)._

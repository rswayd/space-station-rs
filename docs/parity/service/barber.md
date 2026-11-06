# Внешность: парикмахер, косметика, зеркала
Бритьё, стрижка, краска для волос, помада, зеркала и комод (`code/game/objects/items/cosmetics.dm`, `dyespray.dm`, `code/game/objects/structures/mirror.dm`, `dresser.dm`).

### Бритва
Бритвой на голове или рту цели (с её согласия по `do_after`) сбривают/стригут волосы и бороду, выбирая новую причёску.

- **Код:** [`/obj/item/razor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cosmetics.dm#L189), [`shave()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cosmetics.dm#L209), [`attack()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cosmetics.dm#L216)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/service/barber.rs`: бритва в руке по человеку, зона голова/рот; в боевом режиме бреет (5 с; голова — «skinhead», лицо — начисто), вне боевого — список причёсок/бород для другого (6 с, себе — «нужно зеркало»), стиль бороды только мужскому телосложению; проверки: нет головы, рот закрыт маской/шлемом, головной убор, вид без волос, черта «лысый»; звук `hair-clippers`; борода и помада — новые поля `HumanLook` (`facial_hair_style`/`facial_hair_color`, `lip_style`/`lip_color`), рисуются на `HAIR_LAYER`/`BODY_LAYER`, прячутся `HIDEFACIALHAIR`; `// shortcut:` бритва не бьёт живых мобов вне головы (весь `attack` предмета пропускается в бою)

### Помада
Помада наносится на губы (себе или другому), цвет выбирается радиальным меню; стирается салфеткой/мытьём. Особые: «смертельная» чёрная, синдикатская.

- **Код:** [`/obj/item/lipstick`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cosmetics.dm#L1), [`display_radial_menu()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cosmetics.dm#L52), [`/obj/item/lipstick/black/death`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/cosmetics.dm#L102)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/service/barber.rs`: открыть/закрыть в руке (`_uncap` + цветной кончик), Alt-клик — стиль (верхняя/обе/нижняя губа, список вместо радиального меню), на себя сразу, на другого 2 с; маска мешает, старую помаду нужно стереть; бумага в зоне рта стирает (другому 1 с), мытьё (`janitor::wash`) стирает при открытом рте; цвета всех подтипов, случайная помада берёт цвет и имя; `// shortcut:` черты «поцелуй смерти»/синдикатский поцелуй не хранятся (нет поцелуев) — эффект поцелуя в группу эмоций

### Спрей-краска для волос
Красит волосы/бороду (и градиенты) цели в выбранный цвет; может окрашивать органы.

- **Код:** [`/obj/item/dyespray`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/dyespray.dm#L1), [`dye()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/dyespray.dm#L22)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/service/barber.rs`: на себя (в руке) или по человеку: «Волосы/Борода» (что доступно: голова, вид с волосами, без черты «лысый»), выбор цвета (`ColorPickerModal`), 3 с, звук спрея; `// shortcut:` отклонение — краска красит всю причёску/бороду цветом, без узора градиента (tg маскирует градиент волосами через `ICON_ADD`, оверлеи рендера так не умеют); окраска органов (мутантных частей тела) не портирована

### Зеркало
Настенное зеркало: клик меняет причёску и бороду себе; разбивается (плохая примета), снимается/вешается.

- **Код:** [`/obj/structure/mirror`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/mirror.dm#L21), [`change_hair()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/mirror.dm#L177), [`/obj/item/wallframe/mirror`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/mirror.dm#L367)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/service/barber.rs` (`open_mirror`) + `crates/ss13_game/src/decor.rs`: пустая рука по зеркалу — список опций (вместо радиального меню) «причёска»/«борода», после каждой смены меню возвращается; при женском телосложении борода только сбривается; черта «лысый» не даёт отрастить волосы; всё перепроверяется (`can_use_mirror`: целое, рядом, в сознании); бьётся, чинится сваркой, снимается ключом (decor.rs); рама зеркала (`/obj/item/wallframe/mirror`, битая, еретическая) вешается на стену рядом — зеркало на клетке пользователя со сдвигом 28 px к стене; `// shortcut:` нет отражений и тюрьмы ревенанта; «плохая примета» (компонент omen) — отдельная механика, не портирована

### Волшебное зеркало
Меняет имя, расу, пол и внешность (волшебники, аванпосты, вариант «гордыни»).

- **Код:** [`/obj/structure/mirror/magic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/mirror.dm#L389), [`change_race()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/mirror.dm#L200), [`change_name()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/structures/mirror.dm#L188)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/service/barber.rs`: волшебное зеркало — причёска или цвет волос (и бороды у мужского телосложения), борода без ограничений, раса (раундстартовые виды, подтверждение «Да/Нет», затем тон кожи или яркий мутантный цвет), пол и телосложение, цвет глаз, имя (текстовый ввод, очистка); зеркало гордыни и еретика — без смены имени; еретическое для не-еретика меняет выбранное случайно, бьётся при смене вида, сваркой не чинится и не снимается; `// shortcut:` нет 5 с транса еретического зеркала, пол — только телосложение (местоимения не моделируются), флаги видов `MIRROR_MAGIC/PRIDE/BADMIN` не различаются

_«Комод» описано в [../engineering/structures-furniture.md](../engineering/structures-furniture.md)._

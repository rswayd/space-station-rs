# Декали и грязь

Наземные декали карты и «очищаемые» следы (`code/game/objects/effects/decals`): кровь, грязь, рвота, стекло, пепел, масло, надписи мелом. Уборка (швабра, мыло, уборщик) как механика — в разделе сервиса; здесь только сами следы.

### Наземные декали карты (turf_decal)
Разметка пола из маппинга: полосы, «осторожно», зоны погрузки, стрелки, цветные края плитки (`tile`, `trimline`), окантовка (`siding`), песок и снег; при загрузке превращаются в элемент `decal` турфа (оверлей, поворачивается с картой, удаляется при смене пола).

- **Код:** [`/obj/effect/turf_decal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/decal.dm#L50), [`/datum/element/decal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/decals/_decal.dm#L1), [`/obj/effect/turf_decal/tile`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/turfdecal/tilecoloring.dm#L1), [`/obj/effect/turf_decal/stripes`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/turfdecal/markings.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_mapload`: декали карты загружаются и рисуются как обычные атомы со своим `dir`; не превращаются в элемент турфа и не исчезают при вскрытии плитки (`crates/ss13_game/src/construction.rs`)

### Очищаемые декали: каркас
`/obj/effect/decal/cleanable`: след на полу, который сливается с таким же на тайле, может нести реагенты (собираются пипеткой/шваброй) и болезни, смывается `wash()` (швабра, мыло, душ, уборочный бот, пена).

- **Код:** [`/obj/effect/decal/cleanable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/cleanable.dm#L1), [`/atom/proc/wash()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/atom/_atom.dm#L604), [`/datum/component/cleaner`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/cleaner.dm#L7)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/janitor.rs`: все cleanables (с карты и созданные в раунде) получают `clean_type` и смываются `wash()` (швабра, мыло, космоочиститель); путь типа — `ss13_mapload::DmCleanable`; нет слияния одинаковых декалей, реагентов в декалях и болезней

### Кровь, брызги, гиббы и следы
Капли и лужи крови (у разных видов разные цвета, `blood/xeno`, `oil` роботов), брызги от ударов (`hitsplatter`, на окнах), гиббы после разрыва тела, кровавые следы ног (`footprints`, компонент `bloodysoles` пачкает обувь) и волочения (`trail`); старая кровь засыхает.

- **Код:** [`/obj/effect/decal/cleanable/blood`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/cleanable/blood.dm#L1), [`/obj/effect/decal/cleanable/blood/hitsplatter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/cleanable/blood.dm#L792), [`/obj/effect/decal/cleanable/blood/gibs`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/cleanable/blood.dm#L514), [`/obj/effect/decal/cleanable/blood/footprints`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/cleanable/blood.dm#L675), [`/obj/effect/decal/cleanable/blood/trail`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/cleanable/blood.dm#L454), [`/datum/component/bloodysoles`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/bloodysoles.dm#L4), [`/obj/effect/decal/cleanable/blood/oil`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/cleanable/robots.dm#L41)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/janitor.rs`: кровотечение (`bleed`) с шансом `sqrt(amount) × 90`% оставляет капли (до пяти на тайле, шестая превращает их в лужу) и лужи (растут на 50 bloodiness), кровь засыхает за 5 минут (темнеет); нет брызг от ударов, гиббов, следов ног и волочения, цвета по виду крови

### Бытовой мусор: грязь, пыль, паутина, рвота, пепел, стекло, мусор
Грязь от хождения, пыль, паутина, рвота (обычная, токсичная, туманностная с червями), пепел от сгоревшего, осколки стекла (обычные, плазменные, титановые), обломки, обрезки, конфетти, блёстки, остатки еды (томат, яйцо, пирог, мука, соль), муравьи (огненные муравьи жалят).

- **Код:** [`/obj/effect/decal/cleanable/dirt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/cleanable/mess.dm#L66), [`/obj/effect/decal/cleanable/cobweb`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/cleanable/mess.dm#L156), [`/obj/effect/decal/cleanable/vomit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/cleanable/mess.dm#L190), [`/obj/effect/decal/cleanable/ash`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/cleanable/mess.dm#L8), [`/obj/effect/decal/cleanable/glass`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/cleanable/mess.dm#L35), [`/obj/effect/decal/cleanable/garbage`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/cleanable/mess.dm#L365), [`/obj/effect/decal/cleanable/food`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/cleanable/food.dm#L1), [`/obj/effect/decal/cleanable/ants`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/cleanable/ants.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — размещённые на карте грязь и паутина рисуются (`crates/ss13_mapload`); рвоты, пепла от сгорания, осколков от разбитых окон-декалей и мусора в раунде нет (разбитое окно оставляет осколки-предметы: `crates/ss13_game/src/obj_damage.rs`)

### Опасные лужи: топливо, плазма, светящаяся слизь
Лужа сварочного топлива (`fuel_pool`) горит и растекается при поджоге; плазменная и радиоактивная зелёная слизь (`greenglow`) светятся и облучают.

- **Код:** [`/obj/effect/decal/cleanable/fuel_pool`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/cleanable/fuel.dm#L1), [`/obj/effect/decal/cleanable/greenglow`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/cleanable/mess.dm#L113), [`/obj/effect/decal/cleanable/greenglow/radioactive`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/cleanable/mess.dm#L138), [`/obj/effect/decal/cleanable/plasma`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/cleanable/mess.dm#L317)
- **Статус в ss13-rs:** ❌ нет — нет луж топлива и радиации

### Рисунки мелками и баллончиком
Мелок или баллончик рисует руны, буквы, граффити и знаки на полу и стенах (`crayon`), выбор в меню мелка; смываются уборкой.

- **Код:** [`/obj/effect/decal/cleanable/crayon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/crayon.dm#L1), [`/obj/item/toy/crayon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/crayons.dm#L24)
- **Статус в ss13-rs:** ❌ нет — нет мелков

### Останки
Скелетированные останки людей, плазмаменов, ксеноморфов, роботов (после растворения кислотой, сожжения и т.п.); «дымные» останки в техтоннелях при приближении (не чаще раза в 20 с) выпускают облако случайного реагента.

- **Код:** [`/obj/effect/decal/remains`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/remains.dm#L12), [`/obj/effect/decal/remains/human`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/remains.dm#L24), [`/obj/effect/decal/remains/human/smokey`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/remains.dm#L31)
- **BandaStation:** [`code/game/objects/effects/decals/remains.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/remains.dm#L1) — «дымные» останки не выпускают медицину, еду, мутатоксины и ряд опасных токсинов (`GLOB.remains_blocked_reagents`)
- **Статус в ss13-rs:** 🟡 частично — останки, размещённые картой, рисуются (`crates/ss13_mapload`); в раунде не появляются, газ «дымных» останков не выпускается

### Облачко распылителя (chempuff)
Летящее облачко реагентов от распылителя/спрея: наносит реагенты на всё на своём пути.

- **Код:** [`/obj/effect/decal/chempuff`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/effects/decals/chemspray.dm#L2)
- **Статус в ss13-rs:** ❌ нет — нет распылителей

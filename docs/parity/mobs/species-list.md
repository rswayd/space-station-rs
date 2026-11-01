# Виды: список
Каждый вид или семейство видов: стартовые (включены в конфиге), особые и антагские. В ss13-rs любой человек — `/datum/species/human`.

### Люди
Базовый вид без особенностей, тон кожи; `payday_modifier` 1.1.

- **Код:** [`/datum/species/human`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/humans.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/human.rs`: человеческое тело и отрисовка; нет датума вида (черт, органов, модификаторов)

### Фелиниды
Люди с кошачьими ушами и хвостом: боятся воды, ненавидимы собаками, особый язык и мозг; можно сделать генетикой.

- **Код:** [`/datum/species/human/felinid`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/felinid.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Церулеане
Люди-рыбы: рыбьи лёгкие, желудок, печень, язык; хвост-плавник даёт плавание.

- **Код:** [`/datum/species/human/cerulean`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/cerulean.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Ящеры (и эшвалкеры, сильверскейлы)
Холоднокровные (`coldmod` 1.5, `heatmod` 0.67), хвост, свой язык и печень. Эшвалкеры — обитатели Лаваленда, дышат его воздухом; сильверскейлы — иммунные к давлению с серебряной кожей.

- **Код:** [`/datum/species/lizard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/lizardpeople.dm#L1), [`/datum/species/lizard/ashwalker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/lizardpeople.dm#L103), [`/datum/species/lizard/silverscale`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/lizardpeople.dm#L142)
- **Статус в ss13-rs:** ❌ нет

### Моли
Крылья (планируют в невесомости), едят одежду, чувствительные к вспышкам глаза; своя группа крови.

- **Код:** [`/datum/species/moth`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/mothmen.dm#L1)
- **BandaStation:** [`modular_bandastation/species/code/human/species_type/moth.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/human/species_type/moth.dm#L1) — своя группа крови молей
- **Статус в ss13-rs:** ❌ нет

### Эфириалы (и lustrous)
Светящиеся, питаются электричеством (желудок-батарея), `siemens_coeff` 0.5, сердце-кристалл воскрешает; lustrous — пиратский подвид с травмой «пророка».

- **Код:** [`/datum/species/ethereal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/ethereal.dm#L1), [`/datum/species/ethereal/lustrous`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/ethereal.dm#L120)
- **Статус в ss13-rs:** ❌ нет

### Плазмамены
Дышат плазмой, горят в кислороде без защитного костюма (энвиро-костюм и шлем в аутфите), без крови и ДНК, иммунны к радиации.

- **Код:** [`/datum/species/plasmaman`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/plasmamen.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Скреллы (BandaStation)
Вид форка: водные, не тонут, плавают, свои органы, головной карман (прячут предмет в щупальцах), язык курволиус; `payday_modifier` 1.5.

- **Код:** [`/datum/species/skrell`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/human/species_type/skrell.dm#L1)
- **BandaStation:** [`modular_bandastation/species/code/human/species_type/skrell.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/human/species_type/skrell.dm#L1) — весь вид
- **Статус в ss13-rs:** ❌ нет

### Таяры (BandaStation)
Вид форка: кошачьи, кошачьи инстинкты (приземляются на ноги), ненавидимы собаками, лучше переносят холод (`coldmod` 0.7), свои органы и аксессуары.

- **Код:** [`/datum/species/tajaran`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/human/species_type/tajaran.dm#L1)
- **BandaStation:** [`modular_bandastation/species/code/human/species_type/tajaran.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/human/species_type/tajaran.dm#L1) — весь вид
- **Статус в ss13-rs:** ❌ нет

### Вульпканины (BandaStation)
Вид форка: лисоподобные, свои органы (острый нюх/слух), язык канилунц; `payday_modifier` 0.8.

- **Код:** [`/datum/species/vulpkanin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/human/species_type/vulpkanin.dm#L1)
- **BandaStation:** [`modular_bandastation/species/code/human/species_type/vulpkanin.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/human/species_type/vulpkanin.dm#L1) — весь вид
- **Статус в ss13-rs:** ❌ нет

### Желеобразные (джелли, слаймолюди, люминесцентные, звездочёты)
Без сердца, любят токсины (лечатся ими), уязвимы к холоду, регенерируют конечности из объёма крови. Слаймолюди переключаются между телами, люминесцентные используют экстракты слаймов, звездочёты общаются телепатически.

- **Код:** [`/datum/species/jelly`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/jellypeople.dm#L8), [`/datum/species/jelly/slime`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/jellypeople.dm#L215), [`/datum/species/jelly/luminescent`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/jellypeople.dm#L526), [`/datum/species/jelly/stargazer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/jellypeople.dm#L716)
- **BandaStation:** [`code/modules/mob/living/carbon/human/species_types/jellypeople.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/jellypeople.dm#L368) — SSD-индикатор при смене тел
- **Статус в ss13-rs:** ❌ нет

### Мухолюди
Каждый орган — мушиный; рвота при еде, едят с пола, жужжащая речь; результат несчастного случая с телепадом.

- **Код:** [`/datum/species/fly`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/flypeople.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Подолюди (podpeople)
Растения: лечатся на свету, вянут в темноте, безопасны для растительных ядов, уязвимы к жаре; рождаются из семян реплики.

- **Код:** [`/datum/species/pod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/podpeople.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Гриболюди
Не дышат, иммунны к вспышкам, ночное зрение, свой боевой стиль «грибной удар», шляпка.

- **Код:** [`/datum/species/mush`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/mushpeople.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Улиткообразные
Не скользят, раковина-рюкзак, оставляют слизь, `siemens_coeff` 2, медленные.

- **Код:** [`/datum/species/snail`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/snail.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Скелеты
Не дышат, без крови, легко теряют конечности и прикрепляют их обратно, иммунны к холоду/жаре/радиации, лечатся молоком.

- **Код:** [`/datum/species/skeleton`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/skeletons.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Тени (shadowpeople)
Лечатся во тьме и горят на свету; основа антага «Кошмар» (`/datum/species/shadow/nightmare`, см. ../antagonists/).

- **Код:** [`/datum/species/shadow`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/shadowpeople.dm#L1)
- **Статус в ss13-rs:** ❌ нет

_«Зомби» описано в [../antagonists/zombies.md](../antagonists/zombies.md)._

### Вампиры
Пьют кровь вместо еды, не дышат, горят от святой воды и в капелле, клан крови; в форке — правки питания.

- **Код:** [`/datum/species/human/vampire`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/vampire.dm#L7)
- **BandaStation:** [`code/modules/mob/mob.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob.dm#L1503) — вампиры не насыщаются обычной едой (антаг-версия — `modular_bandastation/vampire`, см. ../antagonists/)
- **Статус в ss13-rs:** ❌ нет

### Големы
Каменные: без крови, дыхания, крита и голода, иммунны к лаве; едят минералы для разных бонусов; создаются с шахтёрских шаблонов.

- **Код:** [`/datum/species/golem`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/golems.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Андроиды
Полностью роботизированные тела: не дышат, без крови, голода и ДНК, иммунны к передозу.

- **Код:** [`/datum/species/android`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/android.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Абдукторы
Серые пришельцы-антаги: без еды, дыхания, крови, телепатическая речь, HUD абдукторов.

- **Код:** [`/datum/species/abductor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/abductors.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Дуллаханы
Голова отделена от тела и носится в руке; видят и говорят через неё.

- **Код:** [`/datum/species/dullahan`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/dullahan.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Духи и призраки (spirit)
Летающие бесплотные тела без крови и ДНК (для админ-событий, ролей).

- **Код:** [`/datum/species/spirit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/ghost.dm#L2), [`/datum/species/spirit/ghost`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/ghost.dm#L88)
- **Статус в ss13-rs:** ❌ нет

### Обезьяны
Вид `monkey` для мартышек: маленькое тело, ползают по вентиляции без одежды, не умеют читать и пользоваться сложными инструментами; превращение генетикой.

- **Код:** [`/datum/species/monkey`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/monkeys.dm#L3), [`monkey.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/monkey.dm#L1)
- **Статус в ss13-rs:** ❌ нет

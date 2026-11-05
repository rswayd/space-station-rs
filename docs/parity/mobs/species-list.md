# Виды: список
Каждый вид или семейство видов: стартовые (включены в конфиге), особые и антагские. В ss13-rs вид — `crates/ss13_game/src/species/mod.rs`.

### Люди
Базовый вид без особенностей, тон кожи; `payday_modifier` 1.1.

- **Код:** [`/datum/species/human`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/humans.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/species/mod.rs`: вид по умолчанию, `TRAIT_USES_SKINTONES`, `payday_modifier` 1.1; `crates/ss13_game/src/human.rs`: тело

### Фелиниды
Люди с кошачьими ушами и хвостом: боятся воды, ненавидимы собаками, особый язык и мозг; можно сделать генетикой.

- **Код:** [`/datum/species/human/felinid`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/felinid.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: органы (мозг, глаза, уши, язык кошки), черты, тон кожи; язык нэкомиметик (`ss13_chat::language`); нет ушей и хвоста (аксессуары), боязни воды

### Церулеане
Люди-рыбы: рыбьи лёгкие, желудок, печень, язык; хвост-плавник даёт плавание.

- **Код:** [`/datum/species/human/cerulean`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/cerulean.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: рыбьи лёгкие, желудок, печень, язык (лёгкие дышат как человеческие); нет хвоста-плавника и плавания

### Ящеры (и эшвалкеры, сильверскейлы)
Холоднокровные (`coldmod` 1.5, `heatmod` 0.67), хвост, свой язык и печень. Эшвалкеры — обитатели Лаваленда, дышат его воздухом; сильверскейлы — иммунные к давлению с серебряной кожей.

- **Код:** [`/datum/species/lizard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/lizardpeople.dm#L1), [`/datum/species/lizard/ashwalker`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/lizardpeople.dm#L103), [`/datum/species/lizard/silverscale`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/lizardpeople.dm#L142)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: `coldmod`/`heatmod`, пределы температуры, холоднокровие (`breath.rs`), органы, кровь L, части тела с мутантным цветом; эшвалкеры с цифровыми ногами; сильверскейлы без дыхания, иммунны к давлению, серебряные; нет хвоста, рогов, морды, лавалендских лёгких (дышат как человеческие)

### Моли
Крылья (планируют в невесомости), едят одежду, чувствительные к вспышкам глаза; своя группа крови.

- **Код:** [`/datum/species/moth`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/mothmen.dm#L1)
- **BandaStation:** [`modular_bandastation/species/code/human/species_type/moth.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/human/species_type/moth.dm#L1) — своя группа крови молей
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: части тела и глаза молей, урон частям ×1.15, органы, кровь молей (BandaStation); нет крыльев, поедания одежды, чувствительности к вспышкам

### Эфириалы (и lustrous)
Светящиеся, питаются электричеством (желудок-батарея), `siemens_coeff` 0.5, сердце-кристалл воскрешает; lustrous — пиратский подвид с травмой «пророка».

- **Код:** [`/datum/species/ethereal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/ethereal.dm#L1), [`/datum/species/ethereal/lustrous`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/ethereal.dm#L120)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: `siemens_coeff` 0.5 (`wires.rs`), brute ×1.25, норма и пределы температуры, органы, кровь LE, части тела с цветом; lustrous без дыхания и давления; нет желудка-батареи, свечения, воскрешения сердцем, травмы «пророка»

### Плазмамены
Дышат плазмой, горят в кислороде без защитного костюма (энвиро-костюм и шлем в аутфите), без крови и ДНК, иммунны к радиации.

- **Код:** [`/datum/species/plasmaman`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/plasmamen.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: дышат плазмой (лёгкие `safe_plasma_min`, `breath.rs`), загораются вне герметичного костюма в кислороде, без крови и генов, `heatmod` 1.5, пределы температуры, энвиро-костюм, шлем, перчатки и баллон плазмы при входе в раунд (`equip_species_outfit`); нет аутфитов по профессиям (`plasmaman_outfit`)

### Скреллы (BandaStation)
Вид форка: водные, не тонут, плавают, свои органы, головной карман (прячут предмет в щупальцах), язык курволиус; `payday_modifier` 1.5.

- **Код:** [`/datum/species/skrell`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/human/species_type/skrell.dm#L1)
- **BandaStation:** [`modular_bandastation/species/code/human/species_type/skrell.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/human/species_type/skrell.dm#L1) — весь вид
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: органы, части тела, кровь SK, `payday_modifier` 1.5, черты `TRAIT_NODROWN`/`TRAIT_SWIMMER` (их читатели — в своих группах); язык курволиус (`ss13_chat::language`); нет головного кармана, щупалец

### Таяры (BandaStation)
Вид форка: кошачьи, кошачьи инстинкты (приземляются на ноги), ненавидимы собаками, лучше переносят холод (`coldmod` 0.7), свои органы и аксессуары.

- **Код:** [`/datum/species/tajaran`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/human/species_type/tajaran.dm#L1)
- **BandaStation:** [`modular_bandastation/species/code/human/species_type/tajaran.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/human/species_type/tajaran.dm#L1) — весь вид
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: `coldmod` 0.7, `heatmod` 1.3, пределы температуры, органы, части тела с цифровыми ногами, `payday_modifier` 0.9; нет хвоста и маркировок, кошачьих инстинктов

### Вульпканины (BandaStation)
Вид форка: лисоподобные, свои органы (острый нюх/слух), язык канилунц; `payday_modifier` 0.8.

- **Код:** [`/datum/species/vulpkanin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/human/species_type/vulpkanin.dm#L1)
- **BandaStation:** [`modular_bandastation/species/code/human/species_type/vulpkanin.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/species/code/human/species_type/vulpkanin.dm#L1) — весь вид
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: органы, части тела с цифровыми ногами, пределы температуры, `payday_modifier` 0.8; нет хвоста и маркировок, острого нюха и слуха

### Желеобразные (джелли, слаймолюди, люминесцентные, звездочёты)
Без сердца, любят токсины (лечатся ими), уязвимы к холоду, регенерируют конечности из объёма крови. Слаймолюди переключаются между телами, люминесцентные используют экстракты слаймов, звездочёты общаются телепатически.

- **Код:** [`/datum/species/jelly`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/jellypeople.dm#L8), [`/datum/species/jelly/slime`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/jellypeople.dm#L215), [`/datum/species/jelly/luminescent`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/jellypeople.dm#L526), [`/datum/species/jelly/stargazer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/jellypeople.dm#L716)
- **BandaStation:** [`code/modules/mob/living/carbon/human/species_types/jellypeople.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/jellypeople.dm#L368) — SSD-индикатор при смене тел
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: без сердца, `TRAIT_TOXINLOVER` (токсины лечат, `health.rs`), `coldmod` 6, burn ×0.5, слизистая кровь, лёгкие без вреда от плазмы; нет регенерации конечностей, смены тел, экстрактов, телепатии

### Мухолюди
Каждый орган — мушиный; рвота при еде, едят с пола, жужжащая речь; результат несчастного случая с телепадом.

- **Код:** [`/datum/species/fly`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/flypeople.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: мушиные органы и части тела; нет рвоты при еде, еды с пола, жужжания

### Подолюди (podpeople)
Растения: лечатся на свету, вянут в темноте, безопасны для растительных ядов, уязвимы к жаре; рождаются из семян реплики.

- **Код:** [`/datum/species/pod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/podpeople.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: органы, burn ×1.25, `heatmod` 1.5, кровь-вода, черты; нет лечения светом и увядания

### Гриболюди
Не дышат, иммунны к вспышкам, ночное зрение, свой боевой стиль «грибной удар», шляпка.

- **Код:** [`/datum/species/mush`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/mushpeople.dm#L3)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: без дыхания, запрет слотов (`no_equip_flags`, `inventory.rs`), burn ×1.25; нет грибного удара, шляпки, ночного зрения

### Улиткообразные
Не скользят, раковина-рюкзак, оставляют слизь, `siemens_coeff` 2, медленные.

- **Код:** [`/datum/species/snail`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/snail.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: `siemens_coeff` 2, burn ×2, `coldmod` 0.5, кровь S; нет слизи, раковины, замедления, нескользкости

### Скелеты
Не дышат, без крови, легко теряют конечности и прикрепляют их обратно, иммунны к холоду/жаре/радиации, лечатся молоком.

- **Код:** [`/datum/species/skeleton`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/skeletons.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: без дыхания и крови, иммунны к холоду, жаре, давлению и токсинам; нет лечения молоком, лёгкой потери и прикрепления конечностей

### Тени (shadowpeople)
Лечатся во тьме и горят на свету; основа антага «Кошмар» (`/datum/species/shadow/nightmare`, см. ../antagonists/).

- **Код:** [`/datum/species/shadow`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/shadowpeople.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: без дыхания и крови, burn ×1.5; нет лечения во тьме и урона на свету

_«Зомби» описано в [../antagonists/zombies.md](../antagonists/zombies.md)._

### Вампиры
Пьют кровь вместо еды, не дышат, горят от святой воды и в капелле, клан крови; в форке — правки питания.

- **Код:** [`/datum/species/human/vampire`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/vampire.dm#L7)
- **BandaStation:** [`code/modules/mob/mob.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/mob.dm#L1503) — вампиры не насыщаются обычной едой (антаг-версия — `modular_bandastation/vampire`, см. ../antagonists/)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: вампирское сердце, кровь V, черты; нет питья крови, святой воды, кланов

### Големы
Каменные: без крови, дыхания, крита и голода, иммунны к лаве; едят минералы для разных бонусов; создаются с шахтёрских шаблонов.

- **Код:** [`/datum/species/golem`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/golems.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: урон ×0.5, без дыхания, крови, мягкого крита (`TRAIT_NOSOFTCRIT`), `siemens_coeff` 0.01, запрет слотов; нет поедания минералов, иммунитета к лаве

### Андроиды
Полностью роботизированные тела: не дышат, без крови, голода и ДНК, иммунны к передозу.

- **Код:** [`/datum/species/android`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/android.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: роботизированные части (×0.8), без дыхания, крови, голода, генов, иммунны к температуре, давлению и токсинам, масло вместо крови; иммунитет к передозу не читается

### Абдукторы
Серые пришельцы-антаги: без еды, дыхания, крови, телепатическая речь, HUD абдукторов.

- **Код:** [`/datum/species/abductor`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/abductors.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: без еды, дыхания и крови; нет телепатии и HUD абдукторов

### Дуллаханы
Голова отделена от тела и носится в руке; видят и говорят через неё.

- **Код:** [`/datum/species/dullahan`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/dullahan.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: органы и черты; нет отделённой головы

### Духи и призраки (spirit)
Летающие бесплотные тела без крови и ДНК (для админ-событий, ролей).

- **Код:** [`/datum/species/spirit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/ghost.dm#L2), [`/datum/species/spirit/ghost`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/ghost.dm#L88)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: без крови и генов, запрет обуви, спрайты без ног; нет полёта и бесплотности

### Обезьяны
Вид `monkey` для мартышек: маленькое тело, ползают по вентиляции без одежды, не умеют читать и пользоваться сложными инструментами; превращение генетикой.

- **Код:** [`/datum/species/monkey`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/species_types/monkeys.dm#L3), [`monkey.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/monkey.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/species/mod.rs`: датум вида (органы, части тела, запрет слотов); моб-мартышка и превращение — нет

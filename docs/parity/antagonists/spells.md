# Заклинания
Фреймворк заклинаний `/datum/action/cooldown/spell` (`code/modules/spells`) и все типы заклинаний — их используют маги, культ, еретики, мимы, драконы, админы и др.

### Фреймворк заклинаний
Базовое заклинание: перезарядка, требования (роба, речь, руки), инвокация (крик/шёпот/эмоут), антимагия, уровни улучшения, иконка-кнопка.

- **Код:** [`/datum/action/cooldown/spell`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell.dm#L43), [`spell.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/spells/mod.rs`: `Spells` на мобе (уровень, перезарядка, `level_spell` с `cooldown_reduction_per_rank` и названиями уровней), кнопки в ряду действий, `can_cast_spell` (сознание, не в джаунте, `SPELL_REQUIRES_STATION`, антимагия `can_cast_magic`, роба и шляпа `TRAIT_CASTING_CLOTHING` для человека, любой кастующий предмет для прочих), `before_cast` без языка (2 с перезарядки), инвокация крик/шёпот с искажением 50%, звук, наведение (на себя, `pointed` с `cast_range` и `aim_assist`, `touch` с рукой), `can_block_magic` (нулевой жезл, библии, святая вода); эмоут-инвокации, обет мима, `SPELL_REQUIRES_MIND`, немота и антимагия с зарядами — не перенесены

### Заклинания по площади (AoE)
Knock (открыть двери), магическая ракета, отталкивание, священное пламя, обращение зоны.

- **Код:** [`code/modules/spells/spell_types/aoe_spell`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/aoe_spell)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/spells/effects.rs`: Knock (двери разболчиваются и открываются, шкафы отпираются), Magic Missile (снаряды только по своей цели, паралич 6 с), Repulse (броски 3-5 тайлов, паралич); священное пламя и обращение зоны — культ/капеллан, не перенесены

### Направленные заклинания
Огненный шар, молния, карты, слепота, обмен разумом, обмен местами, завязать шнурки, проклятие скотного двора, бездонный взгляд, доминирование, пальцы-пистолеты, террор.

- **Код:** [`code/modules/spells/spell_types/pointed`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/pointed)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/spells/effects.rs`: Fireball (дальность `6 + 2 × уровень`, 10 + 10 ожога, взрыв), Lightning Bolt (`tesla_zap` по живым с 20 000 мощности), Blind, Swap (со второй целью по ПКМ); карты, обмен разумом, шнурки, скотный двор, бездонный взгляд, доминирование, пальцы-пистолеты, террор — нет

### Касания
Smite (разрыв тела), плоть в камень, проклятая сумка, «Scream For Me».

- **Код:** [`code/modules/spells/spell_types/touch`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/touch)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/spells/effects.rs`: механизм касаний (рука в свободную руку, удар по живому в досягаемости, антимагия, перезарядка после удара) и Smite (гиб, вспышка вокруг); плоть в камень, проклятая сумка, «Scream For Me» — нет

### Заклинания на себя
Лечение, перезарядка, отключение техники, силовая стена, лич, обет мима, мутация, форма жезла, кровавый удар, дым, Soul Tap, искажение пространства, остановка времени, призыв предмета, Голос Бога, призрачность.

- **Код:** [`code/modules/spells/spell_types/self`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/self)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/spells/effects.rs`: дым (плохой дым, радиус 4), перезарядка (заклинания тянутого мага, посохи, палочки, энергооружие), отключение техники (ЭМИ 6/10), силовая стена (30 с), призыв предмета (метка, возврат с сумкой, снятие метки); лечение, лич, обет мима, мутация (нет мутаций Халка и лазерных глаз — genetics), форма жезла, кровавый удар, Soul Tap, искажение пространства, остановка времени, Голос Бога, призрачность — нет

### Джаунты
Эфирный джаунт, кровавое ползание (демоны), теневой шаг (кошмар).

- **Код:** [`code/modules/spells/spell_types/jaunt`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/jaunt)
- **BandaStation:** [`ethereal_jaunt.dm#L102`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/jaunt/ethereal_jaunt.dm#L102) — настраиваемое время выхода из джаунта
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/spells/effects.rs`, `crates/ss13_game/src/server.rs` (шаги джаунта): эфирный джаунт — 5 с невидимости и неуязвимости для кликов и снарядов, движение сквозь стены раз в 0,2 с, не в ЦК, выход на последний свободный тайл (или старт) через 2,5 с; кровавое ползание и теневой шаг — демоны/кошмар, нет

### Телепортация
Blink (короткий прыжок), телепорт в выбранную зону.

- **Код:** [`code/modules/spells/spell_types/teleport`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/teleport)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/spells/effects.rs`, `crates/ss13_game/src/spells/mod.rs`: Blink (случайный тайл в радиусе 6, не в ЦК), Teleport (выбор зоны станции в списке, принудительный перенос на свободный тайл, зона в инвокации), свиток телепортации (без инвокации и робы, 4 заряда); безвредный дым после телепорта не перенесён

### Призыв существ и объектов
Пчёлы, карпы, сыр, конструкты, существа, культовые полы, рой, невидимые стул и стена, связь миров, подарки, обезьяны, камни душ, ловушки, слуга.

- **Код:** [`code/modules/spells/spell_types/conjure`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/conjure), [`code/modules/spells/spell_types/summon`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/summon)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/spells/conjure.rs`: механизм `conjure` (радиус, количество, плотность, срок жизни) и данные всех заклинаний группы, кроме слуги; в книге мага пчёлы, ловушки (1 заряд), обезьяны (+1 за уровень, союз на минуту, на максимуме без робы и Gorilla Form); нет: слуга (нужны кость судьбы и датум слуги), снаряжение обезьян, `immune_minds` ловушек, усадка на стул мима, лазер ED-209; выдача заклинаний мима, конструктов и гранты (`summon_cheese`) — в группах профессий и культа

### Призыв предметов
Бесконечные пушки, невидимая коробка, пакет молний, снежки.

- **Код:** [`code/modules/spells/spell_types/conjure_item`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/conjure_item)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/spells/conjure.rs`, `spells/magic_guns.rs`: `conjure_item` (`delete_old`, `requires_hands`), Lesser Summon Guns и Arcane Barrage (30 зарядов, взаимоисключаются в книге), Thrown Lightning (`throw.rs`: 80 шока, антимагия), снежки, невидимая коробка (50 с); нет: саморазрушения и смены зачарованных винтовок, автоогня и выпадения пустого барража, очистки коробки, режима броска после заклинания

### Превращения
Облики дракона, гориллы, белого медведя, волка; общий шейпшифт.

- **Код:** [`code/modules/spells/spell_types/shapeshift`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/shapeshift)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/spells/shapeshift.rs`: облики дракона, гориллы, медведя, волка и Wild Shapeshift (6 форм, выбор один раз); разум в новом мобе, тело внутри него, заклинания и кнопка следуют за формой, доля урона в обе стороны, смерть формы убивает мага, ближний бой формы; `crate::transform::is_transforming` запрещает превращение в процессе (постоянный `transform_into_mob` не подходит: удаляет старое тело); нет радиального меню (список), вентиляций, уведомления `shapeshifted`; выдача форм волка, медведя и дракона предметами — их группы

### Конусы, заряжаемые и снаряды
Конус холода, удар Теслы (заряжаемый), снарядные заклинания (рывок джаггернаута).

- **Код:** [`code/modules/spells/spell_types/cone`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/cone), [`code/modules/spells/spell_types/charged`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/charged), [`code/modules/spells/spell_types/projectile`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/projectile)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/spells/cone.rs`: Cone of Cold (форма конуса по tg, 4 уровня с задержкой, стены, мерзлота 45 с, 10+20 урона, заморозка предметов на 20 с), Tesla Blast (канал 10 с в движении, 5 отскоков, антимагия), Gauntlet Echo (`BoltKind::GauntletEcho`); заморозка мобов — паралич 20 с (статус `freon` не перенесён), цели по радиусу без `view`/`get_path_to`; выдача Gauntlet Echo — конструкты (культ)

### Телепатия
Отправка мысленного сообщения выбранному существу.

- **Код:** [`telepathy.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/list_target/telepathy.dm#L1)
- **BandaStation:** [`telepathy.dm#L46`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/list_target/telepathy.dm#L46) — озвучка телепатии через TTS
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/spells/telepathy.rs`: список целей, ввод текста, проверки после ожиданий, антимагия разума, сообщения мага, цели и призраков; нет TTS-озвучки BandaStation (в порте нет TTS); выдача заклинания — мутация, еретик, ревенант (их группы)

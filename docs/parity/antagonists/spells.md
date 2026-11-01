# Заклинания
Фреймворк заклинаний `/datum/action/cooldown/spell` (`code/modules/spells`) и все типы заклинаний — их используют маги, культ, еретики, мимы, драконы, админы и др.

### Фреймворк заклинаний
Базовое заклинание: перезарядка, требования (роба, речь, руки), инвокация (крик/шёпот/эмоут), антимагия, уровни улучшения, иконка-кнопка.

- **Код:** [`/datum/action/cooldown/spell`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell.dm#L43), [`spell.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Заклинания по площади (AoE)
Knock (открыть двери), магическая ракета, отталкивание, священное пламя, обращение зоны.

- **Код:** [`code/modules/spells/spell_types/aoe_spell`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/aoe_spell)
- **Статус в ss13-rs:** ❌ нет

### Направленные заклинания
Огненный шар, молния, карты, слепота, обмен разумом, обмен местами, завязать шнурки, проклятие скотного двора, бездонный взгляд, доминирование, пальцы-пистолеты, террор.

- **Код:** [`code/modules/spells/spell_types/pointed`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/pointed)
- **Статус в ss13-rs:** ❌ нет

### Касания
Smite (разрыв тела), плоть в камень, проклятая сумка, «Scream For Me».

- **Код:** [`code/modules/spells/spell_types/touch`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/touch)
- **Статус в ss13-rs:** ❌ нет

### Заклинания на себя
Лечение, перезарядка, отключение техники, силовая стена, лич, обет мима, мутация, форма жезла, кровавый удар, дым, Soul Tap, искажение пространства, остановка времени, призыв предмета, Голос Бога, призрачность.

- **Код:** [`code/modules/spells/spell_types/self`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/self)
- **Статус в ss13-rs:** ❌ нет

### Джаунты
Эфирный джаунт, кровавое ползание (демоны), теневой шаг (кошмар).

- **Код:** [`code/modules/spells/spell_types/jaunt`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/jaunt)
- **BandaStation:** [`ethereal_jaunt.dm#L102`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/jaunt/ethereal_jaunt.dm#L102) — настраиваемое время выхода из джаунта
- **Статус в ss13-rs:** ❌ нет

### Телепортация
Blink (короткий прыжок), телепорт в выбранную зону.

- **Код:** [`code/modules/spells/spell_types/teleport`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/teleport)
- **Статус в ss13-rs:** ❌ нет

### Призыв существ и объектов
Пчёлы, карпы, сыр, конструкты, существа, культовые полы, рой, невидимые стул и стена, связь миров, подарки, обезьяны, камни душ, ловушки, слуга.

- **Код:** [`code/modules/spells/spell_types/conjure`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/conjure), [`code/modules/spells/spell_types/summon`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/summon)
- **Статус в ss13-rs:** ❌ нет

### Призыв предметов
Бесконечные пушки, невидимая коробка, пакет молний, снежки.

- **Код:** [`code/modules/spells/spell_types/conjure_item`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/conjure_item)
- **Статус в ss13-rs:** ❌ нет

### Превращения
Облики дракона, гориллы, белого медведя, волка; общий шейпшифт.

- **Код:** [`code/modules/spells/spell_types/shapeshift`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/shapeshift)
- **Статус в ss13-rs:** ❌ нет

### Конусы, заряжаемые и снаряды
Конус холода, удар Теслы (заряжаемый), снарядные заклинания (рывок джаггернаута).

- **Код:** [`code/modules/spells/spell_types/cone`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/cone), [`code/modules/spells/spell_types/charged`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/charged), [`code/modules/spells/spell_types/projectile`](https://github.com/ss220club/BandaStation/tree/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/projectile)
- **Статус в ss13-rs:** ❌ нет

### Телепатия
Отправка мысленного сообщения выбранному существу.

- **Код:** [`telepathy.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/list_target/telepathy.dm#L1)
- **BandaStation:** [`telepathy.dm#L46`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/spells/spell_types/list_target/telepathy.dm#L46) — озвучка телепатии через TTS
- **Статус в ss13-rs:** ❌ нет

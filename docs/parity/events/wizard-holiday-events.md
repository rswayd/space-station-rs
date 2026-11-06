# Волшебные и праздничные события

События пула мага (Summon Events, `EVENT_CATEGORY_WIZARD`, обычным планировщиком не выпадают) и события, привязанные к праздникам.

В ss13-rs у контроля есть флаг `wizard`, а у планировщика — `Events::wizardmode` (`crates/ss13_game/src/events/controller.rs`): события мага катятся только в режиме мага и тогда вытесняют остальные, как в tg; режим включает ритуал Summon Events из книги заклинаний (`summon_events`: события раз в 1–5 минут, каждое повторение снимает минуту с верхней границы). Праздничные события (`EventControl::holiday`, `crates/ss13_game/src/events/holidays.rs`) выпадают только в свой праздник; отрицательный вес запускает их при первом случайном выборе раунда.

## События мага

### Усиление мага: Robeless / Improved Casting
Robeless: маг (и все, у кого есть заклинания) колдует без робы и шляпы; Improved: заклинания всех получают уровень.

- **Код:** [`/datum/round_event_control/wizard/robelesscasting`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/aid.dm#L4), [`/datum/round_event_control/wizard/improvedcasting`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/aid.dm#L32)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/wizard.rs`: Robeless — флаг `KnownSpell::robeless` снимает требование робы и шляпы у всех известных заклинаний (`crates/ss13_game/src/spells/mod.rs` `can_cast`), Improved — каждое заклинание +1 уровень, максимум на один выше предела; веса и лимиты tg; запуск только в режиме мага или принудительно (Summon Events — в wizard.md)

### Призывы: Summon Guns / Summon Magic
Каждый член экипажа получает случайное оружие (или магический предмет); часть получивших становится выжившими-антагонистами.

- **Код:** [`/datum/round_event_control/wizard/summonguns`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/summons.dm#L1), [`/datum/round_event_control/wizard/summonmagic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/summons.dm#L17)
- **Статус в ss13-rs:** 🟡 частично — Summon Guns готов (wizard-событие и ритуал книги, `crates/ss13_game/src/antag/survivalist.rs`, `events/wizard.rs`); Summon Magic ❌ — нет большинства магических предметов

### Перемешивание: Change Places / Faces / Minds
Все люди меняются местами, внешностью и именами или разумами (массовый обмен телами).

- **Код:** [`/datum/round_event_control/wizard/shuffleloc`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/shuffle.dm#L4), [`/datum/round_event_control/wizard/shufflenames`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/shuffle.dm#L41), [`/datum/round_event_control/wizard/shuffleminds`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/shuffle.dm#L74)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/events/wizard.rs`: Change Places (живые люди на станционных уровнях меняются местами, дым у каждого), Change Faces (имена живых людей перемешиваются, дым), Change Minds (люди в сознании с разумом, кроме магов, попарно меняются разумами через `crate::medical::brainmob::possess`, дым; вес 1, до 3 раз); `// shortcut:` телепорт без проверки антимагии и запретных зон, люди в шкафах не участвуют, роли антагонистов и языки разума остаются с телом (в ss13-rs они на теле), нет `TRAIT_NO_MINDSWAP`

### Подмена личности: Mass Identity Spoof / Imposter Wizard / Race Swap
Всех одевают магами; появляется двойник мага от призрака; все гуманоиды меняют расу на одну случайную.

- **Код:** [`/datum/round_event_control/wizard/identity_spoof`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/identity_spoof.dm#L1), [`/datum/round_event_control/wizard/imposter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/imposter.dm#L1), [`/datum/round_event_control/wizard/race`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/race.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет мага и рас

### Опасная среда: The Floor Is LAVA! / Magical Rain
Пол на время становится лавой (погода `floor_is_lava`, обжигает всех не на мебели); магический дождь (`rain_storm/wizard`) с эффектами.

- **Код:** [`/datum/round_event_control/wizard/lava`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/lava.dm#L1), [`/datum/round_event_control/wizard/magical_rain`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/magical_rain.dm#L1), [`/datum/weather/floor_is_lava`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/weather/weather_types/floor_is_lava.dm#L2)
- **Статус в ss13-rs:** ❌ нет — нет погоды

### Indoor Weather (Cats and Dogs / Meatballs / Loose Change / Fish)
С потолка во всех зонах сыплются животные, еда, монеты или рыба.

- **Код:** [`/datum/round_event_control/wizard/object_rain`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/object_rain.dm#L11), [`/datum/round_event_control/wizard/object_rain/food`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/object_rain.dm#L22), [`/datum/round_event_control/wizard/object_rain/cash`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/object_rain.dm#L27), [`/datum/round_event_control/wizard/object_rain/fish`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/object_rain.dm#L32)
- **Статус в ss13-rs:** ❌ нет — нет события

### Предметы: Cursed Items / Embeddable / Sticky / RPG Loot / Greentext
Всем надевают проклятый неснимаемый набор одежды; все предметы втыкаются в тело при броске (или прилипают к рукам); предметы получают RPG-уровни и свитки улучшений; на станции появляется «гринтекст», дающий победу тому, кто его удержит.

- **Код:** [`/datum/round_event_control/wizard/cursed_items`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/curseditems.dm#L14), [`/datum/round_event_control/wizard/embedpocalypse`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/embeddies.dm#L1), [`/datum/round_event_control/wizard/embedpocalypse/sticky`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/embeddies.dm#L24), [`/datum/round_event_control/wizard/rpgloot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/rpgloot.dm#L1), [`/datum/round_event_control/wizard/greentext`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/greentext.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет события

### Существа: Zombie Outbreak / Magicarp / Petsplosion / G-G-G-Ghosts
На каждом трупе появляется спора блоба, которой может управлять призрак; карпы-маги; питомцы бесконечно размножаются; призраки становятся видимы (или вселяются в предметы).

- **Код:** [`/datum/round_event_control/wizard/blobies`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/blobies.dm#L1), [`/datum/round_event_control/wizard/magicarp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/magicarp.dm#L1), [`/datum/round_event_control/wizard/petsplosion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/petsplosion.dm#L23), [`/datum/round_event_control/wizard/ghost`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/ghost.dm#L1), [`/datum/round_event_control/wizard/possession`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/ghost.dm#L18)
- **Статус в ss13-rs:** ❌ нет — нет мобов

### Хаос: Curse of Madness / Tower of Babel / Departmental Uprising / Invincibility / RPG Titles / Fake Nuclear Explosion
Проклятие безумия с травмами мозга по введённому магом тексту; все забывают свои языки и получают один случайный; отделы объявляют независимость (революция по отделам); все на время неуязвимы; над людьми висят RPG-титулы; фальшивая синематика ядерного взрыва.

- **Код:** [`/datum/round_event_control/wizard/madness`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/madness.dm#L1), [`/datum/round_event_control/wizard/tower_of_babel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/tower_of_babel.dm#L1), [`/datum/round_event_control/wizard/deprevolt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/departmentrevolt.dm#L4), [`/datum/round_event_control/wizard/invincible`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/invincible.dm#L1), [`/datum/round_event_control/wizard/rpgtitles`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/rpgtitles.dm#L1), [`/datum/round_event_control/wizard/fake_explosion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/fakeexplosion.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — Tower of Babel: `crates/ss13_game/src/events/wizard.rs` (вес 3, один раз): каждый игрок-гуманоид на станционных уровнях получает один случайный язык, остальные блокируются (`LanguageSource::Babel` в `crates/ss13_chat/src/language.rs`), язык тела не ограничивает речь (`LanguageHolder::babel`, tg `TRAIT_TOWER_OF_BABEL`), антимагия разума защищает, проклятые не проклинаются повторно, настроение −1 на 15 с, бормотание и шёпот; маги получают все языки; новые члены экипажа проклинаются при входе (`TowerOfBabel`); `// shortcut:` без оповещения мёртвых, админской отмены и значка статуса, проклятие остаётся с телом. Curse of Madness, Departmental Uprising, Invincibility, RPG Titles, Fake Nuclear Explosion — нет

## Праздничные события

### Visit by Santa (Рождество)
В Рождество призрак может стать Сантой, который раздаёт подарки; станция украшается.

- **Код:** [`/datum/round_event_control/santa`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/holiday/xmas.dm#L70)
- **Статус в ss13-rs:** ❌ нет — нет опроса призраков и антагониста Санты (праздник Christmas есть: `crates/ss13_game/src/events/holidays.rs`)

### 2 SPOOKY! (Хэллоуин)
На Хэллоуин станция украшается, появляются тыквы-метеоры и хэллоуинские мелочи.

- **Код:** [`/datum/round_event_control/spooky`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/holiday/halloween.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/holiday_events.rs`: только на Хэллоуин (29.10–2.11), вес −1 (запускается при первом случайном выборе), объявление tg; каждому человеку с рюкзаком — сумка `trick-o-treat` с шестью случайными сладостями (или мозгом); нет Иана в простыне и призрачного Поли, тыквенных метеоров

### Valentines!
В День святого Валентина экипаж разбивают на пары «валентинок» с открытками и задачами защищать пару.

- **Код:** [`/datum/round_event_control/valentines`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/holiday/vday.dm#L8)
- **Статус в ss13-rs:** ✅ готово — `events/holiday_events.rs` + `crates/ss13_game/src/antag/valentine.rs` (пары, третий лишний, разбиватель сердец, подарки). Без опроса (все записаны по умолчанию, как в tg) и текстов открытки/конфеты

### Easter Eggselence / Release the Rabbits!
На Пасху по станции прячут пасхальные яйца; отдельное событие выпускает кроликов.

- **Код:** [`/datum/round_event_control/easter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/holiday/easter.dm#L1), [`/datum/round_event_control/rabbitrelease`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/holiday/easter.dm#L15)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/events/holiday_events.rs`: только на Пасху (с субботы по понедельник); Easter Eggselence — объявление tg (вес −1); Release the Rabbits! (вес 5, до 10 раз) — объявление «Пушистая тревога» и пасхальные кролики на 15% точек техтоннелей, космические на 15% точек карпов; нет яиц-сюрпризов в луте техтоннелей и 35% кроликов на `event_spawn` (эти точки не отслеживаются)

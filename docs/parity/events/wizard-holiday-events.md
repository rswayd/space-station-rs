# Волшебные и праздничные события

События пула мага (Summon Events, `EVENT_CATEGORY_WIZARD`, обычным планировщиком не выпадают) и события, привязанные к праздникам.

## События мага

### Усиление мага: Robeless / Improved Casting
Robeless: маг (и все, у кого есть заклинания) колдует без робы и шляпы; Improved: заклинания всех получают уровень.

- **Код:** [`/datum/round_event_control/wizard/robelesscasting`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/aid.dm#L4), [`/datum/round_event_control/wizard/improvedcasting`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/aid.dm#L32)
- **Статус в ss13-rs:** ❌ нет — нет мага и заклинаний

### Призывы: Summon Guns / Summon Magic
Каждый член экипажа получает случайное оружие (или магический предмет); часть получивших становится выжившими-антагонистами.

- **Код:** [`/datum/round_event_control/wizard/summonguns`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/summons.dm#L1), [`/datum/round_event_control/wizard/summonmagic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/summons.dm#L17)
- **Статус в ss13-rs:** ❌ нет — нет мага

### Перемешивание: Change Places / Faces / Minds
Все люди меняются местами, внешностью и именами или разумами (массовый обмен телами).

- **Код:** [`/datum/round_event_control/wizard/shuffleloc`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/shuffle.dm#L4), [`/datum/round_event_control/wizard/shufflenames`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/shuffle.dm#L41), [`/datum/round_event_control/wizard/shuffleminds`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/wizard/shuffle.dm#L74)
- **Статус в ss13-rs:** ❌ нет — нет события

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
- **Статус в ss13-rs:** ❌ нет — нет события

## Праздничные события

### Visit by Santa (Рождество)
В Рождество призрак может стать Сантой, который раздаёт подарки; станция украшается.

- **Код:** [`/datum/round_event_control/santa`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/holiday/xmas.dm#L70)
- **Статус в ss13-rs:** ❌ нет — нет праздников

### 2 SPOOKY! (Хэллоуин)
На Хэллоуин станция украшается, появляются тыквы-метеоры и хэллоуинские мелочи.

- **Код:** [`/datum/round_event_control/spooky`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/holiday/halloween.dm#L1)
- **Статус в ss13-rs:** ❌ нет — нет праздников

### Valentines!
В День святого Валентина экипаж разбивают на пары «валентинок» с открытками и задачами защищать пару.

- **Код:** [`/datum/round_event_control/valentines`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/holiday/vday.dm#L8)
- **Статус в ss13-rs:** ❌ нет — нет праздников

### Easter Eggselence / Release the Rabbits!
На Пасху по станции прячут пасхальные яйца; отдельное событие выпускает кроликов.

- **Код:** [`/datum/round_event_control/easter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/holiday/easter.dm#L1), [`/datum/round_event_control/rabbitrelease`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/events/holiday/easter.dm#L15)
- **Статус в ss13-rs:** ❌ нет — нет праздников

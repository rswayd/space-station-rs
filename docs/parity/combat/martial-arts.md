# Боевые искусства
Стили рукопашного боя `/datum/martial_art`: заменяют удары, толчки и захваты комбо-приёмами. Даются свитками/книгами, перчатками, поясами, мутациями. В ss13-rs есть механика, CQC, спящий карп, бокс, рестлинг и дзюдо (`crates/ss13_game/src/martial/`).

### Механика боевых искусств
Стиль перехватывает `harm`/`disarm`/`grab` и копит серию действий (`streak`) для комбо; изучается через `teach` (книга-грантер, предмет), может быть временным (пока надет пояс/перчатки).

- **Код:** [`/datum/martial_art`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/martial/_martial.dm#L1), [`proc/teach()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/martial/_martial.dm#L326), [`proc/harm_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/martial/_martial.dm#L197), [`proc/add_to_streak()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/martial/_martial.dm#L282), [`/obj/item/book/granter/martial`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/granters/martial_arts/_martial_arts.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/martial/mod.rs`: стили моба (`MartialArts`, активный первым), серии `H`/`D`/`G`/`E` по одной цели с `combo_timer`, перехват удара, толчка (`crate::security`), захвата (`crate::pull`) и помощи с исходами `Invalid`/`Fail`/`Success`, блоки защитника (`check_block`), обучение свитками и книгами (3 страницы и чтение по 5 с, `uses`, ограничение вампиров) и пока надеты перчатки/пояс, смена стиля и подсказка кнопками; нет счётчика комбо на HUD, `locked_to_use`, `TRAIT_MARTIAL_ARTS_IMMUNE`, разбивания столов; мутации и импланты, дающие стиль, — в своих группах

### CQC
Боевое искусство спецназа (ядерные оперативники, повар — «кулинария близкого контакта» на кухне): сбивания, удушающий захват, обезоруживание, блоки.

- **Код:** [`/datum/martial_art/cqc`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/martial/cqc.dm#L7), [`/datum/martial_art/cqc/under_siege`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/martial/cqc.dm#L406)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/martial/cqc.rs`: бросок, пинок (с нокаутом лежачего без выносливости), сдерживание и удушающий, давление, последовательный CQC, подсечка лёжа, мгновенный агрессивный захват, выбивание оружия, блок в режиме броска; нет сворачивания шеи (нет kill-захвата), дрожи, перенаправления касательных заклинаний и «кулинарии близкого контакта» повара (нет скиллчипов)

### Спящий карп
Стиль из свитка синдиката: отражает снаряды в режиме боя, сильные комбо, запрещает огнестрел.

- **Код:** [`/datum/martial_art/the_sleeping_carp`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/martial/sleeping_carp.dm#L6)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/martial/carp.rs`: четыре комбо, урон выносливости захватами и толчками, нервный захват, отражение снарядов и уворот по стилю (одежда, броня, предметы в руках), запрет огнестрела (`crate::guns`), удары без промаха (`crate::combat`), захват сложнее сбросить (`crate::pull`); свиток и перчатки карпа; нет сворачивания шеи, головокружения и немоты, фракции и языка карпов, черт против ран и расчленения

### Бокс
Боксёрские перчатки: только урон стамине, нокауты; злой бокс и охотничий бокс (Лаваленд) — летальные варианты.

- **Код:** [`/datum/martial_art/boxing`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/martial/boxing.dm#L16), [`/datum/martial_art/boxing/evil`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/martial/boxing.dm#L402), [`/datum/martial_art/boxing/hunter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/martial/boxing.dm#L428)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/martial/boxing.rs`: перчатки (обычные, злые, охотничьи и рукавицы), удары по выносливости, прямой/джеб/хуки/апперкот по кнопкам и руке, подлый удар злого бокса, нокауты и стаггер, блок в режиме броска, охота на зверей (basic mobs); нет навыка атлетики и опыта, «вкуса детектива», эффекта ребьюка и боевого клича

### Борьба (wrestling)
Пояс рестлера: броски, удары с разбега, удержание.

- **Код:** [`/datum/martial_art/wrestling`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/martial/wrestling.dm#L8)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/martial/wrestling.rs`: пояс, кнопки приёмов, бросок тела и бросок схваченного, пинок, удар головой, падение, захват в замок; приёмы срабатывают сразу, без раскручивания, подъёма и залезания на объекты, без криков и сальто

### Плазменный кулак
Стиль волшебника: торнадо, отбрасывание, взрывной финальный приём.

- **Код:** [`/datum/martial_art/plasma_fist`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/martial/plasma_fist.dm#L5)
- **Статус в ss13-rs:** ❌ нет

### Каза Рук
Стиль ящеров (перчатки): удар в шею, подсечка, удар в лёгкие.

- **Код:** [`/datum/martial_art/kaza_ruk`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/martial/kaza_ruk.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Прочие стили
Психотический мордобой (случайные эффекты, от наркотиков), грибной удар (гриболюди), укус паука, искусство джунглей.

- **Код:** [`/datum/martial_art/psychotic_brawling`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/martial/psychotic_brawl.dm#L1), [`/datum/martial_art/mushpunch`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/martial/mushpunch.dm#L1), [`/datum/martial_art/spiders_bite`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/martial/spiders_bite.dm#L1), [`/datum/martial_art/jungle_arts`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/martial/junglearts.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Корпоративное дзюдо (BandaStation)
Стиль форка для СБ: даётся поясом дзюдо, нелетальные броски, захваты и «сбить с толку».

- **Код:** [`/datum/martial_art/judo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/martial_arts/code/judo/judo.dm#L1), [`/obj/item/storage/belt/judobelt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/martial_arts/code/judo/belt.dm#L6)
- **BandaStation:** [`modular_bandastation/martial_arts/code/judo/judo.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/martial_arts/code/judo/judo.dm#L1) — весь стиль
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/martial/judo.rs`: пояс, удары по 6, бросок, рычаг, большое колесо, тычок в глаза, золотой взрыв, «сбить с толку» кнопкой, отвращение к дубинкам; нет анимации вращения, искр и боевого клича, проверки защиты глаз

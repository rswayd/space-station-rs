# Базовые механики basic-мобов
Общий фундамент `/mob/living/basic`: здоровье, атаки, среда обитания, еда, приручение, команды питомцам. В ss13-rs есть basic-мобы (`crates/ss13_game/src/mobs/`) с ИИ на деревьях поведения.

### Базовый тип basic-моба
Упрощённый живой моб без органов и частей тела: одна шкала `health`/`maxHealth`, урон по множителям `damage_coeff`, смерть со сменой спрайта (`look_dead`/`look_alive`), скорость `speed`, флаг `gold_core_spawnable` для ксенобиологии (золотое ядро призывает случайного моба).

- **Код:** [`/mob/living/basic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/basic.dm#L2), [`Initialize()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/basic.dm#L107), [`death()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/basic.dm#L174), [`look_dead()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/basic.dm#L200)
- **BandaStation:** [`modular_bandastation/mobs/code/mobs_resting.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/mobs_resting.dm#L2) — спрайт отдыха `icon_resting` для лежащих basic-мобов; [`overrides.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/overrides.dm#L1) — русские глаголы реакции/атаки и звуки получения урона `damaged_sounds`
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/mod.rs`: прототипы `/mob/living/basic` (карта и спавн) получают определение из `mobs.ron` (`ss13-mapload convert`, `crates/ss13_mapload/src/mobs.rs`): `maxHealth`, `physiology`, одна шкала урона (всё в brute, tg `simple_transfer_to_brute_loss`), смерть при 0 без крита, `speed`, фракции, `icon_living`/`icon_dead`, `DEL_ON_DEATH`, `REMAIN_DENSE_WHILE_DEAD`, deathgasp «перестает двигаться...», осмотр; нет `gold_core_spawnable`, `icon_resting` форка, `FLIP_ON_DEATH`, гиба с лутом

### Ближняя атака и защита basic-моба
Моб бьёт цель через `melee_attack` (урон `melee_damage_lower..upper`, тип `melee_damage_type`, кулдаун `melee_attack_cooldown`, глагол `attack_verb_*`); реакции на руку игрока (погладить/толкнуть/ударить через `response_*`), на удары предметами, пришельцев, животных.

- **Код:** [`proc/melee_attack()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/basic.dm#L229), [`attack_hand()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/basic_defense.dm#L1)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/overrides.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/overrides.dm#L1) — звук `damaged_sounds` при ударе по мобу
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/defense.rs`: `melee_attack` → `attack_animal` (`melee_damage_lower..upper`, тип, кулдаун, глаголы с `ru_attack_verb`, случайная зона), `attack_hand` (гладит/бьет 5-10 по `response_*`), удары предметами (`force`, `attack_speed`), `pet_bonus`-эмоции; `// shortcut:` без брони, блоков, ран, звуков, толчка (ПКМ) и `damaged_sounds`

### Дальние атаки и телеграф атак
Компонент `ranged_attacks` даёт мобу стрельбу снарядами/кастингами с кулдауном и залпами; `basic_mob_attack_telegraph` показывает предупреждение перед ударом (сильные мобы вроде голиафа).

- **Код:** [`/datum/component/ranged_attacks`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/ranged_attacks.dm#L4), [`/datum/component/basic_mob_attack_telegraph`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/basic_mob_attack_telegraph.dm#L4)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ranged.rs`, `crates/ss13_game/src/mobs/ai/fauna.rs`: `ranged_attacks` (снаряд по цели с кулдауном, поворот к цели) хайвботов (10 урона) и стикмена-стрелка (9 мм, 5 с), `basic_ranged_attack` (в обзоре до 9, без огня по своим: шаг в сторону); нет залпов (`burst_shots`), гильз, звуков выстрела, телеграфа атак

### Требования к атмосфере и температуре
Элементы `atmos_requirements` (мин/макс газы, урон `unsuitable_atmos_damage` вне нормы) и `body_temp_sensitive` (урон вне диапазона температур) — поэтому карпы живут в космосе, а корги умирают.

- **Код:** [`/datum/element/atmos_requirements`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/atmos_requirements.dm#L6), [`/datum/element/body_temp_sensitive`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/body_temp_sensitive.dm#L4)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/mobs/mod.rs` (`life`, `breathable`, `body_temperature_step`): `atmos_requirements` по молям газов клетки и `unsuitable_atmos_damage`, температура тела по `/mob/living/handle_environment`, ожоги вне `minimum/maximum_survivable_temperature`; раз в 2 с (SSmobs); алертов нет (у мобов нет HUD)

### Поедание и добыча после смерти
`basic_eating` позволяет мобу есть подходящие предметы (лечит/ранит), `death_drops` выбрасывает лут при смерти (мясо, шкуры, трофеи).

- **Код:** [`/datum/element/basic_eating`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/basic_eating.dm#L6), [`/datum/element/death_drops`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/death_drops.dm#L6)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/eating.rs`: `basic_eating` (корова, коза, ящерицы и крупные ящеры форка, моль-таракан, пингвин, змея, черепаха, улитка; списки еды из их `Initialize`, `// shortcut:`): съедают кормёжку из рук вне боевого режима и то, к чему подходит ИИ, лечатся на `heal_amt`, сообщение с глаголами `[eat_verbs]` форка; поедание мобов (бабочки, тараканы, мыши) удаляет их, кроме мобов игроков; нет `damage_amount`/`drinking`/`add_to_contents`, `death_drops`

### Приручение
Компонент `tameable`: скармливание любимой еды с шансом приручает моба (становится дружелюбным владельцу, иногда даёт езду или команды). Пример: лавовые ящеры, козы, гаты.

- **Код:** [`/datum/component/tameable`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/tameable.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/pets.rs`, `crates/ss13_game/src/mobs/eating.rs`: `tameable` коров, свиней, пони, карпов (затем слушаются команд), попугаев, собак, мышей (не крыс), змей: любимая еда с руки, шанс `tame_chance` растёт на `bonus_tame_chance`, приручивший — друг (`BB_FRIENDS_LIST`, союзная фракция: моб его не трогает), реакции `tamed()` (корова, пони, карп, собака, мышь); карп разлюбляет ударившего друга; нет `TRAIT_BEAST_EMPATHY` (+50%, имя питомцу), езды после приручения

### Команды питомцам
Компонент `obeys_commands`: друг моба выбирает команду в радиальном меню или голосом — «сидеть», «за мной», «фас», «охраняй», «притворись мёртвым», «апорт», «размножайся», «используй способность», «иди туда», «лови рыбу».

- **Код:** [`/datum/component/obeys_commands`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/pet_commands/obeys_commands.dm#L7), [`/datum/pet_command`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/pet_commands/pet_command.dm#L6), [`pet_commands_basic.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/pet_commands/pet_commands_basic.dm#L7)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/pets.rs`, `crates/ss13_game/src/mobs/ai/trees.rs` (`pet_command_root`): друг в обзоре 7 клеток говорит слово команды (рус./англ. `speech_commands` форка): сидеть, гулять, за мной (`start_active` — сразу у нового друга), фас (затем указать цель, друзей отказывается), притворись мёртвым, «хороший мальчик»; поддерево команды во главе дерева моба; нет радиального меню, коллаутов, апорта, «иди туда», трюков, способностей и размножения по команде

### Езда на существах
`riding/creature`: на некоторых мобов (лошади, гориллы, рапторы, крокодилы форка) можно сесть и управлять ими; смещения всадника по направлениям.

- **Код:** [`/datum/component/riding/creature`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/riding/riding_mob.dm#L3)
- **Статус в ss13-rs:** ❌ нет

### Стаи и проход сквозь мобов
`swarming`: несколько мобов на одной клетке рассредоточиваются визуально и не мешают друг другу.

- **Код:** [`/datum/component/swarming`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/components/swarming.dm#L1)
- **BandaStation:** [`carp.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/space_fauna/carp/carp.dm#L23) — карпы роятся, проходят через столы и мобов
- **Статус в ss13-rs:** ❌ нет

### Конструктор питомца
Окно выбора и настройки личного питомца (собака, кошка, лиса, мелкие паразиты, попугай): тип, имя, окрас — для квирка владельца питомца.

- **Код:** [`/datum/pet_customization`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/pet_designer.dm#L33); tgui: [`PetBuilder.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PetBuilder.tsx)
- **Статус в ss13-rs:** ❌ нет

# Ближний бой: клики, режим боя, атаки
Цепочка атаки по клику, режим боя, удары руками и предметами, блоки. Конкретное оружие — [melee-weapons.md](melee-weapons.md).

### Обработка клика (ClickOn)
Клик по миру разбирается по модификаторам (shift — осмотр, ctrl — тянуть, alt — контекст, средняя кнопка — указать), режиму броска, затем проверяются досягаемость (`CanReach`), кулдаун `next_move` и состояние моба; дальше вызывается атака рукой, предметом или дистанционное действие.

- **Код:** [`proc/ClickOn()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/click.dm#L71), [`proc/changeNext_move()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/click.dm#L20)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/click.rs`: shift/ctrl-клики, alt-клик (`click_alt`, иначе панель лута — `loot.rs`), alt+shift, средняя кнопка (`cycle_hand`), shift+средняя (указать — `point.rs`), режим броска, досягаемость, проверка сознания, кулдаун `next_move` (после удара, броска), очередь кликов клиента (по одному за тик) и лимит 15 кликов/с; левая кнопка срабатывает на отпускание, перетаскивание — `MouseDrop` (`drag.rs`, `client.rs`); нет ctrl+shift (передача), alt+правого клика, `CLICK_CD_HANDCUFFED`, дистанционных действий (`// shortcut:` в `click.rs`)

### Режим боя (combat mode)
Переключатель (кнопка HUD, клавиша) меняет поведение пустой руки и многих предметов: в бою — удары, вне боя — помощь, объятия, обыск, лечение.

- **Код:** [`proc/set_combat_mode()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_defense.dm#L254), [`/atom/movable/screen/combattoggle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/screen_objects/screen_objects.dm#L373), [`/datum/keybinding/living/toggle_combat_mode`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/living.dm#L90)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/combat.rs`, `crates/ss13_game/src/hud.rs`

### Цепочка атаки предметом
`melee_attack_chain`: `pre_attack` → `attackby`/`attack` цели → `attacked_by` (урон `force` типа `damtype` по выбранной зоне с броней) → `afterattack`; кулдаун `attack_speed` (по умолчанию `CLICK_CD_MELEE` 0.8 с), сообщение с глаголом атаки и зоной, звук `hitsound`.

- **Код:** [`proc/melee_attack_chain()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/item_attack.dm#L24), [`proc/pre_attack()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/item_attack.dm#L111), [`proc/attack()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/item_attack.dm#L227), [`living/attacked_by()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/item_attack.dm#L319), [`proc/afterattack()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/item_attack.dm#L479), [`proc/send_item_attack_message()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/item_attack.dm#L490)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/combat.rs`: урон `force`/`damtype` по зоне с бронёй `melee` и репликами брони, раны, блоки, кулдаун `attack_speed`, звук `hitsound`, анимация; нет глаголов атаки предмета (`attack_verb_continuous`), `armour_penetration` предметов, `afterattack`-эффектов предметов

### Атака по объектам
Предметы бьют структуры и машины (`attack_atom`), нанося им урон с учётом их брони и `armor_penetration`.

- **Код:** [`proc/attack_atom()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/item_attack.dm#L280)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/obj_damage.rs`: урон объектам от предметов; детали — у агента engineering/items

### Удары без оружия
Пустая рука в режиме боя бьёт (5–10 brute, шанс промаха зависит от урона атакующего), по лежачему — пинок (7–15, без промаха); вид определяет урон и вербы; вне боя — объятия, похлопывание, встряхивание лежачего, CPR.

- **Код:** [`proc/harm()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/_species.dm#L762), [`proc/help()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/_species.dm#L745), [`proc/spec_attack_hand()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/_species.dm#L996)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/combat.rs`

### Анимации атаки
Атакующий дёргается к цели, над целью — эффект удара (кулак, пинок, укус, когти) или иконка предмета.

- **Код:** [`living/do_attack_animation()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_defense.dm#L765)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/animation.rs` (`do_attack_animation_with`, `item_sweep`, `effect_sweep`), вызов из `crates/ss13_game/src/combat.rs`: рывок атакующего к цели и временный спрайт на тайле цели (`ss13_shared::animation::Animation::Transform` по сети, клиент — `animation_client.rs`): предмет летит удар/укол/разрез по `sharpness` (`animate_attack`: blunt/pierce/slash), без оружия — `punch`/`kick` из `effects.dmi`; нет затухания альфы (спрайт исчезает в конце анимации tg), `icon_angle`/`attack_icon` предметов не импортируются (спрайт повёрнут как смотрящий на восток), укусы и когти — с видами без рук

### Блоки и щиты
`check_block` перед попаданием спрашивает у предметов в руках и надетого (`hit_reaction`): щиты, энергомечи, некоторые боевые искусства блокируют атаки и снаряды с `block_chance`; щит может сломаться.

- **Код:** [`proc/check_block()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living_defense.dm#L928), [`human/check_block()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/human_defense.dm#L79), [`proc/hit_reaction()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items.dm#L622), [`proc/on_shield_block()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/game/objects/items/weaponry/shields.dm#L89)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/weapons/block.rs` (`held_block`, `against`, `on_block`), `combat.rs`, `projectile.rs` (`hit_living`), `throw.rs` (`hit_living`): блок предметами в руках против ударов, снарядов (`PROJECTILE_ATTACK`: −½ `armour_penetration` снаряда, прозрачный щит пропускает `PASSGLASS`-лучи, двуручный меч и энергощит оставляют отражаемые выстрелы отражению, сабли/клейморы/посохи из `NO_PROJECTILE_BLOCK` не блокируют выстрелы) и брошенного (`THROWN_PROJECTILE_ATTACK`: щиты +30, реактивная броня тоже), жёлтое свечение `block`, звук, износ и поломка щита; нет `armour_penetration` предметов, прыжков/сокрушающих атак, отражения выстрелов предметами в руках (`IsReflect` — у агента оружия), щит принимает выстрелы своей `melee`-бронёй

### Выбор зоны (zone selector)
Селектор зон на HUD и клавиши нумпада выбирают часть тела (голова, глаза, рот, грудь, пах, руки, ноги), куда направлены атаки, лечение и операции.

- **Код:** [`/atom/movable/screen/zone_sel`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/hud/screen_objects/screen_objects.dm#L732), [`code/datums/keybinding/mob.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/keybinding/mob.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/zone.rs`, `crates/ss13_game/src/medical.rs`

### Шанс попадания по зоне
Удар идёт в выбранную зону с шансом, иначе в случайную часть тела; по лежачим точнее; отсутствующая конечность перенаправляет удар.

- **Код:** [`living/attacked_by()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/_onclick/item_attack.dm#L319)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/combat.rs`, `crates/ss13_game/src/zone.rs`

### Неуклюжесть (clumsy)
Клоуны и мобы с `TRAIT_CLUMSY` иногда бьют себя своим оружием, стреляют себе в ногу, роняют предметы.

- **Код:** [`proc/check_botched()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/gun.dm#L458)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/genetics/mod.rs` (мутация `/datum/mutation/clumsy`, `is_clumsy`, `make_clown_clumsy`; клоун получает её при спавне — `jobs.rs`), `guns.rs` (`check_botched`: 40% выстрел себе в ногу и оружие падает; кроме магических и игрушечных), `security.rs` (`clumsy_baton`: 50% дубинка бьёт по своей голове, 2×force), `restraints.rs` (50% наручники на себя); нет квирка Clumsy, проклятия голода и наркотического источника черты; неуклюжесть остальных предметов (гранаты, вспышка, двуручный меч, удар в глаз, бумага, мышеловки…) — с их механиками у агентов items/security

### Подсечка коленей (kneecapping)
Атака по ногам лежачего острым оружием с задержкой ломает кости.

- **Код:** [`/datum/element/kneecapping`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/kneecapping.dm#L17)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/weapons/kneecap.rs` (`try_start`, `complete`), `combat.rs`: ПКМ бейсбольной битой по ноге (зона — нога) лежащего/обездвиженного/пристёгнутого карбона — 3 с `do_after` (`Job::Kneecap`), затем урон `force` в ногу без брони с `wound_bonus` между порогами тяжёлой и критической тупой раны (+10), крик, анимация, звук; условия проверяются заново по окончании

### Удар о стол
Агрессивно захваченного можно бросить на стол (оглушение), ударить головой о стол или придавить конечность.

- **Код:** [`proc/tablepush()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/table_smash.dm#L140), [`proc/tablelimbsmash()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/elements/table_smash.dm#L170)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/tables.rs` (`smash`): вне боевого режима — уложить за 3,5 с, в боевом — агрессивный захват швыряет на стол (сбивание 3 с, 40 стамины, 10 брута), пассивного мало; нет удара конечностью (нет захвата за шею), переноски на плечах и отдыха (уложенный не ложится)

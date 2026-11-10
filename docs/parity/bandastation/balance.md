# Правки баланса BandaStation
Модпак `balance` — набор точечных изменений апстрим-значений, собранных в одном месте. Переменные, переопределённые здесь, ss13-rs получает автоматически через `ss13_dm_extract` (он разбирает весь `.dme`, включая модпаки) — там, где соответствующая система уже есть.

### RP-скорость передвижения
Все задержки движения пересчитываются из tg-скорости (1.5) в RP-скорость из конфига (`run_delay`): `modified_move_delay()` масштабирует модификаторы скорости, отдельно заданы скорости клоун-мобов и замедление блюспейс-пола.

- **Код:** [`modified_move_delay()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/speed/movespeed_modifier.dm#L1), [`base_speed.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/speed/base_speed.dm#L1)
- **BandaStation:** [`_balance.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/_balance.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/lib.rs` (`RUN_DELAY_SECS` = tg `run_delay` 2.2): нет пересчёта модификаторов скорости и ходьбы, нет скоростей мобов

### Телескопическая дубинка
Урон выносливости телескопической дубинки снижен до 35.

- **Код:** [`/obj/item/melee/baton/telescopic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/weapons/baton.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/security.rs` (берёт `stamina_damage` из извлечённых DM-данных)

### Огнестрел и снаряды СБ
SMG-дизейблер и снайперская винтовка становятся громоздкими (`WEIGHT_CLASS_BULKY`), урон сети дрэгнета 15 и дробового дизейблера 4.5.

- **Код:** [`/obj/item/gun/energy/disabler/smg`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/weapons/disabler.dm#L1), [`/obj/projectile/energy/snare`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/weapons/dragnet.dm#L1), [`/obj/item/gun/ballistic/rifle/sniper_rifle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/weapons/rifle.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — значения приходят из DM: `w_class` (`ss13_mapload::contents::DmWeight`) и урон снарядов (`ss13_mapload::guns::ProjectileData`, `crates/ss13_game/src/projectile.rs`); самого дрэгнета (сеть/телепорт снаряда `snare`) в порте нет — ../combat/

### Торговые автоматы
Свой ассортимент автомата СБ (наручники, стяжки, флешки, светошумовые...), запрет взаимодействия инструментами с «ожившим» автоматом (с ai_controller), премиум-цена RLD.

- **Код:** [`/obj/machinery/vending/security`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/vending_security.dm#L1), [`/obj/machinery/vending/tool_act()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/vending.dm#L1), [`/obj/item/construction/rld`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/tools.dm#L2)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/vending/`, `crates/ss13_mapload/src/vending.rs`: ассортимент и премиум-цены приходят из DM, оплата работает; запрет инструментов касается только «оживших» автоматов — ../mobs/mob-ai.md (их нет); механики самих товаров — в их разделах

### Обыск карманов
Тихо вытащенный из кармана предмет попадает в руки обыскивающего, с логированием.

- **Код:** [`finish_unequip_mob_pocket()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/human_stripping.dm#L11)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/strip.rs` (`pocket_to_hands`): вынутое из кармана через меню обыска уходит в руки обыскивающему-человеку (иначе на пол), запись в лог сервера (нет per-mob attack log)

### Медицина и травмы
Постоянные травмы мозга и скорость его распада из конфига, трещина черепа только от тупого урона, голова снова отделяема, гипоспрей CMO игнорирует защиту, регенеративное ядро лечит изнутри на 50.

- **Код:** [`/obj/item/organ/brain`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/_brain.dm#L1), [`/datum/wound_pregen_data/cranial_fissure`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/wounds/cranial_fissure.dm#L1), [`/obj/item/bodypart/head`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/bodyparts/head.dm#L1), [`/obj/item/reagent_containers/hypospray/cmo`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/containers/reagent_containers.dm#L1), [`regenerative_core`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/regenerative_core.dm#L6)
- **Статус в ss13-rs:** ✅ реализовано — голова отделяема (`crates/ss13_game/src/medical/bodyparts.rs`); гипоспрей CMO в `IGNORE_CLOTHING` (`crates/ss13_game/src/chem.rs`); описание сохранённого ядра «heal partially» (`crates/ss13_game/src/mining/cores.rs`); составы медипенов выживания — из DM. Зависимости в чужих группах: трещины черепа (`cranial_fissure`) нет в ранах — ../medical/; внутреннее ядро (+50) ждёт имплантации ядер — ../mining/ (`cores.rs` shortcut); конфиг `brain_permanent_traumas`/`brain_decay_rate` не заведён — по умолчанию выключен и совпадает с tg (×0.5)

### Мобы
Больше здоровья гигантским паукам (страж 200, засадный 150), шейд оглушаем и не тянет предметы, headslug проходит сквозь мобов и столы.

- **Код:** [`/mob/living/basic/spider/giant/guard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/giant_spiders.dm#L1), [`/mob/living/basic/shade`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/mobs/shade.dm#L1), [`/mob/living/basic/headslug`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/headslug.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — здоровье и урон гигантских пауков берутся из DM (`ss13_mapload::mobs`: `maxHealth`, `health`, `melee_damage_*`). Зависимости: `web_speed` и `poison_per_bite` — способностей пауков нет (../mobs/); `pass_flags` мобов (PASSMOB/PASSTABLE шейда и headslug), `TRAIT_UNDENSE` headslug, `CANSTUN` и запрет таскания у шейда — у мобов нет флагов прохода/оглушаемости (../mobs/)

### Антагонисты
Блоб вне основного этажа станции платит больше за расширение и должен захватить больше тайлов; стартовый клинок еретика может взорваться ограниченное число раз и не ближе 10 тайлов.

- **Код:** [`/mob/eye/blob`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/blob.dm#L1), [`/datum/heretic_knowledge/limited_amount/starting`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/heretic_knife.dm#L3)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/antag/blob/mod.rs` (`check_station_floor`: ×2 цена расширения, 700 тайлов; `main_station_floor` всегда истина — у карт нет `main_floor`, поэтому сообщение/алерт вне этажа не показываются), `crates/ss13_game/src/antag/heretic/server/rune.rs` (взрыв дальнего клинка, лимит 1, не ближе 10 тайлов)

### События и карго
Событие нюк-оперативника — от 30 игроков; заблудившиеся грузовые капсулы взрываются; все капсулы снабжения прилетают телепортом; религиозная секта honorbound недоступна; в КПК по умолчанию программа манифеста экипажа.

- **Код:** [`/datum/round_event_control/operative`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/events.dm#L1), [`/datum/supply_pack`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/supply_packs.dm#L1), [`/datum/religion_sect`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/religion.dm#L1), [`crew_manifest`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/crew_manifest.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/modpc/pda.rs`: манифест экипажа (Plexagon Crew List) есть в каждом КПК; событий, капсул снабжения и религии нет

### Отключённые станционные трейты
Ряд трейтов (бюрократия overflow, skub, GMM spotlight и др.) получает вес 0 и не выпадает.

- **Код:** [`station_traits.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/station_traits.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/station_traits.rs`: шесть отключённых форком трейтов в пул не входят (отмечено в документации модуля)

### Техника: MODsuit ОБР и мех Paddy
Укомплектованный MODsuit «responsory» (блюспейс-ячейка, модули), мех Paddy медленнее и с другими задержками входа.

- **Код:** [`/obj/item/mod/control/pre_equipped/responsory`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/mod_types.dm#L1), [`/obj/vehicle/sealed/mecha/ripley/paddy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/balance/code/vehicle/paddy.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — из DM: комплектация/ячейка/пины `pre_equipped` и броня/сложность темы (`ss13_mapload::modsuit`, `crates/ss13_game/src/modsuit/server/init.rs`), `movedelay` и `*_pressure_step_in` Paddy (`ss13_mapload::mecha`); модули, которых нет в порте, — в своих разделах ../

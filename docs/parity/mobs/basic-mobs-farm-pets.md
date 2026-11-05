# Фермерские животные и питомцы
Домашний скот (`code/modules/mob/living/basic/farm_animals`) и питомцы станции (`basic/pets`). Ничего из этого в ss13-rs нет.

### Коровы
Доятся ведром/стаканом (молоко), едят пшеницу, могут быть опрокинуты (`tipped`). Варианты: мунекорн (молоко-галлюциноген), корова мудрости (даёт опыт навыка).

- **Код:** [`/mob/living/basic/cow`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/cow/_cow.dm#L2), [`/mob/living/basic/cow/moonicorn`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/cow/cow_moonicorn.dm#L2), [`/mob/living/basic/cow/wisdom`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/cow/cow_wisdom.dm#L2)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/friendly/farm_animals.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/friendly/farm_animals.dm#L10) — чёрная, бурая, белая коровы, русские имена и звуки
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ai/trees.rs`, `crates/ss13_game/src/mobs/eating.rs`: бродят, мычат фразами контроллера, ищут и едят пшеницу (с пола и из рук); нет доения, опрокидывания (`tip_reaction`), приручения, мунекорна и коровы мудрости

### Козы
Агрессивны к обидчикам, жуют лианы и кудзу (полезны против ботанических сорняков), доятся.

- **Код:** [`/mob/living/basic/goat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/goat/_goat.dm#L2)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/friendly/farm_animals.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/friendly/farm_animals.dm#L1) — переводы и звуки
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ai/trees.rs`, `crates/ss13_game/src/mobs/eating.rs`: `forage_and_retaliate`: бьют обидчиков и иногда случайных встречных (`capricious_retaliate`, успокаиваются), ищут и съедают лианы, кудзу и светогрибы; нет доения, поедания растений под ногами

### Куры и цыплята
Курица несёт яйца при кормлении зерном; оплодотворённые яйца вылупляются в цыплят, цыплята вырастают в кур.

- **Код:** [`/mob/living/basic/chicken`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/chicken/chicken.dm#L9), [`/mob/living/basic/chick`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/chicken/chick.dm#L6)
- **BandaStation:** [`chicken.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/chicken/chicken.dm#L68) — флаг `can_egg_layer`; [`modular_bandastation/mobs/code/simple_animal/friendly/farm_animals.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/friendly/farm_animals.dm#L50) — петухи (не несут яйца), «крутой» петух
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ai/trees.rs`: куры бегают и кудахчут (`skittish_and_speak`), убегают от обидчиков; петух форка — капризный (`simple_capricious_combat`); нет яиц, кормления зерном, цыплят

### Гуси
Злобные гуси нападают на прохожих; «рвотный» гусь Birdboat глотает предметы и извергает их.

- **Код:** [`/mob/living/basic/goose`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/goose/goose.dm#L5), [`/mob/living/basic/goose/vomit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/goose/goose.dm#L80)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/friendly/farm_animals.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/friendly/farm_animals.dm#L132) — гусята и индейка
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ai/trees.rs`: гуси бродят, гогочут, бьют обидчиков и случайных встречных (`forage_and_retaliate` с `capricious_pick_target`); Birdboat только отвечает на удары (`goose_calm`); нет поедания и извержения предметов

### Гориллы
Сильные мобы, носят ящики (каргорилла — питомец карго), гориллы из генетики (обезьяна + мутация), барная горилла, враждебные варианты.

- **Код:** [`/mob/living/basic/gorilla`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/gorilla/gorilla.dm#L14), [`/mob/living/basic/gorilla/cargorilla`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/gorilla/gorilla.dm#L171)
- **Статус в ss13-rs:** ❌ нет

### Прочий скот: свиньи, овцы, кролики, олени, пони
Свинья (мясо), овца (стрижётся на шерсть), кролик (пасхальный несёт крашеные яйца), олень, пони (приручается, ездовой; синдикатовские и опасные варианты).

- **Код:** [`/mob/living/basic/pig`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/pig.dm#L2), [`/mob/living/basic/sheep`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/sheep.dm#L1), [`/mob/living/basic/rabbit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/rabbit.dm#L9), [`/mob/living/basic/deer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/deer/deer.dm#L1), [`/mob/living/basic/pony`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/pony.dm#L1)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/friendly/pigs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/friendly/pigs.dm#L1) — свиньи переделаны в возрастные стадии (см. [basic-mobs-bandastation.md](basic-mobs-bandastation.md))
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ai/trees.rs`: свиньи и пони дерутся с обидчиками и убегают раненными (`skittish_brawler_combat`, `ai_flee_while_injured`), овцы и кролики пугливы (`skittish_and_speak`), олени убегают от обидчиков и бродят; фразы контроллеров; нет стрижки, яиц, приручения и езды, жизни оленей (трава, вода, деревья)

### Пчёлы
Пчёлы живут в улье (апиарий ботаники), опыляют растения, жалят с реагентом своей королевы; королеву можно пересадить в новый улей.

- **Код:** [`/mob/living/basic/bee`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/bee/_bee.dm#L8), [`/mob/living/basic/bee/queen`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/farm_animals/bee/_bee.dm#L210)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/hostile/bee.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/hostile/bee.dm#L1) — переводы
- **Статус в ss13-rs:** ❌ нет

### Базовый питомец
Общий тип питомцев: ошейник с именем (переименование ошейником), бирка, реакция на поглаживание (настроение), смерть хозяйского питомца даёт плохое настроение.

- **Код:** [`/mob/living/basic/pet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/pet.dm#L1)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/pet.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/pet.dm#L2) — русские глаголы укуса, звук
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/defense.rs`: поглаживание с `pet_bonus`-эмоцией (1 из 3); нет ошейников, бирок, настроения

### Собаки и корги
Корги Иан (питомец ГП, стареет между раундами — persistence), Лиза, щенки, экзотический корги, корги Нар-Си; мопс Макгрифф, бультерьер. Корги носят шляпы и броню через меню раздевания (`dog_fashion`), приносят брошенные предметы, слушаются команд.

- **Код:** [`/mob/living/basic/pet/dog`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/dog/_dog.dm#L20), [`/mob/living/basic/pet/dog/corgi`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/dog/corgi.dm#L3), [`/mob/living/basic/pet/dog/corgi/ian`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/dog/corgi.dm#L388), [`/datum/dog_fashion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/dog_fashion.dm#L1), [`/datum/strippable_item/corgi_head`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/dog/strippable_items.dm#L10)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/pets/dog.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/pets/dog.dm#L1) — тамаскан, немецкая овчарка, бретань, слаймовый и войд-щенки; [`modular_bandastation/mobs/code/simple_animal/pets/security_dogs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/pets/security_dogs.dm#L2) — служебные собаки СБ; [`modular_bandastation/mobs/code/simple_animal/pets/fashion.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/pets/fashion.dm#L5) — новые наряды
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs`: Иан, корги, щенки, мопс Макгрифф с картой, бродят, лают, гладятся; нет шляп/брони, апорта, команд, persistence Иана

### Кошки
Рантайм (кошка ГВ, persistence котят), кошки ловят мышей, могут носить еду; варианты: космическая, хлебная, котёнок, дикая, кейки-торт.

- **Код:** [`/mob/living/basic/pet/cat`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/cat/cat.dm#L1), [`/mob/living/basic/pet/cat/runtime`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/cat/runtime.dm#L5), [`/mob/living/basic/pet/cat/feral`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/cat/feral.dm#L1)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/pets/cat.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/pets/cat.dm#L1) — толстая, белая, бирманская, чёрная кошки
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs`: Рантайм и кошки бродят, мяукают, ловят мышей, отвечают на удары (`ai_retaliate`); нет еды, котят, persistence

### Лисы
Ренальт — лиса капитана; ледяная и дружелюбная лисы.

- **Код:** [`/mob/living/basic/pet/fox`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/fox.dm#L6)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/pets/fox.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/pets/fox.dm#L1) — фенек, лесная лиса
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs`: Ренальт бродит, тявкает, нападает на мелких мобов без свидетелей, убегает от обидчиков

### Попугаи
Попугай Поли (главный инженер) запоминает услышанные фразы и повторяет их между раундами (persistence), ворует и складывает блестящие предметы, садится на плечо; с гарнитурой говорит в радио; призрак Поли.

- **Код:** [`/mob/living/basic/parrot`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/parrot/_parrot.dm#L10), [`/mob/living/basic/parrot/poly`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/parrot/poly.dm#L15)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/mobs.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/mobs.dm#L4) — правки попугая
- **Статус в ss13-rs:** ❌ нет

### Пингвины, ленивцы, гондолы
Пингвины (императорский, птенцы), ленивцы карго (Пэйпворк, Цитрус), молчаливые гондолы и гондолоподы (посадка/капсула доставки).

- **Код:** [`/mob/living/basic/pet/penguin`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/penguin/penguin.dm#L1), [`/mob/living/basic/sloth`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/sloth.dm#L3), [`/mob/living/basic/pet/gondola`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/gondolas/gondola.dm#L6), [`/mob/living/basic/pet/gondola/gondolapod`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/gondolas/gondolapod.dm#L1)
- **BandaStation:** [`modular_bandastation/mobs/code/simple_animal/pets/sloth.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/mobs/code/simple_animal/pets/sloth.dm#L1) — правки ленивца
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/mobs/ai/trees.rs`, `crates/ss13_game/src/mobs/eating.rs`: пингвины (и птенцы) нутят, убегают от обидчиков (раненые), едят рыбу; ленивцы бродят и убегают от обидчиков; гондолы бродят молча; нет рыбалки, яиц, лазанья по деревьям, гондолоподов

### Орби (виртуальный питомец КПК)
Голографический питомец из КПК-приложения NtosVirtualPet: выходит в мир, повышает уровень, получает способности.

- **Код:** [`/mob/living/basic/orbie`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/orbie/orbie.dm#L3); tgui: [`NtosVirtualPet.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/NtosVirtualPet.tsx)
- **Статус в ss13-rs:** ❌ нет

### Культистские питомцы
Питомец, обращённый культом, ищет мёртвых культистов и активирует руны оживления.

- **Код:** [`/datum/action/cooldown/spell/conjure/revive_rune`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/pet_cult/pet_cult_abilities.dm#L1), [`pet_cult_bt.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/basic/pets/pet_cult/pet_cult_bt.dm#L6)
- **Статус в ss13-rs:** ❌ нет

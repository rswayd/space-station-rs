# Личности
Лёгкие модификаторы реакций персонажа на мир (`/datum/personality`, синглтоны): меняют муд-события, иногда дают черту; выбираются в профиле, взаимоисключающие по группам.

### Система личностей и выбор в профиле
Подсистема `SSpersonalities` хранит синглтоны; преференс `personality` применяет выбранные личности к человеку при спавне; группы (`groups`) запрещают несовместимые сочетания.

- **Код:** [`/datum/personality`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/personality/_personality.dm#L13), [`PROCESSING_SUBSYSTEM_DEF(personalities)`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/controllers/subsystem/processing/personality.dm#L1), [`/datum/preference/personality`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/personality/personality_preference.dm#L1), [`_personality_groups.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/personality/_personality_groups.dm#L1); tgui: [`PersonalityPage.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/PreferencesMenu/CharacterPreferences/PersonalityPage.tsx)
- **Статус в ss13-rs:** ❌ нет

### Общие реакции: апатичный, чувствительный, стойкий, угрюмый, надеющийся, пессимист, причудливый, сноб
Ослабляют или усиливают все муд-события, сдвигают базовое настроение.

- **Код:** [`/datum/personality/apathetic`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/personality/reaction_generic.dm#L1), [`/datum/personality/sensitive`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/personality/reaction_generic.dm#L16), [`/datum/personality/snob`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/personality/reaction_generic.dm#L97)
- **Статус в ss13-rs:** ❌ нет

### Досуг и работа: игрок, бездельник, трудолюбивый, атлетичный, эрудит, необразованный, духовный, отдел, лояльный NT
Муд от азартных игр, отлынивания, работы, тренировок, книг, религии, своего отдела и корпорации.

- **Код:** [`/datum/personality/gambler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/personality/reaction_to_recreation.dm#L1), [`/datum/personality/slacking`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/personality/reaction_to_recreation.dm#L7), [`/datum/personality/industrious`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/personality/reaction_to_recreation.dm#L74), [`/datum/personality/department`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/personality/reaction_to_job.dm#L1), [`/datum/personality/nt`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/personality/reaction_to_nt.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Отношения к людям: интроверт, экстраверт, параноик, чёрствый, сострадательный, эмпатичный, мизантроп, отстранённый
Муд от толпы или одиночества, от помощи другим и чужих страданий.

- **Код:** [`/datum/personality/introvert`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/personality/reaction_to_groups.dm#L1), [`/datum/personality/extrovert`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/personality/reaction_to_groups.dm#L10), [`/datum/personality/callous`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/personality/reaction_to_others.dm#L1), [`/datum/personality/compassionate`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/personality/reaction_to_others.dm#L9)
- **Статус в ss13-rs:** ❌ нет

### Животные, искусство, страх, еда
Любитель животных/кошек/собак или их недруг; творческий и лишённый воображения (реакция на искусство); храбрый и трусливый; аскет, гурман, трезвенник, любитель выпить.

- **Код:** [`/datum/personality/animal_friend`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/personality/reaction_to_animals.dm#L1), [`/datum/personality/creative`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/personality/reaction_to_art.dm#L1), [`/datum/personality/brave`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/personality/reaction_to_fear.dm#L1), [`/datum/personality/gourmand`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/personality/reaction_to_food.dm#L9), [`/datum/personality/teetotal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/personality/reaction_to_food.dm#L17)
- **Статус в ss13-rs:** ❌ нет

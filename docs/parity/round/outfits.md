# Аутфиты
`/datum/outfit` — наборы одежды и снаряжения для профессий, антагонистов, ОБР и админ-спавна; экспорт/импорт и редактор для админов.

### Экипировка аутфита
`equip`: слоты (униформа, костюм, обувь, перчатки, голова, маска, глаза, уши, пояс, ID, руки, карманы, рюкзак), содержимое рюкзака и пояса, коробка выживания, баллон для internals, отпечатки; `pre_equip`/`post_equip`.

- **Код:** [`/datum/outfit`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/outfit.dm#L16), [`/datum/outfit/proc/equip()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/outfit.dm#L178), [`/datum/outfit/proc/pre_equip()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/outfit.dm#L143), [`/datum/outfit/proc/post_equip()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/outfit.dm#L158)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/outfit.rs`: слоты, `box`, `backpack_contents`, `belt_contents` (`fill_storage`), аксессуар, импланты; нет `can_equip`, `internals_slot`, отпечатков

### Аутфиты профессий
`/datum/outfit/job/*`: экипировка профессии с выбором рюкзака/сумки/вещмешка по предпочтениям, ID-картой трима, КПК, гарнитурой.

- **Код:** [`/datum/outfit/job`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/_job.dm#L334), [`/datum/outfit/job/post_equip()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/jobs/job_types/_job.dm#L394)
- **BandaStation:** [`lawyer.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/outfits/code/lawyer.dm#L1), [`blueshield.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/outfits/code/blueshield.dm#L1), [`magistrate.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/outfits/code/magistrate.dm#L1), [`nanotrasen_representative.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/outfits/code/nanotrasen_representative.dm#L1), [`explorer.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/outfits/code/explorer.dm#L1) — аутфиты профессий форка
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/outfit.rs`, `crates/ss13_game/src/jobs.rs`, `crates/ss13_game/src/modpc/pda.rs`: ID-карта трима и КПК аутфита с привязкой имени и должности (`post_equip`); рюкзак всегда серый (не по предпочтению и отделу)

### Органы в аутфитах (BandaStation)
Аутфит может вставлять органы (импланты, кибернетика) при экипировке; поле `organs` экспортируется и импортируется.

- **Код:** [`/datum/outfit/equip()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/outfits/code/outfit_organs.dm#L4), [`/datum/outfit/get_json_data()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/outfits/code/outfit_export.dm#L1)
- **BandaStation:** [`_outfits.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/outfits/_outfits.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Аутфиты фракций BandaStation
Наборы для админ-спавна и событий: ЦК (28), ТСФ (23), СССП (25), Синдикат.

- **Код:** [`centcom.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/outfits/code/centcom.dm#L1), [`tsf.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/outfits/code/tsf.dm#L1), [`ussp.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/outfits/code/ussp.dm#L1), [`syndicate.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/outfits/code/syndicate.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Экспорт, импорт и менеджер аутфитов
Сохранение аутфита в JSON и загрузка; админские менеджер и редактор аутфитов (TGUI).

- **Код:** [`/datum/outfit/proc/get_json_data()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/outfit.dm#L408), [`/datum/outfit/proc/load_from()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/outfit.dm#L476), [`/datum/outfit_manager`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/admin/outfit_manager.dm#L5); tgui: [`OutfitEditor.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/OutfitEditor.jsx)
- **BandaStation:** [`outfit.dm#L506`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/datums/outfit.dm#L506) — исправлен импорт содержимого пояса
- **Статус в ss13-rs:** ❌ нет

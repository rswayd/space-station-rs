# Медицина

Медицина оригинального сервера: система хирургических операций и инструменты, органы и кибернетика, части тела, раны, болезни и вирусология, кровь и кровотечение, медицинские предметы (стаки, анализаторы, дефибрилляторы, аптечки), машины медотсека, СЛР и оживление. Источники: `code/modules/surgery`, `code/datums/wounds`, `code/datums/diseases`, `code/modules/mob/living/blood.dm`, `code/game/objects/items/stacks/medical.dm`, `code/game/machinery/{sleepers,stasis,iv_drip,medical_kiosk,limbgrower,fat_sucker,defibrillator_mount}.dm`; форк — `modular_bandastation/medical`, `augmentation_preferences`, органы видов из `modular_bandastation/species`. В ss13-rs есть урон по шести частям тела, лечение стаками, марля, анализаторы, СЛР и выбор зоны (`crates/ss13_game/src/medical.rs`, `health.rs`, `health_scan.rs`, `zone.rs`), кровь и переливание с капельницей (`medical/blood.rs`), раны (`medical/wounds.rs`), хирургия на операционном столе (`medical/surgery.rs`) с операционным компьютером (`medical/operating.rs`), дефибрилляторы с оживлением (`medical/defib.rs`), органы (`medical/organs.rs`) и ампутированные конечности (`medical/bodyparts.rs`); нет болезней и машин кроме капельницы. Пандемия и лекарства — ../chemistry/, клонирование — ../genetics/.

| Файл | Что внутри |
| --- | --- |
| [surgery.md](surgery.md) | система операций, базовые шаги, шанс провала, инструменты, операционная, вскрытие |
| [surgery-operations.md](surgery-operations.md) | все группы операций, включая настройку голосовых связок форка |
| [organs.md](organs.md) | орган как объект, сердце, лёгкие, печень, желудок, глаза, уши, язык, мозг, аппендикс, внешние и видовые органы |
| [cybernetics.md](cybernetics.md) | импланты рук, груди, глаз и мозга, кибер-органы, автохирург, аугментации в настройках форка |
| [bodyparts.md](bodyparts.md) | урон частей тела, отключение, приживление, протезы, видовые части, обрубки |
| [wounds.md](wounds.md) | механика ран, переломы, порезы, проколы, ожоги, потеря конечности, шрамы |
| [diseases.md](diseases.md) | механика болезней, группы болезней, продвинутые болезни и симптомы, вирусология |
| [blood.md](blood.md) | объём крови, кровотечение, группы крови, переливание, брызги и следы |
| [medical-items.md](medical-items.md) | медстаки, марля, жгут, анализаторы, дефибриллятор, аптечки, мешки и каталки, медHUD |
| [medical-machines.md](medical-machines.md) | слипер, стазис, криокапсула, капельница, киоск, лимбгроуэр, липосактор, органный холодильник |
| [cpr-revival.md](cpr-revival.md) | СЛР, оживление, условия дефибрилляции, странный реагент |

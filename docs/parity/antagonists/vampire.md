# Вампир (BandaStation)
Порт вампира с Paradise (модпак `vampire`): пьёт кровь экипажа, копит её на способности и выбирает специализацию. Раундстарт-рулсет `Roundstart Vampire`.

### Антагонист «Вампир»
`/datum/antagonist/vampire`: запасы крови (используемая и общая), HUD крови и голода, уязвимость к святой воде, свету и огню; окно информации; телохранитель-вариант.

- **Код:** [`/datum/antagonist/vampire`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/datum_vampire.dm#L1), [`/datum/antagonist/vampire/bodyguard`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/datum_vampire.dm#L54), [`blood_hud.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/blood_hud.dm#L1); tgui: [`AntagInfoVampire.tsx`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/tgui/packages/tgui/interfaces/AntagInfoVampire.tsx)
- **BandaStation:** [`_vampire.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/_vampire.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/vampire/` (`server.rs`: `Vampire`, `add_vampire`, `refresh`; `client.rs`): запасы крови, приветствие форка, окно `AntagInfoVampire` (информация, специализации, выбор), HUD-иконка, счётчик доступной крови, кнопки способностей (`crates/ss13_game/src/actions.rs`, `actions_client.rs`), отчёт конца раунда с кровью; нет вампирской иконки голода, телохранителя, админ-команд и звука

### Укус и питьё крови
Укус жертвы в захвате выкачивает кровь; кровь мёртвых и неразумных даёт меньше; прирост открывает способности по порогам.

- **Код:** [`/datum/component/vampire_biter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vampire_bite.dm#L2)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/antag/vampire/server.rs` (`bite_clicks`, `can_bloodsuck`, `bloodsuck_tick`): боевой режим, голова, пустая рука; цикл `do_after` по 5 с, 25 ед. крови жертвы (`medical::blood::Blood`), `min(20, выпито) × 0.5` крови при живой жертве-члене экипажа, лимит 200 с жертвы, предупреждения, сытость; маска мешает (любой предмет в слоте маски)

### Цели и рулсет
Цели: набрать кровь, выбрать специализацию, обустроить логово; рулсет запрещает часть профессий и рас.

- **Код:** [`/datum/objective/vampire`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vampire_objectives.dm#L2), [`/datum/objective/vampire/blood`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vampire_objectives.dm#L5), [`/datum/objective/vampire/specialization`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vampire_objectives.dm#L22), [`/datum/objective/vampire/lair`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vampire_objectives.dm#L40), [`/datum/dynamic_ruleset/roundstart/vampire`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/dynamic_rulesets.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/antag/vampire/server.rs` (`forge_objectives`, `VampireObjective`) и `crates/ss13_game/src/antag/dynamic.rs` (`VAMPIRE_RULESET`): кровь 150–400, специализация / кража / защита, убийство, логово, побег / выживание; рулсет с весом 10, `min_pop` 3, капом `population / 24`, запретом священника и ГП; все расы здесь люди, так что проверка расы не нужна

### Базовые способности
Омоложение (снятие оглушений), гипнотический взгляд, логово, подъём вампиров; пассивы по количеству крови (ночное зрение, иммунитеты); регенерация в гробу.

- **Код:** [`vampire_powers.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vampire_powers/vampire_powers.dm#L1), [`vampire_spell_passives.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vampire_spell_passives.dm#L1), [`/datum/component/vampire_coffin_regeneration`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vampire_coffin_regeneration.dm#L1), [`/datum/component/vampire_ability`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/spell_handler.dm#L2)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/vampire/server.rs` (`UPGRADE_TIERS`, `cast_check`, `rejuvenate`, `glare`, `begin_lair`): пороги крови, стоимость с нуллификацией, кулдауны, омоложение (+ лечение 14 с с регенерацией), взгляд (2 заряда, отклонение по направлению), логово (наведение кликом через `click::ClickIntercept`, ритуал 10 с, гроб закрепляется); зрение выдаётся без эффекта (нет порога освещения клиента, термо/рентген-зрения), нет подъёма вампиров, регенерации в гробу (нет сна), заглушения и вспышки взгляда

### Специализация Umbrae
Тьма: покров тьмы, теневая ловушка, якорь души, тёмный проход, гашение света, теневой бой, вечная тьма.

- **Код:** [`/datum/vampire_subclass/umbrae`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vampire_subclasses.dm#L58), [`umbrae_powers.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vampire_powers/umbrae_powers.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/vampire/server.rs` (`toggle_cloak`, `handle_cloak`): покров тьмы (альфа 40 и ускорение при освещённости ≤ 0.2, иначе 200; гаснет в огне); остальные способности только в окне, не выдаются

### Специализация Hemomancer
Кровь: вампирские когти, кровавые щупальца, барьер, кровавая лужа, чувства хищника, извержение, ритуал несущего кровь.

- **Код:** [`/datum/vampire_subclass/hemomancer`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vampire_subclasses.dm#L76), [`hemomancer_powers.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vampire_powers/hemomancer_powers.dm#L1)
- **Статус в ss13-rs:** ❌ нет — выбирается в окне (`Subclass::Hemomancer`, полная сила), но ни одна способность не выдаётся

### Специализация Gargantua
Сила: кровавое усиление, сейсмический топот, подавляющая сила, кровавый рывок, демоническая хватка, таран, осквернённая дуэль.

- **Код:** [`/datum/vampire_subclass/gargantua`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vampire_subclasses.dm#L93), [`gargantua_powers.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vampire_powers/gargantua_powers.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/vampire/server.rs` (`blood_rush`): кровавый рывок (250 крови, 30 с, −0.5 дс/клетку на 10 с, снимает нокдаун), улучшенное лечение омоложением; нет кровавого усиления (нет физиологических множителей), топота, подавляющей силы, тарана, хватки и арены

### Специализация Dantalion и рабы
Подчинение (обращение в раба-трэлла), общение с рабами, усмирение, подпространственный обмен, приманка, сбор рабов, кровавая связь, массовая истерия; рабы — отдельный антаг-датум со связью.

- **Код:** [`/datum/vampire_subclass/dantalion`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vampire_subclasses.dm#L112), [`dantalion_powers.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vampire_powers/dantalion_powers.dm#L1), [`/datum/antagonist/vampire_thrall`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vamp_thrall.dm#L1), [`/datum/component/vampire_thrall_communication`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vampire_owner_abilities.dm#L97)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/vampire/server.rs` (`switch_places`): подпространственный обмен (250 крови, наведение на живое существо до 7 клеток); нет рабов, подчинения, общения, усмирения, приманки, сбора, связи и истерии

### Древний вампир
Админский/особый вариант с полным набором способностей и своим аутфитом.

- **Код:** [`/datum/vampire_subclass/ancient`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vampire_subclasses.dm#L133), [`/datum/outfit/ancient_vampire`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/ancient_vampire_outfit.dm#L2)
- **Статус в ss13-rs:** ❌ нет

### Святая вода и статус-эффекты
Святая вода жжёт и снимает способности; статус-эффекты вампира (жажда, ожоги на свету, ослабление в часовне и т.п.).

- **Код:** [`/datum/component/vampire_holywater`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vampire_holywater.dm#L2), [`vampire_status_effects.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/vampire/code/vampire_status_effects.dm#L1)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/antag/vampire/server.rs` (`vampire_life`, `check_sun`, `holy_water`, `vamp_burn`): свет звёзд на незатенённом космосе (`power::solar::is_sunlight_blocked`), часовня, святая вода в крови (нуллификация, потеря крови, ожоги, возгорание по циклам), огонь (5 крови за `Life()`), спад нуллификации; нет брызг святой воды, нуллродов, превращения в пепел (смерть вместо него), алертов статус-эффектов

# СЛР и оживление
Сердечно-лёгочная реанимация и пути возвращения мёртвых к жизни. Дефибриллятор — [medical-items.md](medical-items.md), операция оживления — [surgery-operations.md](surgery-operations.md), клонирование — ../genetics/.

### СЛР (CPR)
Пустая рука вне режима боя по лежащему в крите: 3 с, затем лечит до 7 oxyloss и повторяется каждые 0.8 с, пока пациент в крите; рот должен быть открыт.

- **Код:** [`proc/do_cpr()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/human/human.dm#L503)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/medical.rs` (`do_cpr`)

### Оживление (revive)
`revive` возвращает мёртвого к жизни (с исцелением по флагам или без), призрак возвращается в тело; карбону нужны мозг и сердце, тело не гнилое и не хаск, урон ниже порога.

- **Код:** [`proc/revive()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/living.dm#L887), [`carbon/revive()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L642), [`carbon/can_be_revived()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L712), [`carbon/heal_and_revive()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L654)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/defib.rs` (`revive`), `crates/ss13_game/src/ghost.rs` (`return_to_body`): мёртвый с здоровьем выше −100 оживает в состояние по здоровью, призрак возвращается; нет полного исцеления и `excess_healing`

### Условия дефибрилляции
Тело подходит для дефиба, если сердце и мозг целы, нет гниения, не прошло слишком много времени (`DEFIB_TIME_LIMIT`), нет DNR и душа может вернуться; иначе — понятное сообщение.

- **Код:** [`proc/can_defib()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L719), [`proc/can_defib_heart()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L747), [`proc/can_defib_brain()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L760), [`proc/can_defib_client()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L772)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/medical/defib.rs`: brute/burn ≥ 180, разложение сердца (~6 мин) и мозга по `STANDARD_ORGAN_DECAY` и температуре тела; нет самоубийства, хаска, DNR, отказа клиента

### Оживление реагентами
Странный реагент (strange reagent) оживляет труп с ограничением по урону, лечит часть урона.

- **Код:** [`carbon/do_strange_reagent_revival()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/mob/living/carbon/carbon.dm#L708)
- **Статус в ss13-rs:** ❌ нет

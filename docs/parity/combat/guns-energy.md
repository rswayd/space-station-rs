# Энергетическое оружие
Оружие на батареях: механика заряда и режимов, семейства. Снаряды — [ammo-projectiles.md](ammo-projectiles.md).

### Механика энергооружия
Выстрел тратит заряд ячейки (`e_cost`); оружие с несколькими `ammo_type` переключает режимы огня кликом по себе; заряжается в зарядниках, некоторые — самозаряжаются; индикатор заряда на спрайте; ЭМИ разряжает.

- **Код:** [`/obj/item/gun/energy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy.dm#L1), [`proc/select_fire()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy.dm#L250), [`/obj/item/ammo_casing/energy`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/ammunition/energy/_energy.dm#L1)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/guns.rs`: ячейка (полная при спавне), `e_cost` за выстрел, смена режима (Z), зарядка в зарядном устройстве (`crates/ss13_game/src/power/chargers.rs`); без оверлеев заряда

### Лазеры
Лазерная винтовка, пистолет, карабин, штурмовая, ретро, адский лазер, капитанская антиквариатная (самозаряд), скаттер, лазерная пушка, рентгеновский; лазертаг; в форке — X-02, TNG-3, EG-14 и Exploreverse для экспедиций.

- **Код:** [`/obj/item/gun/energy/laser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/laser.dm#L1), [`/obj/item/gun/energy/laser/captain`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/laser.dm#L142), [`/obj/item/gun/energy/laser/scatter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/laser.dm#L185), [`/obj/item/gun/energy/lasercannon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/laser.dm#L223), [`/obj/item/gun/energy/laser/xray`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/laser.dm#L239), [`/obj/item/gun/energy/laser/bluetag`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/laser.dm#L260)
- **BandaStation:** [`modular_bandastation/weapon/code/ranged/energy/laser.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/weapon/code/ranged/energy/laser.dm#L1) — лазеры форка (`eg_14.dm`, `awaymission_gun.dm`)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/guns.rs`: лазерные винтовки на общей механике энергооружия

### Энергопистолеты и e-gun
Энергетическое ружьё (disable/kill), мини e-gun, ГСБ-шный X-01 (три режима), ядерное с самозарядом, дрэгнет (сеть-телепорт).

- **Код:** [`/obj/item/gun/energy/e_gun`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/energy_gun.dm#L1), [`/obj/item/gun/energy/e_gun/hos`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/energy_gun.dm#L76), [`/obj/item/gun/energy/e_gun/nuclear`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/energy_gun.dm#L150), [`/obj/item/gun/energy/e_gun/dragnet`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/energy_gun.dm#L88)
- **Статус в ss13-rs:** ✅ реализовано — `crates/ss13_game/src/guns.rs`: e-gun (обезвреживание/летальный)

### Нелетальное: тазеры и дизейблеры
Тазер (электрод, оглушает), продвинутый тазер, дизейблер и SMG-дизейблер (урон стамине), гладкоствольный дизейблер с ручной зарядкой.

- **Код:** [`/obj/item/gun/energy/taser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/stun.dm#L1), [`/obj/item/gun/energy/e_gun/advtaser`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/stun.dm#L10), [`/obj/item/gun/energy/disabler`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/stun.dm#L30), [`/obj/item/gun/energy/disabler/smg`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/stun.dm#L45)
- **Статус в ss13-rs:** 🟡 частично — `crates/ss13_game/src/taser.rs`: дизейблер; тазер и гибридный тазер — электроды цепляются (`/datum/status_effect/tased`): 60 выносливости/с за счёт ячейки, пока стрелок держит оружие и цель в досягаемости и видимости, замедления `being_tased`/`tasing_someone`, блок выстрела, снятие кликом стрелка или resist (5 с); нет луча-провода, искр, джиттера, заикания, настроения

### Импульсное оружие
Импульсная винтовка, карабин, пистолет, «разрушитель» — ЦК-класса, разрушают стены.

- **Код:** [`/obj/item/gun/energy/pulse`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/pulse.dm#L1)
- **BandaStation:** [`modular_bandastation/weapon/code/ranged/energy/pulse.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/weapon/code/ranged/energy/pulse.dm#L1) — варианты с бойком лояльности
- **Статус в ss13-rs:** ❌ нет

### Перезаряжаемые и кинетические
Кинетический акселератор шахтёров (модули, сильнее в низком давлении), энергоарбалет синдиката (усыпляющие болты), «рыбак» (выключает свет и электронику).

- **Код:** [`/obj/item/gun/energy/recharge`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/recharge.dm#L2), [`/obj/item/gun/energy/recharge/kinetic_accelerator`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/kinetic_accelerator.dm#L4), [`/obj/item/gun/energy/recharge/ebow`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/recharge.dm#L103), [`/obj/item/gun/energy/recharge/fisher`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/recharge.dm#L151)
- **Статус в ss13-rs:** 🟡 `ss13_game::mining::rocks`: самозарядка (`recharge_time` 1,6 с ПКА / 2 с арбалет, без носителя-моба разряжается, кроме `holds_charge`), кинетический заряд бурит минеральные стены и слабеет в 4 раза при давлении выше 50 кПа; нет модулей ПКА, болтов арбалета и «рыбака»

### Специальное энергооружие
Ионная винтовка (ЭМИ), флорапушка (мутации растений), температурная (нагрев/охлаждение), плазменный резак (копает камень), портальная пушка, гравитационная, тесла-пушка, метеоритная, марксман-револьвер с рикошетом от монет, фотонная, миниган, «горизонт событий», дуэльные пистолеты, наплечные/вмонтированные лазеры.

- **Код:** [`/obj/item/gun/energy/ionrifle`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/special.dm#L1), [`/obj/item/gun/energy/floragun`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/special.dm#L39), [`/obj/item/gun/energy/temperature`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/special.dm#L321), [`/obj/item/gun/energy/plasmacutter`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/special.dm#L87), [`/obj/item/gun/energy/wormhole_projector`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/special.dm#L206), [`/obj/item/gun/energy/gravity_gun`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/special.dm#L342), [`/obj/item/gun/energy/tesla_cannon`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/special.dm#L376), [`/obj/item/gun/energy/marksman_revolver`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/special.dm#L434), [`/obj/item/gun/energy/minigun`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/laser_gatling.dm#L86), [`/obj/item/gun/energy/dueling`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/dueling.dm#L148)
- **Статус в ss13-rs:** ❌ нет

### Ручные «мушкеты» и термальные пистолеты
Лазерный мушкет с ручной накачкой, термальные пистолеты (инферно/крио) — заряжаются от тепла владельца.

- **Код:** [`/obj/item/gun/energy/laser/musket`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/crank_guns.dm#L1), [`/obj/item/gun/energy/laser/thermal`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/code/modules/projectiles/guns/energy/crank_guns.dm#L105)
- **Статус в ss13-rs:** ❌ нет

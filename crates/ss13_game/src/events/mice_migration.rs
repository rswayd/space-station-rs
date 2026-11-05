//! Mice Migration (`code/modules/events/mice_migration.dm`): 5-15 mice on exposed station
//! wires (`SSminor_mapping.trigger_migration`, [`crate::mobs::spawn::trigger_migration`]).
// shortcut: no Rat King (`vermin_chances` brings only mice here).

use bevy::prelude::*;

use super::controller::{EventRng, EventState, RoundEvent};

pub const NAME: &str = "Mice Migration";
/// `minimum_mice`, `maximum_mice`.
const MINIMUM_MICE: i64 = 5;
const MAXIMUM_MICE: i64 = 15;
const SQUEAK: &str = "sound/mobs/non-humanoids/mouse/mousesqueek.ogg";

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(MiceMigration)
}

struct MiceMigration;

impl RoundEvent for MiceMigration {
    fn announce(&mut self, world: &mut World, _: &mut EventState, _: bool) {
        let mut rng = world.resource_mut::<EventRng>();
        let cause = *rng
            .pick(&[
                "космозимы",
                "сокращения бюджета",
                "Рагнарёка",
                "того, что космос холодный",
                "[ДАННЫЕ УДАЛЕНЫ]",
                "климатических изменений",
                "неудачи",
            ])
            .expect("non-empty");
        let at_most = format!("не более чем {MAXIMUM_MICE}");
        let plural = rng
            .pick(&["кучка", "орда", "стая", "рой", at_most.as_str()])
            .expect("non-empty")
            .to_string();
        let name = *rng
            .pick(&[
                "грызунов",
                "мышей",
                "проводоедов",
                "[ДАННЫЕ УДАЛЕНЫ]",
                "поглощающих энергию паразитов",
            ])
            .expect("non-empty");
        let movement = *rng
            .pick(&["мигрировала", "зароилась", "убежала в панике", "проникла"])
            .expect("non-empty");
        let location = *rng
            .pick(&[
                "технические тоннели",
                "технические помещения",
                "[ДАННЫЕ УДАЛЕНЫ]",
                "кишащее сочными проводами место",
            ])
            .expect("non-empty");
        let text = format!("Из-за {cause}, {plural} {name} {movement} в {location}.");
        super::priority_announce_sound(world, "Оповещение о миграции", &text, SQUEAK);
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let mut rng = world.resource_mut::<EventRng>();
        let count = rng.0.range(MINIMUM_MICE, MAXIMUM_MICE) as usize;
        let seed = rng.0.next_u64();
        crate::mobs::spawn::trigger_migration(world, count, seed);
    }
}

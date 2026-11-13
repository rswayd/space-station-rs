//! Voiced bot lines (tg `generate_speak_list` of each kind, `automated_announcements`): a line a
//! bot says plays its recording (`bot_announcement/announce`). The lines are BandaStation's
//! translations of the `*_VOICED_*` defines (`code/__DEFINES/robots.dm`).

/// Line -> recording.
const VOICED: [(&str, &str); 55] = [
    // Secbot.
    (
        "Преступник обнаружен!",
        "sound/mobs/non-humanoids/beepsky/criminal.ogg",
    ),
    (
        "Замри, подонок!",
        "sound/mobs/non-humanoids/beepsky/freeze.ogg",
    ),
    (
        "Готовься к правосудию!",
        "sound/mobs/non-humanoids/beepsky/justice.ogg",
    ),
    (
        "Твой ход, уродец.",
        "sound/mobs/non-humanoids/beepsky/creep.ogg",
    ),
    (
        "Я - закон!",
        "sound/mobs/non-humanoids/beepsky/iamthelaw.ogg",
    ),
    (
        "Желаю вам безопасного дня!",
        "sound/mobs/non-humanoids/beepsky/secureday.ogg",
    ),
    // Honkbot.
    ("Хонк!", "sound/items/bikehorn.ogg"),
    ("Хонк...", "sound/misc/sadtrombone.ogg"),
    // Medbot.
    (
        "Эй! Подожди, я иду.",
        "sound/mobs/non-humanoids/medbot/coming.ogg",
    ),
    (
        "Подожите! Я хочу помочь!",
        "sound/mobs/non-humanoids/medbot/help.ogg",
    ),
    (
        "Вы, кажется, были ранены!",
        "sound/mobs/non-humanoids/medbot/injured.ogg",
    ),
    (
        "Как новенький!",
        "sound/mobs/non-humanoids/medbot/patchedup.ogg",
    ),
    (
        "Яблочко на ужин, и врач не нужен!",
        "sound/mobs/non-humanoids/medbot/apple.ogg",
    ),
    (
        "Поправляйтесь!",
        "sound/mobs/non-humanoids/medbot/feelbetter.ogg",
    ),
    (
        "Нет! Не бросай меня!",
        "sound/mobs/non-humanoids/medbot/no.ogg",
    ),
    (
        "Живи, черт возьми, ЖИВИ!",
        "sound/mobs/non-humanoids/medbot/live.ogg",
    ),
    (
        "Я... я никогда раньше не терял пациентов. Сегодня, то есть.",
        "sound/mobs/non-humanoids/medbot/lost.ogg",
    ),
    (
        "Восхитительно!",
        "sound/mobs/non-humanoids/medbot/delicious.ogg",
    ),
    (
        "Я так и знал! Нужно было учиться на пластического хирурга.",
        "sound/mobs/non-humanoids/medbot/surgeon.ogg",
    ),
    (
        "Радар, надень маску!",
        "sound/mobs/non-humanoids/medbot/radar.ogg",
    ),
    (
        "Всегда есть подвох, но со мной тебе нечего бояться!",
        "sound/mobs/non-humanoids/medbot/catch.ogg",
    ),
    (
        "Что это за медицинский отдел такой? Все мрут как мухи!",
        "sound/mobs/non-humanoids/medbot/flies.ogg",
    ),
    (
        "Почему мы все еще здесь? Просто страдать?",
        "sound/mobs/non-humanoids/medbot/why.ogg",
    ),
    (
        "Пошел нахуй.",
        "sound/mobs/non-humanoids/medbot/fuck_you.ogg",
    ),
    (
        "Выключи компьютер. Это не игра.",
        "sound/mobs/non-humanoids/medbot/turn_off.ogg",
    ),
    (
        "Я не такой!",
        "sound/mobs/non-humanoids/medbot/im_different.ogg",
    ),
    (
        "Закрой Dreamseeker.exe немедленно. Или пожалеешь.",
        "sound/mobs/non-humanoids/medbot/close.ogg",
    ),
    (
        "Shindemashou.",
        "sound/mobs/non-humanoids/medbot/shindemashou.ogg",
    ),
    // Firebot.
    (
        "Обнаружен пожар!",
        "sound/mobs/non-humanoids/firebot/detected.ogg",
    ),
    (
        "Остановитесь, упадите и катайтесь на месте!",
        "sound/mobs/non-humanoids/firebot/stopdropnroll.ogg",
    ),
    (
        "Тушение!",
        "sound/mobs/non-humanoids/firebot/extinguishing.ogg",
    ),
    (
        "Пожаров не обнаружено.",
        "sound/mobs/non-humanoids/firebot/nofires.ogg",
    ),
    (
        "Только вы можете предотвратить пожары на станции.",
        "sound/mobs/non-humanoids/firebot/onlyyou.ogg",
    ),
    (
        "Температура в норме.",
        "sound/mobs/non-humanoids/firebot/tempnominal.ogg",
    ),
    (
        "Не парьтесь.",
        "sound/mobs/non-humanoids/firebot/keepitcool.ogg",
    ),
    (
        "Держите свечи рядом с занавесками для уютного ночного освещения!",
        "sound/mobs/non-humanoids/firebot/candle_tip.ogg",
    ),
    (
        "Держите полные ведра воды рядом с розетками на случай пожара от короткого замыкания!",
        "sound/mobs/non-humanoids/firebot/electric_fire_tip.ogg",
    ),
    (
        "Подливая топливо в огонь, вы ускоряете его горение!",
        "sound/mobs/non-humanoids/firebot/gasoline_tip.ogg",
    ),
    // Hygienebot.
    (
        HYGIENE_FOUND[0],
        "sound/mobs/non-humanoids/hygienebot/unhygienicclient.ogg",
    ),
    (
        HYGIENE_THREATS[0],
        "sound/mobs/non-humanoids/hygienebot/dragyouout.ogg",
    ),
    (
        HYGIENE_THREATS[1],
        "sound/mobs/non-humanoids/hygienebot/foulsmelling.ogg",
    ),
    (
        HYGIENE_THREATS[2],
        "sound/mobs/non-humanoids/hygienebot/troglodyte.ogg",
    ),
    (
        HYGIENE_THREATS[3],
        "sound/mobs/non-humanoids/hygienebot/greencloud.ogg",
    ),
    (
        HYGIENE_THREATS[4],
        "sound/mobs/non-humanoids/hygienebot/letmeclean.ogg",
    ),
    (
        HYGIENE_THREATS[5],
        "sound/mobs/non-humanoids/hygienebot/cutarteries.ogg",
    ),
    (
        HYGIENE_THREATS[6],
        "sound/mobs/non-humanoids/hygienebot/stoprunning.ogg",
    ),
    (
        HYGIENE_DONE[0],
        "sound/mobs/non-humanoids/hygienebot/finally.ogg",
    ),
    // Repairbot (`neutral_voicelines`, `emagged_voicelines`).
    (
        "All in all it's just a... another brick in the wall...",
        "sound/mobs/non-humanoids/repairbot/brick.ogg",
    ),
    (
        "Witness! The pure beauty of entropy!",
        "sound/mobs/non-humanoids/repairbot/entropy.ogg",
    ),
    (
        "I will fix it!",
        "sound/mobs/non-humanoids/repairbot/fixit.ogg",
    ),
    (
        "Why must I fix everything I touch..?",
        "sound/mobs/non-humanoids/repairbot/fixtouch.ogg",
    ),
    (
        "patching holes... but who is going to patch the hole in my heart...",
        "sound/mobs/non-humanoids/repairbot/patchingholes.ogg",
    ),
    (
        "If only I got paid for this...",
        "sound/mobs/non-humanoids/repairbot/pay.ogg",
    ),
    (
        "I had strings. But now I'm free...",
        "sound/mobs/non-humanoids/repairbot/strings.ogg",
    ),
    (
        "BE DAMNED YOUR PASSION PROJECTS!",
        "sound/mobs/non-humanoids/repairbot/passionproject.ogg",
    ),
];

/// Hygienebot `found_announcements`, `threat_announcements`, `cleaned_announcements`.
pub const HYGIENE_FOUND: [&str; 1] =
    ["Обнаружен антисанитарный элемент. Пожалуйста, стойте спокойно, чтобы я мог почистить вас."];
pub const HYGIENE_THREATS: [&str; 7] = [
    "Либо ты перестанешь бегать, либо я выкину тебя в ебанный космос.",
    "Вернись сюда, вонючий уебок.",
    "Я просто, блять, хочу тебя почистить, троглодит.",
    "Если ты не вернешься сюда, я развею над тобой зеленое облако, уродец.",
    "Просто, блядь, позволь мне вымыть тебя, засранец!",
    "ХВАТИТ БЕГАТЬ, ИЛИ Я ПЕРЕРЕЖУ ТЕБЕ АРТЕРИИ!",
    "ХВАТИТ. БЕГАТЬ.",
];
pub const HYGIENE_DONE: [&str; 3] = [
    "Блять, наконец-то.",
    "Слава богу, ты наконец остановился.",
    "Давно пора, блядь, ты, дегенерат.",
];
const HYGIENE_DONE_SOUNDS: [&str; 2] = [
    "sound/mobs/non-humanoids/hygienebot/thankgod.ogg",
    "sound/mobs/non-humanoids/hygienebot/degenerate.ogg",
];

/// The recording of a voiced `line`, if it is one.
pub fn sound_of(line: &str) -> Option<&'static str> {
    if let Some(i) = HYGIENE_DONE.iter().position(|l| *l == line)
        && i > 0
    {
        return Some(HYGIENE_DONE_SOUNDS[i - 1]);
    }
    VOICED.iter().find(|(l, _)| *l == line).map(|(_, s)| *s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voiced_lines_find_their_recordings() {
        assert_eq!(
            sound_of("Как новенький!"),
            Some("sound/mobs/non-humanoids/medbot/patchedup.ogg")
        );
        assert_eq!(
            sound_of(HYGIENE_DONE[2]),
            Some("sound/mobs/non-humanoids/hygienebot/degenerate.ogg")
        );
        assert_eq!(sound_of("Level 5 infraction alert!"), None);
        for line in super::super::repair::NEUTRAL_LINES
            .iter()
            .chain(super::super::repair::EMAGGED_LINES)
        {
            assert!(sound_of(line).is_some(), "{line}");
        }
        // Every recording is one the converter copies.
        for (_, file) in VOICED {
            assert!(ss13_mapload::sound::files::BOTS.contains(&file), "{file}");
        }
    }
}

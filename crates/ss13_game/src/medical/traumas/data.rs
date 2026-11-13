//! The trauma table: each kind's definition and tg's random trauma pool.

use super::*;

impl Kind {
    pub fn def(self) -> Def {
        use Span::{Danger, Notice, Warning};
        let mild = |path, name, scan_desc, gain, lose| Def {
            path,
            name,
            scan_desc,
            gain,
            lose,
            category: Category::Mild,
            resilience: Resilience::Basic,
        };
        let severe = |path, name, scan_desc, gain, lose| Def {
            path,
            name,
            scan_desc,
            gain,
            lose,
            category: Category::Severe,
            resilience: Resilience::Surgery,
        };
        let special = |path, name, scan_desc, gain, lose| Def {
            path,
            name,
            scan_desc,
            gain,
            lose,
            category: Category::Special,
            resilience: Resilience::Basic,
        };
        let magic = |path, name, scan_desc, gain, lose| Def {
            path,
            name,
            scan_desc,
            gain,
            lose,
            category: Category::Magic,
            resilience: Resilience::Lobotomy,
        };
        match self {
            Self::Stuttering => mild(
                "/datum/brain_trauma/mild/stuttering",
                "Заикание",
                "пониженная координация речи",
                (Warning, "Говорить четко становится все труднее."),
                (Notice, "Вы чувствуете, что контролируете свою речь."),
            ),
            Self::Dumbness => mild(
                "/datum/brain_trauma/mild/dumbness",
                "Отупение",
                "снижение мозговой активности",
                (Warning, "Вы чувствуете себя глупее."),
                (Notice, "Вы снова чувствуете себя умным."),
            ),
            Self::SpeechImpediment => mild(
                "/datum/brain_trauma/mild/speech_impediment",
                "Дефект речи",
                "коммуникативное расстройство",
                (
                    Danger,
                    "Кажется, вы не можете сформулировать ни одной связной мысли!",
                ),
                (Danger, "Ваш разум становится более ясным."),
            ),
            Self::Concussion => mild(
                "/datum/brain_trauma/mild/concussion",
                "Сотрясение",
                "сотрясение",
                (Warning, "У вас болит голова!"),
                // tg's line carries a stray Latin "T".
                (Notice, "Давление в вашей голове начинает ослабевать."),
            ),
            Self::Healthy => mild(
                "/datum/brain_trauma/mild/healthy",
                "Анозогнозия",
                "дефицит самосознания",
                (Notice, "Вы чувствуете себя прекрасно!"),
                (Warning, "Вы больше не чувствуете себя совершенно здоровым."),
            ),
            Self::MuscleWeakness => mild(
                "/datum/brain_trauma/mild/muscle_weakness",
                "Мышечная слабость",
                "слабый сигнал двигательного нерва",
                (Warning, "Ваши мышцы ощущаются странно слабыми."),
                (Notice, "Вы снова чувствуете контроль над своими мышцами."),
            ),
            Self::NervousCough => mild(
                "/datum/brain_trauma/mild/nervous_cough",
                "Нервный кашель",
                "нервный кашель",
                (Warning, "У вас постоянно чешется горло..."),
                (Notice, "Ваше горло перестает чесаться."),
            ),
            Self::Possessive => mild(
                "/datum/brain_trauma/mild/possessive",
                "Собственничество",
                "собственничество",
                (Warning, "Вы начинаете беспокоиться о своих вещах."),
                (Notice, "Вы меньше беспокоитесь о своих вещах."),
            ),
            Self::Mute => severe(
                "/datum/brain_trauma/severe/mute",
                "Немота",
                "обширное повреждение речевого центра мозга",
                (Warning, "Вы разучились говорить!"),
                (Notice, "Вы вдруг вспоминаете, как говорить."),
            ),
            // The lines name the paralysed part ([`gain_text`]).
            Self::Paralysis(_) => severe(
                "/datum/brain_trauma/severe/paralysis",
                "Паралич",
                "церебральный паралич",
                (Warning, ""),
                (Notice, ""),
            ),
            Self::Pacifism => severe(
                "/datum/brain_trauma/severe/pacifism",
                "Травматическое ненасилие",
                "синдром пацифизма",
                (Notice, "Вы чувствуете себя странно умиротворенным."),
                (
                    Notice,
                    "Вы больше не чувствуете себя обязанным не причинять вреда.",
                ),
            ),
            Self::ColorBlindness => mild(
                "/datum/brain_trauma/mild/color_blindness",
                "Ахроматопсия",
                "цветовая слепота",
                (Warning, "Кажется, что мир вокруг вас теряет свои краски."),
                (Notice, "Мир снова кажется ярким и красочным."),
            ),
            Self::Blindness => severe(
                "/datum/brain_trauma/severe/blindness",
                "Корковая слепота",
                "обширное повреждение затылочной доли мозга",
                (Warning, "Вы не можете видеть!"),
                (Notice, "Ваше зрение возвращается."),
            ),
            Self::Aphasia => severe(
                "/datum/brain_trauma/severe/aphasia",
                "Афазия",
                "обширное повреждение языкового центра мозга",
                (Warning, "У вас проблемы с формированием слов в голове..."),
                (Notice, "Вы вдруг вспоминаете, как работают языки."),
            ),
            Self::Hallucinations => mild(
                "/datum/brain_trauma/mild/hallucinations",
                "Галлюцинации",
                "шизофрения",
                (
                    Warning,
                    "Вы чувствуете, что ваша связь с реальностью ослабевает...",
                ),
                (Notice, "Вы чувствуете себя более приземленным."),
            ),
            // The lines name the phobia ([`gain_text`]).
            Self::Phobia(_) => mild(
                "/datum/brain_trauma/mild/phobia",
                "Фобия",
                "фобия",
                (Warning, ""),
                (Notice, ""),
            ),
            Self::Narcolepsy { permanent } => Def {
                path: if permanent {
                    "/datum/brain_trauma/severe/narcolepsy/permanent"
                } else {
                    "/datum/brain_trauma/severe/narcolepsy"
                },
                scan_desc: if permanent {
                    "хроническая нарколепсия"
                } else {
                    "травматическая нарколепсия"
                },
                ..severe(
                    "",
                    "Нарколепсия",
                    "",
                    (Warning, "У вас постоянное чувство сонливости..."),
                    (
                        Notice,
                        "Вы снова чувствуете себя бодрым и осознающим происходящее.",
                    ),
                )
            },
            Self::Monophobia => severe(
                "/datum/brain_trauma/severe/monophobia",
                "Монофобия",
                "монофобия",
                (Warning, "Вы чувствуете себя по-настоящему одиноким..."),
                (
                    Notice,
                    "Вам кажется, что вы могли бы чувствовать себя в безопасности в одиночестве.",
                ),
            ),
            Self::Dyslexia => severe(
                "/datum/brain_trauma/severe/dyslexia",
                "Дислексия",
                "дислексия",
                (Warning, "У вас проблемы с чтением или письмом..."),
                (Notice, "Вы вдруг вспоминаете, как читать и писать."),
            ),
            Self::SplitPersonality => severe(
                "/datum/brain_trauma/severe/split_personality",
                "Split Personality",
                "complete lobe separation",
                (Warning, "You feel like your mind was split in two."),
                (Notice, "You feel alone again."),
            ),
            Self::ImaginaryFriend => Def {
                category: Category::Special,
                resilience: Resilience::Basic,
                ..mild(
                    "/datum/brain_trauma/special/imaginary_friend",
                    "Воображаемый друг",
                    "частичная шизофрения",
                    (
                        Notice,
                        "По какой-то причине вы чувствуете себя в хорошей компании.",
                    ),
                    (Warning, "Вы снова чувствуете себя одиноким."),
                )
            },
            // The lines name the phrase ([`gain_hypnosis`], [`lose_text`]).
            Self::Hypnosis => Def {
                category: Category::Special,
                ..severe(
                    "/datum/brain_trauma/hypnosis",
                    "Гипноз",
                    "повторяющийся мыслительный паттерн",
                    (Notice, ""),
                    (Notice, ""),
                )
            },
            Self::Obsessed => Def {
                category: Category::Special,
                resilience: Resilience::Lobotomy,
                ..mild(
                    "/datum/brain_trauma/special/obsessed",
                    "Психотическая шизофрения",
                    "психотические шизофренические бредовые идеи",
                    (
                        Warning,
                        "Вы слышите отвратительный, скрипучий голос у себя в голове. Он требует от вас выполнения одного небольшого задания...",
                    ),
                    (Warning, "Голоса в вашей голове замолкают.."),
                )
            },
            Self::Godwoken => special(
                "/datum/brain_trauma/special/godwoken",
                "Синдром пробужденного Бога",
                "божественная иллюзия",
                (Notice, "Вы чувствуете высшую силу внутри своего разума..."),
                (
                    Warning,
                    "Божественное присутствие покидает вашу голову, потеряв интерес.",
                ),
            ),
            Self::BluespaceProphet => special(
                "/datum/brain_trauma/special/bluespace_prophet",
                "Блюспейс пророчество",
                "bluespace attunement",
                (
                    Notice,
                    "Вы чувствуете, как блюспейс пульсирует вокруг тебя...",
                ),
                (Warning, "Слабая пульсация блюспейса сменяется тишиной."),
            ),
            Self::QuantumAlignment => special(
                "/datum/brain_trauma/special/quantum_alignment",
                "Квантовое выравнивание",
                "квантовое выравнивание",
                (
                    Notice,
                    "Вы чувствуете слабую связь со всем, что вас окружает...",
                ),
                (
                    Warning,
                    "Вы больше не чувствуете связи со своим окружением.",
                ),
            ),
            Self::Tenacity => special(
                "/datum/brain_trauma/special/tenacity",
                "Упорство",
                "травматическая невропатия",
                (Warning, "Вы внезапно перестаете чувствовать боль."),
                (Warning, "Вы понимаете, что снова можете чувствовать боль."),
            ),
            Self::DeathWhispers => special(
                "/datum/brain_trauma/special/death_whispers",
                "Функциональный церебральный некроз",
                "хронический функциональный некроз",
                (Warning, "Вы чувствуете себя мертвым внутри."),
                (Notice, "Вы снова чувствуете себя живым."),
            ),
            Self::ExistentialCrisis => special(
                "/datum/brain_trauma/special/existential_crisis",
                "Экзистенциальный кризис",
                "экзистенциальный кризис",
                (Warning, "Вы чувствуете себя менее реальным."),
                (Notice, "Вы снова чувствуете себя более значимым."),
            ),
            Self::Beepsky => special(
                "/datum/brain_trauma/special/beepsky",
                "Преступник",
                "преступный склад ума",
                (Warning, "Правосудие придет за вами."),
                (
                    Notice,
                    "Вы были освобождены от ответственности за ваши преступления.",
                ),
            ),
            Self::Ptsd => Def {
                resilience: Resilience::Absolute,
                ..special(
                    "/datum/brain_trauma/special/ptsd",
                    "Боевое ПТСР",
                    "ПТСР",
                    (
                        Warning,
                        "Вы возвращаетесь в хаос прошлого! Взрывы! Стрельба! Эмоции покинули строй!",
                    ),
                    (
                        Notice,
                        "Вы чувствуете, как исчезают воспоминания о прошлом, как возвращаются ваши эмоции и проясняется разум.",
                    ),
                )
            },
            Self::Lumiphobia => magic(
                "/datum/brain_trauma/magic/lumiphobia",
                "Люмифобия",
                "светочувствительность",
                (Warning, "Вы чувствуете тягу к темноте."),
                (Notice, "Свет больше не беспокоит вас."),
            ),
            Self::Poltergeist => magic(
                "/datum/brain_trauma/magic/poltergeist",
                "Полтергейст",
                "паранормальная активность",
                (
                    Warning,
                    "Вы чувствуете ненавистное присутствие рядом с собой.",
                ),
                (
                    Notice,
                    "Вы чувствуете, как ненавистное присутствие исчезает.",
                ),
            ),
            Self::Antimagic => magic(
                "/datum/brain_trauma/magic/antimagic",
                "Атаумазия",
                "таумическая пустота",
                (Notice, "Вы понимаете, что магия не может быть реальной."),
                (Notice, "Вы понимаете, что магия может быть реальной."),
            ),
            Self::Stalker { cosmic } => Def {
                path: if cosmic {
                    "/datum/brain_trauma/magic/stalker/cosmic"
                } else {
                    "/datum/brain_trauma/magic/stalker"
                },
                ..magic(
                    "",
                    "Преследующий призрак",
                    "экстрасенсорная паранойя",
                    (
                        Warning,
                        "Вы чувствуете себя так, словно что-то хочет убить вас...",
                    ),
                    (
                        Notice,
                        "Вы больше не чувствуете, что кто-то смотрит вам в спину.",
                    ),
                )
            },
            Self::Voided { stable } => Def {
                path: if stable {
                    "/datum/brain_trauma/voided/stable"
                } else {
                    "/datum/brain_trauma/voided"
                },
                scan_desc: if stable {
                    "stable cosmic neural pattern"
                } else {
                    "cosmic neural pattern"
                },
                ..magic("", "Voided", "", (Notice, ""), (Notice, ""))
            },
            Self::Kleptomaniac => severe(
                "/datum/brain_trauma/severe/kleptomaniac",
                "Клептомания",
                "клептомания",
                (
                    Warning,
                    "Вы чувствуете внезапное желание взять это. Конечно, никто не заметит.",
                ),
                (Notice, "Вы больше не чувствуете желания брать чужие вещи."),
            ),
        }
    }

    /// tg `istype(trauma, type)` between two of the same tree: paralysis types are one type.
    pub(super) fn same_type(self, other: Self) -> bool {
        std::mem::discriminant(&self) == std::mem::discriminant(&other)
    }

    /// `scan_desc` (a phobia names what it fears).
    pub(super) fn scan_desc(self) -> String {
        match self {
            Self::Phobia(i) => format!("фобия {}", phobia_label(i)),
            _ => self.def().scan_desc.to_owned(),
        }
    }

    pub(super) fn gain_text(self) -> (Span, String) {
        match self {
            Self::Paralysis(p) => (
                Span::Warning,
                format!("Вы больше не чувствуете {}!", p.subject()),
            ),
            Self::Phobia(i) => (
                Span::Warning,
                format!(
                    "Вы начинаете находить {} очень нервирующим...",
                    phobia_label(i)
                ),
            ),
            _ => (self.def().gain.0, self.def().gain.1.to_owned()),
        }
    }

    pub(super) fn lose_text(self) -> (Span, String) {
        match self {
            Self::Paralysis(p) => (
                Span::Notice,
                format!("Вы снова чувствуете {}!", p.subject()),
            ),
            Self::Phobia(i) => (
                Span::Notice,
                format!("Вы больше не боитесь {}.", phobia_label(i)),
            ),
            _ => (self.def().lose.0, self.def().lose.1.to_owned()),
        }
    }
}

/// The Russian `phobia_type` of phobia `i`.
pub(super) fn phobia_label(i: usize) -> &'static str {
    super::super::phobia_data::PHOBIAS
        .get(i)
        .map_or("", |p| p.label)
}

/// tg's randomly gainable trauma types (`subtypesof(category)` with `random_gain` and
/// `can_gain`, the abstract roots out): type, the ported kind (a paralysis of a placeholder
/// kind), category, `initial(resilience)`. Random gains pick among all of them, so each ported
/// trauma comes as often as in tg.
// shortcut: a pick of an unported type gives no trauma; add each type's kind as it is ported.
pub const TG_POOL: [(&str, Option<Kind>, Category, Resilience); 41] = {
    use Category::{Magic, Mild, Severe, Special};
    use Resilience::{Basic, Lobotomy, Surgery};
    [
        (
            "/datum/brain_trauma/mild/color_blindness",
            Some(Kind::ColorBlindness),
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/mild/concussion",
            Some(Kind::Concussion),
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/mild/dumbness",
            Some(Kind::Dumbness),
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/mild/expressive_aphasia",
            None,
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/mild/hallucinations",
            Some(Kind::Hallucinations),
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/mild/healthy",
            Some(Kind::Healthy),
            Mild,
            Basic,
        ),
        ("/datum/brain_trauma/mild/mind_echo", None, Mild, Basic),
        ("/datum/brain_trauma/mild/muscle_spasms", None, Mild, Basic),
        (
            "/datum/brain_trauma/mild/muscle_weakness",
            Some(Kind::MuscleWeakness),
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/mild/nervous_cough",
            Some(Kind::NervousCough),
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/mild/phobia",
            Some(Kind::Phobia(0)),
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/mild/possessive",
            Some(Kind::Possessive),
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/mild/speech_impediment",
            Some(Kind::SpeechImpediment),
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/mild/stuttering",
            Some(Kind::Stuttering),
            Mild,
            Basic,
        ),
        (
            "/datum/brain_trauma/severe/aphasia",
            Some(Kind::Aphasia),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/blindness",
            Some(Kind::Blindness),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/discoordination",
            None,
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/dyslexia",
            Some(Kind::Dyslexia),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/hypnotic_stupor",
            None,
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/kleptomaniac",
            Some(Kind::Kleptomaniac),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/monophobia",
            Some(Kind::Monophobia),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/mute",
            Some(Kind::Mute),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/narcolepsy",
            Some(Kind::Narcolepsy { permanent: false }),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/narcolepsy/permanent",
            Some(Kind::Narcolepsy { permanent: true }),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/pacifism",
            Some(Kind::Pacifism),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/paralysis",
            Some(Kind::Paralysis(Paralysis::Full)),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/severe/split_personality",
            Some(Kind::SplitPersonality),
            Severe,
            Surgery,
        ),
        (
            "/datum/brain_trauma/special/bluespace_prophet",
            Some(Kind::BluespaceProphet),
            Special,
            Basic,
        ),
        (
            "/datum/brain_trauma/special/death_whispers",
            Some(Kind::DeathWhispers),
            Special,
            Basic,
        ),
        (
            "/datum/brain_trauma/special/existential_crisis",
            Some(Kind::ExistentialCrisis),
            Special,
            Basic,
        ),
        (
            "/datum/brain_trauma/special/godwoken",
            Some(Kind::Godwoken),
            Special,
            Basic,
        ),
        (
            "/datum/brain_trauma/special/imaginary_friend",
            Some(Kind::ImaginaryFriend),
            Special,
            Basic,
        ),
        (
            "/datum/brain_trauma/special/primal_instincts",
            None,
            Special,
            Surgery,
        ),
        (
            "/datum/brain_trauma/special/psychotic_brawling",
            None,
            Special,
            Basic,
        ),
        (
            "/datum/brain_trauma/special/psychotic_brawling/bath_salts",
            None,
            Special,
            Basic,
        ),
        (
            "/datum/brain_trauma/special/quantum_alignment",
            Some(Kind::QuantumAlignment),
            Special,
            Basic,
        ),
        (
            "/datum/brain_trauma/special/tenacity",
            Some(Kind::Tenacity),
            Special,
            Basic,
        ),
        (
            "/datum/brain_trauma/magic/antimagic",
            Some(Kind::Antimagic),
            Magic,
            Lobotomy,
        ),
        (
            "/datum/brain_trauma/magic/lumiphobia",
            Some(Kind::Lumiphobia),
            Magic,
            Lobotomy,
        ),
        (
            "/datum/brain_trauma/magic/poltergeist",
            Some(Kind::Poltergeist),
            Magic,
            Lobotomy,
        ),
        (
            "/datum/brain_trauma/magic/stalker",
            Some(Kind::Stalker { cosmic: false }),
            Magic,
            Lobotomy,
        ),
    ]
};

//! The operation table: what each operation takes, says and requires.

use super::*;

use Implement::{Hand, Path, Tool};

pub(super) const INCISE: &[(Implement, f32)] = &[
    (Tool("scalpel"), 1.0),
    (Path("/obj/item/melee/energy/sword"), 1.33),
    (Path("/obj/item/knife"), 1.5),
    (Path("/obj/item/shard"), 2.25),
    (Path("/obj/item/screwdriver"), 5.0),
    (Path("/obj/item/pen"), 5.0),
    (Path("/obj/item"), 3.33),
];
/// The screwdriver operations of `operation_generic_mechanic.dm`.
pub(super) const MECHANIC_SCREW: &[(Implement, f32)] = &[
    (Tool("screwdriver"), 1.0),
    (Tool("scalpel"), 1.33),
    (Path("/obj/item/knife"), 2.0),
    (Path("/obj/item"), 10.0),
];
/// The wrench operations of `operation_generic_mechanic.dm`.
pub(super) const MECHANIC_WRENCH: &[(Implement, f32)] =
    &[(Tool("wrench"), 1.0), (Tool("retractor"), 1.33)];
pub(super) const CLAMP: &[(Implement, f32)] = &[
    (Tool("hemostat"), 1.0),
    (Tool("cutters"), 1.67),
    (Path("/obj/item/stack/package_wrap"), 2.85),
    (Path("/obj/item/stack/cable_coil"), 6.67),
];
pub(super) const TEND: &[(Implement, f32)] = &[
    (Tool("hemostat"), 1.0),
    (Tool("screwdriver"), 1.5),
    (Tool("cutters"), 1.67),
    (Path("/obj/item/pen"), 1.8),
];
pub(super) const ORGAN_CUT: &[(Implement, f32)] = &[
    (Tool("scalpel"), 1.05),
    (Path("/obj/item/melee/energy/sword"), 1.5),
    (Path("/obj/item/knife"), 2.25),
    (Path("/obj/item/shard"), 2.85),
];
/// organ manipulation's `remove_implements`.
pub(super) const ORGAN_REMOVE: &[(Implement, f32)] = &[
    (Tool("hemostat"), 1.0),
    (Tool("crowbar"), 1.8),
    (Path("/obj/item/kitchen/fork"), 2.85),
];
/// `organ/brainwash` `implements`.
pub(super) const BRAINWASH: &[(Implement, f32)] = &[
    (Tool("hemostat"), 1.15),
    (Tool("cutters"), 2.0),
    (Path("/obj/item/stack/package_wrap"), 2.85),
    (Path("/obj/item/stack/cable_coil"), 6.67),
];
pub(super) const SENSE_REPAIR: &[(Implement, f32)] = &[
    (Tool("hemostat"), 1.05),
    (Tool("screwdriver"), 2.25),
    (Path("/obj/item/pen"), 4.0),
];
/// `autopsy/all_required_strings` around the limb's own.
const AUTOPSY_REQUIRED: &[&str] = &[
    "операция на груди",
    "пациент должен быть мёртвым",
    "пациент ранее не должен подвергаться вскрытию",
];
pub(super) const SENSE_REPAIR_BLOCKED: &[&str] = &[
    "орган не должен быть кибернетическим",
    "если в конечности есть кости, они должны быть целы",
];
pub(super) const ORGAN_REPAIR_BLOCKED: &[&str] = &[
    "орган не должен быть кибернетическим",
    "орган не должен был подвергаться хирургическому восстановлению ранее",
];

impl Op {
    /// Every operation and option, in tg type order.
    pub fn all() -> Vec<Self> {
        let mut ops = vec![
            Self::InciseSkin,
            Self::RetractSkin,
            Self::CloseSkin,
            Self::ClampBleeders,
            Self::UnclampBleeders,
            Self::SawBones,
            Self::FixBones,
            Self::DrillBones,
            Self::InciseOrgans,
            Self::MechanicalIncision,
            Self::MechanicalOpen,
            Self::MechanicalClose,
            Self::PrepareElectronics,
            Self::MechanicUnwrench,
            Self::MechanicWrench,
        ];
        for tend in Tend::ALL {
            if tend.combo() {
                ops.push(Self::TendWounds(tend, Heal::Combo));
            } else {
                ops.push(Self::TendWounds(tend, Heal::Brute));
                ops.push(Self::TendWounds(tend, Heal::Burn));
            }
        }
        ops.extend([
            Self::RepairDislocation,
            Self::RepairHairline,
            Self::ResetCompound,
            Self::RepairCompound,
        ]);
        ops.extend(Slot::ALL.map(Self::RemoveOrgan));
        ops.extend(Slot::ALL.map(Self::InsertOrgan));
        ops.extend(ImplantSlot::ALL.map(Self::RemoveImplant));
        ops.push(Self::InsertImplant);
        ops.extend([
            Self::Amputate,
            Self::AmputateMechanic,
            Self::AttachLimb,
            Self::Revival,
            Self::CoronaryBypass,
            Self::BrainSurgery,
            Self::Lobotomy,
            Self::Pacify,
            Self::Brainwash,
            Self::BrainwashSleeper,
            Self::Lobectomy,
            Self::Hepatectomy,
            Self::Gastrectomy,
            Self::EarSurgery,
            Self::EyeSurgery,
            Self::Debride,
            Self::RepairPuncture,
            Self::SealVeins,
            Self::FilterBlood,
            Self::Autopsy,
            Self::AutopsyMechanic,
            Self::Dissection,
            Self::ViralBonding,
            Self::Bionecrosis,
            Self::TuneVocalCords,
        ]);
        ops.extend(Bioware::ALL.map(Self::Bioware));
        ops
    }

    /// `required_bodytype` of a limb operation: `Some(true)` only robotic limbs
    /// (`BODYTYPE_ROBOTIC`), `Some(false)` only the others (`~BODYTYPE_ROBOTIC`).
    // shortcut: the other `mechanic` variants (organ manipulation, bioware, organ repairs) are
    // not split off; those operations take any limb.
    pub fn required_robotic(self) -> Option<bool> {
        match self {
            Self::MechanicalIncision
            | Self::MechanicalOpen
            | Self::MechanicalClose
            | Self::PrepareElectronics
            | Self::MechanicUnwrench
            | Self::MechanicWrench
            | Self::AmputateMechanic
            | Self::AutopsyMechanic => Some(true),
            Self::InciseSkin
            | Self::RetractSkin
            | Self::CloseSkin
            | Self::ClampBleeders
            | Self::UnclampBleeders
            | Self::SawBones
            | Self::FixBones
            | Self::DrillBones
            | Self::InciseOrgans
            | Self::Amputate
            | Self::Autopsy
            | Self::Dissection
            | Self::ViralBonding => Some(false),
            _ => None,
        }
    }

    /// One operation per tg type (the operating computer's catalog).
    pub fn types() -> Vec<Self> {
        let mut seen = Vec::new();
        Self::all()
            .into_iter()
            .filter(|op| {
                let path = op.def().path;
                let new = !seen.contains(&path);
                seen.push(path);
                new
            })
            .collect()
    }

    /// The operation of a tg type path, for disks and designs (`None`: not ported).
    pub fn of_path(path: &str) -> Option<Self> {
        Self::types().into_iter().find(|op| op.def().path == path)
    }

    pub fn def(self) -> OpDef {
        use state::*;
        let base = OpDef {
            path: "",
            name: "",
            catalog_name: "",
            desc: "",
            time: 10.0,
            flags: 0,
            target: Target::Limb,
            all_required: 0,
            any_required: 0,
            any_blocked: 0,
            implements: &[],
            verb: "оперировать",
            replaced_by: None,
            tool_rec: "",
            required: &[],
            blocked: &[],
        };
        let organ_manipulation = OpDef {
            path: "/datum/surgery_operation/limb/organ_manipulation/internal",
            catalog_name: "Манипуляции с органами",
            desc: "Манипуляции с внутренними органами пациента.",
            time: 24.0,
            all_required: SKIN_OPEN | ORGANS_CUT,
            tool_rec: "hemostat / орган",
            required: &[
                "если оперируется мозг или любые органы грудной клетки, кость ДОЛЖНА быть \
                 распилена",
            ],
            ..base
        };
        let organ_repair = OpDef {
            all_required: SKIN_OPEN | ORGANS_CUT | BONE_SAWED,
            required: &["орган должен быть умеренно поврежден"],
            blocked: ORGAN_REPAIR_BLOCKED,
            ..base
        };
        let def = match self {
            Self::InciseSkin => OpDef {
                path: "/datum/surgery_operation/limb/incise_skin",
                name: "Сделать разрез кожи",
                desc: "Сделайте разрез на коже пациента, чтобы получить доступ к внутренним \
                       органам. Вызывает хирургическое состояние \"кожа разрезана\".",
                time: 16.0,
                any_blocked: ALL_SKIN,
                implements: INCISE,
                verb: "делать разрез на",
                tool_rec: "scalpel",
                ..base
            },
            Self::RetractSkin => OpDef {
                path: "/datum/surgery_operation/limb/retract_skin",
                name: "retract skin",
                desc: "Раздвигает кожу пациента, чтобы получить доступ к внутренним органам. \
                       Вызывает хирургическое состояние \"кожа раздвинута\".",
                time: 24.0,
                all_required: SKIN_CUT,
                implements: &[
                    (Tool("retractor"), 1.0),
                    (Tool("screwdriver"), 2.25),
                    (Tool("cutters"), 2.85),
                    (Path("/obj/item/stack/rods"), 2.85),
                    (Path("/obj/item/kitchen/fork"), 2.85),
                ],
                verb: "раздвигать кожу на",
                tool_rec: "retractor",
                ..base
            },
            Self::CloseSkin => OpDef {
                path: "/datum/surgery_operation/limb/close_skin",
                name: "Прижечь разрез кожи",
                desc: "Прижечь разрез на коже пациента, закрывая его. Устраняет большинство \
                       хирургических состояний.",
                time: 24.0,
                any_required: ALL_SKIN,
                implements: &[
                    (Tool("cautery"), 1.0),
                    (Path("/obj/item/stack/medical/suture"), 1.0),
                    (Path("/obj/item/gun/energy/laser"), 1.15),
                    (Tool("welder"), 1.5),
                    (Path("/obj/item"), 3.33),
                ],
                verb: "прижигать разрез на",
                tool_rec: "cautery",
                required: &["конечность должна иметь кожу"],
                ..base
            },
            Self::ClampBleeders => OpDef {
                path: "/datum/surgery_operation/limb/clamp_bleeders",
                name: "Зажать сосуды",
                desc: "Накладывает зажим на кровеносных сосудах пациента, чтобы предотвратить \
                       потерю крови. Вызывает хирургическое состояние \"сосуды зажаты\".",
                time: 24.0,
                all_required: SKIN_OPEN | VESSELS_UNCLAMPED,
                implements: CLAMP,
                verb: "зажимать кровеносные сосуды в",
                tool_rec: "hemostat",
                ..base
            },
            Self::UnclampBleeders => OpDef {
                path: "/datum/surgery_operation/limb/unclamp_bleeders",
                name: "Разжать сосуды",
                desc: "Снять зажимы с кровеносных сосудов в теле пациента, чтобы восстановить \
                       кровоток. Убирает хирургическое состояние \"сосуды зажаты\".",
                time: 24.0,
                all_required: SKIN_OPEN | VESSELS_CLAMPED,
                implements: CLAMP,
                verb: "разжимать кровеносные сосуды в",
                tool_rec: "hemostat",
                required: &["конечность должна иметь кровеносные сосуды"],
                ..base
            },
            Self::SawBones => OpDef {
                path: "/datum/surgery_operation/limb/saw_bones",
                name: "Распилить кость",
                desc: "Пропилите кость пациента, чтобы получить доступ к внутренним органам. \
                       Вызывает хирургическое состояние \"кость распилена\".",
                time: 54.0,
                all_required: SKIN_OPEN,
                any_blocked: BONE_SAWED | BONE_DRILLED,
                implements: &[
                    (Tool("saw"), 1.0),
                    (Path("/obj/item/shovel/serrated"), 1.33),
                    (Path("/obj/item/melee/arm_blade"), 1.33),
                    (Path("/obj/item/fireaxe"), 2.0),
                    (Path("/obj/item/hatchet"), 2.85),
                    (Path("/obj/item/knife/butcher"), 2.85),
                    (Path("/obj/item"), 4.0),
                ],
                verb: "пилить кости в",
                tool_rec: "saw",
                ..base
            },
            Self::FixBones => OpDef {
                path: "/datum/surgery_operation/limb/fix_bones",
                name: "Фиксация сломанной кости",
                desc: "Соединить рассечённые или зафиксировать сломанные кости. Убирает \
                       хирургические состояния \"кость распилена\" и \"кость просверлена\".",
                time: 40.0,
                all_required: SKIN_OPEN,
                any_required: ALL_BONE,
                implements: &[
                    (Path("/obj/item/stack/medical/bone_gel"), 1.0),
                    (
                        Path("/obj/item/stack/medical/wrap/sticky_tape/surgical"),
                        1.0,
                    ),
                    (Path("/obj/item/stack/medical/wrap/sticky_tape/super"), 2.0),
                    (Path("/obj/item/stack/medical/wrap/sticky_tape"), 3.33),
                ],
                verb: "лечить кости в",
                tool_rec: "bone gel",
                required: &["конечность должна иметь кости"],
                ..base
            },
            Self::DrillBones => OpDef {
                path: "/datum/surgery_operation/limb/drill_bones",
                name: "Сверление кости",
                desc: "Просверливание кости пациента. Вызывает хирургическое состояние \"кость \
                       просверлена\".",
                time: 30.0,
                all_required: SKIN_OPEN,
                any_blocked: BONE_SAWED | BONE_DRILLED,
                implements: &[
                    (Tool("drill"), 1.0),
                    (Path("/obj/item/screwdriver/power"), 1.25),
                    (Path("/obj/item/pickaxe/drill"), 1.67),
                    (Tool("screwdriver"), 4.0),
                    (Path("/obj/item/kitchen/spoon"), 5.0),
                    (Path("/obj/item"), 6.67),
                ],
                verb: "просверливать кость в",
                tool_rec: "drill",
                ..base
            },
            Self::InciseOrgans => OpDef {
                path: "/datum/surgery_operation/limb/incise_organs",
                name: "Сделать разрез органа",
                desc: "Сделайте разрез на тканях внутренних органов, что позволит вылечить или \
                       манипулировать с органом. Вызывает хирургическое состояние \"орган \
                       разрезан\".",
                time: 24.0,
                all_required: SKIN_OPEN,
                any_blocked: ORGANS_CUT,
                implements: &[
                    (Tool("scalpel"), 1.0),
                    (Path("/obj/item/melee/energy/sword"), 1.33),
                    (Path("/obj/item/knife"), 1.5),
                    (Path("/obj/item/shard"), 2.25),
                    (Path("/obj/item/pen"), 5.0),
                    (Path("/obj/item"), 3.33),
                ],
                verb: "делать разрез органа в",
                tool_rec: "scalpel",
                ..base
            },
            Self::TendWounds(tend, heal) => OpDef {
                path: tend.path(),
                name: match heal {
                    Heal::Brute => "Обработка ушибов",
                    Heal::Burn => "Обработка ожогов",
                    Heal::Combo => "Обработайте ушибы и ожоги",
                },
                catalog_name: tend.rnd_name(),
                desc: "Проведите поверхностный уход за ушибами и ожогами пациента.",
                time: if tend.combo() { 10.0 } else { 25.0 },
                flags: flags::LOOPING
                    | flags::IGNORE_CLOTHES
                    | if tend == Tend::Basic {
                        0
                    } else {
                        flags::LOCKED
                    },
                target: Target::Basic(TargetZone::Chest),
                any_required: ALL_SKIN,
                implements: TEND,
                verb: "обрабатывать раны:",
                replaced_by: tend.replaced_by().map(Tend::path),
                tool_rec: "hemostat",
                required: &["у пациента должны быть ушибы или ожоги"],
                ..base
            },
            Self::RepairDislocation => OpDef {
                path: "/datum/surgery_operation/limb/repair_dislocation",
                name: "reset dislocation",
                desc: "Reset a dislocated bone in a patient's limb. Similar to the field \
                       procedure, but quicker and safer due to being performed in a controlled \
                       environment.",
                time: 24.0,
                flags: flags::STANDING_ALLOWED | flags::IGNORE_CLOTHES,
                implements: &[
                    (Tool("bonesetter"), 1.0),
                    (Tool("crowbar"), 2.0),
                    (Hand, 5.0),
                ],
                verb: "вправлять вывих на",
                tool_rec: "bonesetter",
                required: &["the limb must be dislocated"],
                ..base
            },
            Self::RepairHairline => OpDef {
                path: "/datum/surgery_operation/limb/repair_hairline",
                name: "Сращивание трещины",
                desc: "Заживление трещины в кости пациента.",
                time: 40.0,
                any_required: ALL_SKIN,
                implements: &[
                    (Tool("bonesetter"), 1.0),
                    (Path("/obj/item/stack/medical/bone_gel"), 1.0),
                    (
                        Path("/obj/item/stack/medical/wrap/sticky_tape/surgical"),
                        1.0,
                    ),
                    (Path("/obj/item/stack/medical/wrap/sticky_tape/super"), 2.0),
                    (Path("/obj/item/stack/medical/wrap/sticky_tape"), 3.33),
                ],
                verb: "сращивать трещину в",
                tool_rec: "bonesetter",
                required: &["конечность должна иметь трещину"],
                ..base
            },
            Self::ResetCompound => OpDef {
                path: "/datum/surgery_operation/limb/reset_compound",
                name: "reset compound fracture",
                desc: "Вправление сложного перелома кости пациента, для подготавливки её к \
                       правильному заживлению.",
                time: 60.0,
                all_required: SKIN_OPEN,
                any_blocked: VESSELS_UNCLAMPED,
                implements: &[
                    (Tool("bonesetter"), 1.0),
                    (
                        Path("/obj/item/stack/medical/wrap/sticky_tape/surgical"),
                        1.66,
                    ),
                    (Path("/obj/item/stack/medical/wrap/sticky_tape/super"), 2.5),
                    (Path("/obj/item/stack/medical/wrap/sticky_tape"), 5.0),
                ],
                verb: "вправлять открытый перелом в",
                tool_rec: "bonesetter",
                required: &["конечность должна иметь сложный перелом"],
                ..base
            },
            Self::RepairCompound => OpDef {
                path: "/datum/surgery_operation/limb/repair_compound",
                name: "repair compound fracture",
                desc: "Залечивание сложного перелома кости пациента.",
                time: 40.0,
                any_required: ALL_SKIN,
                implements: &[
                    (Path("/obj/item/stack/medical/bone_gel"), 1.0),
                    (
                        Path("/obj/item/stack/medical/wrap/sticky_tape/surgical"),
                        1.0,
                    ),
                    (Path("/obj/item/stack/medical/wrap/sticky_tape/super"), 2.0),
                    (Path("/obj/item/stack/medical/wrap/sticky_tape"), 3.33),
                ],
                verb: "заживлять открытый перелом в",
                tool_rec: "bone gel",
                required: &["сложный перелом конечности должен быть вправлен"],
                ..base
            },
            Self::RemoveOrgan(slot) => OpDef {
                name: manip_names(slot).0,
                verb: "извлекать",
                implements: ORGAN_REMOVE,
                ..organ_manipulation
            },
            Self::RemoveImplant(slot) => OpDef {
                name: slot.remove_name(),
                verb: "извлекать",
                implements: ORGAN_REMOVE,
                ..organ_manipulation
            },
            Self::InsertImplant => OpDef {
                name: "Вставить имплант",
                verb: "вставлять",
                implements: &[(Path(cyberimps::ROOT), 1.0)],
                ..organ_manipulation
            },
            Self::InsertOrgan(slot) => OpDef {
                name: manip_names(slot).1,
                verb: "вставлять",
                // shortcut: no food or plants as dummy organs (`random_item_organ_pool`).
                implements: &[(Path("/obj/item/organ"), 1.0)],
                ..organ_manipulation
            },
            Self::Amputate => OpDef {
                path: "/datum/surgery_operation/limb/amputate",
                name: "Ампутация конечности",
                catalog_name: "Разъединение (Ампутация)",
                desc: "Отделение конечности от тела пациента.",
                time: 64.0,
                all_required: SKIN_OPEN | BONE_SAWED,
                any_blocked: VESSELS_UNCLAMPED,
                implements: &[
                    (Path("/obj/item/shears"), 0.33),
                    (Tool("saw"), 1.0),
                    (Tool("scalpel"), 1.0),
                    (Path("/obj/item/melee/arm_blade"), 1.25),
                    (Path("/obj/item/shovel/serrated"), 1.33),
                    (Path("/obj/item/fireaxe"), 2.0),
                    (Path("/obj/item/hatchet"), 2.5),
                    (Path("/obj/item/knife/butcher"), 4.0),
                ],
                verb: "ампутировать",
                tool_rec: "amputation shears",
                ..base
            },
            Self::AmputateMechanic => OpDef {
                path: "/datum/surgery_operation/limb/amputate/mechanic",
                name: "Разбор конечности",
                catalog_name: "Разборка (Ампутация)",
                desc: "Отделение конечности от тела пациента.",
                time: 20.0,
                all_required: SKIN_OPEN,
                any_blocked: VESSELS_UNCLAMPED,
                implements: &[
                    (Path("/obj/item/shovel/giant_wrench"), 0.33),
                    (Tool("wrench"), 1.0),
                    (Tool("crowbar"), 1.0),
                    (Tool("scalpel"), 2.0),
                    (Tool("saw"), 2.0),
                ],
                verb: "разбирать",
                tool_rec: "wrench",
                ..base
            },
            Self::MechanicalIncision => OpDef {
                path: "/datum/surgery_operation/limb/mechanical_incision",
                name: "Отвинтите корпус",
                desc: "Отвинтите корпус механического пациента, чтобы получить доступ к его \
                       внутренним компонентам. Вызывает хирургическое состояние \"кожа \
                       разрезана\".",
                time: 24.0,
                flags: flags::SELF_OPERABLE,
                any_blocked: ALL_SKIN,
                implements: MECHANIC_SCREW,
                verb: "отвинчивать корпус в",
                tool_rec: "Любой острый предмет",
                ..base
            },
            Self::MechanicalOpen => OpDef {
                path: "/datum/surgery_operation/limb/mechanical_open",
                name: "Открыть люк",
                desc: "Откройте люк механического пациента, чтобы получить доступ к его \
                       внутренним компонентам. Вызывает хирургическое состояние \"кожа \
                       раздвинута\" и \"сосуды зажаты\".",
                time: 10.0,
                flags: flags::SELF_OPERABLE,
                all_required: SKIN_CUT,
                implements: &[(Hand, 1.0), (Tool("crowbar"), 1.0)],
                verb: "открывать фиксаторы люка в",
                tool_rec: "crowbar",
                ..base
            },
            Self::MechanicalClose => OpDef {
                path: "/datum/surgery_operation/limb/mechanical_close",
                name: "Закрутить корпус",
                desc: "Привинтите корпус механического пациента обратно на место. Устраняет \
                       большинство хирургических состояний.",
                time: 24.0,
                flags: flags::SELF_OPERABLE,
                any_required: ALL_SKIN,
                implements: MECHANIC_SCREW,
                verb: "завинчивать корпус в",
                tool_rec: "Любой острый предмет",
                ..base
            },
            Self::PrepareElectronics => OpDef {
                path: "/datum/surgery_operation/limb/prepare_electronics",
                name: "Подготовьте электронику",
                desc: "Подготовьте внутреннюю электронику механического пациента к операции. \
                       Вызывает хирургическое состояние \"орган разрезан\".",
                time: 24.0,
                flags: flags::SELF_OPERABLE,
                all_required: SKIN_OPEN,
                any_blocked: ORGANS_CUT,
                implements: &[(Tool("multitool"), 1.0), (Tool("hemostat"), 1.33)],
                verb: "подготавливать электронику в",
                tool_rec: "multitool",
                ..base
            },
            Self::MechanicUnwrench => OpDef {
                path: "/datum/surgery_operation/limb/mechanic_unwrench",
                name: "Открутить эндоскелет",
                desc: "Открутите эндоскелет механического пациента, чтобы получить доступ к его \
                       внутренним элементам. Вызывает хирургическое состояние \"кость \
                       распилена\".",
                time: 24.0,
                flags: flags::SELF_OPERABLE,
                all_required: SKIN_OPEN,
                any_blocked: BONE_SAWED | BONE_DRILLED,
                implements: MECHANIC_WRENCH,
                verb: "откручивать несколько болтов в",
                tool_rec: "wrench",
                ..base
            },
            Self::MechanicWrench => OpDef {
                path: "/datum/surgery_operation/limb/mechanic_wrench",
                name: "Закрутить эндоскелет",
                desc: "Закрутите обратно эндоскелет механического пациента. Убирает \
                       хирургические состояния \"кость распилена\".",
                time: 24.0,
                flags: flags::SELF_OPERABLE,
                all_required: SKIN_OPEN | BONE_SAWED,
                implements: MECHANIC_WRENCH,
                verb: "закручивать несколько болтов в",
                tool_rec: "wrench",
                required: &["конечность должна иметь кости"],
                ..base
            },
            Self::AttachLimb => OpDef {
                path: "/datum/surgery_operation/limb/prosthetic_replacement",
                name: "Прикрепить конечность",
                catalog_name: "prosthetic replacement",
                desc: "Замена отсутствующей конечности протезом (или произвольным предметом).",
                time: 32.0,
                flags: flags::STANDING_ALLOWED | flags::PRIORITY_NEXT_STEP | flags::IGNORE_CLOTHES,
                all_required: SKIN_OPEN,
                any_blocked: VESSELS_UNCLAMPED,
                // shortcut: no arbitrary items as prosthetic arms.
                implements: &[(Path("/obj/item/bodypart"), 1.0)],
                verb: "заменять отсутствующую конечность в",
                tool_rec: "любая конечность / любой предмет",
                required: &["конечность должна отсутствовать / быть повреждённой"],
                ..base
            },
            Self::Revival => OpDef {
                path: "/datum/surgery_operation/basic/revival",
                name: "Дефибрилляция мозга",
                catalog_name: "Дефибриляция мозга (Оживление)",
                desc: "Использование дефибриллятора, чтобы вернуть мозг пациента к жизни.",
                time: 50.0,
                target: Target::Basic(TargetZone::Head),
                all_required: SKIN_OPEN | BONE_SAWED,
                implements: &[
                    (Path("/obj/item/shockpaddles"), 1.0),
                    (Path("/obj/item/melee/touch_attack/shock"), 1.0),
                    (Path("/obj/item/melee/baton/security"), 1.33),
                    (Path("/obj/item/gun/energy"), 1.67),
                ],
                tool_rec: "defibrillator paddles",
                required: &[
                    "пациент должен быть мёртв",
                    "пациент должен быть в состоянии, пригодном для реанимации",
                ],
                ..base
            },
            Self::CoronaryBypass => OpDef {
                path: "/datum/surgery_operation/organ/repair/coronary_bypass",
                name: "Коронарное шунтирование",
                catalog_name: "Аортокоронарное шунтирование (Восстановление сердца)",
                desc: "Установка шунта на поврежденное сердце пациента для восстановления \
                       нормального кровотока. Эту процедуру можно провести только один раз для \
                       каждого органа.",
                time: 90.0,
                target: Target::Organ(Slot::Heart),
                all_required: SKIN_OPEN | ORGANS_CUT | BONE_SAWED,
                implements: &[
                    (Tool("hemostat"), 1.05),
                    (Tool("cutters"), 2.85),
                    (Path("/obj/item/stack/package_wrap"), 6.67),
                    (Path("/obj/item/stack/cable_coil"), 2.0),
                ],
                tool_rec: "hemostat",
                required: &["орган должен быть умеренно поврежден"],
                blocked: ORGAN_REPAIR_BLOCKED,
                ..base
            },
            Self::BrainSurgery => OpDef {
                path: "/datum/surgery_operation/organ/repair/brain",
                name: "Операция на мозге",
                catalog_name: "Нейрохирургия (Хирургия мозга)",
                desc: "Восстановление поврежденных тканей мозга пациента для возвращения \
                       когнитивных функций.",
                time: 100.0,
                flags: flags::LOOPING,
                target: Target::Organ(Slot::Brain),
                all_required: SKIN_OPEN | BONE_SAWED,
                any_blocked: VESSELS_UNCLAMPED,
                implements: &[
                    (Tool("hemostat"), 1.05),
                    (Tool("screwdriver"), 2.85),
                    (Path("/obj/item/pen"), 6.67),
                ],
                tool_rec: "hemostat",
                blocked: &["орган не должен быть кибернетическим"],
                ..base
            },
            Self::Lobotomy => OpDef {
                path: "/datum/surgery_operation/organ/lobotomy",
                name: "Лоботомия",
                catalog_name: "Лоботомия (Лоботомия)",
                desc: "Исправление большинства травм мозга пациента с риском вызвать новые \
                       постоянные травмы.",
                flags: flags::LOCKED,
                target: Target::Organ(Slot::Brain),
                all_required: SKIN_OPEN | BONE_SAWED,
                any_blocked: VESSELS_UNCLAMPED,
                implements: &[
                    (Tool("scalpel"), 1.15),
                    (Path("/obj/item/melee/energy/sword"), 0.55),
                    (Path("/obj/item/knife"), 2.85),
                    (Path("/obj/item/shard"), 4.0),
                    (Path("/obj/item"), 5.0),
                ],
                tool_rec: "Любой острый предмет",
                blocked: &["орган не должен быть кибернетическим"],
                ..base
            },
            Self::Pacify => OpDef {
                path: "/datum/surgery_operation/organ/pacify",
                name: "pacification",
                catalog_name: "Паксопсия (Пацификация)",
                desc: "Удаление агрессивных наклонностей из мозга пациента.",
                time: 40.0,
                flags: flags::LOCKED,
                target: Target::Organ(Slot::Brain),
                all_required: SKIN_OPEN | BONE_SAWED,
                any_blocked: VESSELS_UNCLAMPED,
                implements: &[
                    (Tool("hemostat"), 1.0),
                    (Tool("screwdriver"), 2.85),
                    (Path("/obj/item/pen"), 6.67),
                ],
                tool_rec: "hemostat",
                blocked: &["орган не должен быть кибернетическим"],
                ..base
            },
            Self::Brainwash => OpDef {
                path: "/datum/surgery_operation/organ/brainwash",
                name: "Промывание мозгов",
                catalog_name: "Нейронное промывание мозгов (Промывание мозгов)",
                desc: "Внедрение директивы в мозг пациента, сделав её его абсолютным приоритетом.",
                time: 200.0,
                flags: flags::LOCKED,
                target: Target::Organ(Slot::Brain),
                all_required: SKIN_OPEN | ORGANS_CUT | BONE_SAWED,
                implements: BRAINWASH,
                tool_rec: "hemostat",
                blocked: &["орган не должен быть кибернетическим"],
                ..base
            },
            Self::BrainwashSleeper => OpDef {
                path: "/datum/surgery_operation/organ/brainwash/sleeper",
                name: "Установить директивы спящего агента",
                catalog_name: "Имплантация спящего агента (Промывание мозгов)",
                ..Self::Brainwash.def()
            },
            Self::Lobectomy => OpDef {
                path: "/datum/surgery_operation/organ/repair/lobectomy",
                name: "Удаление поврежденной доли легкого",
                catalog_name: "Лобэктомия (Восстановление лёгких)",
                desc: "Восстановление поврежденных лёгких пациента путем удаления наиболее \
                       поврежденной доли. Эту процедуру можно провести только один раз для \
                       каждого органа.",
                time: 42.0,
                target: Target::Organ(Slot::Lungs),
                implements: ORGAN_CUT,
                tool_rec: "scalpel",
                ..organ_repair
            },
            Self::Hepatectomy => OpDef {
                path: "/datum/surgery_operation/organ/repair/hepatectomy",
                name: "Удаление поврежденной части печени",
                catalog_name: "Гепатэктомия (Восстановление печени)",
                desc: "Восстановление поврежденной печени пациента путем удаления наиболее \
                       поврежденного сегмента. Эту процедуру можно провести только один раз \
                       для каждого органа.",
                time: 52.0,
                target: Target::Organ(Slot::Liver),
                implements: ORGAN_CUT,
                tool_rec: "scalpel",
                ..organ_repair
            },
            Self::Gastrectomy => OpDef {
                path: "/datum/surgery_operation/organ/repair/gastrectomy",
                name: "Удаление поврежденной части желудка",
                catalog_name: "Гастрэктомия (Восстановление желудка)",
                desc: "Восстановление желудка пациента путем удаления поврежденного сегмента. \
                       Эту процедуру можно провести только один раз для каждого органа.",
                time: 52.0,
                target: Target::Organ(Slot::Stomach),
                implements: &[
                    (Tool("scalpel"), 1.05),
                    (Path("/obj/item/melee/energy/sword"), 1.5),
                    (Path("/obj/item/knife"), 2.25),
                    (Path("/obj/item/shard"), 2.85),
                    (Path("/obj/item"), 4.0),
                ],
                tool_rec: "scalpel",
                ..organ_repair
            },
            Self::EarSurgery => OpDef {
                path: "/datum/surgery_operation/organ/repair/ears",
                name: "Операция на ушах",
                catalog_name: "Ототомия (Хирургия ушей)",
                desc: "Восстановление поврежденных ушей пациента для возвращения слуха.",
                time: 64.0,
                target: Target::Organ(Slot::Ears),
                all_required: SKIN_OPEN,
                any_blocked: VESSELS_UNCLAMPED,
                implements: SENSE_REPAIR,
                tool_rec: "hemostat",
                required: &[],
                blocked: SENSE_REPAIR_BLOCKED,
                ..organ_repair
            },
            Self::EyeSurgery => OpDef {
                path: "/datum/surgery_operation/organ/repair/eyes",
                name: "Операция на глазах",
                catalog_name: "Витрэктомия (Хирургия глаз)",
                desc: "Восстановление поврежденных глаз пациента для возвращения зрения.",
                time: 64.0,
                target: Target::Organ(Slot::Eyes),
                all_required: SKIN_OPEN,
                any_blocked: VESSELS_UNCLAMPED,
                implements: SENSE_REPAIR,
                tool_rec: "hemostat",
                required: &[],
                blocked: SENSE_REPAIR_BLOCKED,
                ..organ_repair
            },
            Self::FilterBlood => OpDef {
                path: "/datum/surgery_operation/limb/filter_blood",
                name: "Фильтрация крови",
                catalog_name: "Гемодиализ (Фильтрация крови)",
                desc: "Удаление нежелательных химических веществ из кровотока пациента.",
                time: 25.0,
                flags: flags::LOOPING,
                all_required: SKIN_OPEN,
                any_blocked: VESSELS_UNCLAMPED,
                implements: &[(Path("/obj/item/blood_filter"), 1.0)],
                tool_rec: "blood filter",
                required: &["операция на груди", "пациент не должен быть хаском"],
                ..base
            },
            Self::Autopsy => OpDef {
                path: "/datum/surgery_operation/limb/autopsy",
                name: "Аутопсия",
                catalog_name: "Андротомия (Вскрытие и аутопсия)",
                desc: "Проведение детального анализа тела умершего пациента.",
                time: 100.0,
                flags: flags::IGNORE_CLOTHES,
                all_required: SKIN_OPEN,
                implements: &[(Path(AUTOPSY_SCANNER), 1.0)],
                tool_rec: "autopsy scanner",
                required: AUTOPSY_REQUIRED,
                ..base
            },
            Self::AutopsyMechanic => OpDef {
                path: "/datum/surgery_operation/limb/autopsy/mechanic",
                name: "Анализ системных сбоев",
                catalog_name: "Анализ системных сбоев (Вскрытие и аутопсия)",
                desc: "Выполнение детального анализа деактивированных систем роботизированного \
                       пациента.",
                ..Self::Autopsy.def()
            },
            Self::Dissection => OpDef {
                path: "/datum/surgery_operation/basic/dissection",
                name: "Экспериментальное вскрытие",
                catalog_name: "Экспериментальная андротомия (Экспериментальное вскрытие и \
                               аутопсия)",
                desc: "Проведение экспериментального вскрытия пациента, чтобы получить \
                       исследовательские очки.",
                time: 120.0,
                flags: flags::LOCKED | flags::IGNORE_CLOTHES,
                target: Target::Basic(TargetZone::Chest),
                any_required: ALL_SKIN,
                implements: &[
                    (Path(AUTOPSY_SCANNER), 1.0),
                    (Tool("scalpel"), 1.66),
                    (Tool("knife"), 5.0),
                    (Path("/obj/item/shard"), 10.0),
                ],
                tool_rec: "autopsy scanner",
                required: &[
                    "пациент должен быть мёртвым",
                    "пациент ранее не должен подвергаться вскрытию",
                ],
                ..base
            },
            Self::ViralBonding => OpDef {
                path: "/datum/surgery_operation/basic/viral_bonding",
                name: "Вирусное связывание",
                catalog_name: "Виропластика (Вирусное связывание)",
                desc: "Создание симбиотической связи между пациентом и вирусом, которым он \
                       заражён.",
                time: 100.0,
                flags: flags::LOCKED,
                target: Target::Basic(TargetZone::Chest),
                all_required: SKIN_OPEN | ORGANS_CUT,
                implements: &[
                    (Tool("cautery"), 1.0),
                    (Tool("welder"), 2.0),
                    (Path("/obj/item"), 3.33),
                ],
                tool_rec: "cautery",
                required: &[
                    "у пациента должен быть вирус для формирования связи",
                    "пациент должен получить >1u Spaceacillin",
                    "пациент должен получить >1u Virus Food",
                    "пациент должен получить >1u Formaldehyde",
                ],
                ..base
            },
            Self::Bionecrosis => OpDef {
                path: "/datum/surgery_operation/limb/bionecrosis",
                name: "Вызвать бионекроз",
                catalog_name: "Бионекропластика (Некротическое оживление)",
                desc: "Введение реагентов, стимулирующих рост Ромерол-опухоли в мозге пациента.",
                time: 50.0,
                flags: flags::LOCKED,
                all_required: SKIN_OPEN | BONE_SAWED,
                any_blocked: VESSELS_UNCLAMPED,
                implements: &[
                    (Path("/obj/item/reagent_containers/syringe"), 1.0),
                    (Path("/obj/item/pen"), 3.33),
                ],
                tool_rec: "syringe",
                required: &["в конечности должен присутствовать мозг"],
                blocked: &["в конечности не должно быть Ромерол-опухоли"],
                ..base
            },
            Self::TuneVocalCords => OpDef {
                path: "/datum/surgery_operation/limb/tune_vocal_cords",
                name: "Настройка голосовых связок",
                desc: "Проведите операцию на связках пациента для изменения голоса.",
                time: 64.0,
                all_required: SKIN_OPEN,
                implements: &[
                    (Tool("scalpel"), 1.0),
                    (Path("/obj/item/knife"), 2.0),
                    (Tool("cutters"), 2.85),
                    (Path("/obj/item/pen"), 5.0),
                ],
                tool_rec: "scalpel",
                required: &["оперируйте голову"],
                ..base
            },
            Self::Bioware(kind) => bioware_def(kind, base),
            Self::Debride => OpDef {
                path: "/datum/surgery_operation/limb/debride",
                name: "Обработка зараженной плоти",
                catalog_name: "Обработка раны",
                desc: "Удаление инфицированной или омертвевшей плоти из раны пациента, чтобы \
                       ускорить заживление.",
                time: 30.0,
                flags: flags::LOOPING | flags::PRIORITY_NEXT_STEP,
                implements: &[
                    (Tool("hemostat"), 1.0),
                    (Tool("scalpel"), 1.25),
                    (Tool("saw"), 1.66),
                    (Tool("cutters"), 2.5),
                ],
                tool_rec: "hemostat",
                required: &["на конечности должен быть ожог второй степени или хуже"],
                ..base
            },
            Self::RepairPuncture => OpDef {
                path: "/datum/surgery_operation/limb/repair_puncture",
                name: "Соединение сосудов",
                desc: "Соединение разорванных кровеносных сосудов пациента для подготовки к их \
                       прижиганию.",
                time: 30.0,
                flags: flags::PRIORITY_NEXT_STEP,
                all_required: SKIN_OPEN | ORGANS_CUT,
                implements: &[
                    (Tool("hemostat"), 1.0),
                    (Tool("scalpel"), 1.15),
                    (Tool("cutters"), 2.5),
                ],
                tool_rec: "hemostat",
                required: &["конечность должна иметь необработанную колотую рану"],
                ..base
            },
            Self::SealVeins => OpDef {
                path: "/datum/surgery_operation/limb/seal_veins",
                name: "Прижигание сосудов",
                desc: "Прижигание ранее соединенных кровеносных сосудов пациента.",
                time: 32.0,
                flags: flags::PRIORITY_NEXT_STEP,
                all_required: SKIN_OPEN | ORGANS_CUT,
                implements: &[
                    (Tool("cautery"), 1.0),
                    (Path("/obj/item/gun/energy/laser"), 1.12),
                    (Tool("welder"), 1.5),
                    (Path("/obj/item"), 3.33),
                ],
                tool_rec: "cautery",
                required: &["конечность должна иметь подготовленную колотую рану"],
                ..base
            },
        };
        OpDef {
            catalog_name: if def.catalog_name.is_empty() {
                def.name
            } else {
                def.catalog_name
            },
            ..def
        }
    }
}

/// The bioware operations (`/datum/surgery_operation/limb/bioware`: by hand, through open skin,
/// sawed bone and cut organs, locked).
pub(super) fn bioware_def(kind: Bioware, base: OpDef) -> OpDef {
    use state::*;
    const CIRCULATION: &[&str] = &[
        "пациент ранее не должен был проходить Симваскулодез (Венозная \
                                    нить) OR Миоваскулопластика (Мышечная мембрана вены)",
    ];
    const NERVES: &[&str] = &[
        "пациент ранее не должен был проходить Симневродез (Сращивание \
                               нервов) OR Ксантоневропластика (Заземление нервов)",
    ];
    const CORTEX: &[&str] = &[
        "пациент ранее не должен был проходить Энцефалофрактопластика \
                               (Складывание коры) OR Энцефалопластика (Отпечаток коры)",
    ];
    const LIGAMENTS: &[&str] = &[
        "пациент ранее не должен был проходить Артропластика (Крюк для связок) OR \
                                  Артрорафия (Укрепление связок)",
    ];
    const CHEST: &[&str] = &["операция на грудь (цель грудь)"];
    const HEAD: &[&str] = &["операция на голова (цель голова)"];
    let bioware = OpDef {
        time: 125.0,
        flags: flags::LOCKED,
        all_required: SKIN_OPEN | BONE_SAWED | ORGANS_CUT,
        implements: &[(Hand, 1.0)],
        tool_rec: "hands",
        required: CHEST,
        ..base
    };
    match kind {
        Bioware::VeinThreading => OpDef {
            path: "/datum/surgery_operation/limb/bioware/vein_threading",
            name: "Венозная нить",
            catalog_name: "Симваскулодез (Венозная нить)",
            desc: "Сплетение вен пациента в усиленную сетку, для уменьшения кровопотери при \
                   травмах.",
            blocked: CIRCULATION,
            ..bioware
        },
        Bioware::MuscledVeins => OpDef {
            path: "/datum/surgery_operation/limb/bioware/muscled_veins",
            name: "Мышечная мембрана вены",
            catalog_name: "Миоваскулопластика (Мышечная мембрана вены)",
            desc: "Добавление мышечной оболочки к венам пациента, что позволит ему перекачивать \
                   кровь без участия сердца.",
            blocked: CIRCULATION,
            ..bioware
        },
        Bioware::NerveSplicing => OpDef {
            path: "/datum/surgery_operation/limb/bioware/nerve_splicing",
            name: "Сращивание нервов",
            catalog_name: "Симневродез (Сращивание нервов)",
            desc: "Соединение нервов пациента вместе, чтобы сделать их более устойчивыми к \
                   оглушению.",
            time: 155.0,
            blocked: NERVES,
            ..bioware
        },
        Bioware::NerveGrounding => OpDef {
            path: "/datum/surgery_operation/limb/bioware/nerve_grounding",
            name: "Заземление нервов",
            catalog_name: "Ксантоневропластика (Заземление нервов)",
            desc: "Перенаправление нервов пациента так, чтобы они действовали как заземляющие \
                   стержни, защищая их от поражения электрическим током.",
            time: 155.0,
            blocked: NERVES,
            ..bioware
        },
        Bioware::CortexImprint => OpDef {
            path: "/datum/surgery_operation/limb/bioware/cortex_imprint",
            name: "Отпечаток коры",
            catalog_name: "Энцефалопластика (Отпечаток коры)",
            desc: "Биологическая модернизация, которая формирует самозапечатлевающийся узор на \
                   коре головного мозга пациента, увеличивая плотность и устойчивость нервных \
                   клеток.",
            required: HEAD,
            blocked: CORTEX,
            ..bioware
        },
        Bioware::LigamentHook => OpDef {
            path: "/datum/surgery_operation/limb/bioware/ligament_hook",
            name: "Изменение формы связок",
            catalog_name: "Артропластика (Крюк для связок)",
            desc: "Изменение формы связок пациента, чтобы в случае отсоединения конечности её \
                   можно было прикрепить вручную, за счет облегчения отсоединения конечностей.",
            blocked: LIGAMENTS,
            ..bioware
        },
        Bioware::LigamentReinforcement => OpDef {
            path: "/datum/surgery_operation/limb/bioware/ligament_reinforcement",
            name: "Укрепление связок",
            catalog_name: "Артрорафия (Укрепление связок)",
            desc: "Укрепление связок пациента, чтобы затруднить расчленение за счет облегчения \
                   прерывания нервных связей.",
            blocked: LIGAMENTS,
            ..bioware
        },
    }
}

/// The bioware operations' `on_preop` and `on_success` lines: (own, others) of each, the pain.
pub(super) fn bioware_lines(kind: Bioware, target: &str, user: &str) -> [String; 6] {
    let (start, started, done, did, pain_start, pain_done) = match kind {
        Bioware::VeinThreading => (
            "начинаете плести кровеносную систему",
            "начинает плести кровеносную систему",
            "сплетаете кровеносную систему у {t} в прочную сеть!",
            "сплетает кровеносную систему у {t} в прочную сеть!",
            "Всё ваше тело горит в агонии!",
            "Вы можете почувствовать, как кровь движется по усиленным венам!",
        ),
        Bioware::MuscledVeins => (
            "начинаете обматывать мышцами кровеносные сосуды",
            "начинает обматывать мышцами кровеносные сосуды",
            "изменяете форму кровеносных сосудов у {t}, добавляя мышечную оболочку!",
            "изменяет форму кровеносных сосудов у {t}, добавляя мышечную оболочку!",
            "Всё ваше тело горит в агонии!",
            "Вы можете чувствовать, как мощные удары вашего сердца разносятся по всему телу!",
        ),
        Bioware::NerveSplicing => (
            "начинаете соединять нервы",
            "начинает соединять нервы",
            "успешно сращиваете нервную систему у {t}!",
            "успешно сращивает нервную систему у {t}!",
            "Все ваше тело немеет!",
            "Вы вновь обретаете чувствительность в своем теле; вам кажется, что всё происходит \
             вокруг вас в замедлении!",
        ),
        Bioware::NerveGrounding => (
            "начинаете перенаправлять нервы",
            "начинает перенаправлять нервы",
            "успешно перенаправляете нервную систему у {t}!",
            "успешно перенаправляет нервы у {t}!",
            "Все ваше тело немеет!",
            "Вы возвращаете своему телу ощущение свежести! Вы чувствуете прилив сил!",
        ),
        Bioware::CortexImprint => (
            "начинаете вырезать на внешней коре головного мозга самопечатающийся шаблон",
            "начинает вырезать на внешней коре головного мозга самопечатающийся шаблон",
            "преобразуете внешнюю кору головного мозга у {t} в самопечатающийся шаблон!",
            "перестраивает внешнюю кору головного мозга у {t} в самопечатающийся шаблон!",
            "Ваша голова раскалывается от ужасной боли, с ней почти невозможно справиться!",
            "Ваш мозг становится сильнее... более устойчивым!",
        ),
        Bioware::LigamentHook => (
            "начинаете придавать связкам форму крючка",
            "начинает придавать связкам форму крючка",
            "придаете связкам у {t} форму крючка!",
            "придает связкам у {t} форму крючка!",
            "Ваши конечности горят от сильной боли!",
            "Ваши конечности кажутся... странно свободными.",
        ),
        Bioware::LigamentReinforcement => (
            "начинаете укреплять связки",
            "начинает укреплять связки",
            "укрепляете связки у {t}!",
            "укрепляет связки у {t}!",
            "Ваши конечности горят от сильной боли!",
            "Ваши конечности чувствуют себя более защищенными, но также более хрупкими.",
        ),
    };
    [
        format!("Вы {start} у {target}."),
        format!("{user} {started} у {target}."),
        format!("Вы {}", done.replace("{t}", target)),
        format!("{user} {}", did.replace("{t}", target)),
        pain_start.to_owned(),
        pain_done.to_owned(),
    ]
}

/// Organ manipulation's radial options: "Извлечь …", "Вставить …".
pub(super) fn manip_names(slot: Slot) -> (&'static str, &'static str) {
    match slot {
        Slot::Brain => ("Извлечь мозг", "Вставить мозг"),
        Slot::Appendix => ("Извлечь аппендикс", "Вставить аппендикс"),
        Slot::Stomach => ("Извлечь желудок", "Вставить желудок"),
        Slot::Ears => ("Извлечь уши", "Вставить уши"),
        Slot::Eyes => ("Извлечь глаза", "Вставить глаза"),
        Slot::Lungs => ("Извлечь лёгкие", "Вставить лёгкие"),
        Slot::Heart => ("Извлечь сердце", "Вставить сердце"),
        Slot::Liver => ("Извлечь печень", "Вставить печень"),
        Slot::Tongue => ("Извлечь язык", "Вставить язык"),
        Slot::Zombie => ("Извлечь гнойную массу", "Вставить гнойную массу"),
    }
}

/// The organ an `organ` operation works on.
pub fn organ_target(op: Op) -> Option<Slot> {
    match op.def().target {
        Target::Organ(slot) => Some(slot),
        _ => None,
    }
}

/// `parse_zone`.
pub(super) fn zone_name(zone: TargetZone) -> &'static str {
    match zone {
        TargetZone::Head => "голова",
        TargetZone::Eyes => "глаза",
        TargetZone::Mouth => "рот",
        TargetZone::Chest => "грудь",
        TargetZone::Groin => "пах",
        TargetZone::LArm => "левая рука",
        TargetZone::RArm => "правая рука",
        TargetZone::LLeg => "левая нога",
        TargetZone::RLeg => "правая нога",
    }
}

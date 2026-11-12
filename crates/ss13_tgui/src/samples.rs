//! Plausible data for every client view, for looking at a window without a server (the
//! `ui_gallery` tool of `ss13_client`). [`sample`] gives a view's title, static data and data,
//! encoded as the server sends them.

use crate::interfaces::ai_airlock::{AiAirlockData, AiAirlockUi, AirlockPower, AirlockWires};
use crate::interfaces::air_alarm::{
    AirAlarmData, AirAlarmUi, AlarmControls, EnvEntry, ModeEntry, ScrubberEntry, TlvEntry,
    VentEntry,
};
use crate::interfaces::airlock_controller::{
    AirlockButtonControllerData, AirlockButtonControllerUi, AirlockControllerData,
    AirlockControllerUi,
};
use crate::interfaces::airlock_electronics::{
    AccessRegion, AirlockElectronicsData, AirlockElectronicsStatic, AirlockElectronicsUi,
    RegionAccess,
};
use crate::interfaces::alert::{AlertStatic, AlertUi, PromptTimer};
use crate::interfaces::apc::{ApcData, ApcUi, PowerChannel};
use crate::interfaces::atmos::{
    AtmosFilterData, AtmosFilterUi, AtmosMixerData, AtmosMixerUi, AtmosPumpData, AtmosPumpUi,
    AtmosTempGateData, AtmosTempGateUi, AtmosTempPumpData, AtmosTempPumpUi, CanisterData,
    CanisterStatic, CanisterUi, GasToggle, HoldingTank, PipeScrubberData, PipeScrubberStatic,
    PipeScrubberUi, PortablePumpData, PortablePumpUi, PortableScrubberData, PortableScrubberUi,
};
use crate::interfaces::atmos_alert::{AtmosAlertData, AtmosAlertUi};
use crate::interfaces::atmos_control::{
    AtmosControlData, AtmosControlStatic, AtmosControlUi, Chamber, PortInfo,
};
use crate::interfaces::checkbox_input::{CheckboxInputStatic, CheckboxInputUi};
use crate::interfaces::chem_dispenser::{
    ChemDispenserData, ChemDispenserUi, DispensableReagent, DispenserBeaker,
};
use crate::interfaces::chem_heater::{ActiveReaction, ChemHeaterData, ChemHeaterUi, HeaterBeaker};
use crate::interfaces::chem_master::{
    AnalyzableBeaker, AnalyzableReagent, ChemMasterData, ChemMasterStatic, ChemMasterUi,
    ContainerCategory, PrintableContainer,
};
use crate::interfaces::color_picker::{ColorPickerStatic, ColorPickerUi};
use crate::interfaces::cryo::{CryoBeaker, CryoData, CryoOccupant, CryoUi};
use crate::interfaces::crystallizer::{
    CrystallizerData, CrystallizerStatic, CrystallizerUi, InternalGas, RecipeEntry,
};
use crate::interfaces::destination_tagger::{
    DestinationTaggerData, DestinationTaggerStatic, DestinationTaggerUi,
};
use crate::interfaces::disposal_unit::{DisposalUnitData, DisposalUnitUi};
use crate::interfaces::dna_console::{DnaConsoleData, DnaConsoleUi, DnaMutation};
use crate::interfaces::electrolyzer::{ElectrolyzerData, ElectrolyzerUi};
use crate::interfaces::gas_analyzer::{
    AtmosHandbook, GasAnalyzerData, GasAnalyzerUi, Gasmix, HandbookGas,
};
use crate::interfaces::gps::{GpsData, GpsSignal, GpsUi};
use crate::interfaces::gravity_generator::{GravityGeneratorData, GravityGeneratorUi};
use crate::interfaces::holodeck::{HoloProgram, HolodeckData, HolodeckUi};
use crate::interfaces::implant_chair::{ImplantChairData, ImplantChairOccupant, ImplantChairUi};
use crate::interfaces::implant_pad::{ImplantPadData, ImplantPadUi};
use crate::interfaces::iv_drip::{IvDripData, IvDripUi};
use crate::interfaces::job_selection::{
    DepartmentSlots, DepartmentStatic, JobSelectionData, JobSelectionStatic, JobSelectionUi,
    JobSlots, JobStatic,
};
use crate::interfaces::key_combo::{KeyComboStatic, KeyComboUi};
use crate::interfaces::limbgrower::{
    LimbgrowerCategory, LimbgrowerData, LimbgrowerDesign, LimbgrowerReagent, LimbgrowerStatic,
    LimbgrowerUi,
};
use crate::interfaces::list_input::{ListInputData, ListInputUi};
use crate::interfaces::mech_bay::{
    BayCell, BayMech, MechBayData, MechBayPowerConsoleUi, RechargePort,
};
use crate::interfaces::medical_kiosk::{MedicalKioskData, MedicalKioskUi};
use crate::interfaces::number_input::{NumberInputStatic, NumberInputUi};
use crate::interfaces::pipe_dispenser::{
    InitDirections, PipeCategory, PipeDispenserData, PipeDispenserStatic, PipeDispenserUi,
    PipeRecipeEntry,
};
use crate::interfaces::portable_generator::{PortableGeneratorData, PortableGeneratorUi};
use crate::interfaces::portagrav::{PortagravData, PortagravUi};
use crate::interfaces::power_monitor::{PowerMonitorArea, PowerMonitorData, PowerMonitorUi};
use crate::interfaces::rapid_pipe_dispenser::{
    RapidPipeDispenserUi, RpdCategory, RpdData, RpdPreview, RpdRecipe, RpdStatic,
};
use crate::interfaces::sleeper::{SleeperChem, SleeperData, SleeperOccupant, SleeperUi};
use crate::interfaces::smart_vend::{FridgeItem, SmartVendData, SmartVendUi};
use crate::interfaces::smes::{SmesData, SmesStatic, SmesUi};
use crate::interfaces::solar_control::{SolarControlData, SolarControlUi};
use crate::interfaces::space_heater::{SpaceHeaterData, SpaceHeaterUi};
use crate::interfaces::supermatter::{SmEntry, SmFactor, SmGasShare, SupermatterUi};
use crate::interfaces::tank::{TankData, TankStatic, TankUi};
use crate::interfaces::teg::{
    CirculatorData, ThermoElectricGeneratorData, ThermoElectricGeneratorUi,
};
use crate::interfaces::text_input::{TextInputStatic, TextInputUi};
use crate::interfaces::thermo_machine::{ThermoMachineData, ThermoMachineUi};
use crate::interfaces::vending::{
    ProductRecord, StockItem, UserData, VendingData, VendingStatic, VendingUi,
};
use crate::interfaces::wires::{WireEntry, WiresData, WiresUi};
use crate::{UiInterface, encode};

/// A view's sample: window title, static data, data.
pub struct Sample {
    pub title: String,
    pub static_data: Vec<u8>,
    pub data: Vec<u8>,
}

fn of<T: UiInterface>(title: &str, static_data: &T::StaticData, data: &T::Data) -> Sample {
    Sample {
        title: title.to_owned(),
        static_data: encode(static_data),
        data: encode(data),
    }
}

fn gas(id: &str, name: &str, enabled: bool) -> GasToggle {
    GasToggle {
        gas_id: id.into(),
        gas_name: name.into(),
        enabled,
    }
}

fn gas_list() -> Vec<GasToggle> {
    vec![
        gas("o2", "Oxygen", false),
        gas("n2", "Nitrogen", false),
        gas("co2", "Carbon Dioxide", true),
        gas("plasma", "Plasma", true),
        gas("n2o", "Nitrous Oxide", true),
        gas("water_vapor", "Water Vapor", true),
        gas("bz", "BZ", false),
        gas("tritium", "Tritium", true),
    ]
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

fn floor_mix() -> Gasmix {
    Gasmix {
        name: "Floor".into(),
        gases: vec![
            ("o2".into(), "Oxygen".into(), 21.82),
            ("n2".into(), "Nitrogen".into(), 82.09),
            ("plasma".into(), "Plasma".into(), 5.0),
        ],
        temperature: 293.15,
        volume: 2500.0,
        pressure: 106.4,
        total_moles: 108.91,
        reactions: vec![("plasmafire".into(), "Plasma Combustion".into(), 0.4)],
        reference: "1:0".into(),
    }
}

fn handbook() -> AtmosHandbook {
    let entry = |id: &str, name: &str, description: &str, specific_heat: f32| HandbookGas {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        specific_heat,
        export_value: 0.0,
        reactions: Vec::new(),
    };
    AtmosHandbook {
        gas_info: vec![
            entry("o2", "Oxygen", "Нужен для дыхания и горения.", 20.0),
            entry("n2", "Nitrogen", "Инертный газ, основа воздуха.", 20.0),
            entry("plasma", "Plasma", "Горючий и токсичный газ.", 200.0),
            entry("bz", "Bz", "Образец для прокрутки.", 20.0),
            entry("freon", "Freon", "Образец для прокрутки.", 20.0),
            entry("halon", "Halon", "Образец для прокрутки.", 20.0),
            entry("healium", "Healium", "Образец для прокрутки.", 20.0),
            entry("helium", "Helium", "Образец для прокрутки.", 20.0),
            entry("hydrogen", "Hydrogen", "Образец для прокрутки.", 20.0),
            entry("nitrium", "Nitrium", "Образец для прокрутки.", 20.0),
            entry("pluoxium", "Pluoxium", "Образец для прокрутки.", 20.0),
            entry(
                "proto_nitrate",
                "Proto_nitrate",
                "Образец для прокрутки.",
                20.0,
            ),
            entry("tritium", "Tritium", "Образец для прокрутки.", 20.0),
            entry("water_vapor", "Water_vapor", "Образец для прокрутки.", 20.0),
            entry("zauker", "Zauker", "Образец для прокрутки.", 20.0),
        ],
        reaction_info: Vec::new(),
        money_symbol: "cr".into(),
        money_name: "credits".into(),
    }
}

fn timer() -> PromptTimer {
    PromptTimer {
        timeout: Some(30.0),
    }
}

/// The sample of the view named `name` (`UiInterface::NAME`), `None` when it has none.
pub fn sample(name: &str) -> Option<Sample> {
    Some(match name {
        AtmosPumpUi::NAME => of::<AtmosPumpUi>(
            "Gas pump",
            &(),
            &AtmosPumpData {
                on: true,
                rate: 0.0,
                max_rate: 0.0,
                pressure: 101.325,
                max_pressure: 4500.0,
            },
        ),
        AtmosTempGateUi::NAME => of::<AtmosTempGateUi>(
            "Temperature gate",
            &(),
            &AtmosTempGateData {
                on: true,
                temperature: 293.15,
                min_temperature: 2.7,
                max_temperature: 4500.0,
            },
        ),
        AtmosTempPumpUi::NAME => of::<AtmosTempPumpUi>(
            "Temperature pump",
            &(),
            &AtmosTempPumpData {
                on: false,
                rate: 50.0,
                max_heat_transfer_rate: 100.0,
            },
        ),
        AtmosFilterUi::NAME => of::<AtmosFilterUi>(
            "Gas filter",
            &(),
            &AtmosFilterData {
                on: true,
                rate: 200.0,
                max_rate: 200.0,
                filter_types: gas_list(),
            },
        ),
        AtmosMixerUi::NAME => of::<AtmosMixerUi>(
            "Gas mixer",
            &(),
            &AtmosMixerData {
                on: true,
                set_pressure: 4500.0,
                max_pressure: 4500.0,
                node1_concentration: 21.0,
                node2_concentration: 79.0,
            },
        ),
        CanisterUi::NAME => of::<CanisterUi>(
            "Plasma canister",
            &CanisterStatic {
                default_release_pressure: 101.0,
                min_release_pressure: 10.0,
                max_release_pressure: 2533.0,
                pressure_limit: 546_000.0,
                holding_tank_leak_pressure: 3040.0,
                holding_tank_frag_pressure: 4053.0,
            },
            &CanisterData {
                port_connected: true,
                tank_pressure: 4560.0,
                release_pressure: 101.0,
                valve_open: false,
                holding_tank: Some(HoldingTank {
                    name: "plasma tank".into(),
                    pressure: 1013.0,
                }),
                shielding: false,
                cell_charge: 100.0,
                has_hypernob_crystal: true,
                reaction_suppression_enabled: false,
            },
        ),
        PortablePumpUi::NAME => of::<PortablePumpUi>(
            "Portable air pump",
            &(),
            &PortablePumpData {
                on: true,
                direction: false,
                connected: false,
                pressure: 1520.0,
                target_pressure: 101.325,
                default_pressure: 101.325,
                min_pressure: 10.1325,
                max_pressure: 2533.125,
                holding: None,
                has_hypernob_crystal: false,
                reaction_suppression_enabled: false,
            },
        ),
        PortableScrubberUi::NAME => of::<PortableScrubberUi>(
            "Portable air scrubber",
            &(),
            &PortableScrubberData {
                on: true,
                connected: false,
                pressure: 340.0,
                filter_types: gas_list(),
                holding: Some(HoldingTank {
                    name: "emergency oxygen tank".into(),
                    pressure: 0.0,
                }),
                has_hypernob_crystal: false,
                reaction_suppression_enabled: false,
            },
        ),
        PipeScrubberUi::NAME => of::<PipeScrubberUi>(
            "Pipe scrubber",
            &PipeScrubberStatic {
                pressure_limit_pump: 500_000.0,
                pressure_limit_tank: 563_000.0,
            },
            &PipeScrubberData {
                on: true,
                direction: true,
                connected: true,
                pressure_tank: 12_400.0,
                pressure_pump: 180.0,
                has_hypernob_crystal: true,
                reaction_suppression_enabled: true,
                filter_types: gas_list(),
            },
        ),
        PipeDispenserUi::NAME => of::<PipeDispenserUi>(
            "Pipe dispenser",
            &pipe_dispenser_static(),
            &PipeDispenserData {
                piping_layer: 3,
                selected_color: "green".into(),
                init_directions: InitDirections::from_bits(1 | 2 | 4),
            },
        ),
        RapidPipeDispenserUi::NAME => of::<RapidPipeDispenserUi>(
            "rapid pipe dispenser (RPD)",
            &rpd_static(),
            &RpdData {
                category: 0,
                mode: 1 | 2 | 4 | 8,
                multi_layer: false,
                pipe_layers: 1 << 2,
                selected_category: "Binary".into(),
                selected_recipe: 1,
                p_dir: 4,
                selected_color: "blue".into(),
                init_directions: InitDirections::from_bits(15),
            },
        ),
        ThermoMachineUi::NAME => of::<ThermoMachineUi>(
            "Temperature control unit",
            &(),
            &ThermoMachineData {
                on: true,
                min: 73.15,
                max: 573.15,
                target: 73.15,
                initial: 293.15,
                temperature: 154.3,
                pressure: 1830.0,
            },
        ),
        GasAnalyzerUi::NAME => of::<GasAnalyzerUi>(
            "Gas analyzer",
            &handbook(),
            &GasAnalyzerData {
                gasmixes: vec![floor_mix()],
            },
        ),
        HolodeckUi::NAME => of::<HolodeckUi>(
            "holodeck control console",
            &(),
            &HolodeckData {
                default_programs: vec![
                    HoloProgram {
                        id: "holodeck_offline".into(),
                        name: "Holodeck - Offline".into(),
                    },
                    HoloProgram {
                        id: "holodeck_beach".into(),
                        name: "Holodeck - Beach".into(),
                    },
                ],
                emagged: true,
                emag_programs: vec![HoloProgram {
                    id: "holodeck_burntest".into(),
                    name: "Holodeck - Atmospheric Burn Test".into(),
                }],
                program: "holodeck_beach".into(),
                can_toggle_safety: false,
            },
        ),
        AtmosAlertUi::NAME => of::<AtmosAlertUi>(
            "Atmospheric alert console",
            &(),
            &AtmosAlertData {
                priority: strings(&["Toxins Storage", "Engine Room"]),
                minor: strings(&["Departure Lounge", "Chemical Storage"]),
            },
        ),
        AtmosControlUi::NAME => of::<AtmosControlUi>(
            "Plasma Supply Control",
            &AtmosControlStatic {
                max_input: 1000.0,
                max_output: 4500.0,
                control: true,
                reconnecting: false,
                default_gas: Some("plasma".into()),
                handbook: handbook(),
            },
            &AtmosControlData {
                chambers: vec![Chamber {
                    id: "plasma".into(),
                    name: "Plasma Supply".into(),
                    gasmix: Some(floor_mix()),
                    input_info: Some(PortInfo {
                        active: true,
                        amount: 200.0,
                    }),
                    output_info: Some(PortInfo {
                        active: false,
                        amount: 4500.0,
                    }),
                }],
            },
        ),
        AirAlarmUi::NAME => air_alarm(),
        SolarControlUi::NAME => of::<SolarControlUi>(
            "Solar panel control",
            &(),
            &SolarControlData {
                supply: 34_500.0,
                capacity: 64_000.0,
                azimuth_current: 135.0,
                azimuth_rate: 1.0,
                max_rotation_rate: 10.0,
                tracking_state: 2,
                connected_panels: 32,
                connected_tracker: true,
                history_supply: (0..20).map(|i| 20_000.0 + 800.0 * i as f32).collect(),
                history_capacity: vec![64_000.0; 20],
            },
        ),
        PowerMonitorUi::NAME => of::<PowerMonitorUi>(
            "power monitoring console",
            &(),
            &PowerMonitorData {
                stored: 60,
                interval: 5.0,
                attached: true,
                history_supply: (0..30).map(|i| 400_000.0 + 2_000.0 * i as f32).collect(),
                history_demand: (0..30).map(|i| 250_000.0 + 1_500.0 * i as f32).collect(),
                areas: vec![
                    PowerMonitorArea {
                        name: "Bridge".into(),
                        charge: 100.0,
                        load: "3.42 kW".into(),
                        charging: 2,
                        eqp: 3,
                        lgt: 3,
                        env: 3,
                    },
                    PowerMonitorArea {
                        name: "Medbay".into(),
                        charge: 41.0,
                        load: "8.10 kW".into(),
                        charging: 1,
                        eqp: 1,
                        lgt: 3,
                        env: 2,
                    },
                ],
            },
        ),
        ThermoElectricGeneratorUi::NAME => of::<ThermoElectricGeneratorUi>(
            "thermoelectric generator",
            &(),
            &ThermoElectricGeneratorData {
                error_message: None,
                last_power_output: 245_000.0,
                cold_data: Some(CirculatorData {
                    temperature_inlet: 73.2,
                    temperature_outlet: 312.8,
                    pressure_inlet: 4_520.0,
                    pressure_outlet: 4_490.6,
                }),
                hot_data: Some(CirculatorData {
                    temperature_inlet: 1_830.0,
                    temperature_outlet: 640.4,
                    pressure_inlet: 3_010.0,
                    pressure_outlet: 2_987.3,
                }),
            },
        ),
        MechBayPowerConsoleUi::NAME => of::<MechBayPowerConsoleUi>(
            "Mech bay power control console",
            &(),
            &MechBayData {
                recharge_port: Some(RechargePort {
                    mech: Some(BayMech {
                        health: 160.0,
                        maxhealth: 200.0,
                        cell: Some(BayCell {
                            charge: 7_500.0,
                            maxcharge: 10_000.0,
                        }),
                        name: "APLU \"Ripley\"".into(),
                    }),
                }),
            },
        ),
        AirlockElectronicsUi::NAME => of::<AirlockElectronicsUi>(
            "airlock electronics",
            &AirlockElectronicsStatic {
                regions: vec![AccessRegion {
                    name: "Engineering".into(),
                    accesses: [
                        ("Engineering", "engineering"),
                        ("Maintenance", "maint_tunnels"),
                    ]
                    .into_iter()
                    .map(|(desc, access)| RegionAccess {
                        desc: desc.into(),
                        access: access.into(),
                    })
                    .collect(),
                }],
            },
            &AirlockElectronicsData {
                accesses: vec!["engineering".into()],
                one_access: false,
                unres_direction: 2,
                passed_name: "Engine Room".into(),
                shell: false,
            },
        ),
        AiAirlockUi::NAME => of::<AiAirlockUi>(
            "Airlock control",
            &(),
            &AiAirlockData {
                power: AirlockPower {
                    main: 2,
                    main_timeleft: 0,
                    backup: 0,
                    backup_timeleft: 12,
                },
                shock: 2,
                shock_timeleft: 0,
                id_scanner: true,
                emergency: false,
                locked: true,
                feedback: true,
                safe: true,
                speed: true,
                welded: false,
                opened: false,
                wires: AirlockWires {
                    main_1: true,
                    main_2: true,
                    backup_1: false,
                    backup_2: true,
                    shock: true,
                    id_scanner: true,
                    bolts: true,
                    feedback: true,
                    safe: true,
                    timing: true,
                },
            },
        ),
        ApcUi::NAME => apc(),
        WiresUi::NAME => wires(),
        VendingUi::NAME => vending(),
        JobSelectionUi::NAME => job_selection(),
        DisposalUnitUi::NAME => of::<DisposalUnitUi>(
            "Disposal unit",
            &(),
            &DisposalUnitData {
                flush: false,
                full_pressure: false,
                pressure_charging: true,
                panel_open: false,
                per: 64.0,
                isai: false,
            },
        ),
        DnaConsoleUi::NAME => dna_console(),
        CrystallizerUi::NAME => crystallizer(),
        SmartVendUi::NAME => of::<SmartVendUi>(
            "smartfridge",
            &(),
            &SmartVendData {
                contents: [("Apple", 3), ("Banana", 1), ("Carrot", 12)]
                    .iter()
                    .map(|(name, amount)| FridgeItem {
                        path: format!("/obj/item/food/grown/{}-{name}", name.to_lowercase()),
                        name: (*name).into(),
                        amount: *amount,
                        icon: None,
                    })
                    .collect(),
                name: "smartfridge".into(),
                isdryer: false,
                drying: false,
            },
        ),
        ChemDispenserUi::NAME => of::<ChemDispenserUi>(
            "soda dispenser",
            &(),
            &ChemDispenserData {
                amount: 10.0,
                energy: 9_000_000.0,
                max_energy: 10_000_000.0,
                displayed_units: 90_000.0,
                displayed_max_units: 100_000.0,
                show_ph: false,
                has_beaker_in_hand: false,
                chemicals: [
                    ("Coffee", "#482000"),
                    ("Cream", "#DFD7AF"),
                    ("Ice", "#619494"),
                    ("Space Cola", "#100800"),
                    ("Tonic Water", "#0064C8"),
                    ("Water", "#AAAAAA77"),
                ]
                .iter()
                .map(|(name, color)| DispensableReagent {
                    title: (*name).into(),
                    id: (*name).into(),
                    ph: 7.0,
                    color: (*color).into(),
                    ph_col: "green".into(),
                })
                .collect(),
                beaker: Some(DispenserBeaker {
                    max_volume: 50.0,
                    transfer_amounts: vec![5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 50.0],
                    ph: 7.0,
                    current_volume: 20.0,
                    contents: vec![("Space Cola".into(), 10.0), ("Ice".into(), 10.0)],
                }),
                recipes: vec!["Cuba Libre".into(), "Iced Coffee".into()],
                recording_recipe: None,
            },
        ),
        ChemMasterUi::NAME => chem_master(),
        ChemHeaterUi::NAME => of::<ChemHeaterUi>(
            "reaction chamber",
            &(),
            &ChemHeaterData {
                target_temp: 450.0,
                is_active: true,
                upgrade_level: 2.0,
                has_beaker_in_hand: false,
                beaker: Some(HeaterBeaker {
                    max_volume: 100.0,
                    ph: 6.4,
                    current_volume: 40.0,
                    contents: vec![("Welding Fuel".into(), 25.0), ("Oxygen".into(), 15.0)],
                }),
                current_temp: 431.0,
                active_reactions: vec![ActiveReaction {
                    name: "Napalm".into(),
                    quality: 0.85,
                    inverse: 0.25,
                    min_pure: 0.0,
                    reacted_vol: 6.0,
                    target_vol: 15.0,
                    ..Default::default()
                }],
                is_flashing: false,
                acidic_buffer_vol: 20.0,
                basic_buffer_vol: 0.0,
                dispense_volume: 1.0,
            },
        ),
        SmesUi::NAME => of::<SmesUi>(
            "power storage unit",
            &SmesStatic {
                capacity: 5.0e7,
                input_level_max: 200_000.0,
                output_level_max: 200_000.0,
            },
            &SmesData {
                charge: 2.5e7,
                input_attempt: true,
                inputting: true,
                input_level: 50_000.0,
                input_available: 120_000.0,
                output_attempt: true,
                output_level: 90_000.0,
                output_used: 60_000.0,
                outputting: true,
            },
        ),
        SupermatterUi::NAME => of::<SupermatterUi>(
            "Supermatter",
            &(),
            &SmEntry {
                uid: 1,
                area_name: "Supermatter Engine".into(),
                integrity: 98.4,
                integrity_factors: vec![SmFactor {
                    name: "Термическое повреждение".into(),
                    amount: -0.02,
                }],
                internal_energy: 1450.0,
                internal_energy_factors: vec![SmFactor {
                    name: "Внутреннее снижение мощности".into(),
                    amount: -23.5,
                }],
                zap_transmission: 1.6e6,
                zap_transmission_factors: vec![SmFactor {
                    name: "Базовая передача импульса".into(),
                    amount: 1.5e6,
                }],
                temp_limit: 313.15,
                temp_limit_factors: vec![SmFactor {
                    name: "Базовая термостойкость".into(),
                    amount: 313.15,
                }],
                waste_multiplier: 1.0,
                waste_multiplier_factors: vec![SmFactor {
                    name: "Множитель базовых отходов".into(),
                    amount: 1.0,
                }],
                absorbed_ratio: 0.15,
                gas_composition: vec![SmGasShare {
                    name: "Nitrogen".into(),
                    color: [255, 255, 0],
                    amount: 1.0,
                }],
                gas_temperature: 290.0,
                gas_total_moles: 420.0,
            },
        ),
        PortagravUi::NAME => of::<PortagravUi>(
            "Portable Gravity Unit",
            &(),
            &PortagravData {
                percentage: Some(87.0),
                gravity: 1.0,
                range: 4,
                maxrange: 6,
                on: true,
                wiremode: false,
                draw: 4000.0,
            },
        ),
        GravityGeneratorUi::NAME => of::<GravityGeneratorUi>(
            "gravitational generator",
            &(),
            &GravityGeneratorData {
                breaker: true,
                charge_count: 64,
                charging_state: 1,
                on: false,
                operational: true,
            },
        ),
        PortableGeneratorUi::NAME => of::<PortableGeneratorUi>(
            "P.A.C.M.A.N.-type portable generator",
            &(),
            &PortableGeneratorData {
                active: true,
                sheet_name: "Plasma".into(),
                sheets: 15,
                stack_percent: 42.5,
                anchored: true,
                connected: true,
                ready_to_boot: true,
                power_generated: 10_000.0,
                power_output: 20_000.0,
                power_available: 150_000.0,
                current_heat: 76.0,
            },
        ),
        ElectrolyzerUi::NAME => of::<ElectrolyzerUi>(
            "electrolyzer",
            &(),
            &ElectrolyzerData {
                open: true,
                on: true,
                has_powercell: true,
                anchored: false,
                power_level: Some(47.0),
            },
        ),
        SpaceHeaterUi::NAME => of::<SpaceHeaterUi>(
            "space heater",
            &(),
            &SpaceHeaterData {
                open: true,
                on: true,
                mode: "auto".into(),
                has_powercell: true,
                power_level: Some(73.0),
                target_temp: 20.0,
                min_temp: 0.0,
                max_temp: 60.0,
                current_temp: Some(14.0),
            },
        ),
        ImplantChairUi::NAME => of::<ImplantChairUi>(
            "mindshield implanter",
            &(),
            &ImplantChairData {
                occupied: true,
                open: false,
                occupant: Some(ImplantChairOccupant {
                    name: "Robert Smith".into(),
                    stat: 0,
                }),
                special_name: None,
                ready_implants: 3,
                ready: true,
                replenishing: false,
            },
        ),
        ImplantPadUi::NAME => of::<ImplantPadUi>(
            "implant pad",
            &(),
            &ImplantPadData {
                has_case: true,
                has_implant: true,
                case_information: "<b>Implant Specifications:</b><br>Name: Tracking Implant".into(),
                saved_deathrattle_group: None,
                current_deathrattle_group: None,
            },
        ),
        TankUi::NAME => of::<TankUi>(
            "oxygen tank",
            &TankStatic {
                default_release_pressure: 16.0,
                min_release_pressure: 0.0,
                max_release_pressure: 304.0,
                leak_pressure: 3040.0,
                fragment_pressure: 4053.0,
            },
            &TankData {
                tank_pressure: 1013.0,
                release_pressure: 16.0,
                connected: true,
            },
        ),
        DestinationTaggerUi::NAME => of::<DestinationTaggerUi>(
            "Destination tagger",
            &DestinationTaggerStatic {
                locations: strings(&[
                    "Disposals",
                    "Cargo Bay",
                    "QM Office",
                    "Engineering",
                    "CE Office",
                    "Atmospherics",
                    "Security",
                    "HoS Office",
                    "Medbay",
                    "CMO Office",
                ]),
            },
            &DestinationTaggerData { current_tag: 3 },
        ),
        GpsUi::NAME => of::<GpsUi>(
            "global positioning system (COM0)",
            &(),
            &GpsData {
                power: true,
                tag: "COM0".into(),
                updating: true,
                globalmode: true,
                current_area: "Bridge".into(),
                current_coords: "128, 112, 2".into(),
                signals: vec![
                    GpsSignal {
                        entrytag: "ENG0".into(),
                        coords: "150, 98, 2".into(),
                        dist: Some(26),
                        degrees: Some(122.0),
                    },
                    GpsSignal {
                        entrytag: "MINE0".into(),
                        coords: "80, 140, 5".into(),
                        dist: None,
                        degrees: None,
                    },
                ],
            },
        ),
        AlertUi::NAME => of::<AlertUi>(
            "Вопрос",
            &AlertStatic {
                message: "Вы уверены, что хотите наблюдать? Вы не сможете играть в этом раунде!"
                    .into(),
                buttons: strings(&["Да", "Нет"]),
                autofocus: true,
                large_buttons: false,
                swapped_buttons: true,
            },
            &timer(),
        ),
        TextInputUi::NAME => of::<TextInputUi>(
            "Вопрос",
            &TextInputStatic {
                message: "Сообщение для остальных служителей".into(),
                placeholder: "Слава Геометру".into(),
                max_length: 1024,
                ..Default::default()
            },
            &timer(),
        ),
        NumberInputUi::NAME => of::<NumberInputUi>(
            "Вопрос",
            &NumberInputStatic {
                message: "Сколько кредитов снять?".into(),
                init_value: 100.0,
                min_value: 0.0,
                max_value: 500.0,
                round_value: true,
                swapped_buttons: true,
                ..Default::default()
            },
            &timer(),
        ),
        ListInputUi::NAME => of::<ListInputUi>(
            "Вопрос",
            &(),
            &ListInputData {
                title: String::new(),
                message: "Выберите руну".into(),
                init_value: "Руна предложения".into(),
                items: strings(&[
                    "Руна предложения",
                    "Руна усиления",
                    "Руна телепортации",
                    "Руна воскрешения",
                    "Руна барьера",
                    "Руна призыва",
                ]),
                timeout: Some(30.0),
            },
        ),
        CheckboxInputUi::NAME => of::<CheckboxInputUi>(
            "Вопрос",
            &CheckboxInputStatic {
                message: "Какие отделы оповестить?".into(),
                items: strings(&["Инженерия", "Медицина", "Наука", "Охрана", "Снабжение"]),
                min_checked: 1,
                max_checked: 3,
                swapped_buttons: true,
                ..Default::default()
            },
            &timer(),
        ),
        KeyComboUi::NAME => of::<KeyComboUi>(
            "Вопрос",
            &KeyComboStatic {
                message: "Нажмите новую клавишу для «Говорить»".into(),
                init_value: "T".into(),
                swapped_buttons: true,
                ..Default::default()
            },
            &timer(),
        ),
        ColorPickerUi::NAME => of::<ColorPickerUi>(
            "Вопрос",
            &ColorPickerStatic {
                message: "Выберите цвет волос".into(),
                default_color: "#a0522d".into(),
                autofocus: true,
                swapped_buttons: true,
                ..Default::default()
            },
            &timer(),
        ),
        IvDripUi::NAME => of::<IvDripUi>(
            "IV drip",
            &(),
            &IvDripData {
                has_container: true,
                can_remove_container: true,
                mode: true,
                can_draw: true,
                transfer_rate: 5.0,
                transfer_step: 0.01,
                min_transfer_rate: 0.0,
                max_transfer_rate: 5.0,
                has_object_attached: true,
                object_name: "Иван Петров".into(),
                container_reagent_color: Some([0xC8, 0x00, 0x00, 0xFF]),
                container_current_volume: 150.0,
                container_max_volume: 200.0,
                ..Default::default()
            },
        ),
        SleeperUi::NAME => of::<SleeperUi>(
            "sleeper",
            &(),
            &SleeperData {
                occupied: true,
                open: false,
                chems: [
                    ("Epinephrine", true),
                    ("Morphine", true),
                    ("Convermol", false),
                    ("Libital", true),
                    ("Aiuri", true),
                ]
                .iter()
                .map(|(name, allowed)| SleeperChem {
                    name: (*name).into(),
                    id: format!("/datum/reagent/medicine/{}", name.to_lowercase()),
                    allowed: *allowed,
                })
                .collect(),
                occupant: Some(SleeperOccupant {
                    name: "Иван Петров".into(),
                    stat: "Critical".into(),
                    statstate: "average".into(),
                    health: -12.0,
                    max_health: 100.0,
                    min_health: -100.0,
                    brute_loss: 64.0,
                    oxy_loss: 30.0,
                    tox_loss: 3.0,
                    fire_loss: 15.0,
                    brain_loss: 0.0,
                    reagents: vec![("Epinephrine".into(), 10.0)],
                }),
            },
        ),
        CryoUi::NAME => of::<CryoUi>(
            "криокапсула",
            &(),
            &CryoData {
                is_operating: true,
                is_open: false,
                auto_eject: true,
                occupant: Some(CryoOccupant {
                    name: "Иван Петров".into(),
                    stat: "Unconscious".into(),
                    body_temperature: 182.0,
                    health: 12.0,
                    max_health: 100.0,
                    brute_loss: 55.0,
                    oxy_loss: 20.0,
                    tox_loss: 3.0,
                    fire_loss: 10.0,
                }),
                cell_temperature: 73.0,
                beaker: Some(CryoBeaker {
                    max_volume: 50.0,
                    current_volume: 48.5,
                    contents: vec![("Cryoxadone".into(), 48.5)],
                }),
            },
        ),
        MedicalKioskUi::NAME => of::<MedicalKioskUi>(
            "медицинский киоск",
            &(),
            &MedicalKioskData {
                kiosk_cost: 10.0,
                patient_name: "Иван Петров".into(),
                patient_health: 62.0,
                brute_health: 25.0,
                burn_health: 8.0,
                toxin_health: 5.0,
                suffocation_health: 0.0,
                brain_health: "Brain patterns normal.".into(),
                patient_status: "Injured".into(),
                trauma_status: "Patient is free of unique brain trauma.".into(),
                patient_illness: "Patient does not show signs of disease.".into(),
                illness_info: "Not Applicable.".into(),
                bleed_status: "Patient is not currently bleeding.".into(),
                blood_name: "Blood".into(),
                blood_levels: 96.0,
                blood_status: "Patient blood levels are currently reading 96%.".into(),
                chemical_list: vec![("Libital".into(), 4.5)],
                hallucinating_status: "Patient is not hallucinating.".into(),
                active: [true, true, false, false],
                ..Default::default()
            },
        ),
        LimbgrowerUi::NAME => of::<LimbgrowerUi>(
            "Limb Grower",
            &LimbgrowerStatic {
                categories: ["human", "lizard", "moth"]
                    .iter()
                    .map(|&c| LimbgrowerCategory {
                        name: c.into(),
                        designs: ["Left Arm", "Right Arm", "Heart"]
                            .iter()
                            .map(|&n| LimbgrowerDesign {
                                parent_category: c.into(),
                                name: n.into(),
                                path: format!("/datum/design/{}", n.to_lowercase()),
                                needed_reagents: vec![("Synthflesh".into(), 25.0)],
                            })
                            .collect(),
                    })
                    .collect(),
            },
            &LimbgrowerData {
                reagents: vec![LimbgrowerReagent {
                    reagent_name: "Synthflesh".into(),
                    reagent_amount: 60.0,
                    reagent_type: "/datum/reagent/medicine/c2/synthflesh".into(),
                }],
                total_reagents: 60.0,
                max_reagents: 100.0,
                busy: false,
            },
        ),
        AirlockButtonControllerUi::NAME => of::<AirlockButtonControllerUi>(
            "Virology Access Console",
            &(),
            &AirlockButtonControllerData {
                interior_door: true,
                exterior_door: true,
                interior_door_closed: true,
                exterior_door_closed: false,
                busy: false,
            },
        ),
        AirlockControllerUi::NAME => of::<AirlockControllerUi>(
            "incinerator access console",
            &(),
            &AirlockControllerData {
                airlock_state: "depressurize".into(),
                sensor_pressure: Some(52.4),
                pump_status: "siphon".into(),
                interior_status: "closed".into(),
                exterior_status: "closed".into(),
            },
        ),
        _ => return None,
    })
}

fn air_alarm() -> Sample {
    let env = |name: &str, value: &str, danger: Option<u8>| EnvEntry {
        name: name.into(),
        value: value.into(),
        danger,
    };
    let tlv = |id: &str, name: &str, unit: &str, limits: [f32; 4]| TlvEntry {
        id: id.into(),
        name: name.into(),
        unit: unit.into(),
        hazard_min: limits[0],
        warning_min: limits[1],
        warning_max: limits[2],
        hazard_max: limits[3],
    };
    let mode = |name: &str, desc: &str, danger: bool, path: &str| ModeEntry {
        name: name.into(),
        desc: desc.into(),
        danger,
        path: path.into(),
    };
    of::<AirAlarmUi>(
        "Air alarm",
        &(),
        &AirAlarmData {
            locked: false,
            silicon_user: false,
            emagged: false,
            danger_level: 1,
            atmos_alarm: false,
            fire_alarm: false,
            sensor: true,
            allow_link_change: true,
            env_data: vec![
                env("Linked area", "Ordnance Freezer Chamber", None),
                env("Pressure", "98.4 kPa", Some(0)),
                env("Temperature", "293.15 K (20 C)", Some(0)),
                env("Oxygen", "20.9%", Some(0)),
                env("Nitrogen", "78.1%", Some(0)),
                env("Carbon Dioxide", "1.0%", Some(1)),
            ],
            tlv_settings: vec![
                tlv("pressure", "Pressure", "kPa", [80.0, 92.0, 120.0, 140.0]),
                tlv(
                    "temperature",
                    "Temperature",
                    "K",
                    [263.15, 273.15, 313.15, 333.15],
                ),
                tlv("o2", "Oxygen", "kPa", [16.0, 19.0, 135.0, 140.0]),
                tlv("co2", "Carbon Dioxide", "kPa", [-1.0, -1.0, 5.0, 10.0]),
            ],
            controls: Some(AlarmControls {
                vents: vec![VentEntry {
                    ref_id: "1".into(),
                    long_name: "Air Vent #1".into(),
                    power: true,
                    overclock: false,
                    integrity: 1.0,
                    checks: 1,
                    excheck: true,
                    incheck: false,
                    direction: true,
                    external: 101.325,
                    internal: 0.0,
                    extdefault: true,
                    intdefault: true,
                }],
                scrubbers: vec![ScrubberEntry {
                    ref_id: "2".into(),
                    long_name: "Air Scrubber #1".into(),
                    power: true,
                    scrubbing: true,
                    widenet: false,
                    filter_types: gas_list(),
                }],
                selected_mode_path: "filtering".into(),
                modes: vec![
                    mode("Filtering", "Scrubs out contaminants", false, "filtering"),
                    mode(
                        "Contaminated",
                        "Scrubs out ALL contaminants quickly",
                        false,
                        "contaminated",
                    ),
                    mode(
                        "Panic Siphon",
                        "Siphons air out of the room",
                        true,
                        "siphon",
                    ),
                ],
                panic_siphon_path: "siphon".into(),
                filtering_path: "filtering".into(),
            }),
        },
    )
}

fn apc() -> Sample {
    let channel = |title: &str, load: &str, status: u8, key: &str| PowerChannel {
        title: title.into(),
        power_load: load.into(),
        status,
        key: key.into(),
    };
    of::<ApcUi>(
        "Area power controller",
        &(),
        &ApcData {
            locked: false,
            fail_time: 0,
            is_operating: true,
            external_power: 2,
            power_cell_status: Some(87.0),
            charge_mode: true,
            charging_status: 1,
            charging_power_display: "2.4 kW".into(),
            total_load: "6.1 kW".into(),
            cover_locked: true,
            remote_access: false,
            silicon_user: false,
            malf_status: 0,
            emergency_lights: false,
            nightshift_lights: false,
            disable_nightshift_toggle: false,
            power_channels: vec![
                channel("Equipment", "3.2 kW", 3, "equipment"),
                channel("Lighting", "1.4 kW", 3, "lighting"),
                channel("Environment", "1.5 kW", 3, "environ"),
            ],
        },
    )
}

fn wires() -> Sample {
    let wire = |color: &str, wire: Option<&str>, cut: bool, attached: bool| WireEntry {
        color: color.into(),
        shown_color: color.into(),
        wire: wire.map(String::from),
        cut,
        attached,
    };
    of::<WiresUi>(
        "Airlock wires",
        &(),
        &WiresData {
            proper_name: Some("Airlock".into()),
            wires: vec![
                wire("red", Some("Main power"), false, false),
                wire("blue", None, true, false),
                wire("green", None, false, true),
                wire("yellow", None, false, false),
                wire("orange", Some("Bolts"), false, false),
                wire("purple", None, false, false),
            ],
            status: strings(&[
                "The door bolts have fallen!",
                "The test light is on.",
                "The check wiring light is off.",
            ]),
        },
    )
}

fn vending() -> Sample {
    let product = |name: &str, price: u32, reference: u32, category: &str| ProductRecord {
        path: format!("/obj/item/sample/{reference}"),
        name: name.into(),
        price,
        reference,
        category: category.into(),
        colorable: false,
        premium: false,
        icon: None,
    };
    of::<VendingUi>(
        "NanoMed Plus",
        &VendingStatic {
            ad: "Лекарства — лучше, чем болезни!".into(),
            all_products_free: false,
            department: "Medical".into(),
            job_discount: 0.5,
            displayed_currency_icon: "coins".into(),
            displayed_currency_name: " cr".into(),
            product_records: vec![
                product("Brute patch", 25, 1, "Лечение"),
                product("Burn patch", 25, 2, "Лечение"),
                product("Syringe", 10, 3, "Инструменты"),
                product("Health analyzer", 40, 4, "Инструменты"),
            ],
            coin_records: Vec::new(),
            hidden_records: Vec::new(),
            categories: vec![
                ("Лечение".into(), "suitcase-medical".into()),
                ("Инструменты".into(), "syringe".into()),
            ],
        },
        &VendingData {
            user: Some(UserData {
                name: "Иван Петров".into(),
                cash: None,
                job: "Medical Doctor".into(),
                department: "Medical".into(),
                is_captain_card: false,
            }),
            stock: vec![
                StockItem {
                    amount: 8,
                    free: false,
                },
                StockItem {
                    amount: 0,
                    free: false,
                },
                StockItem {
                    amount: 12,
                    free: true,
                },
                StockItem {
                    amount: 3,
                    free: false,
                },
            ],
            extended_inventory: false,
        },
    )
}

fn job_selection() -> Sample {
    let job = |title: &str, command: bool| JobStatic {
        title: title.into(),
        command,
        description: format!("{title}: описание должности."),
    };
    let slots = |used: u32, open: Option<u32>| JobSlots {
        used_slots: used,
        open_slots: open,
        prioritized: false,
        unavailable_reason: None,
    };
    of::<JobSelectionUi>(
        "Выбор должности",
        &JobSelectionStatic {
            departments: vec![
                DepartmentStatic {
                    name: "Инженерия".into(),
                    color: "#ffa62b".into(),
                    jobs: vec![
                        job("Chief Engineer", true),
                        job("Station Engineer", false),
                        job("Atmospheric Technician", false),
                    ],
                },
                DepartmentStatic {
                    name: "Медицина".into(),
                    color: "#3399ff".into(),
                    jobs: vec![
                        job("Chief Medical Officer", true),
                        job("Medical Doctor", false),
                    ],
                },
            ],
        },
        &JobSelectionData {
            departments: vec![
                DepartmentSlots {
                    open_slots: Some(5),
                    jobs: vec![slots(0, Some(1)), slots(2, Some(5)), slots(1, Some(3))],
                },
                DepartmentSlots {
                    open_slots: Some(4),
                    jobs: vec![slots(1, Some(1)), slots(3, None)],
                },
            ],
            round_duration: "00:42".into(),
            shuttle_status: String::new(),
        },
    )
}

fn dna_console() -> Sample {
    let mutation = |alias: &str, name: &str, discovered: bool, active: bool| DnaMutation {
        alias: alias.into(),
        name: name.into(),
        description: format!("{name}: пример описания мутации."),
        discovered,
        active,
        sequence: "ATCGGCTAATCGGCTA".into(),
        instability: 20,
        quality: 1,
        class: 1,
        extra: false,
    };
    of::<DnaConsoleUi>(
        "DNA console",
        &(),
        &DnaConsoleData {
            is_scanner_connected: true,
            is_open: false,
            is_viable_subject: true,
            subject_name: "Иван Петров".into(),
            subject_status: 0,
            subject_health: 92.0,
            subject_enzymes: "0F3A4C".into(),
            subject_unique_identity: "1A2B3C4D5E6F".into(),
            subject_stability: 85,
            injector_seconds: 0.0,
            mutations: vec![
                mutation("Mut #1", "Telekinesis", true, false),
                mutation("Mut #2", "Unknown", false, false),
                mutation("Mut #3", "Clumsiness", true, true),
            ],
        },
    )
}

/// A short version of `GLOB.atmos_pipe_recipes` and `GLOB.pipe_paint_colors`.
fn crystallizer() -> Sample {
    let recipes = [
        ("Nothing", ""),
        ("Hyper-Noblium Crystal", "hyper_crystalium"),
        ("Metallic Hydrogen", "metal_h"),
        ("Healium Crystal", "healium_g"),
        ("Hot Ice", "hot_ice"),
        ("Diamond", "diamond"),
        ("Solidified Plasma", "plasma_sheet"),
        ("Standard Fuel Pellet", "fuel_basic"),
        ("Nitrium Crystal", "crystallized_nitrium"),
    ];
    let gas = |name: &str, id: &str, amount: f32| InternalGas {
        name: name.into(),
        id: id.into(),
        amount,
    };
    of::<CrystallizerUi>(
        "crystallizer",
        &CrystallizerStatic {
            selected_recipes: recipes
                .iter()
                .map(|(name, id)| RecipeEntry {
                    name: (*name).into(),
                    id: (*id).into(),
                })
                .collect(),
        },
        &CrystallizerData {
            on: true,
            selected: "fuel_basic".into(),
            internal_gas_data: vec![gas("Oxygen", "o2", 61.27), gas("Plasma", "plasma", 118.4)],
            requirements: "To create Standard Fuel Pellet you will need:\n-50 moles of Oxygen\n\
                           -100 moles of Plasma\nIn a temperature range between 2.7 K and 1e+31 \
                           K\nThe crystallization reaction will be endothermic"
                .into(),
            internal_temperature: 412.6,
            progress_bar: 42.0,
            gas_input: 0.5,
        },
    )
}

fn pipe_dispenser_static() -> PipeDispenserStatic {
    let category = |name: &str, recipes: &[&str]| PipeCategory {
        cat_name: name.into(),
        recipes: recipes
            .iter()
            .enumerate()
            .map(|(i, r)| PipeRecipeEntry {
                pipe_name: (*r).into(),
                pipe_index: i as u32 + 1,
                all_layers: true,
                dir: 1,
            })
            .collect(),
    };
    PipeDispenserStatic {
        paint_colors: [
            ("omni", "#EEEEEE"),
            ("green", "#00FF00"),
            ("blue", "#0000FF"),
            ("red", "#FF0000"),
            ("orange", "#FFA62B"),
            ("cyan", "#00FFFF"),
        ]
        .map(|(n, h)| (n.to_owned(), h.to_owned()))
        .to_vec(),
        categories: vec![
            category(
                "Pipes",
                &["Pipe", "Layer Adapter", "Color Adapter", "Bridge Pipe"],
            ),
            category(
                "Binary",
                &["Manual Valve", "Digital Valve", "Pressure Pump"],
            ),
            category(
                "Devices",
                &["Gas Filter", "Gas Mixer", "Connector", "Meter"],
            ),
            category("Heat Exchange", &["Pipe", "Manifold", "Junction"]),
        ],
    }
}

/// Two categories of the RPD's list.
fn rpd_static() -> RpdStatic {
    let preview = |dir: u8, name: &str| RpdPreview {
        dir,
        dir_name: name.into(),
        flipped: false,
    };
    let straight = || vec![preview(1, "Vertical"), preview(4, "Horizontal")];
    RpdStatic {
        paint_colors: pipe_dispenser_static().paint_colors,
        max_pipe_layers: 5,
        categories: vec![
            RpdCategory {
                cat_name: "Pipes".into(),
                recipes: vec![RpdRecipe {
                    pipe_index: 1,
                    pipe_name: "Pipe".into(),
                    previews: vec![preview(2, "Pipe")],
                    ..Default::default()
                }],
            },
            RpdCategory {
                cat_name: "Binary".into(),
                recipes: vec![
                    RpdRecipe {
                        pipe_index: 1,
                        pipe_name: "Manual Valve".into(),
                        pipe_desc: "Blocks the flow of gas through the pipeline.<br>Does not \
                                    require power."
                            .into(),
                        previews: straight(),
                        ..Default::default()
                    },
                    RpdRecipe {
                        pipe_index: 2,
                        pipe_name: "Pressure Pump".into(),
                        previews: [(1, "North"), (2, "South"), (8, "West"), (4, "East")]
                            .map(|(d, n)| preview(d, n))
                            .to_vec(),
                        ..Default::default()
                    },
                ],
            },
        ],
    }
}

fn chem_master() -> Sample {
    let reagent = |name: &str, volume: f32, color: &str| AnalyzableReagent {
        r#ref: format!("/datum/reagent/{}", name.to_lowercase()),
        name: name.into(),
        volume,
        ph: 7.0,
        color: color.into(),
        description: format!("Some {name}."),
        purity: 1.0,
        meta_rate: 0.2,
        overdose: 30.0,
        addiction_types: Vec::new(),
    };
    let container = |r: &str, name: &str, volume: f32| PrintableContainer {
        r#ref: r.into(),
        name: name.into(),
        icon: None,
        volume,
    };
    let categories = vec![
        ContainerCategory {
            name: "tubes".into(),
            containers: vec![container(
                "/obj/item/reagent_containers/cup/tube",
                "tube",
                20.0,
            )],
        },
        ContainerCategory {
            name: "pills".into(),
            containers: (1..=6)
                .map(|i| {
                    container(
                        &format!("/obj/item/reagent_containers/applicator/pill/style/{i}"),
                        "pill",
                        50.0,
                    )
                })
                .collect(),
        },
    ];
    of::<ChemMasterUi>(
        "ChemMaster 3000",
        &ChemMasterStatic {
            max_printable: 50,
            max_pill_duration: 30,
            categories,
        },
        &ChemMasterData {
            selected_pill_duration: 3.0,
            custom_transfer_amount: 15.0,
            beaker: Some(AnalyzableBeaker {
                max_volume: 100.0,
                current_volume: 30.0,
                contents: vec![reagent("Bicaridine", 30.0, "#C8A5DC")],
            }),
            buffer: AnalyzableBeaker {
                max_volume: 100.0,
                current_volume: 20.0,
                contents: vec![reagent("Kelotane", 20.0, "#FFA800")],
            },
            is_transfering: true,
            suggested_container_ref: "/obj/item/reagent_containers/cup/tube".into(),
            selected_container_ref: "/obj/item/reagent_containers/applicator/pill/style/1".into(),
            selected_container_volume: 50.0,
            selected_container_category: Some("pills".into()),
            ..Default::default()
        },
    )
}

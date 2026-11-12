//! Ordered click handler registries of [`crate::inventory`]'s click chain (tg's `attack_self`,
//! `attack_hand`, `item_interaction`/`interact_with_atom` chains).
//!
//! Each module registers its hooks from its own plugin with
//! [`AddClickHandler::add_click_handler`]; the inventory runs a phase's handlers by ascending
//! priority and stops at the first that returns `true` (handled). Nothing handled: the
//! inventory's own fallback (selecting the hand, picking up, placing the item).
//!
//! Priorities are multiples of 100 that reproduce the order of the former single `match`; a new
//! hook takes a free number between its neighbours. Two handlers of one phase never share a
//! priority (registration panics), so the order never depends on plugin order. `core` marks
//! handlers registered by `crate::inventory` itself.
//!
//! [`ClickPhase::AttackSelf`] (the active hand's item clicked, tg `attack_self`):
//!
//! | prio | handler |
//! | ---- | ------- |
//! | 89 | `device_wires::attack_self` (an unscrewed radio shows its wires) |
//! | 100 | `atmos` analyzer scans the user's turf |
//! | 150 | `silicon::mmi::attack_self` |
//! | 160 | `silicon::aicard::attack_self` |
//! | 165 | `silicon::pai::attack_self` (the pAI card's menu) |
//! | 199 | `smoking::attack_self` (lighters before `tools`, the battery lighter, putting out smokables, emptying pipes and vapes) |
//! | 200 | `tools::attack_self` |
//! | 210 | `t_scanner::toggle` |
//! | 220 | `inducer::attack_self` (the cell out of an open hatch) |
//! | 300 | `holosign::attack_self` |
//! | 400 | `forensics::server::attack_self` (scanner window, evidence bag, forensics spoofer window) |
//! | 500 | `antag::cult::server::attack_self` |
//! | 600 | `antag::heretic::server::attack_self` |
//! | 601 | `antag::heretic::server::items::attack_self` (the crimson medallion squeezed, a crucible brew or the ether drunk) |
//! | 700 | `antag::wizard::server::attack_self` |
//! | 800 | `vision::visor_attack_self` |
//! | 893 | `antag::abductor::server::gear` (science tool, advanced baton, mental interface device modes) |
//! | 900 | `security::attack_self` (baton toggles, the electrostaff's wield, the gun cane's safety, flashes) |
//! | 989 | `service::burden` pray refill (the chaplain's revolver) |
//! | 993 | `guns_special::attack_self` (the last syringe out of a syringe gun) |
//! | 997 | `guns::box_attack_self` (a round out of a held ammo box or magazine) |
//! | 1000 | `guns::attack_self` |
//! | 1093 | `food::open_can` (a canned food's tab) |
//! | 1097 | `sprays::attack_self` (stream or spray mode; lets `chem::attack_self` run) |
//! | 1100 | `chem::attack_self` |
//! | 1200 | `medical::attack_self` |
//! | 1300 | `disposals::attack_self` |
//! | 1400 | `cargo::mail::attack_self` |
//! | 1500 | `antag::nukeop::server::attack_self` |
//! | 1600 | `antag::syndie_items::attack_self` |
//! | 1700 | `devices::attack_self` (pinpointers, handheld crew monitor) |
//! | 1710 | `devices::flashlight::toggle_light` |
//! | 1717 | `modsuit::link::scryer_attack_self` (the MODlink scryer's label) |
//! | 1720 | `devices::gps::interact` |
//! | 1730 | `devices::rcd::attack_self` (the design list) |
//! | 1737 | `devices::rhd::attack_self` (RLD, RTD, RDD, RPLD, RWD, RSF lists) |
//! | 1740 | `devices::taperecorder::attack_self` (recorder and tape menus) |
//! | 1743 | `devices::beacons::attack_self` (tracking beacons toggle, choice beacons list) |
//! | 1745 | `science::teleportation::hand_tele::attack_self` (the hand tele's destination list, the locator's window) |
//! | 1747 | `instruments::attack_self` (a held instrument's song editor, the boombox menu) |
//! | 1750 | `radiation::geiger_attack_self` |
//! | 1757 | `internals::open_window` (the tank's window) |
//! | 1753 | `devices::table_clock::interact` (repairing a broken clock) |
//! | 1763 | `devices::broadcast_camera::attack_self` (live or offline) |
//! | 1767 | `clothing::gear::toggle_magboots` |
//! | 1773 | `devices::door_remote::attack_self` (the next mode) |
//! | 1800 | `explosions::sources::c4_attack_self` |
//! | 1897 | `science::relics::attack_self` (a strange object: the hint, or its power) |
//! | 1900 | `assemblies::server::attack_self` |
//! | 1960 | `wiremod::server::attack_self` |
//! | 2000 | `grenades::attack_self` |
//! | 2100 | `tables::attack_self` |
//! | 2200 | `furniture::attack_self` |
//! | 2300 | `atmos_machinery::rpd::attack_self` |
//! | 2400 | `science::experi_scanner::attack_self` |
//! | 2500 | `atmos_machinery::fitting::attack_self` |
//! | 2593 | `modpc::laptop::attack_self` (a closed laptop opens) |
//! | 2600 | `modpc::pda::attack_self` |
//! | 2700 | `radio::attack_self` |
//! | 2800 | `martial::attack_self` |
//! | 2900 | `botany::attack_self` |
//! | 2993 | `fancy::attack_self` (a donut, egg or cigarette box opens, closes or folds) |
//! | 2999 | `service::sparring` contract menu |
//! | 3000 | `service::server::attack_self` |
//! | 3010 | `service::barber::attack_self` (lipstick twist, the dye spray on oneself) |
//! | 3050 | `fishing::server::attack_self` (rods), `fishing::aquarium::attack_self` (fish tanks), `fishing::catalog::attack_self` (the fish catalog) |
//! | 3097 | `plumbing_net::attack_self` (a duct stack's layer) |
//! | 3100 | `silicon::law_rack::attack_self` |
//! | 3150 | `vehicles::attack_self` |
//! | 3155 | `clothing::toggles::attack_self` (adjustable masks) |
//! | 3159 | `clothing::reactive::attack_self` (reactive armor on and off) |
//! | 3160 | `airlock_construction::attack_self` (airlock electronics) |
//! | 3165 | `cargo::mail::counterfeit_attack_self` |
//! | 3166 | `cargo::coupon::cigarettes_attack_self` |
//! | 3183 | `mining::cores::attack_self` (monster cores) |
//! | 3184 | `mining::resonator::attack_self` |
//! | 3185 | `mining::capsule::attack_self` (shelter capsules) |
//! | 3186 | `mining::beacons::attack_self` (marker beacon stacks) |
//! | 3187 | `cargo::money::coin_attack_self` (coin flip) |
//! | 3188 | `mining::fulton::attack_self` (packs link, beacon kits unfold) |
//! | 3189 | `shuttle_remote::use_remote` |
//! | 3190 | `janitor::broom_attack_self` (push broom wield) |
//! | 3192 | `power::lighting::replacer_attack_self` (the fixtures on the user's tile) |
//! | 3194 | `closets::kinds::bodybag_attack_self` (a folded body bag unfolds) |
//! | 3203 | `crayons::attack_self` (the stencil menu, a spray can's cap and colour) |
//! | 3213 | `gifts::open` |
//! | 3221 | `weapons::two_handed::attack_self` (wield, let go) |
//! | 3245 | `mining::crusher::attack_self` (the kinetic crusher wields) |
//! | 3227 | `misc_items::charter::attack_self` (the station name question) |
//! | 3239 | `stack_recipes::attack_self` (a stack's crafting menu) |
//! | 3249 | `sheets::crush_attack_self` (a bluespace crystal crushed: blink) |
//! | 3259 | `traps::attack_self` (a sturdy tripwire cable recoils) |
//! | 3269 | `deployable_turrets::attack_self` (a folded machine gun deploys) |
//! | 3267 | `watertank::nozzle_attack_self` (the firefighter nozzle's modes) |
//! | 3275 | `extinguisher::attack_self` (the safety) |
//! | 3277 | `antag::revenant::scatter_residue` (the glimmering residue scattered by hand) |
//! | 3285 | `recycling::conveyor::attack_self` (a switch assembly links nearby belt stacks) |
//! | 3301 | `antag::wizard::server::runic::attack_self` (the scepter's recharge) |
//! | 3295 | `power::cable_coil::attack_self` (the layer menu) |
//! | 3305 | `legcuffs::attack_self` (a bear trap arms and disarms) |
//! | 3309 | `kitchen_ware::attack_self` (pizza boxes open and close, tongs drop or clack) |
//! | 3319 | `toys::toggle_sword` (a toy sword flicks out and in) |
//! | 3325 | `cargo::black_market::attack_self` (the market uplink's window) |
//! | 3327 | `talking_toys::attack_self` (action figures, pull-string toys, the big red button, the toy nuke) |
//! | 3329 | `brig::sentence::attack_self` (a prisoner ID tells its points) |
//! | 3335 | `implants::pad::attack_self` (the implant pad's window) |
//! | 3345 | `clothing::secgear::attack_self` (a security helmet's straps) |
//! | 4993 | `id_show::show` (an ID card shown to everyone around; after the cards with their own use) |
//!
//! [`ClickPhase::OtherHand`] (the other hand's item clicked; `held` is the active hand's):
//!
//! | prio | handler |
//! | ---- | ------- |
//! | 77 | `science::gizmo` (a gizmo spawned after map load gets its puzzle; never takes the click) |
//! | 89 | `machine_wires::held_on_target` (a screwdriver on a radio, wire tools on an open MOD or radio in the other hand) |
//! | 91 | `modsuit::anomalock::item_interaction` (an anomaly core into a locked MOD module, a screwdriver taking it out) |
//! | 97 | `modsuit` construction, paint and scryer cell (a part, core or tool on a MOD shell, a cell on a scryer, the paint kit or skin applier on a unit in the other hand) |
//! | 100 | `modsuit::server::hand_interaction` |
//! | 150 | `silicon::mmi::item_interaction` |
//! | 200 | `bots::construction::item_interaction` |
//! | 300 | `forensics::server::item_interaction` (scanner, evidence bag, forensics spoofer) |
//! | 400 | `medical::cyberimps::item_interaction` |
//! | 450 | `fishing::server::insert_bait`, `fishing::aquarium::item_interaction`, `fishing::catalog::analyze` (the fish analyzer on a held fish or fish tank) |
//! | 455 | `inducer::item_interaction` (screwdriver, a cell, plasma onto the inducer) |
//! | 457 | `devices::rhd::load` (a cartridge onto the RSF, cable onto the RWD) |
//! | 460 | `devices::rcd::load` (matter or a disk onto the RCD) |
//! | 465 | `devices::taperecorder::item_interaction` (tape in, respool), `attack_hand` (eject) |
//! | 467 | `devices::laser_pointer::item_interaction` (diode, crystal lens, their removal) |
//! | 473 | `cargo::money::item_interaction` (cash onto the ID in the other hand) |
//! | 477 | `instruments::boombox::other_hand` (a music tape into the boombox in the other hand, its menu) |
//! | 479 | `toys::item_on_item` (caps into a cap gun, toy sword tools, filling and bursting balloons, emagging a Mini-Meteor) |
//! | 481 | `silicon::mob_cameras::repair` (cable on a broken bodycam or its jumpsuit in the other hand) |
//! | 480 | `clothing::accessories::attach` (before a jumpsuit's holster takes the item) |
//! | 483 | `gifts::wrap_parcel` (wrapping paper onto a parcel, before it takes the paper) |
//! | 487 | `misc_items::documents::forge_seal` (a red or blue crayon on a photocopy) |
//! | 489 | `smoking::item_on_item` (lighting smokables, packing pipes, rolling paper, striking matches, vape tools, dipping) |
//! | 497 | `talking_toys` emag on a toy nuke in the other hand |
//! | 499 | `kitchen_ware::other_hand` (food onto plates, pizza boxes, tongs; empty hand: food off a plate, pizza or the top box) |
//! | 500 | core: a storage opens or takes the held item |
//! | 600 | `implants::item_interaction` |
//! | 700 | `assemblies::server::item_interaction` |
//! | 760 | `wiremod::server::item_interaction` |
//! | 800 | `grenades::item_interaction` |
//! | 900 | `service::server::item_interaction` |
//! | 901 | `service::cards::item_interaction`, `attack_hand` (empty active hand: draw from the deck) |
//! | 909 | `service::photo::frame_take_photo` (empty active hand: the photo out of a held picture frame) |
//! | 1000 | `antag::uplink::item_interaction` |
//! | 1009 | `brig::sentence::set_sentence` (an ID on the prisoner ID in the other hand: the sentence) |
//! | 1100 | `antag::syndie_items::item_interaction` |
//! | 1150 | `emag::interact_with_atom` on a modular computer (`modpc::pda`) |
//! | 1200 | `modpc::pda::item_interaction` |
//! | 1250 | `radiation::geiger_interact` |
//! | 1255 | `clothing::item_interaction` |
//! | 1259 | `clothing::reactive::shell_interaction` (an anomaly core into a reactive armor shell) |
//! | 1261 | `weapons` whetstone (`sharpener`), shield bash (`block::bash`) |
//! | 1300 | `disposals::mail_interaction` |
//! | 1350 | `botany::grown_interaction` (log cutting, potato batteries) |
//! | 1369 | `sheets::item_interaction` (a hot tool on burning sheets, rods on titanium in the other hand) |
//! | 1385 | `recycling::conveyor::link_stack` (a switch assembly on a belt stack in the other hand) |
//! | 1393 | `guns_special::item_interaction` (a syringe or DNA injector into a syringe gun, a grenade into a grenade launcher) |
//! | 1400 | `chem::item_interaction` |
//! | 1450 | `mining::hand_interaction` (modkits, point cards, gibtonite ore) |
//! | 1477 | `security::item_interaction` (a screwdriver, cell, conversion kit, crystal or diamond onto a baton in the other hand) |
//! | 1491 | `gun_attachments::attach` (a seclite, suppressor, bayonet or screwdriver on the gun or a security helmet) |
//! | 1483 | `gun_modes::try_sawoff` (a saw on the gun) |
//! | 1493 | `devices::table_clock::attackby` (a heavy item smashes the clock) |
//! | 1495 | `clothing::secgear::tool_act` (a screwdriver or wirecutters on a security mask's restrictor) |
//! | 1195 | `modpc::programs::station` (a program disk into the computer in the other hand) |
//! | 1489 | `service::burden` refuses manual reloads of the chaplain's revolver |
//! | 1487 | `gun_pins` (a firing pin into a gun, the DNA pin keyed on a carbon; in the other hand also the tools that take a pin out) |
//! | 1497 | `guns::box_interaction` (rounds into the other hand's ammo box or magazine) |
//! | 1500 | `guns::item_interaction` |
//! | 1600 | `guns::attack_hand` (empty active hand) |
//! | 1700 | `construction::split_into_hand` (empty active hand) |
//!
//! [`ClickPhase::EmptyHand`] (a world click with the active hand empty; tg `attack_hand`):
//!
//! | prio | handler |
//! | ---- | ------- |
//! | 3 | `modsuit::ninja` cloak (a cloaked wearer touching something living drops the cloak; never takes the click) |
//! | 9 | `deployable_turrets::gunner_click` (a turret's gunner aims and fires) |
//! | 13 | `antag::raiders::ninja::hack` (the hacker module's right click: drains, hacks, the electric touch) |
//! | 95 | `shuttle_docker::interact` (a navigation computer: take, leave, rotate; its operator's clicks designate the landing spot) |
//! | 97 | `science::gizmo` (a gizmo spawned after map load gets its puzzle; never takes the click) |
//! | 99 | `machine_wires::empty_hand` (an open panel shows the wires) |
//! | 100 | `silicon::interact::attack_robot` |
//! | 200 | `silicon::cyborg::inventory_action` (`EmptyHandClick`) |
//! | 300 | `antag::cult::server::interact` |
//! | 400 | `antag::heretic::server::interact` |
//! | 401 | `antag::heretic::server::items::crucible_hand` (the mawed crucible's brews, a non-heretic's hand bitten) |
//! | 500 | `kitchen::interact` |
//! | 550 | `restaurant::attack_hand` |
//! | 600 | `cooking::interact` |
//! | 700 | `smartfridge::interact` |
//! | 800 | `gibber::interact` |
//! | 900 | `coffee::interact` |
//! | 1000 | `stove::interact` |
//! | 1100 | `barsign::interact` |
//! | 1200 | `icecream::interact` |
//! | 1295 | `chem_machines::interact` (smoke machine, distillation apparatus, HPLC, debug synthesizer, reaction tester, PANDEMIC; right click: a beaker out) |
//! | 1300 | `chem_dispenser::interact` |
//! | 1400 | `chem_master::interact` |
//! | 1500 | `chem_heater::interact` |
//! | 1509 | `eng_machines::manufactorio` storage unit dump (`storagebox/attack_hand`), the sorter's window |
//! | 1510 | `teapot::interact` |
//! | 1600 | `botany::interact` |
//! | 1700 | `multiz::interact` |
//! | 1787 | `science::ordnance_machines::interact` (the `DopplerArray` and `TankCompressor` windows) |
//! | 1790 | `atmos_machinery::space_heater::interact` |
//! | 1800 | `atmos_machinery::windows::interact` |
//! | 1900 | `power::chargers::interact` |
//! | 1950 | `power::pacman::interact` |
//! | 1960 | `power::smes::interact` |
//! | 1970 | `power::shields::interact` |
//! | 1979 | `eng_machines::atmos_shield::interact` (toggle; right click: lock) |
//! | 1989 | `eng_machines::flatpacker::interact` (the `Flatpacker` window) |
//! | 1999 | `eng_machines::modular_shield::interact` (`ModularShieldGen`, `ModularShieldConsole` windows) |
//! | 2000 | `wallmount::interact` |
//! | 2010 | `power::lighting::remove_by_hand` (right click: a bulb out of its fixture) |
//! | 2012 | `power::lighting::floodlight_hand` (setting, the tube out with the panel open) |
//! | 2017 | `misc_items::puzzle::press` (ruin puzzle buttons) |
//! | 2027 | `misc_items::flatpacks::unload` (the topmost flatpack off a cart) |
//! | 2037 | `modsuit::link::remove_cell` (right click: the MODlink scryer's cell out) |
//! | 2100 | `power::apc_ui::interact` (right click by a human: `toggle_lock`) |
//! | 2200 | `mecha::bay::interact` |
//! | 2207 | `mecha::control::interact` (the exosuit control console's window) |
//! | 2300 | `mecha::core::attack_hand` |
//! | 2400 | `modpc::pda::console_interact` |
//! | 2403 | `modpc::laptop::attack_hand_secondary` (right click: open or use a laptop where it lies) |
//! | 2500 | `disposals::interact` |
//! | 2600 | `genetics::console::interact` |
//! | 2650 | `medical::pods::interact` (sleepers, cryo cells, lipid extractors, organ harvesters) |
//! | 2653 | `cryosleep::interact` (the cryo console's window) |
//! | 2660 | `medical::kiosk::interact` |
//! | 2661 | `silicon::camera_advanced::attack_hand` (the advanced camera console's eye) |
//! | 2663 | `science::xenobio_console::take_controls` (the slime console's controls) |
//! | 2673 | `science::cell_lines::attack_hand` (microscope readout and dish removal, vat resampler and flush) |
//! | 2683 | `antag::abductor::server::ship` (abductor console, experimentation machine, gland storage windows) |
//! | 2670 | `medical::limbgrower::interact` |
//! | 2685 | `brig::gulag::interact` (labor camp teleporter, its console, the item reclaimer) |
//! | 2695 | `brig::labor::interact` (claim console, plate press; right click: produceporter) |
//! | 2700 | `tables::attack_hand` |
//! | 2703 | `airlock_controllers::attack_hand` (access buttons and consoles, airlock consoles and sensors) |
//! | 2800 | `falsewall::attack_hand` |
//! | 2810 | `door_structures::mineral_attack_hand` (mineral doors) |
//! | 2900 | `mining::interact` |
//! | 3000 | `holosign::attack_hand` |
//! | 3097 | `plumbing_net::interact` (plumbing machine settings) |
//! | 3100 | `plumbing::interact` |
//! | 3149 | `service::photo::frame_hand` (a wall picture frame shows its photo) |
//! | 3150 | `decor::fold_flag` (right click), `decor::interact` |
//! | 3155 | `tank_storage::attack_hand` (tank dispensers, tank holders) |
//! | 3156 | `displaycase::attack_hand` |
//! | 3160 | `flora::attack_hand` |
//! | 3169 | `safes::interact` (the `Safe` window) |
//! | 3179 | `secure_safes::interact` (the `LockedSafe` keypad, the unlocked storage) |
//! | 3170 | `botany::unique::pickup_backfire` (never handles) |
//! | 3175 | `events::space_vines::attack_hand` (frees the mob tangled in kudzu) |
//! | 3200 | `service::server::interact` |
//! | 3201 | `service::stationery::cutter_right_click` (right click) |
//! | 3211 | `service::cards::attack_hand` (draw from a deck; right click: face up, flip a card) |
//! | 3221 | `service::slots` (the slot machine window) |
//! | 3250 | `fishing::aquarium::interact` (aquariums, the fish-porter), `fishing::mount::interact` (a trophy off its mount) |
//! | 3300 | `defenses::server::interact` |
//! | 3305 | `defenses::turret_build::attack_hand` (a frame's gun or sensor back out) |
//! | 3309 | `forensics::board::attack_hand` (the detective board's window) |
//! | 3400 | `silicon::law_rack::interact` |
//! | 3470 | `station_goals::interact` (satellites) |
//! | 3473 | `devices::beacons::attack_self` (right click toggles a tracking beacon) |
//! | 3475 | `science::teleportation` (teleporter station toggle; teleporter, launchpad, satellite and gateway consoles; away gateways) |
//! | 3485 | `science::teleportation::hand_tele::portal_hand` (through an adjacent portal) |
//! | 3450 | `vehicles::attack_hand` |
//! | 3451 | `antag::wizard::server::ritual::interact` (a wizard invokes a grand rune) |
//! | 3460 | `wiremod::server::attack_hand` |
//! | 3455 | `clothing::sensors::interact` (crew console) |
//! | 3456 | `devices::flashlight::toggle_light` (right click) |
//! | 3457 | `devices::gps::interact` (survival pod computer) |
//! | 3458 | `cargo::mail_sorter::interact` |
//! | 3459 | `cargo::express::interact` |
//! | 3461 | `cargo::materials_market::interact` |
//! | 3462 | `papershredder::empty_hand` (right click) |
//! | 3463 | `silicon::ai_slipper::interact` |
//! | 3464 | `science::bitrunning::avatar::ladder_click` (hololadders) |
//! | 3465 | `effects::teleporters::portal_click` (permanent portals) |
//! | 3466 | `fireplace::attack_hand` (taking a bonfire apart) |
//! | 3467 | `science::quantum_pad::attack_hand` |
//! | 3468 | `janitor::washer_attack_hand` (washing machine door; right click starts it) |
//! | 3471 | `medical::special_traumas::stream_click` (a bluespace prophet's stream) |
//! | 3481 | `science::anomaly_refinery::interact` (the `AnomalyRefinery` window) |
//! | 3491 | `antag::xeno::reproduction::empty_hand` (an awake facehugger jumps at the hand) |
//! | 3493 | `holodeck::interact` (the `Holodeck` window, ready buttons) |
//! | 3497 | `telecomms_config::interact` (telecomms machine, monitor and log browser windows) |
//! | 3507 | `blackbox::attack_hand` (the blackbox out of its recorder) |
//! | 3517 | `ntnet_relay::interact` (the `NtnetRelay` window) |
//! | 3529 | `cannons::attack_hand` (a mounted gun fires, the repeater winds) |
//! | 3539 | `gym::punch` (the punching bag) |
//! | 3547 | `instruments::attack_hand` (piano editor, jukebox window and right-click toggle, boombox menu) |
//! | 3559 | `events::meteors::punch` (a legendary athlete punches a meteor away) |
//! | 3565 | `recycling::conveyor::switch_hand` (the lever; right click: backwards) |
//! | 3575 | `legcuffs::attack_hand` (an energy snare springs on the hand that touches it) |
//! | 3585 | `cargo::black_market::ltsrbt_hand` (the LTSRBT opens and closes; right click: its window) |
//!
//! [`ClickPhase::HeldOnTarget`] (a world click with the active hand's item; tg
//! `item_interaction`, `interact_with_atom`, `pre_attack`, `attackby`):
//!
//! | prio | handler |
//! | ---- | ------- |
//! | 3 | `modsuit::ninja` cloak (a cloaked wearer hitting something living drops the cloak; never takes the click) |
//! | 43 | `embedding::item_interaction` (a hemostat or wirecutters pluck what is embedded in the aimed bodypart) |
//! | 50 | `fishing::server::held_on_target` (casts, a fisher's clicks), `fishing::aquarium::item_interaction`, `fishing::mount::item_interaction` (a fish onto a trophy mount, its screwdriver), `fishing::catalog::analyze` (the fish analyzer) |
//! | 59 | `deployable_turrets::item_interaction` (a wrench packs an HMG up) |
//! | 79 | `gym::item_interaction` (punching bag wrench/crowbar right clicks, the loom) |
//! | 69 | `cannons::item_interaction` (cannon balls, charge, fuse, wrench; mounted gun ammo and wrench) |
//! | 53 | `holodeck::item_interaction` (holo floors take no items, ready buttons, the holodeck conveyor) |
//! | 100 | `antag::uplink::item_interaction` |
//! | 200 | `antag::syndie_items::item_interaction` |
//! | 247 | `weapons::disarm_attack` (a right click with a shield or a pillow shoves) |
//! | 251 | `weapons` whetstone (`sharpener`), shield bash (`block::bash`) |
//! | 219 | `clothing::chameleon::scanner_click` (the chameleon scanner on a human, or a turf with one) |
//! | 229 | `smoking::held_on_target` (lighters and matches on mobs, lighting someone's cigarette, smokables, matchboxes, vapes, dipping) |
//! | 233 | `devices::hand_items::held_on_target` (the slapper on a mob or table, the stealing hand) |
//! | 237 | `modsuit` construction, paint, MODlink (MOD shell steps, core repairs, paint kit and skin applier; right click: recolor, multitool frequency; a cell into a scryer) |
//! | 277 | `science::gizmo` (a gizmo spawned after map load gets its puzzle; never takes the click) |
//! | 289 | `machine_wires::held_on_target` (panel screwdrivers of machines, cyborgs, mechs and radios; wire tools on an open panel, MOD or radio) |
//! | 300 | `emag::interact_with_atom` (left click) |
//! | 317 | `misc_items::puzzle::swipe` (keycards on keycard doors and panels) |
//! | 320 | `janitor::key_on_door` (the janitor's key on a door) |
//! | 330 | `shuttle_console_server` (a held item on a shuttle console) |
//! | 340 | `shuttle_nav` (a held item on a navigation console) |
//! | 345 | `shuttle_remote::link` (a shuttle remote on a shuttle console) |
//! | 355 | `transport::elevator` (an emag on an elevator panel) |
//! | 400 | `modsuit::server::item_interaction` |
//! | 500 | `assemblies::server::item_interaction` |
//! | 555 | `wiremod::devices::cable_interaction` (USB cables on shells and machines) |
//! | 560 | `wiremod::server::item_interaction` |
//! | 600 | `grenades::item_interaction` |
//! | 603 | `science::cell_lines::swab_interaction` (swabs and biopsy tools: sampling, onto petri dishes) |
//! | 700 | `implants::item_interaction` (an implanter on a case, a case into the implant pad) |
//! | 707 | `modsuit::pathfinder::attack` (the MOD pathfinder module implants a human) |
//! | 800 | `defenses::server::item_interaction` (turrets, panels, cameras, scanner gates) |
//! | 805 | `defenses::turret_build::item_interaction` (a frame's parts and tools) |
//! | 809 | `forensics::board::item_interaction` (papers and photos pinned to the detective board, its wrench) |
//! | 815 | `defenses::barricades::item_interaction` (welder and crowbar on barricades, planks on doors and windows) |
//! | 900 | `silicon::law_rack::item_interaction` |
//! | 1000 | `forensics::server::item_interaction` (scanner, evidence bag, forensics spoofer) |
//! | 1100 | `explosions::sources::c4_interact` |
//! | 1200 | `explosions::sources::fueltank_welder_act` |
//! | 1300 | `antag::cult::server::item_interaction` |
//! | 1391 | `antag::heretic::server::items::held_on_target` (organs, flasks, codex and grasp on the mawed crucible; the phylactery's right click; wirecutters on an eldritch painting) |
//! | 1400 | `antag::heretic::server::item_interaction` |
//! | 1410 | `antag::nightmare::item_interaction` (the light eater) |
//! | 1420 | `silicon::aicard::interact_with_atom` |
//! | 1425 | `silicon::pai::cable_interaction` (the door jack cable into an airlock) |
//! | 1450 | `silicon::mmi::item_interaction` |
//! | 1460 | `silicon::ai_core::item_interaction` |
//! | 1497 | `guns::box_interaction` (rounds into an ammo box or a magazine that is in no gun) |
//! | 1487 | `gun_pins::pin_interaction` (a firing pin into a gun, the DNA pin keyed on a carbon) |
//! | 1500 | `bots::construction::item_interaction` |
//! | 1505 | `extinguisher::refill_click` (a tank refills it) |
//! | 1527 | `science::ordnance_machines::item_interaction` (data disks into the doppler array and tank compressor, tanks into the compressor, doppler wrench) |
//! | 1535 | `chem_machines::item_interaction` (containers, fuel, condensers, droppers and syringes, tools refused while working, wrenches) |
//! | 1549 | `eng_machines::manufactorio::item_interaction` (wrench, router, unloader, lathe and sorter multitool, sorter item filter, storage unit screwdriver) |
//! | 1545 | `science::teleportation::hand_tele::item_interaction` (dispelling its portals, right click portal; items on a portal) |
//! | 1511 | `mobs::lavaland::minebot` (a welder repairs a minebot) |
//! | 1515 | `cargo::black_market::ltsrbt_item` (an item into the open LTSRBT, money restocking the market) |
//! | 1519 | `events::meteors::attacked` (a mining tool breaks a meteor, a baseball bat sends it away) |
//! | 1555 | `inducer::interact_with_atom` (charging a cell) |
//! | 1557 | `telecomms_config` (a multitool on a telecomms machine opens its window) |
//! | 1559 | `eng_machines::modular_shield::item_interaction` (multitool buffer, connect, link; panel-open wrench turn) |
//! | 1560 | `devices::rcd::item_interaction` (builds, deconstruction, loading) |
//! | 1565 | `devices::taperecorder::item_interaction` (a tape into a recorder, respooling) |
//! | 1567 | `devices::laser_pointer::item_interaction` (diode, crystal lens, their removal) |
//! | 1569 | `eng_machines::flatpacker::item_interaction` (boards, packed parts, stacks; multitool on flatpacks) |
//! | 1571 | `modsuit::anomalock::item_interaction` (an anomaly core into a locked MOD module, a screwdriver taking it out) |
//! | 1575 | `science::teleportation` multitool (teleporter station and launchpad buffers and links, gateway calibration) |
//! | 1577 | `devices::rhd::item_interaction` (the RCD family's clicks and loading) |
//! | 1579 | `clothing::reactive::shell_interaction` (an anomaly core into a reactive armor shell) |
//! | 1580 | `events::anomalies::neutralizer_act`, `releaser_act` (anomalies, anomaly cores) |
//! | 1581 | `science::anomaly_refinery::item_interaction` (raw cores, transfer valves) |
//! | 1585 | `events::radiation_leak::tool_act` (a leaking machine's fixing tools) |
//! | 1587 | `blackbox::item_interaction` (the blackbox back into its recorder) |
//! | 1589 | `eng_machines::atmos_shield::item_interaction` (ID lock, tool refusals, wrench) |
//! | 1590 | `teapot::item_interaction` (before `machine_frame` takes it apart) |
//! | 1591 | `service::slots` (coins, holochips, multitool and emag on a slot machine) |
//! | 1593 | `devices::door_remote::use_on` (a door remote on a door or turf next to the user; far clicks: `RangedClick`) |
//! | 1595 | `science::bitrunning::avatar::crowbar_act` (prying a netpod open) |
//! | 1597 | `science::quantum_pad::item_interaction` (multitool links, keycards) |
//! | 1599 | `eng_machines::sheetifier::item_interaction` (meat in, wrench) |
//! | 1600 | `machine_frame::item_interaction` |
//! | 1650 | `radiation::geiger_interact` |
//! | 1680 | `service::barber::item_interaction` (razor, lipstick, paper on lips, dye spray, mirror frames) |
//! | 1690 | `service::stationery::cabinet_wrench` (right click) |
//! | 1691 | `service::cards::item_interaction` (cards, hands and decks on each other, pens marking) |
//! | 1700 | `service::server::item_interaction` |
//! | 1750 | `medical::bodyparts::item_interaction` |
//! | 1773 | `science::xenobio_console::item_interaction` (potions, monkey cubes into the slime console) |
//! | 1777 | `science::xenobio::item_interaction` (slimes, extracts) |
//! | 1783 | `science::cell_lines::dish_interaction` (a petri dish into the microscope or a growing vat) |
//! | 1800 | `kitchen::item_interaction` |
//! | 1803 | `antag::abductor::server::ship` (a science tool or vest onto the abductor console, a gland into storage) |
//! | 1809 | `kitchen_ware::held_on_target` (tank taps, tongs, plates, pizza boxes) |
//! | 1813 | `antag::abductor::server::gear` (science tool, silencer, mental interface device on a mob; far clicks: `RangedClick`) |
//! | 1850 | `restaurant::item_interaction` |
//! | 1893 | `food::crack_egg` (a right click with an egg on a griddle) |
//! | 1900 | `cooking::item_interaction` |
//! | 2000 | `smartfridge::item_interaction` |
//! | 2100 | `coffee::item_interaction` |
//! | 2105 | `papershredder::item_interaction` |
//! | 2200 | `stove::item_interaction` |
//! | 2300 | `icecream::item_interaction` |
//! | 2400 | `chem_dispenser::item_interaction` |
//! | 2500 | `chem_master::item_interaction` |
//! | 2600 | `chem_heater::item_interaction` |
//! | 2610 | `medical::cryo::item_interaction` (a cup into the cell) |
//! | 2630 | `medical::limbgrower::item_interaction` (limb design disks) |
//! | 2700 | `wallmount::item_interaction` |
//! | 2703 | `airlock_controllers` (any item presses an access button or console) |
//! | 2707 | `wallmount::frames::item_interaction` (wall frames on walls, their screwdriver and wrench) |
//! | 2710 | `power::lighting::item_interaction` (bulbs, replacers, fixtures, frames, wall frames, floodlights and their frames) |
//! | 2711 | `silicon::mob_cameras::repair` (cable on a broken bodycam or its jumpsuit) |
//! | 2715 | `power::lighting::swing_bulb` (a bulb shatters on a mob; never handles) |
//! | 2719 | `records::console::photo_interaction` (a photo on a security records console makes a new record) |
//! | 2785 | `recycling::held_on_target` (belts, switches, flaps, recyclers, disposal constructs and welding, belt stacks on floors) |
//! | 2800 | `power::chargers::item_interaction` |
//! | 2820 | `power::apc_ui::toggle_lock` (an ID or a PDA with one on an APC) |
//! | 2830 | `power::emitter::toggle_lock` (an ID or a PDA with one on an emitter) |
//! | 2850 | `power::pacman::item_interaction` (also PACMAN boards) |
//! | 2860 | `power::smes::item_interaction` (also SMES terminals) |
//! | 2870 | `power::shields::item_interaction` |
//! | 2880 | `airlock_construction::item_interaction` (door assemblies, the airlock painter) |
//! | 2885 | `door_structures::item_interaction` (firelock frames and bolts, windoor assemblies, mineral doors) |
//! | 2900 | `mecha::item_interaction` |
//! | 2950 | `vehicles::item_interaction` |
//! | 2960 | `flora::item_interaction` |
//! | 2970 | `cargo::mail_sorter::item_interaction` |
//! | 2975 | `cargo::coupon::attack_atom` (supply consoles) |
//! | 2977 | `cargo::money::item_interaction` (cash and ID cards, holochips) |
//! | 2980 | `cargo::express::item_interaction` |
//! | 2985 | `cargo::materials_market::item_interaction` |
//! | 3000 | `mining::item_interaction` |
//! | 3100 | `science::item_interaction` |
//! | 3110 | `science::exodrone::server::item_interaction` (the drone launcher) |
//! | 3200 | `botany::item_interaction` |
//! | 3260 | `mobs::farm::feed_egg_layer` |
//! | 3300 | `food::item_interaction` |
//! | 3397 | `plumbing_net::item_interaction` (ducts, duct stacks, wrenches on machines) |
//! | 3400 | `plumbing::item_interaction` |
//! | 3419 | `secure_safes::item_interaction` (panel screwdriver, multitool hack, items into the storage) |
//! | 3429 | `safes::item_interaction` (a stethoscope on a closed safe, items into an open one) |
//! | 3439 | `traps::item_interaction` (tripwire posts and cables: tying, wiring, cutting, wrench) |
//! | 3440 | `tank_storage::item_interaction` |
//! | 3451 | `antag::wizard::server::runic::held_on_target` (the scepter of vendormancy, a screwdriver on a runic vendor) |
//! | 3441 | `displaycase::item_interaction` (display cases, their chassis) |
//! | 3442 | `fireplace::item_interaction` (fuel, lighting, bonfire rods and grill) |
//! | 3447 | `instruments::item_interaction` (a wrench on pianos and jukeboxes, music tapes into boomboxes) |
//! | 3449 | `sheets::item_interaction` (a hot tool on burning sheets, rods on titanium) |
//! | 3450 | `decor::item_interaction` |
//! | 3455 | `clothing::item_interaction` |
//! | 3465 | `effects::teleporters::portal_click` (permanent portals) |
//! | 3475 | `mining::crusher::item_interaction` (crusher trophies on and off) |
//! | 3485 | `curses::cut_strand` (wirecutters on a strangled mob) |
//! | 3487 | `hazard_decals::ignite_click` (a hot item on a fuel pool) |
//! | 3500 | `janitor::item_interaction` |
//! | 3510 | `janitor::washer_item_interaction` (a load into a washing machine) |
//! | 3600 | `genetics::console::item_interaction` |
//! | 3647 | `talking_toys` emag on a toy nuke |
//! | 3649 | `toys::held_on_target` (caps, toy swords, balloons, the Mini-Meteor; water balloons never hit mobs) |
//! | 3660 | `mobs::farm` milking, `mobs::critters::turtle_drink` (before `chem::item_interaction`) |
//! | 3700 | `chem::item_interaction` |
//! | 3800 | `medical::item_interaction` |
//! | 3900 | `multiz::interact` (out of combat mode) |
//! | 3985 | `atmos_machinery::space_heater::item_interaction` |
//! | 3995 | `power::cable_coil::held_on_target` (laying, cutting, the multitool readout, robotic limb repair) |
//! | 4000 | `atmos_machinery::windows::item_interaction` |
//! | 4100 | `modpc::pda::item_interaction_secondary` (right click) |
//! | 4200 | `disposals::item_interaction` |
//! | 4247 | `misc_items::documents::forge_seal` (a red or blue crayon on a photocopy) |
//! | 4253 | `crayons::interact` (drawing, spray painting, chalk outlines, eating a crayon) |
//! | 4257 | `misc_items::target::welder_act` (a lit welder clears a target's marks) |
//! | 4263 | `syndie_spraycan::interact` (the Syndicate rune) |
//! | 4273 | `stickers::attempt_attach` |
//! | 4275 | `clothing::secgear::tool_act` (the same on a security mask lying around) |
//! | 4283 | `gifts::wrap_parcel` (wrapping paper on a parcel) |
//! | 4285 | `clothing::muzzle::attack` (a tape piece onto someone's mouth, combat mode off) |
//! | 4287 | `security::item_interaction` (the same onto a baton lying around) |
//! | 4291 | `gun_attachments::attach` (a seclite, bayonet or screwdriver on a gun or helmet lying around) |
//! | 4293 | `gun_modes::try_sawoff` (a saw on a gun lying around) |
//! | 4300 | `tables::item_interaction` |
//! | 4313 | `devices::table_clock::attackby` (a heavy item smashes the clock) |
//! | 4400 | `science::experi_scanner::pre_attack` (not on living mobs) |
//! | 4460 | `closets::kinds::held_on_target` (large crates, multitools on abandoned crates) |
//! | 4491 | `weapons::rip_and_tear::pre_attack` (a wielded sledgehammer on a wall) |
//! | 4501 | `antag::xeno::reproduction::held_on_target` (a held facehugger pressed to a face) |
//! | 4495 | `events::space_vines::attacked_by` (an item hitting kudzu) |
//! | 4467 | `misc_items::flatpacks::load` (a flatpack onto a flatpack cart) |
//! | 4500 | `construction::item_interaction` |
//! | 4600 | core: a cyborg module or `NoDrop` item stops here |
//! | 4650 | `lockbox::item_interaction` (`closets` plugin) |
//! | 4700 | core: a storage takes the held item |
//! | 4745 | `extinguisher::spray_click` (far clicks: `RangedClick`) |
//! | 4753 | `devices::laser_pointer::laser_act` (shining at what is clicked; far clicks: `RangedClick`) |
//! | 4759 | `service::photo::snap` (a camera takes a picture; far clicks: `RangedClick`) |
//! | 4800 | `cargo::bounty::item_interaction` |
//! | 4900 | `brig::prisoner::item_interaction` |
//! | 4925 | `brig::labor::item_interaction` (blank plates, grown items, prisoner IDs, sheets) |
//! | 5195 | `modpc::programs::station` (a program disk into a computer) |
//! | 5205 | `modpc::programs::station` (Physical Scanner: a PDA on a mob) |
//! | 5000 | `cargo::scanner::item_interaction` |
//! | 5090 | `events::anomalies::analyzer_act` (anomalies, anomaly cores) |
//! | 5100 | `atmos` analyzer scans the clicked atom |
//! | 5200 | `modpc::pda::item_interaction` |

use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::protocol::ClickModifiers;

/// A step of the click chain with its own ordered handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickPhase {
    /// The active hand's item clicked in its hand slot (tg `attack_self`); `target` = `held`.
    AttackSelf,
    /// The other hand's item (`target`) clicked in its hand slot, `held` the active hand's item
    /// (may be empty).
    OtherHand,
    /// A world click on `target` with the active hand empty (tg `attack_hand`, `interact`).
    EmptyHand,
    /// A world click on `target` with `held` (tg `item_interaction`, `attackby`).
    HeldOnTarget,
}

impl ClickPhase {
    const COUNT: usize = 4;

    fn index(self) -> usize {
        match self {
            Self::AttackSelf => 0,
            Self::OtherHand => 1,
            Self::EmptyHand => 2,
            Self::HeldOnTarget => 3,
        }
    }
}

/// What a click handler sees.
#[derive(Clone, Copy, Debug)]
pub struct ClickCtx {
    pub client: Entity,
    pub mob: Entity,
    pub mob_pos: TilePos,
    /// The active hand's item.
    pub held: Option<Entity>,
    /// The clicked entity: the item in the clicked hand slot, or the world click's target.
    pub target: Entity,
    /// The target's tile (the mob's own for hand slot clicks).
    pub pos: TilePos,
    /// Default for hand slot clicks.
    pub modifiers: ClickModifiers,
    /// The active hand's index (`Inventory::active`).
    pub active_hand: usize,
}

/// Handles the click (`true`) or passes it on (`false`).
pub type ClickHandler = fn(&mut World, &ClickCtx) -> bool;

/// The handlers of every phase, sorted by ascending priority.
#[derive(Resource, Default)]
pub struct ClickHandlers {
    phases: [Vec<(u32, ClickHandler)>; ClickPhase::COUNT],
}

impl ClickHandlers {
    /// Inserts `handler` in priority order. Panics on a priority already taken in `phase`.
    pub fn add(&mut self, phase: ClickPhase, priority: u32, handler: ClickHandler) {
        let list = &mut self.phases[phase.index()];
        let at = list.partition_point(|&(p, _)| p < priority);
        assert!(
            list.get(at).is_none_or(|&(p, _)| p != priority),
            "click handler priority {priority} of {phase:?} is taken"
        );
        list.insert(at, (priority, handler));
    }

    fn get(&self, phase: ClickPhase, index: usize) -> Option<ClickHandler> {
        self.phases[phase.index()].get(index).map(|&(_, h)| h)
    }
}

/// Runs `phase`'s handlers in priority order until one returns `true`; whether one did.
pub fn run(world: &mut World, phase: ClickPhase, ctx: &ClickCtx) -> bool {
    // By index: a handler gets the whole world, the registry included.
    for index in 0.. {
        let Some(handler) = world
            .get_resource::<ClickHandlers>()
            .and_then(|h| h.get(phase, index))
        else {
            break;
        };
        if handler(world, ctx) {
            return true;
        }
    }
    false
}

pub trait AddClickHandler {
    /// Registers `handler` in `phase` at `priority` (see the tables in [`crate::click_handlers`]).
    fn add_click_handler(
        &mut self,
        phase: ClickPhase,
        priority: u32,
        handler: ClickHandler,
    ) -> &mut Self;
}

impl AddClickHandler for App {
    fn add_click_handler(
        &mut self,
        phase: ClickPhase,
        priority: u32,
        handler: ClickHandler,
    ) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<ClickHandlers>()
            .add(phase, priority, handler);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Calls(Vec<u32>);

    fn ctx(world: &mut World) -> ClickCtx {
        let e = world.spawn_empty().id();
        ClickCtx {
            client: e,
            mob: e,
            mob_pos: TilePos::default(),
            held: None,
            target: e,
            pos: TilePos::default(),
            modifiers: ClickModifiers::default(),
            active_hand: 0,
        }
    }

    fn call(world: &mut World, n: u32, handled: bool) -> bool {
        world.resource_mut::<Calls>().0.push(n);
        handled
    }

    #[test]
    fn runs_in_priority_order_until_handled() {
        let mut app = App::new();
        app.init_resource::<Calls>();
        // Registered out of order.
        app.add_click_handler(ClickPhase::HeldOnTarget, 300, |w, _| call(w, 300, true))
            .add_click_handler(ClickPhase::HeldOnTarget, 100, |w, _| call(w, 100, false))
            .add_click_handler(ClickPhase::HeldOnTarget, 400, |w, _| call(w, 400, true))
            .add_click_handler(ClickPhase::HeldOnTarget, 200, |w, _| call(w, 200, false))
            .add_click_handler(ClickPhase::EmptyHand, 50, |w, _| call(w, 50, true));
        let world = app.world_mut();
        let ctx = ctx(world);
        assert!(run(world, ClickPhase::HeldOnTarget, &ctx));
        assert_eq!(world.resource::<Calls>().0, [100, 200, 300]);
        // A phase without handlers handles nothing.
        assert!(!run(world, ClickPhase::AttackSelf, &ctx));
    }

    #[test]
    fn nothing_handled() {
        let mut app = App::new();
        app.init_resource::<Calls>();
        app.add_click_handler(ClickPhase::OtherHand, 200, |w, _| call(w, 200, false))
            .add_click_handler(ClickPhase::OtherHand, 100, |w, _| call(w, 100, false));
        let world = app.world_mut();
        let ctx = ctx(world);
        assert!(!run(world, ClickPhase::OtherHand, &ctx));
        assert_eq!(world.resource::<Calls>().0, [100, 200]);
    }

    #[test]
    #[should_panic(expected = "is taken")]
    fn duplicate_priority_panics() {
        let mut app = App::new();
        app.add_click_handler(ClickPhase::EmptyHand, 100, |_, _| false)
            .add_click_handler(ClickPhase::EmptyHand, 100, |_, _| true);
    }

    /// Every `add_click_handler(ClickPhase::X, N, ...)` in `src` (outside this file's tests) has
    /// its `| N |` row in the module table under phase X, and every row is registered somewhere:
    /// the table is how a new hook finds a free priority.
    #[test]
    fn the_priority_table_lists_every_registration() {
        use std::collections::BTreeSet;
        const PHASES: [&str; 4] = ["AttackSelf", "OtherHand", "EmptyHand", "HeldOnTarget"];
        fn sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    sources(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        sources(&src, &mut files);
        let needle = "add_click_handler(ClickPhase::";
        let mut registered = BTreeSet::new();
        for file in &files {
            let text = std::fs::read_to_string(file).unwrap();
            // This module's own tests register made-up priorities.
            let text = if file.ends_with("click_handlers.rs") {
                text.split("#[cfg(test)]")
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            } else {
                text
            };
            for (at, _) in text.match_indices(needle) {
                let rest = &text[at + needle.len()..];
                let phase: String = rest.chars().take_while(|c| c.is_alphanumeric()).collect();
                let prio: String = rest[phase.len()..]
                    .trim_start_matches([',', ' '])
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                assert!(
                    PHASES.contains(&phase.as_str()),
                    "{}: {phase}",
                    file.display()
                );
                registered.insert((phase, prio.parse::<u32>().unwrap()));
            }
        }
        let doc = include_str!("click_handlers.rs");
        let mut listed = BTreeSet::new();
        let mut phase = None;
        for line in doc.lines().take_while(|l| l.starts_with("//!")) {
            if let Some(p) = PHASES
                .iter()
                .find(|p| line.starts_with(&format!("//! [`ClickPhase::{p}`]")))
            {
                phase = Some(*p);
            }
            let prio = line
                .strip_prefix("//! | ")
                .and_then(|r| r.split(' ').next())
                .and_then(|n| n.parse::<u32>().ok());
            if let (Some(phase), Some(prio)) = (phase, prio) {
                listed.insert((phase.to_owned(), prio));
            }
        }
        let missing: Vec<_> = registered.difference(&listed).collect();
        let stale: Vec<_> = listed.difference(&registered).collect();
        assert!(
            missing.is_empty() && stale.is_empty(),
            "registered but not in the table: {missing:?}; in the table but not registered: {stale:?}"
        );
    }
}

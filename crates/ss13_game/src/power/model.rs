//! tg power math per machine tick, as pure functions over plain state: the APC
//! (`apc_main.dm`, `apc_power_proc.dm`), the SMES (`smes.dm`), the area channels (`areas.dm`),
//! cells (`power_store.dm`) and a light fixture's emergency cell (`lighting/light.dm`).
//! Energies are joules, rates watts, `dt` the tick length in seconds (tg `seconds_per_tick`).

use bevy::prelude::*;

use super::grid::Powernet;

/// tg `CHARGELEVEL`: an APC cell charges at most 1% of its capacity per second.
pub const CHARGELEVEL: f64 = 0.01;
/// tg `APC_CHANNEL_LIGHT_TRESHOLD`, `APC_CHANNEL_EQUIP_TRESHOLD`, `APC_CHANNEL_ALARM_TRESHOLD`.
pub const LIGHT_THRESHOLD: f64 = 15.0;
pub const EQUIP_THRESHOLD: f64 = 30.0;
pub const ALARM_THRESHOLD: f64 = 75.0;

/// tg `APC_CHANNEL_*`.
pub const CHANNEL_OFF: u8 = 0;
pub const CHANNEL_AUTO_OFF: u8 = 1;
pub const CHANNEL_ON: u8 = 2;
pub const CHANNEL_AUTO_ON: u8 = 3;

/// tg `APC_NOT_CHARGING`, `APC_CHARGING`, `APC_FULLY_CHARGED`.
pub const NOT_CHARGING: u8 = 0;
pub const CHARGING: u8 = 1;
pub const FULLY_CHARGED: u8 = 2;

/// tg `APC_NO_POWER`, `APC_LOW_POWER`, `APC_HAS_POWER`.
pub const NO_POWER: u8 = 0;
pub const LOW_POWER: u8 = 1;
pub const HAS_POWER: u8 = 2;

/// tg area power channels; the index into per-channel arrays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
pub enum Channel {
    Equip = 0,
    Light = 1,
    Environ = 2,
}

impl Channel {
    pub const ALL: [Channel; 3] = [Channel::Equip, Channel::Light, Channel::Environ];

    /// tg `AREA_USAGE_EQUIP` 1, `AREA_USAGE_LIGHT` 2, `AREA_USAGE_ENVIRON` 3; anything else is
    /// equipment, the `/obj/machinery` default.
    pub fn from_dm(n: f64) -> Self {
        match n as i64 {
            2 => Channel::Light,
            3 => Channel::Environ,
            _ => Channel::Equip,
        }
    }
}

/// tg `/obj/item/stock_parts/power_store`.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct Cell {
    pub charge: f64,
    pub maxcharge: f64,
    /// Watts.
    pub chargerate: f64,
}

impl Cell {
    pub fn percent(&self) -> f64 {
        100.0 * self.charge / self.maxcharge
    }

    pub fn used_charge(&self) -> f64 {
        self.maxcharge - self.charge
    }

    /// tg `use()`: all or nothing, or with `force` whatever is left.
    pub fn take(&mut self, amount: f64, force: bool) -> f64 {
        if !force && self.charge < amount {
            return 0.0;
        }
        let used = amount.min(self.charge).max(0.0);
        self.charge -= used;
        used
    }

    /// tg `give()`.
    pub fn give(&mut self, amount: f64) -> f64 {
        let given = amount.min(self.maxcharge - self.charge).max(0.0);
        self.charge += given;
        given
    }
}

/// The power state of a tg area.
#[derive(Component, Clone, Debug, Default, Reflect)]
pub struct AreaPower {
    pub requires_power: bool,
    pub always_unpowered: bool,
    /// tg `power_equip`, `power_light`, `power_environ`.
    pub power: [bool; 3],
    pub lightswitch: bool,
    /// Static loads of the machines in the area per channel, watts (tg keeps them as joules
    /// per tick in `energy_usage[AREA_USAGE_STATIC_*]`).
    pub static_power: [f64; 3],
    /// Dynamic use this tick per channel (tg `energy_usage[AREA_USAGE_EQUIP..ENVIRON]`).
    pub used: [f64; 3],
    /// tg `energy_usage[AREA_USAGE_APC_CHARGE]`.
    pub apc_charge: f64,
    pub apc: Option<Entity>,
}

impl AreaPower {
    /// tg `/area/proc/powered(chan)`.
    pub fn powered(&self, channel: Channel) -> bool {
        if !self.requires_power {
            return true;
        }
        !self.always_unpowered && self.power[channel as usize]
    }

    /// tg `clear_usage()`.
    pub fn clear_usage(&mut self) {
        self.used = [0.0; 3];
        self.apc_charge = 0.0;
    }
}

/// How the charging pass of the tick limits an APC cell (tg `SSMACHINES_APCS_*` steps): each
/// step charges at most up to the threshold that keeps that channel on, plus 5%.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChargeStep {
    Environment,
    Lights,
    Equipment,
    Full,
}

impl ChargeStep {
    pub const ORDER: [ChargeStep; 4] = [
        ChargeStep::Environment,
        ChargeStep::Lights,
        ChargeStep::Equipment,
        ChargeStep::Full,
    ];
}

/// tg `/obj/machinery/power/apc` state the power tick uses.
#[derive(Component, Clone, Debug, Reflect)]
pub struct Apc {
    pub area: Entity,
    pub cell: Option<Cell>,
    /// `equipment`, `lighting`, `environ`, indexed by [`Channel`].
    pub channels: [u8; 3],
    pub operating: bool,
    pub chargemode: bool,
    pub shorted: bool,
    pub locked: bool,
    /// tg `aidisabled` (the AI wire); there is no AI yet.
    pub aidisabled: bool,
    pub charging: u8,
    pub last_charging: u8,
    pub main_status: u8,
    /// Seconds of a power failure left (tg `failure_timer`).
    pub failure_timer: f64,
    pub force_update: bool,
    /// What the last tick used per channel and to charge the cell, joules (tg `lastused_*`).
    pub lastused: [f64; 3],
    pub lastused_charge: f64,
    /// tg `coverlocked`, `emergency_lights` (true: the area's lights keep no emergency
    /// power), `nightshift_lights`.
    pub coverlocked: bool,
    pub emergency_lights: bool,
    pub nightshift_lights: bool,
}

impl Apc {
    pub fn new(area: Entity, cell: Option<Cell>) -> Self {
        Self {
            area,
            cell,
            channels: [CHANNEL_AUTO_ON; 3],
            operating: true,
            chargemode: true,
            shorted: false,
            locked: true,
            aidisabled: false,
            charging: NOT_CHARGING,
            last_charging: NOT_CHARGING,
            main_status: NO_POWER,
            failure_timer: 0.0,
            force_update: false,
            lastused: [0.0; 3],
            lastused_charge: 0.0,
            coverlocked: true,
            emergency_lights: false,
            nightshift_lights: false,
        }
    }

    pub fn channel_on(&self, channel: Channel) -> bool {
        self.channels[channel as usize] >= CHANNEL_ON
    }

    /// tg `early_process()`: the area's static loads, from the grid first, then the cell.
    pub fn early_process(&mut self, area: &mut AreaPower, net: Option<&mut Powernet>, dt: f64) {
        if let Some(cell) = &self.cell
            && cell.charge < cell.maxcharge
        {
            self.last_charging = self.charging;
            self.charging = NOT_CHARGING;
        }
        let mut total = 0.0;
        if self.operating {
            for ch in Channel::ALL {
                if self.channel_on(ch) {
                    total += area.static_power[ch as usize] * dt;
                }
            }
        }
        area.clear_usage();
        if total > 0.0 {
            let grid = net.map_or(0.0, |net| {
                let used = net.surplus().min(total);
                net.load += used;
                used
            });
            if total > grid
                && let Some(cell) = &mut self.cell
            {
                cell.take(total - grid, true);
            }
        }
    }

    /// tg `charge_channel()`.
    pub fn charge_channel(
        &mut self,
        step: ChargeStep,
        area: &mut AreaPower,
        net: Option<&mut Powernet>,
        dt: f64,
    ) {
        let Some(net) = net else {
            return;
        };
        let surplus = net.surplus();
        let Some(cell) = &mut self.cell else {
            return;
        };
        if self.shorted
            || !self.operating
            || !self.chargemode
            || surplus <= 0.0
            || cell.used_charge() <= 0.0
        {
            return;
        }
        let need_for_channel = match step {
            ChargeStep::Environment => cell.maxcharge * 0.05 - cell.charge,
            ChargeStep::Lights => cell.maxcharge * (LIGHT_THRESHOLD + 5.0) * 0.01 - cell.charge,
            ChargeStep::Equipment => cell.maxcharge * (EQUIP_THRESHOLD + 5.0) * 0.01 - cell.charge,
            ChargeStep::Full => cell.used_charge(),
        };
        // tg subtracts this tick's charging energy from a rate in watts; kept as is.
        let remaining_rate = cell.chargerate.min(cell.maxcharge * CHARGELEVEL) - area.apc_charge;
        let need = need_for_channel.min(remaining_rate) * dt;
        if need < 0.0 {
            return;
        }
        // `charge_cell(grid_only = TRUE)` -> `use_energy(ignore_apc = TRUE)`.
        let demand = need.min(cell.used_charge());
        let grid = if demand > 0.0 {
            surplus.min(demand)
        } else {
            0.0
        };
        net.load += grid;
        area.apc_charge += grid;
        cell.give(grid);
        if cell.charge >= cell.maxcharge {
            cell.charge = cell.maxcharge;
            self.charging = FULLY_CHARGED;
        } else {
            self.charging = CHARGING;
        }
    }

    /// tg `late_process()` (after `charge_channel(null)`). Returns whether the area's channels
    /// changed (tg `update()` ran).
    pub fn late_process(&mut self, area: &mut AreaPower, net: Option<&Powernet>, dt: f64) -> bool {
        if !area.requires_power {
            return false;
        }
        if self.failure_timer > 0.0 {
            self.failure_timer = (self.failure_timer - dt).max(0.0);
            self.force_update = true;
            return false;
        }
        for ch in Channel::ALL {
            let i = ch as usize;
            self.lastused[i] = if self.operating && self.channel_on(ch) {
                area.used[i] + area.static_power[i] * dt
            } else {
                0.0
            };
        }
        self.lastused_charge = if self.charging == CHARGING {
            area.apc_charge
        } else {
            0.0
        };
        let before = self.channels;
        let (avail, surplus) = net.map_or((0.0, 0.0), |n| (n.avail, n.surplus()));
        self.main_status = if avail <= 0.0 {
            NO_POWER
        } else if surplus <= 0.0 {
            LOW_POWER
        } else {
            HAS_POWER
        };
        use Autoset::*;
        let [equip, light, env] = match &self.cell {
            Some(_) if self.shorted => [Keep, Keep, Keep],
            Some(cell) if cell.charge <= 0.0 => [ForceOff, ForceOff, ForceOff],
            Some(cell) if cell.percent() < LIGHT_THRESHOLD => [Off, Off, On],
            Some(cell) if cell.percent() < EQUIP_THRESHOLD => [Off, On, On],
            Some(_) => [On, On, On],
            None => {
                self.charging = NOT_CHARGING;
                [ForceOff, ForceOff, ForceOff]
            }
        };
        for (ch, mode) in [
            (Channel::Equip, equip),
            (Channel::Light, light),
            (Channel::Environ, env),
        ] {
            let i = ch as usize;
            self.channels[i] = autoset(self.channels[i], mode);
        }
        if before != self.channels || self.force_update {
            self.force_update = false;
            return self.update(area);
        }
        false
    }

    /// tg `update()`: the area's channels follow the APC. Returns whether they changed.
    pub fn update(&self, area: &mut AreaPower) -> bool {
        let before = area.power;
        area.power = if self.operating && !self.shorted && self.failure_timer <= 0.0 {
            self.channels.map(|c| c > CHANNEL_AUTO_OFF)
        } else {
            [false; 3]
        };
        before != area.power
    }

    /// The tg `check_updates()` overlay key: changes exactly when the overlays do.
    pub fn look_key(&self) -> u16 {
        u16::from(self.operating)
            | u16::from(self.locked) << 1
            | u16::from(self.charging) << 2
            | u16::from(self.channels[Channel::Equip as usize]) << 4
            | u16::from(self.channels[Channel::Light as usize]) << 6
            | u16::from(self.channels[Channel::Environ as usize]) << 8
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Autoset {
    Keep,
    /// tg `AUTOSET_FORCE_OFF`: manual "on" channels go off too.
    ForceOff,
    /// tg `AUTOSET_OFF`: automatic channels go off.
    Off,
    /// tg `AUTOSET_ON`: automatic channels come back.
    On,
}

/// tg `autoset()`.
fn autoset(val: u8, mode: Autoset) -> u8 {
    match (mode, val) {
        (Autoset::ForceOff, CHANNEL_ON) => CHANNEL_OFF,
        (Autoset::ForceOff | Autoset::Off, CHANNEL_AUTO_ON) => CHANNEL_AUTO_OFF,
        (Autoset::On, CHANNEL_AUTO_OFF) => CHANNEL_AUTO_ON,
        _ => val,
    }
}

/// tg `/obj/machinery/power/smes` state; `charge` is the sum of its batteries.
#[derive(Component, Clone, Debug, Reflect)]
pub struct Smes {
    pub charge: f64,
    pub capacity: f64,
    pub input_level: f64,
    pub output_level: f64,
    pub input_attempt: bool,
    pub output_attempt: bool,
    pub inputting: bool,
    pub outputting: bool,
    pub input_available: f64,
    pub output_used: f64,
    /// Its terminal; without one tg breaks the SMES (`atom_break`) and it does nothing.
    pub terminal: Option<Entity>,
}

impl Smes {
    /// tg `chargedisplay()`, 0..=5.
    pub fn charge_display(&self) -> u8 {
        if self.capacity <= 0.0 {
            return 0;
        }
        (5.0 * self.charge / self.capacity).round().clamp(0.0, 5.0) as u8
    }

    fn operational(&self) -> bool {
        self.terminal.is_some()
    }

    /// tg `process()`, output half: adds this tick's output to the grid's next `avail`.
    pub fn process_output(&mut self, out: Option<&mut Powernet>, dt: f64) {
        if !self.operational() {
            return;
        }
        self.outputting = false;
        let Some(out) = out.filter(|_| self.output_attempt) else {
            return;
        };
        let energy = self.output_level * dt;
        self.output_used = 0.0;
        if energy <= 0.0 {
            return;
        }
        self.output_used = energy.min(self.charge).max(0.0);
        self.charge -= self.output_used;
        if self.output_used > 0.0 {
            out.newavail += self.output_used;
            self.outputting = self.output_used >= 0.1;
        }
    }

    /// tg `process()`, input half: charges from the terminal net's surplus.
    pub fn process_input(&mut self, input: Option<&mut Powernet>, dt: f64) {
        if !self.operational() {
            return;
        }
        self.inputting = false;
        let Some(input) = input.filter(|_| self.input_attempt) else {
            return;
        };
        let energy = self.input_level * dt;
        self.input_available = input.surplus();
        if energy <= 0.0 {
            return;
        }
        let load = energy
            .min(self.input_available)
            .min(self.capacity - self.charge)
            .max(0.0);
        self.charge += load;
        if load > 0.0 {
            input.load += load;
            self.inputting = true;
        }
    }

    /// tg `restore()`: takes back what the output net did not use last tick.
    pub fn restore(&mut self, out: &mut Powernet) {
        if !self.operational() {
            return;
        }
        if !self.outputting {
            self.output_used = 0.0;
            return;
        }
        let excess = self.output_used.min(out.netexcess);
        let excess = excess.min(self.capacity - self.charge).max(0.0);
        self.charge += excess;
        out.netexcess -= excess;
        self.output_used -= excess;
    }
}

/// tg `LIGHT_EMERGENCY_POWER_USE`: an emergency light draws 2 W from its cell.
pub const LIGHT_EMERGENCY_POWER_USE: f64 = 2.0;
/// `/obj/item/stock_parts/power_store/cell/emergency_light` `maxcharge`: ~10 minutes.
pub const EMERGENCY_CELL_CHARGE: f64 = 1200.0;

/// What a light fixture shows (tg `on` / `low_power_mode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub enum LightMode {
    On,
    /// Dim red from the fixture's emergency cell.
    Emergency,
    Off,
}

/// tg `update()` decision for a working bulb: on with area power and the switch on, else
/// emergency light while the cell holds a tick's draw (unless the switch turned it off).
pub fn light_mode(area: &AreaPower, emergency_charge: Option<f64>, dt: f64) -> LightMode {
    let power_light = area.power[Channel::Light as usize];
    if area.lightswitch && power_light {
        return LightMode::On;
    }
    let turned_off = !area.lightswitch && power_light;
    match emergency_charge {
        Some(c) if c >= LIGHT_EMERGENCY_POWER_USE * dt && !turned_off => LightMode::Emergency,
        _ => LightMode::Off,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(static_power: [f64; 3]) -> AreaPower {
        AreaPower {
            requires_power: true,
            power: [true; 3],
            lightswitch: true,
            static_power,
            ..default()
        }
    }

    fn apc(charge: f64) -> Apc {
        // tg's default APC cell: `battery/upgraded`, 2.5 MJ, 500 kW.
        Apc::new(
            Entity::PLACEHOLDER,
            Some(Cell {
                charge,
                maxcharge: 2.5e6,
                chargerate: 5e5,
            }),
        )
    }

    fn net(avail: f64) -> Powernet {
        Powernet { avail, ..default() }
    }

    /// One machine tick of one APC, in tg order.
    fn tick(apc: &mut Apc, area: &mut AreaPower, net: &mut Option<Powernet>) -> bool {
        let dt = 2.0;
        apc.early_process(area, net.as_mut(), dt);
        for step in ChargeStep::ORDER {
            apc.charge_channel(step, area, net.as_mut(), dt);
        }
        apc.late_process(area, net.as_ref(), dt)
    }

    #[test]
    fn grid_pays_static_load_and_charges_the_cell_at_one_percent_per_second() {
        let mut a = area([1000.0, 500.0, 250.0]);
        let mut apc = apc(1.0e6);
        let mut n = Some(net(1.0e6));
        assert!(!tick(&mut apc, &mut a, &mut n));
        let n = n.unwrap();
        // 1750 W for 2 s from the grid, then 1% of 2.5 MJ per second for 2 s into the cell.
        assert_eq!(apc.cell.unwrap().charge, 1.0e6 + 50_000.0);
        assert_eq!(n.load, 3500.0 + 50_000.0);
        assert_eq!(a.apc_charge, 50_000.0);
        assert_eq!((apc.charging, apc.main_status), (CHARGING, HAS_POWER));
        assert_eq!(apc.lastused, [2000.0, 1000.0, 500.0]);
    }

    #[test]
    fn without_grid_the_cell_drains_and_channels_shut_in_tg_order() {
        let mut a = area([1000.0, 500.0, 250.0]);
        // Just above the equipment threshold: 30% + one tick of load.
        let mut apc = apc(2.5e6 * 0.30 + 3500.0);
        let mut no_grid = None;
        assert!(!tick(&mut apc, &mut a, &mut no_grid));
        assert_eq!(apc.cell.unwrap().charge, 2.5e6 * 0.30);
        assert_eq!(apc.main_status, NO_POWER);
        apc.cell.as_mut().unwrap().charge -= 1.0;
        // Below 30%: equipment goes (auto) off, lights and environment stay.
        assert!(tick(&mut apc, &mut a, &mut no_grid));
        assert_eq!(
            apc.channels,
            [CHANNEL_AUTO_OFF, CHANNEL_AUTO_ON, CHANNEL_AUTO_ON]
        );
        assert_eq!(a.power, [false, true, true]);
        assert!(!a.powered(Channel::Equip) && a.powered(Channel::Light));
        // Below 15%: lights too; at 0 everything, also channels switched on by hand.
        apc.cell.as_mut().unwrap().charge = 2.5e6 * 0.10;
        apc.channels[Channel::Environ as usize] = CHANNEL_ON;
        assert!(tick(&mut apc, &mut a, &mut no_grid));
        assert_eq!(a.power, [false, false, true]);
        apc.cell.as_mut().unwrap().charge = 0.0;
        assert!(tick(&mut apc, &mut a, &mut no_grid));
        assert_eq!(
            apc.channels,
            [CHANNEL_AUTO_OFF, CHANNEL_AUTO_OFF, CHANNEL_OFF]
        );
        assert_eq!(a.power, [false; 3]);
        // Power back: automatic channels return as the cell refills, the manual one stays off.
        let mut n = Some(net(1.0e7));
        for _ in 0..20 {
            tick(&mut apc, &mut a, &mut n);
            n.as_mut().unwrap().finish_reset();
            n.as_mut().unwrap().avail = 1.0e7;
        }
        assert!(apc.cell.unwrap().percent() >= EQUIP_THRESHOLD);
        assert_eq!(a.power, [true, true, false]);
    }

    #[test]
    fn charge_steps_stop_at_thresholds_while_surplus_is_short() {
        // 30 kJ of surplus: the environment step takes it all, the cell sits at 1.2%.
        let mut a = area([0.0; 3]);
        let mut apc = apc(0.0);
        let mut n = Some(net(30_000.0));
        tick(&mut apc, &mut a, &mut n);
        assert_eq!(apc.cell.unwrap().charge, 30_000.0);
        assert_eq!(n.unwrap().surplus(), 0.0);
        assert_eq!(apc.charging, CHARGING);
        // Full cell, no load: nothing drawn, shown as fully charged.
        let mut apc = apc_full();
        let mut n = Some(net(1.0e6));
        tick(&mut apc, &mut a, &mut n);
        assert_eq!(n.unwrap().load, 0.0);
    }

    fn apc_full() -> Apc {
        let mut apc = apc(2.5e6);
        apc.charging = FULLY_CHARGED;
        apc
    }

    #[test]
    fn smes_output_restore_and_input() {
        let mut smes = Smes {
            charge: 5.0e7,
            capacity: 5.0e7,
            input_level: 50_000.0,
            output_level: 90_000.0,
            input_attempt: true,
            output_attempt: true,
            inputting: false,
            outputting: false,
            input_available: 0.0,
            output_used: 0.0,
            terminal: Some(Entity::PLACEHOLDER),
        };
        let mut out = Powernet::default();
        smes.process_output(Some(&mut out), 2.0);
        assert_eq!(out.newavail, 180_000.0);
        assert!(smes.outputting);
        assert_eq!(smes.charge, 5.0e7 - 180_000.0);
        // Next tick the grid used 30 kJ of it: the SMES takes the other 150 kJ back.
        out.finish_reset();
        out.load = 30_000.0;
        assert!(out.begin_reset());
        smes.restore(&mut out);
        assert_eq!(smes.charge, 5.0e7 - 30_000.0);
        assert_eq!(out.netexcess, 0.0);
        // Input: limited by its level, the terminal net's surplus and free capacity.
        let mut input = net(1.0e6);
        smes.process_input(Some(&mut input), 2.0);
        assert_eq!(smes.charge, 5.0e7);
        assert_eq!(input.load, 30_000.0);
        // Without a terminal tg breaks the SMES: nothing in, nothing out.
        smes.terminal = None;
        let mut out = Powernet::default();
        smes.process_output(Some(&mut out), 2.0);
        assert_eq!(out.newavail, 0.0);
    }

    #[test]
    fn light_modes() {
        let mut a = area([0.0; 3]);
        assert_eq!(light_mode(&a, Some(1200.0), 2.0), LightMode::On);
        a.power[Channel::Light as usize] = false;
        assert_eq!(light_mode(&a, Some(1200.0), 2.0), LightMode::Emergency);
        assert_eq!(light_mode(&a, Some(3.0), 2.0), LightMode::Off);
        assert_eq!(light_mode(&a, None, 2.0), LightMode::Off);
        // Switched off with power: dark, no emergency light.
        a.power[Channel::Light as usize] = true;
        a.lightswitch = false;
        assert_eq!(light_mode(&a, Some(1200.0), 2.0), LightMode::Off);
    }
}

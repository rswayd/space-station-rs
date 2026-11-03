//! tg `atmos_scan` (`gas_analyzer.dm`): the chat readout of a gas analyzer.

use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};

use crate::gas::{CELL_VOLUME, GASES, GasMixture, T0C, byond_round};

/// One mixture a scanned atom holds.
#[derive(Clone, Copy, Debug)]
pub struct ScanMix<'a> {
    pub gas: &'a GasMixture,
    pub volume: f64,
    /// A turf's mixture (an empty one counts `HEAT_CAPACITY_VACUUM`).
    pub turf: bool,
}

impl ScanMix<'_> {
    pub fn pressure(&self) -> f64 {
        if self.volume <= 0.0 {
            return 0.0;
        }
        self.gas.total_moles() * crate::gas::R_IDEAL_GAS_EQUATION * self.gas.temperature
            / self.volume
    }

    pub fn heat_capacity(&self) -> f64 {
        if self.turf {
            self.gas.heat_capacity()
        } else {
            self.gas.heat_capacity_raw()
        }
    }
}

/// The readout for `air` on `target` (the scanned atom's name, tg `[target]`): a turf.
pub fn scan_message(target: &str, air: &GasMixture) -> ChatMessage {
    scan_mixes(
        target,
        &[ScanMix {
            gas: air,
            volume: CELL_VOLUME,
            turf: true,
        }],
    )
}

/// tg `atmos_scan` of an atom holding `mixes` (machine ports are "Node N").
pub fn scan_mixes(target: &str, mixes: &[ScanMix]) -> ChatMessage {
    let mut lines = vec![(
        Span::BoldNotice,
        format!("Results of analysis of {target}."),
    )];
    let r = |x: f64| num(byond_round(x, 0.01));
    for (i, mix) in mixes.iter().enumerate() {
        if mixes.len() > 1 {
            lines.push((Span::BoldNotice, format!("Node {}", i + 1)));
        }
        let air = mix.gas;
        let total = air.total_moles();
        if total > 0.0 {
            lines.push((Span::Notice, format!("Moles: {} mol", r(total))));
            for (info, &amount) in GASES.iter().zip(&air.moles) {
                if amount != 0.0 {
                    lines.push((
                        Span::Notice,
                        format!(
                            "{}: {} mol ({} %)",
                            info.name,
                            r(amount),
                            r(amount / total * 100.0)
                        ),
                    ));
                }
            }
            let t = air.temperature;
            lines.push((
                Span::Notice,
                format!("Temperature: {} °C ({} K)", r(t - T0C), r(t)),
            ));
            lines.push((Span::Notice, format!("Volume: {} L", num(mix.volume))));
            lines.push((Span::Notice, format!("Pressure: {} kPa", r(mix.pressure()))));
            let capacity = mix.heat_capacity();
            lines.push((
                Span::Notice,
                format!("Heat Capacity: {} / K", display_energy(capacity)),
            ));
            lines.push((
                Span::Notice,
                format!("Thermal Energy: {}", display_energy(t * capacity)),
            ));
        } else {
            let empty = if mixes.len() > 1 {
                "This node is empty!".to_owned()
            } else {
                format!("{target} is empty!")
            };
            lines.push((Span::Notice, empty));
            lines.push((Span::Notice, format!("Volume: {} L", num(mix.volume))));
        }
    }
    let segments = lines
        .into_iter()
        .enumerate()
        .map(|(i, (span, text))| {
            ChatSegment::new(span, if i == 0 { text } else { format!("\n{text}") })
        })
        .collect();
    ChatMessage {
        channel: ChatChannel::Info,
        segments,
    }
}

/// BYOND's default number text: 6 significant digits, exponent form outside 1e-4..1e6.
pub fn num(x: f64) -> String {
    if x == 0.0 {
        return "0".into();
    }
    if !x.is_finite() {
        return format!("{x}");
    }
    // Rounded to 6 significant digits first, so 999999.7 counts as 1e+06.
    let sci = format!("{x:.5e}");
    let (mantissa, e) = sci.split_once('e').expect("exponent form");
    let e: i32 = e.parse().expect("integer exponent");
    if (-4..6).contains(&e) {
        let decimals = (5 - e) as usize;
        trim(&format!("{x:.decimals$}")).to_owned()
    } else {
        let sign = if e < 0 { '-' } else { '+' };
        format!("{}e{sign}{:02}", trim(mantissa), e.abs())
    }
}

fn trim(s: &str) -> &str {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        s
    }
}

/// tg `display_energy` = `siunit(units, "J", 3)`.
pub fn display_energy(units: f64) -> String {
    const PREFIXES: [&str; 21] = [
        "q", "r", "y", "z", "a", "f", "p", "n", "μ", "m", "", "k", "M", "G", "T", "P", "E", "Z",
        "Y", "R", "Q",
    ];
    if units.abs() < 1e-30 {
        return "0 J".into();
    }
    let exponent = units.abs().log10().clamp(-30.0, 30.0);
    let step = (exponent / 3.0).floor();
    let divider = 10f64.powf(step * 3.0);
    let mut coefficient = byond_round(units / divider, 1e-3);
    let mut prefix = step as i32 + 10;
    if coefficient >= 1000.0 && prefix < 20 {
        coefficient /= 1e3;
        prefix += 1;
    }
    format!(
        "{} {}J",
        num(coefficient),
        PREFIXES[prefix.clamp(0, 20) as usize]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byond_number_text() {
        assert_eq!(num(103.98), "103.98");
        assert_eq!(num(byond_round(103.982_009, 0.01)), "103.98");
        assert_eq!(num(2500.0), "2500");
        assert_eq!(num(20.0), "20");
        assert_eq!(num(-270.45), "-270.45");
        assert_eq!(num(1_234_567.0), "1.23457e+06");
        assert_eq!(num(0.5), "0.5");
        assert_eq!(display_energy(2079.64), "2.08 kJ");
        assert_eq!(display_energy(609_646.0), "609.646 kJ");
        assert_eq!(display_energy(999_999.9), "1 MJ");
    }

    #[test]
    fn standard_air_readout() {
        let air = GasMixture::parse("o2=22;n2=82;TEMP=293.15").unwrap();
        let text = scan_message("floor", &air).plain_text();
        let expected = "Results of analysis of floor.\n\
            Moles: 104 mol\n\
            Oxygen: 22 mol (21.15 %)\n\
            Nitrogen: 82 mol (78.85 %)\n\
            Temperature: 20 °C (293.15 K)\n\
            Volume: 2500 L\n\
            Pressure: 101.34 kPa\n\
            Heat Capacity: 2.08 kJ / K\n\
            Thermal Energy: 609.752 kJ";
        assert_eq!(text, expected);
        let empty = scan_message("space", &GasMixture::vacuum()).plain_text();
        assert_eq!(
            empty,
            "Results of analysis of space.\nspace is empty!\nVolume: 2500 L"
        );
    }
}

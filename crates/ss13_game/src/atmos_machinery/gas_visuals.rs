//! Gas seen through pipes: tg `/datum/pipeline/proc/CalculateGasmixColor` and the
//! `/obj/effect/abstract/gas_visual` each pipe shows (`!pipe_gas_overlays.dmi`, the pipe's own
//! state, `RESET_COLOR`). The pipeline's color blends its gases' `primary_color`s by moles in
//! HSV, then toward black while the pipe is far from full.
// shortcut: tg animates the color over half a second and shares one overlay object per
// pipeline; here every pipe of a pipeline gets the new color at once, and colors are rounded
// to steps of 4 per channel so a slowly changing pipeline does not resend its pipes every fire.
// The HSV blend interpolates the hue along the shorter arc (BYOND `hsv_gradient`'s exact hue
// rule is not documented).

use bevy::prelude::*;
use ss13_atmos::gas::PRIMARY_COLORS;
use ss13_atmos::pipenet::Pipenet;
use ss13_atmos::pipenet::air::PipeAir;
use ss13_shared::{Appearance, IconStateId, Overlay};

use super::AtmosDevice;

/// A pipe's gas overlay: its state (the pipe's own, from `!pipe_gas_overlays.dmi`) and the
/// color drawn.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipeGasOverlay {
    pub state: IconStateId,
    pub color: [u8; 3],
}

fn rgb_to_hsv([r, g, b]: [u8; 3]) -> (f64, f64, f64) {
    let (r, g, b) = (
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max == 0.0 { 0.0 } else { d / max };
    (h, s, max)
}

fn hsv_to_rgb((h, s, v): (f64, f64, f64)) -> [u8; 3] {
    let c = v * s;
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r, g, b].map(|ch| ((ch + m) * 255.0).round().clamp(0.0, 255.0) as u8)
}

/// `BlendHSV(a, b, amount)`.
pub fn blend_hsv(a: [u8; 3], b: [u8; 3], amount: f64) -> [u8; 3] {
    let (ha, sa, va) = rgb_to_hsv(a);
    let (hb, sb, vb) = rgb_to_hsv(b);
    // A gray has no hue: it takes the other color's.
    let (ha, hb) = match (sa == 0.0, sb == 0.0) {
        (true, false) => (hb, hb),
        (false, true) => (ha, ha),
        _ => (ha, hb),
    };
    let mut dh = hb - ha;
    if dh > 180.0 {
        dh -= 360.0;
    } else if dh < -180.0 {
        dh += 360.0;
    }
    hsv_to_rgb((
        ha + dh * amount,
        sa + (sb - sa) * amount,
        va + (vb - va) * amount,
    ))
}

/// `CalculateGasmixColor`.
pub fn gasmix_color(air: &PipeAir) -> [u8; 3] {
    let mut weight = 0.0;
    let mut color: Option<[u8; 3]> = None;
    for (&moles, &gas_color) in air.gas.moles.iter().zip(&PRIMARY_COLORS) {
        if moles == 0.0 {
            continue;
        }
        weight += moles;
        color = Some(match color {
            None => gas_color,
            Some(c) => blend_hsv(c, gas_color, moles / weight),
        });
    }
    let Some(color) = color else {
        return [0; 3];
    };
    let empty = (air.volume * 1.5 - weight) / 10.0;
    if empty > 0.0 {
        blend_hsv([0; 3], color, weight / (empty + weight))
    } else {
        color
    }
}

fn rounded(c: [u8; 3]) -> [u8; 3] {
    c.map(|ch| ch & !3)
}

/// Every fire: each pipe with a gas overlay takes its pipeline's color.
pub fn sync_gas_visuals(
    net: Res<Pipenet>,
    mut pipes: Query<(&AtmosDevice, &mut PipeGasOverlay, &mut Appearance)>,
) {
    let mut colors: std::collections::HashMap<u32, [u8; 3]> = Default::default();
    for (device, mut overlay, mut appearance) in &mut pipes {
        let Some(line) = net.pipeline_of(device.id, 0) else {
            continue;
        };
        let color = *colors.entry(line.0).or_insert_with(|| {
            net.pipeline(line)
                .map_or([0; 3], |l| rounded(gasmix_color(&l.air)))
        });
        if overlay.color == color
            && appearance
                .overlays
                .iter()
                .any(|o| o.icon_state == overlay.state)
        {
            continue;
        }
        overlay.color = color;
        let state = overlay.state;
        appearance.overlays.retain(|o| o.icon_state != state);
        let [r, g, b] = color;
        appearance.overlays.push(Overlay {
            color: Some([r, g, b, 255]),
            ..Overlay::new(state)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_atmos::GasMixture;

    #[test]
    fn colors_follow_the_mix() {
        let air = |text: &str, volume| PipeAir::with_gas(GasMixture::parse(text).unwrap(), volume);
        // Empty pipes are black; full oxygen is its blue.
        assert_eq!(gasmix_color(&PipeAir::new(70.0)), [0; 3]);
        assert_eq!(gasmix_color(&air("o2=1000;TEMP=293", 70.0)), [0, 0, 255]);
        // A little gas in a big pipe is dark.
        let dim = gasmix_color(&air("o2=1;TEMP=293", 200.0));
        assert!(dim[2] < 30, "{dim:?}");
        assert_eq!(blend_hsv([0, 0, 255], [255, 255, 0], 0.0), [0, 0, 255]);
        assert_eq!(blend_hsv([0, 0, 255], [255, 255, 0], 1.0), [255, 255, 0]);
    }
}

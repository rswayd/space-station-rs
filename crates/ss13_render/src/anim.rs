//! `.dmi` frame timing: per-frame delays in deciseconds, `loop` count and `rewind`.

use ss13_assets::manifest::IconStateInfo;

use crate::atlas::frame_count;

/// Guards against zero delays, which would make a cycle take no time.
const MIN_DELAY_DS: f32 = 0.1;

fn delay(info: &IconStateInfo, frame: usize) -> f32 {
    info.delays
        .get(frame)
        .copied()
        .unwrap_or(1.0)
        .max(MIN_DELAY_DS)
}

/// Frame order of one cycle: `0..n`, then with rewind `n-2..=1` back.
fn cycle(n: usize, rewind: bool) -> impl Iterator<Item = usize> + Clone {
    let back = if rewind && n > 2 { n - 2 } else { 0 };
    (0..n).chain((1..=back).rev())
}

/// Seconds of one cycle of the state's frames: how long BYOND's `flick` of it shows. A single
/// frame shows for its own delay.
pub fn cycle_secs(info: &IconStateInfo) -> f32 {
    let n = frame_count(info).max(1);
    cycle(n, info.rewind).map(|f| delay(info, f)).sum::<f32>() / 10.0
}

/// Frame shown `elapsed` seconds after the animation started, and seconds until it changes
/// (`None` = never: single frame or finished). A finite `loop_count` stops on the last frame,
/// or on frame 0 with rewind, like BYOND.
pub fn frame_at(info: &IconStateInfo, elapsed: f32) -> (usize, Option<f32>) {
    let n = frame_count(info);
    if n <= 1 {
        return (0, None);
    }
    let order = cycle(n, info.rewind);
    let cycle_ds: f32 = order.clone().map(|f| delay(info, f)).sum();
    let t = elapsed.max(0.0) * 10.0;
    if info.loop_count > 0 && t >= cycle_ds * info.loop_count as f32 {
        return (if info.rewind { 0 } else { n - 1 }, None);
    }
    let mut t = t % cycle_ds;
    for f in order {
        let d = delay(info, f);
        if t < d {
            return (f, Some((d - t) / 10.0));
        }
        t -= d;
    }
    // Float remainder at the very end of the cycle.
    (0, Some(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anim(delays: &[f32], loop_count: u32, rewind: bool) -> IconStateInfo {
        IconStateInfo {
            dmi: "t.dmi".into(),
            state: "a".into(),
            atlas: 0,
            width: 32,
            height: 32,
            dirs: 1,
            rects: vec![[0, 0, 32, 32]; delays.len()],
            delays: delays.to_vec(),
            loop_count,
            rewind,
            movement: false,
        }
    }

    fn frames(info: &IconStateInfo, times_ds: &[f32]) -> Vec<usize> {
        times_ds
            .iter()
            .map(|t| frame_at(info, t / 10.0).0)
            .collect()
    }

    #[test]
    fn single_frame_is_static() {
        assert_eq!(frame_at(&anim(&[], 0, false), 123.0), (0, None));
        assert_eq!(frame_at(&anim(&[5.0], 0, false), 123.0), (0, None));
    }

    #[test]
    fn uneven_delays_loop_forever() {
        let a = anim(&[1.0, 3.0, 2.0], 0, false);
        // Cycle = 6 ds: frame 0 in [0,1), 1 in [1,4), 2 in [4,6).
        assert_eq!(
            frames(&a, &[0.0, 0.9, 1.0, 3.9, 4.0, 5.9, 6.0, 7.5, 600.5]),
            vec![0, 0, 1, 1, 2, 2, 0, 1, 0]
        );
    }

    #[test]
    fn finite_loops_stop_on_last_frame() {
        let a = anim(&[1.0, 1.0, 1.0], 2, false);
        assert_eq!(
            frames(&a, &[2.5, 3.5, 5.5, 6.0, 100.0]),
            vec![2, 0, 2, 2, 2]
        );
    }

    #[test]
    fn rewind_ping_pongs_without_repeating_ends() {
        let a = anim(&[1.0, 1.0, 1.0, 1.0], 0, true);
        // 0 1 2 3 2 1 | 0 ...
        let t: Vec<f32> = (0..8).map(|i| i as f32 + 0.5).collect();
        assert_eq!(frames(&a, &t), vec![0, 1, 2, 3, 2, 1, 0, 1]);
        let once = anim(&[1.0, 1.0, 1.0], 1, true);
        // 0 1 2 1, then rests on 0.
        assert_eq!(
            frames(&once, &[0.5, 1.5, 2.5, 3.5, 4.5, 50.0]),
            vec![0, 1, 2, 1, 0, 0]
        );
    }

    #[test]
    fn a_flick_lasts_one_cycle() {
        assert!((cycle_secs(&anim(&[1.0, 3.0, 2.0], 0, false)) - 0.6).abs() < 1e-5);
        // 0 1 2 1.
        assert!((cycle_secs(&anim(&[1.0, 1.0, 1.0], 0, true)) - 0.4).abs() < 1e-5);
        assert!((cycle_secs(&anim(&[5.0], 0, false)) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn zero_delays_do_not_hang() {
        let a = anim(&[0.0, 0.0], 0, false);
        assert!(frame_at(&a, 10.0).0 < 2);
    }

    #[test]
    fn reports_time_to_next_change() {
        let a = anim(&[1.0, 3.0], 0, false);
        let (f, next) = frame_at(&a, 0.15);
        assert_eq!(f, 1);
        assert!((next.unwrap() - 0.25).abs() < 1e-5);
        assert_eq!(frame_at(&anim(&[1.0, 1.0], 1, false), 5.0), (1, None));
    }
}

//! Load generator: N headless bots join an external server and walk at random, in stages.
//!
//! `cargo run --release -p ss13_bots --example swarm -- <addr> <digest> <stage>...`, e.g.
//! `127.0.0.1:7777 <hex> 10 50 100`: grows to each stage's bot count, holds it `SWARM_HOLD_SECS`
//! (default 60), prints `stage N ready=R` lines, then the next stage.
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use ss13_bots::bot::Bot;
use ss13_shared::Dir;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let addr: SocketAddr = args.next().expect("addr").parse()?;
    let digest = args.next().expect("digest");
    let stages: Vec<usize> = args.map(|a| a.parse()).collect::<Result<_, _>>()?;
    let hold = std::env::var("SWARM_HOLD_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60u64);
    let tick = Duration::from_millis(50);
    let mut bots: Vec<Bot> = Vec::new();
    let mut rng: u64 = 0x9E37_79B9_7F4A_7C15;
    for target in stages {
        while bots.len() < target {
            let name = format!("bot{}", bots.len());
            bots.push(Bot::connect(&name, name.clone(), addr, &digest));
        }
        println!("stage {target} start");
        let end = Instant::now() + Duration::from_secs(hold);
        let mut frame: u64 = 0;
        while Instant::now() < end {
            let started = Instant::now();
            for (i, bot) in bots.iter_mut().enumerate() {
                bot.update();
                // A step about every 0.5 s per bot, staggered.
                if bot.ready() && (frame + i as u64).is_multiple_of(10) {
                    rng ^= rng << 13;
                    rng ^= rng >> 7;
                    rng ^= rng << 17;
                    bot.step(Dir::CARDINALS[(rng % 4) as usize]);
                }
            }
            frame += 1;
            if let Some(rest) = tick.checked_sub(started.elapsed()) {
                std::thread::sleep(rest);
            }
        }
        let ready = bots.iter().filter(|b| b.ready()).count();
        println!("stage {target} ready={ready}");
    }
    Ok(())
}

//! Dev and test aid: artificial latency on everything an app receives, so movement prediction can
//! be checked against a slow link on loopback. Applied where received messages enter Bevy
//! ([`Inbox`]), so the transport itself is untouched.

// Without a side the crate only exports its types.
#![cfg_attr(not(any(feature = "client", feature = "server")), allow(dead_code))]

use core::time::Duration;
use std::collections::VecDeque;
use std::time::Instant;

use bevy::prelude::*;
use tokio::sync::mpsc;

/// Extra delay on every message this app receives. Set it on both ends: 50 ms on the server and
/// 50 ms on the client make a 100 ms round trip. Insert before the net plugin to override;
/// otherwise read from `SS13_SIM_LATENCY_MS` (default 0: no delay).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SimLatency(pub Duration);

pub(crate) fn plugin(app: &mut App) {
    if !app.world().contains_resource::<SimLatency>() {
        let ms = std::env::var("SS13_SIM_LATENCY_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        app.insert_resource(SimLatency(Duration::from_millis(ms)));
    }
    let latency = app.world().resource::<SimLatency>().0;
    if !latency.is_zero() {
        warn!("dev: {latency:?} simulated latency on received messages");
    }
}

/// The Bevy end of a bounded channel of received items stamped with their arrival time.
pub(crate) struct Inbox<T> {
    rx: mpsc::Receiver<(Instant, T)>,
    /// Arrived but not yet due under [`SimLatency`]; empty without it.
    held: VecDeque<(Instant, T)>,
}

impl<T> Inbox<T> {
    pub fn new(rx: mpsc::Receiver<(Instant, T)>) -> Self {
        Self {
            rx,
            held: VecDeque::new(),
        }
    }

    /// Passes every item that arrived at least `latency` ago to `deliver`, in arrival order.
    pub fn drain(&mut self, latency: Duration, mut deliver: impl FnMut(T)) {
        if latency.is_zero() && self.held.is_empty() {
            while let Ok((_, item)) = self.rx.try_recv() {
                deliver(item);
            }
            return;
        }
        while let Ok(item) = self.rx.try_recv() {
            self.held.push_back(item);
        }
        let now = Instant::now();
        while self
            .held
            .front()
            .is_some_and(|(at, _)| *at + latency <= now)
        {
            deliver(self.held.pop_front().expect("checked").1);
        }
    }
}

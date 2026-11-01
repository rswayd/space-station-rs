//! Runs one case on a running server: connects its bots, places them on the case's level, runs
//! the setup steps and the tests in order, then disconnects the bots (their bodies stay on the
//! level, which no later case uses).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClientIntent, DragEnd, NetId};
use ss13_tgui::UiInterface as _;

use crate::bot::{Bot, IconNames};
use crate::case::{Action, Case, DragTarget, Step, StepKind, Target};
use crate::cond::{self, Cond, Subject};
use crate::logcap::CaseLog;
use crate::server::Server;

/// Connecting and placing the bots.
const JOIN_TIMEOUT: f64 = 30.0;
/// One test hook answer, one move ack.
const REPLY_TIMEOUT: f64 = 10.0;
/// Conditions are re-evaluated this often (about a server tick: server queries are not free).
const EVAL_PERIOD: Duration = Duration::from_millis(50);
/// Chat lines per bot shown with a failure.
const FAILURE_CHAT: usize = 8;

#[derive(Clone, Debug)]
pub struct TestResult {
    pub name: String,
    pub failure: Option<String>,
    pub secs: f64,
}

#[derive(Clone, Debug)]
pub struct CaseResult {
    pub name: String,
    pub file: PathBuf,
    pub exclusive: bool,
    /// Worker (server) that ran it.
    pub worker: usize,
    /// The case could not start (server, bots, setup steps): its tests did not run.
    pub error: Option<String>,
    pub tests: Vec<TestResult>,
    pub secs: f64,
    pub log: PathBuf,
}

impl CaseResult {
    pub fn passed(&self) -> bool {
        self.error.is_none() && self.tests.iter().all(|t| t.failure.is_none())
    }
}

pub struct Driver<'a> {
    pub server: &'a mut Server,
    pub bots: Vec<Bot>,
    z: u16,
    icons: &'a IconNames,
    log: &'a mut CaseLog,
    /// `save`d values.
    vars: std::collections::HashMap<String, Value>,
}

impl<'a> Driver<'a> {
    /// Advances the server (at its tick rate) and every bot once.
    pub fn pump(&mut self) {
        self.server.tick_if_due();
        for bot in &mut self.bots {
            bot.update();
        }
        let idle = self
            .server
            .until_next_tick()
            .clamp(Duration::from_micros(500), Duration::from_millis(2));
        std::thread::sleep(idle);
    }

    fn pump_for(&mut self, secs: f64) {
        let end = Instant::now() + Duration::from_secs_f64(secs);
        while Instant::now() < end {
            self.pump();
        }
    }

    /// Until the server ran `ticks` more ticks: an intent sent now has been applied after two.
    fn settle(&mut self, ticks: u32) {
        let start = self.server.ticks();
        while self.server.ticks() < start + u64::from(ticks) {
            self.pump();
        }
    }

    fn tile(&self, (x, y): (i32, i32)) -> TilePos {
        TilePos::new(x, y, self.z)
    }

    /// A test hook command through `bot`'s connection.
    pub fn request(&mut self, bot: usize, command: Value) -> Result<Value, String> {
        let refused = self.bots[bot].observed().refused.len();
        let id = self.bots[bot].request(&command);
        let start = Instant::now();
        loop {
            self.pump();
            let observed = self.bots[bot].observed();
            if let Some(why) = observed.refused.get(refused) {
                return Err(format!("server refused a test hook: {why}"));
            }
            if observed.responses.contains_key(&id) {
                let result = self.bots[bot]
                    .observed_mut()
                    .responses
                    .remove(&id)
                    .expect("checked");
                return result
                    .map_err(|e| format!("{}: {e}", command["op"].as_str().unwrap_or("?")))
                    .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()));
            }
            if start.elapsed().as_secs_f64() > REPLY_TIMEOUT {
                return Err(format!("no answer to {command} in {REPLY_TIMEOUT}s"));
            }
        }
    }

    /// The condition's subject as JSON (`None`: the subject itself is missing).
    fn subject(&mut self, subject: &Subject) -> Result<Option<Value>, String> {
        Ok(match subject {
            Subject::Bot(i) => Some(self.bots[*i].observe(self.icons)),
            Subject::Tile(x, y) => {
                let pos = self.tile((*x, *y));
                Some(self.request(0, json!({"op": "tile", "pos": [pos.x, pos.y, pos.z]}))?)
            }
            Subject::Mob(i) => Some(self.request(*i, json!({"op": "mob"}))?),
            Subject::Obj(x, y, what) => {
                let pos = self.tile((*x, *y));
                let tile = self.request(0, json!({"op": "tile", "pos": [pos.x, pos.y, pos.z]}))?;
                find_entity(&tile, what).cloned()
            }
        })
    }

    /// Whether `cond` holds, and the value it looked at.
    fn eval(&mut self, cond: &Cond) -> Result<(bool, Option<Value>), String> {
        let actual = self
            .subject(&cond.subject)?
            .and_then(|v| cond::select(&v, &cond.path));
        let value = cond::resolve(&cond.value, &|n: &str| self.vars.get(n).cloned())?;
        let resolved = Cond {
            value,
            ..cond.clone()
        };
        Ok((cond::holds(&resolved, actual.as_ref()), actual))
    }

    fn until(&mut self, cond: &Cond, timeout: f64) -> Result<(), String> {
        let start = Instant::now();
        loop {
            let (ok, actual) = self.eval(cond)?;
            if ok {
                return Ok(());
            }
            if start.elapsed().as_secs_f64() > timeout {
                return Err(format!(
                    "timed out after {timeout}s waiting for `{}`; actual: {}",
                    cond.text,
                    shown(actual.as_ref())
                ));
            }
            let next = Instant::now() + EVAL_PERIOD;
            while Instant::now() < next {
                self.pump();
            }
        }
    }

    fn wait_reply(
        &mut self,
        what: &str,
        mut done: impl FnMut(&Self) -> bool,
    ) -> Result<(), String> {
        let start = Instant::now();
        while !done(self) {
            if start.elapsed().as_secs_f64() > REPLY_TIMEOUT {
                return Err(format!("timed out waiting for {what}"));
            }
            self.pump();
        }
        Ok(())
    }

    /// The `NetId` the clicking bot has for the target.
    fn resolve(&mut self, bot: usize, target: &Target) -> Result<NetId, String> {
        let id = match target {
            Target::Tile(x, y) => {
                let pos = self.tile((*x, *y));
                self.bots[bot]
                    .topmost(pos, self.icons)
                    .ok_or_else(|| format!("nothing clickable on ({x}, {y}) is replicated"))?
            }
            Target::Match(x, y, what) => {
                let pos = self.tile((*x, *y));
                let tile =
                    self.request(bot, json!({"op": "tile", "pos": [pos.x, pos.y, pos.z]}))?;
                let entity =
                    find_entity(&tile, what).ok_or_else(|| format!("no {what:?} on ({x}, {y})"))?;
                entity["net_id"]
                    .as_u64()
                    .map(NetId)
                    .ok_or_else(|| format!("{what:?} on ({x}, {y}) is not replicated to the bot"))?
            }
            Target::Bot(other) if *other == bot => {
                self.bots[bot].own_id().ok_or("the bot has no mob")?
            }
            Target::Bot(other) => {
                let mob = self.request(*other, json!({"op": "mob"}))?;
                let (pos, name) = (mob["pos"].clone(), mob["name"].clone());
                let (Some(x), Some(y)) = (pos[0].as_i64(), pos[1].as_i64()) else {
                    return Err(format!("{} has no tile", self.bots[*other].name));
                };
                let pos = self.tile((x as i32, y as i32));
                let tile =
                    self.request(bot, json!({"op": "tile", "pos": [pos.x, pos.y, pos.z]}))?;
                tile["entities"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|e| e["name"] == name && e.get("health").is_some())
                    .and_then(|e| e["net_id"].as_u64())
                    .map(NetId)
                    .ok_or_else(|| {
                        format!("{} is not replicated to the bot", self.bots[*other].name)
                    })?
            }
        };
        if !self.bots[bot].sees(id) {
            return Err(format!("{id:?} is not replicated to the bot"));
        }
        Ok(id)
    }

    fn action(&mut self, bot: usize, action: &Action) -> Result<(), String> {
        match action {
            Action::Move(dirs) => {
                for &dir in dirs {
                    let seq = self.bots[bot].step(dir);
                    self.wait_reply(&format!("the ack of the move {dir:?}"), |d| {
                        d.bots[bot].observed().acked >= seq
                    })?;
                }
                // The ack is sent before the bot gets the replicated move.
                self.settle(1);
                return Ok(());
            }
            Action::Click { target, modifiers } => {
                let target = self.resolve(bot, target)?;
                self.bots[bot].send(ClientIntent::Click {
                    target,
                    modifiers: *modifiers,
                });
            }
            Action::Drag { from, to } => {
                let mut end = |t: &DragTarget| -> Result<DragEnd, String> {
                    Ok(match t {
                        DragTarget::Thing(target) => DragEnd::Atom(self.resolve(bot, target)?),
                        DragTarget::Slot(slot) => DragEnd::Slot(*slot),
                    })
                };
                let (from, to) = (end(from)?, end(to)?);
                self.bots[bot].send(ClientIntent::Drag {
                    from,
                    to,
                    modifiers: Default::default(),
                });
            }
            Action::Slot(slot) => self.bots[bot].send(ClientIntent::ClickSlot(*slot)),
            Action::AltSlot(slot) => self.bots[bot].send(ClientIntent::AltClickSlot(*slot)),
            Action::Say(text) => self.bots[bot].send(ClientIntent::Say(text.clone())),
            Action::Act {
                interface,
                action,
                params,
            } => {
                let window = self.window(bot, interface)?;
                let params = params
                    .iter()
                    .map(|(k, v)| Ok((k.clone(), self.param(v)?)))
                    .collect::<Result<_, String>>()?;
                self.bots[bot].send(ClientIntent::UiAct {
                    window,
                    action: action.clone(),
                    params,
                });
            }
            Action::Zone(zone) => {
                let window = self.window(bot, ss13_game::hud::HealthHudUi::NAME)?;
                self.bots[bot].send(ClientIntent::UiAct {
                    window,
                    action: "zone".into(),
                    params: vec![("zone".into(), zone.clone())],
                });
            }
            Action::Close(interface) => {
                let window = self.window(bot, interface)?;
                self.bots[bot].close_window(window);
                self.bots[bot].observed_mut().windows.remove(&window);
            }
            Action::Teleport(pos) => {
                let pos = self.tile(*pos);
                self.request(bot, json!({"op": "setup", "pos": [pos.x, pos.y, pos.z]}))?;
                self.wait_reply("the teleport", |d| d.bots[bot].pos() == Some(pos))?;
            }
            Action::Hurt(kind, amount) => {
                self.request(bot, json!({"op": "hurt", "kind": kind, "amount": amount}))?;
            }
            Action::Swap => self.bots[bot].send(ClientIntent::SwapHands),
            Action::Drop => self.bots[bot].send(ClientIntent::DropActive),
            Action::Combat => self.bots[bot].send(ClientIntent::ToggleCombatMode),
            Action::Throw => self.bots[bot].send(ClientIntent::ToggleThrowMode),
            Action::Resist => self.bots[bot].send(ClientIntent::Resist),
            Action::StopPull => self.bots[bot].send(ClientIntent::StopPulling),
            Action::LateJoin => self.bots[bot].send(ClientIntent::LateJoinMenu),
            Action::Admin { verb, args } => {
                let ckeys: std::collections::HashMap<String, String> = self
                    .bots
                    .iter()
                    .map(|b| (b.name.clone(), ss13_admin::rights::ckey(&b.token)))
                    .collect();
                let request =
                    crate::admin::request(verb, args, &|name| ckeys.get(name).cloned(), &|ckey| {
                        self.bots[bot].ticket_of(ckey)
                    })?;
                self.bots[bot].send_admin(request);
            }
        }
        self.settle(2);
        Ok(())
    }

    /// An act parameter: as written, or `@<subject>.<path>` read like a condition's left side
    /// (`@alice.windows.Storage.data.items[0].key`, `@obj(4,3,/obj/machinery/door).wires.colors.Bolts`).
    fn param(&mut self, value: &str) -> Result<String, String> {
        let Some(lhs) = value.strip_prefix('@') else {
            return Ok(value.to_owned());
        };
        let names: Vec<String> = self.bots.iter().map(|b| b.name.clone()).collect();
        let (subject, path) = cond::parse_lhs(lhs, &|n: &str| names.iter().position(|b| b == n))?;
        match self
            .subject(&subject)?
            .and_then(|v| cond::select(&v, &path))
        {
            Some(Value::String(s)) => Ok(s),
            Some(v) if !v.is_null() => Ok(v.to_string()),
            _ => Err(format!("{value}: missing")),
        }
    }

    fn window(&self, bot: usize, interface: &str) -> Result<u32, String> {
        self.bots[bot].window(interface).ok_or_else(|| {
            let open: Vec<&str> = self.bots[bot]
                .observed()
                .windows
                .values()
                .map(|w| w.interface.as_str())
                .collect();
            format!("no open {interface} window (open: {open:?})")
        })
    }

    fn step(&mut self, step: &Step) -> Result<(), String> {
        match &step.kind {
            StepKind::Wait(secs) => self.pump_for(*secs),
            StepKind::Until(cond, timeout) => self.until(cond, *timeout)?,
            StepKind::Expect(cond) => {
                let (ok, actual) = self.eval(cond)?;
                if !ok {
                    return Err(format!(
                        "expected `{}`; actual: {}",
                        cond.text,
                        shown(actual.as_ref())
                    ));
                }
            }
            StepKind::Spawn { path, pos } => {
                let pos = self.tile(*pos);
                self.request(
                    0,
                    json!({"op": "spawn", "path": path, "pos": [pos.x, pos.y, pos.z]}),
                )?;
            }
            StepKind::Event(name) => {
                self.request(0, json!({"op": "event", "name": name}))?;
            }
            StepKind::Bot { bot, action } => self.action(*bot, action)?,
            StepKind::Repeat {
                period,
                bot,
                action,
                until,
                timeout,
            } => {
                let start = Instant::now();
                let mut times = 0;
                loop {
                    let (ok, actual) = self.eval(until)?;
                    if ok {
                        break;
                    }
                    if start.elapsed().as_secs_f64() > *timeout {
                        return Err(format!(
                            "timed out after {timeout}s ({times} times) waiting for `{}`; actual: {}",
                            until.text,
                            shown(actual.as_ref())
                        ));
                    }
                    let next = Instant::now() + Duration::from_secs_f64(*period);
                    self.action(*bot, action)?;
                    times += 1;
                    while Instant::now() < next {
                        self.pump();
                    }
                }
            }
            StepKind::Save { name, value } => {
                let (_, actual) = self.eval(value)?;
                let actual = actual.ok_or_else(|| format!("`{}` is missing", value.text))?;
                self.log.line(&format!("  ${name} = {actual}"));
                self.vars.insert(name.clone(), actual);
            }
        }
        Ok(())
    }

    /// Runs steps in order; the failure names the step.
    fn steps(&mut self, steps: &[Step]) -> Result<(), String> {
        for step in steps {
            let start = Instant::now();
            let result = self.step(step);
            let took = start.elapsed().as_secs_f64();
            match &result {
                Ok(()) => self.log.line(&format!(
                    "  ok   {:>4} {} ({took:.2}s)",
                    step.line, step.text
                )),
                Err(e) => self
                    .log
                    .line(&format!("  FAIL {:>4} {}: {e}", step.line, step.text)),
            }
            result.map_err(|e| {
                format!(
                    "line {} `{}`: {e}{}",
                    step.line,
                    step.text,
                    self.chat_tail()
                )
            })?;
        }
        Ok(())
    }

    fn chat_tail(&self) -> String {
        let mut out = String::new();
        for bot in &self.bots {
            let chat = &bot.observed().chat;
            let tail = &chat[chat.len().saturating_sub(FAILURE_CHAT)..];
            out.push_str(&format!("\n    last chat of {}:", bot.name));
            if tail.is_empty() {
                out.push_str(" (none)");
            }
            for line in tail {
                out.push_str(&format!("\n      {line}"));
            }
        }
        out
    }
}

/// The first entity of a tile answer matching `what` (a path prefix, or a name part).
fn find_entity<'v>(tile: &'v Value, what: &str) -> Option<&'v Value> {
    tile["entities"].as_array()?.iter().find(|e| {
        if what.starts_with('/') {
            e["path"].as_str().is_some_and(|p| p.starts_with(what))
        } else {
            e["name"].as_str().is_some_and(|n| n.contains(what))
        }
    })
}

fn shown(value: Option<&Value>) -> String {
    let text = value.map_or_else(|| "missing".to_owned(), Value::to_string);
    if text.chars().count() > 400 {
        format!("{}…", text.chars().take(400).collect::<String>())
    } else {
        text
    }
}

/// Runs `case` on `server` with its map at level `z`.
pub fn run_case(
    server: &mut Server,
    z: u16,
    case: &Case,
    icons: &IconNames,
    log: &mut CaseLog,
) -> (Option<String>, Vec<TestResult>) {
    // Listed before they connect: the server sends a listed admin its rights on connect.
    for b in &case.bots {
        if let Some(keywords) = &b.admin {
            let rights = ss13_admin::rights::keywords(keywords, None, &mut Vec::new())
                | ss13_shared::admin::rights::R_AUTOADMIN;
            server
                .app
                .world_mut()
                .resource_mut::<ss13_admin::server::Admins>()
                .grant(&format!("{}-{}", case.name, b.name), "Bot", rights);
        }
    }
    let mut d = Driver {
        bots: case
            .bots
            .iter()
            .map(|b| {
                let token = format!("{}-{}", case.name, b.name);
                Bot::connect(&b.name, token, server.addr, &server.digest.clone())
            })
            .collect(),
        server,
        z,
        icons,
        log,
        vars: Default::default(),
    };
    let result = start(&mut d, case);
    let outcome = match result {
        Err(e) => {
            d.log.line(&format!("case did not start: {e}"));
            (Some(e), Vec::new())
        }
        Ok(()) => {
            let mut results = Vec::new();
            for test in &case.tests {
                d.log
                    .line(&format!("test {} (line {})", test.name, test.line));
                for bot in &mut d.bots {
                    let mut observed = bot.observed_mut();
                    observed.test_start = observed.chat.len();
                }
                let start = Instant::now();
                let failure = d.steps(&test.steps).err();
                results.push(TestResult {
                    name: test.name.clone(),
                    failure,
                    secs: start.elapsed().as_secs_f64(),
                });
            }
            (None, results)
        }
    };
    if case.time_scale.is_some() {
        let _ = d.request(0, json!({"op": "time_scale", "scale": 1.0}));
    }
    for bot in &d.bots {
        d.log
            .line(&format!("chat of {} ({}):", bot.name, bot.token));
        for line in &bot.observed().chat {
            d.log.line(&format!("  {line}"));
        }
    }
    let server = d.server;
    drop(d.bots);
    // Let the server see the disconnects before the next case connects.
    for _ in 0..4 {
        server.tick_if_due();
        std::thread::sleep(server.until_next_tick());
    }
    let _ = d.log.flush();
    outcome
}

fn start(d: &mut Driver<'_>, case: &Case) -> Result<(), String> {
    let begin = Instant::now();
    while !d.bots.iter().all(Bot::ready) {
        if begin.elapsed().as_secs_f64() > JOIN_TIMEOUT {
            let states: Vec<String> = d
                .bots
                .iter()
                .map(|b| format!("{}: {:?}", b.name, b.state()))
                .collect();
            return Err(format!("bots did not join in {JOIN_TIMEOUT}s: {states:?}"));
        }
        d.pump();
    }
    d.log.line(&format!(
        "{} bots joined in {:.2}s",
        d.bots.len(),
        begin.elapsed().as_secs_f64()
    ));
    if let Some(scale) = case.time_scale {
        d.request(0, json!({"op": "time_scale", "scale": scale}))?;
    }
    for (i, spec) in case.bots.iter().enumerate() {
        let pos = d.tile(spec.pos);
        let mut command = json!({"op": "setup", "pos": [pos.x, pos.y, pos.z], "give": spec.give});
        if let Some(job) = &spec.job {
            command["job"] = json!(job);
        }
        if let Some(antag) = &spec.antag {
            command["antag"] = json!(antag);
        }
        d.request(i, command)
            .map_err(|e| format!("bot {}: {e}", spec.name))?;
    }
    for (i, spec) in case.bots.iter().enumerate() {
        let pos = d.tile(spec.pos);
        d.wait_reply(&format!("{} to appear at {pos:?}", spec.name), |d| {
            d.bots[i].pos() == Some(pos)
        })?;
    }
    // The HUD windows of a new body arrive with its first ticks.
    d.settle(2);
    d.log.line("setup");
    d.steps(&case.setup)
}

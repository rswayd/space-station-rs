//! Behavior trees, after tg `/datum/bt_node` (`code/datums/ai/_ai_bt_node.dm`,
//! `_ai_bt_composites.dm`, `_ai_bt_decorators.dm`, `_ai_behavior.dm`): selectors, sequences,
//! subplans, parallels, decorators with observer aborts, and behavior leaves with their
//! `setup`/`perform`/`finish_action` and perform cooldowns. Nodes keep tg's pre-order execution
//! indices, which the aborts compare with the running leaf's.
//!
//! Decorator conditions are polled every controller tick (tg's fallback when a decorator
//! registers no signals): a change runs tg `on_observed_change` (abort self / lower priority).

use super::behaviors::{Behavior, Perform};
use super::{AiCtx, Key};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Success,
    Failure,
    Running,
}

/// `observer_abort`: `BT_ABORT_NONE`, `SELF`, `LOWER_PRIORITY`, `BOTH`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Abort {
    None,
    SelfOnly,
    Lower,
    Both,
}

impl Abort {
    fn has_self(self) -> bool {
        matches!(self, Self::SelfOnly | Self::Both)
    }
    fn has_lower(self) -> bool {
        matches!(self, Self::Lower | Self::Both)
    }
}

/// Decorator conditions (tg `/datum/bt_node/decorator/*` `check_condition`).
#[derive(Clone, Debug, PartialEq)]
pub enum Cond {
    /// `bb_key_set` / `bb_key_true`: the key holds a truthy value (a living entity, `TRUE`, a
    /// non-empty list, a time).
    KeySet(Key),
    /// `random_chance`: `prob(chance * 100)`.
    RandomChance(f32),
    /// `key_off_cooldown`: the key's time is unset or past.
    KeyOffCooldown(Key),
    /// `carp_should_flee`: something to flee from, and not `BB_BASIC_MOB_STOP_FLEEING`.
    CarpShouldFlee(Key),
    /// `no_humans_watching`: no player's mob within view.
    NoHumansWatching,
    /// `is_at_distance` with `maximum_distance` (no minimum, no reach check): the key's target
    /// exists and is that close.
    AtDistance(Key, u32),
    /// The bot decorators (`crate::bots::ai`).
    Bot(crate::bots::ai::BotCond),
    /// The robot customer's state (`crate::restaurant`).
    Customer(crate::restaurant::CustomerCond),
    /// Pet command, turtle ability and breeding decorators ([`super::fauna`]).
    Fauna(super::fauna::FaunaCond),
    /// Monkey decorators ([`super::monkey`]).
    Monkey(super::monkey::MonkeyCond),
}

impl Cond {
    fn check(&self, ctx: &mut AiCtx) -> bool {
        match *self {
            Self::KeySet(key) => ctx.truthy(key),
            Self::RandomChance(chance) => ctx.prob(chance * 100.0),
            Self::KeyOffCooldown(key) => ctx.bb.time(key).is_none_or(|t| t <= ctx.now),
            Self::CarpShouldFlee(key) => ctx.truthy(key) && !ctx.truthy(Key::StopFleeing),
            Self::NoHumansWatching => !super::behaviors::player_in_view(ctx),
            Self::AtDistance(key, max) => ctx
                .pawn_pos()
                .zip(ctx.target_pos(key))
                .and_then(|(a, b)| a.chebyshev(b))
                .is_some_and(|d| d <= max),
            Self::Bot(ref cond) => cond.check(ctx),
            Self::Customer(cond) => cond.check(ctx),
            Self::Fauna(cond) => cond.check(ctx),
            Self::Monkey(cond) => cond.check(ctx),
        }
    }
}

/// `BT_PARALLEL_*` policies and the parallel's flags.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ParallelOpts {
    pub success_all: bool,
    pub failure_any: bool,
    pub repeat_secondary: bool,
    /// Seconds.
    pub repeat_secondary_delay: f64,
    pub finish_on_primary: bool,
}

#[derive(Debug)]
pub enum Kind {
    Selector {
        children: Vec<Node>,
        running: usize,
    },
    Sequence {
        children: Vec<Node>,
        running: usize,
    },
    Subplan {
        child: Box<Node>,
        loop_success: bool,
        loop_failure: bool,
        loop_delay: f64,
        next_loop: f64,
    },
    Parallel {
        children: Vec<Node>,
        opts: ParallelOpts,
        ready_at: Vec<f64>,
    },
    Decorator {
        cond: Cond,
        invert: bool,
        abort: Abort,
        child: Box<Node>,
        child_active: bool,
        registered: bool,
        last_poll: Option<bool>,
    },
    /// `true_for_time` with `succeed_on_timeout`: the child runs at most `duration` seconds.
    TimeLimit {
        child: Box<Node>,
        duration: f64,
        started: Option<f64>,
        succeed_on_timeout: bool,
    },
    Leaf {
        behavior: Behavior,
        running: bool,
        next_perform: f64,
        failed_last: bool,
    },
}

#[derive(Debug)]
pub struct Node {
    pub index: u32,
    pub last: u32,
    pub kind: Kind,
}

pub fn selector(children: Vec<Node>) -> Node {
    node(Kind::Selector {
        children,
        running: 0,
    })
}

pub fn sequence(children: Vec<Node>) -> Node {
    node(Kind::Sequence {
        children,
        running: 0,
    })
}

/// A subplan: `loop_success`/`loop_failure` restart the child (`BT_SUBPLAN_LOOP_ON_*`).
pub fn subplan(child: Node, loop_success: bool, loop_failure: bool, loop_delay: f64) -> Node {
    node(Kind::Subplan {
        child: Box::new(child),
        loop_success,
        loop_failure,
        loop_delay,
        next_loop: 0.0,
    })
}

pub fn parallel(children: Vec<Node>, opts: ParallelOpts) -> Node {
    node(Kind::Parallel {
        children,
        opts,
        ready_at: Vec::new(),
    })
}

pub fn decorator(cond: Cond, invert: bool, abort: Abort, child: Node) -> Node {
    node(Kind::Decorator {
        cond,
        invert,
        abort,
        child: Box::new(child),
        child_active: false,
        registered: false,
        last_poll: None,
    })
}

pub fn time_limit(duration: f64, succeed_on_timeout: bool, child: Node) -> Node {
    node(Kind::TimeLimit {
        child: Box::new(child),
        duration,
        started: None,
        succeed_on_timeout,
    })
}

pub fn leaf(behavior: Behavior) -> Node {
    node(Kind::Leaf {
        behavior,
        running: false,
        next_perform: 0.0,
        failed_last: false,
    })
}

fn node(kind: Kind) -> Node {
    Node {
        index: 0,
        last: 0,
        kind,
    }
}

/// A controller's tree (tg `behavior_nodes`): roots ticked in order until one runs.
#[derive(Debug, Default)]
pub struct Tree {
    pub roots: Vec<Node>,
}

impl Tree {
    /// tg `finalize_tree`: pre-order indices from 1.
    pub fn new(roots: Vec<Node>) -> Self {
        let mut tree = Self { roots };
        let mut counter = 1;
        for root in &mut tree.roots {
            counter = root.assign(counter);
        }
        tree
    }

    /// tg `SelectBehaviors`.
    pub fn tick(&mut self, ctx: &mut AiCtx) {
        ctx.cancelled = false;
        for root in &mut self.roots {
            root.poll(ctx);
        }
        if ctx.cancelled {
            self.reset(ctx);
        }
        for root in &mut self.roots {
            if root.tick(ctx) == Status::Running {
                break;
            }
        }
        if ctx.cancelled {
            self.reset(ctx);
        }
    }

    /// tg `cancel_current_plan` -> `reset_bt_tick_states`.
    pub fn reset(&mut self, ctx: &mut AiCtx) {
        ctx.active = 0;
        for root in &mut self.roots {
            root.reset(ctx);
        }
    }
}

impl Node {
    fn assign(&mut self, counter: u32) -> u32 {
        self.index = counter;
        let mut next = counter + 1;
        match &mut self.kind {
            Kind::Selector { children, .. }
            | Kind::Sequence { children, .. }
            | Kind::Parallel { children, .. } => {
                for c in children {
                    next = c.assign(next);
                }
            }
            Kind::Subplan { child, .. }
            | Kind::Decorator { child, .. }
            | Kind::TimeLimit { child, .. } => next = child.assign(next),
            Kind::Leaf { .. } => {}
        }
        self.last = next - 1;
        next
    }

    fn tick(&mut self, ctx: &mut AiCtx) -> Status {
        let index = self.index;
        match &mut self.kind {
            Kind::Selector { children, running } => tick_ordered(children, running, ctx, true),
            Kind::Sequence { children, running } => tick_ordered(children, running, ctx, false),
            Kind::Subplan {
                child,
                loop_success,
                loop_failure,
                loop_delay,
                next_loop,
            } => {
                if *loop_delay > 0.0 && *next_loop > ctx.now {
                    return Status::Running;
                }
                let result = child.tick(ctx);
                if ctx.cancelled {
                    return Status::Failure;
                }
                let restart = match result {
                    Status::Running => return Status::Running,
                    Status::Failure => *loop_failure,
                    Status::Success => *loop_success,
                };
                if restart {
                    child.reset(ctx);
                    if *loop_delay > 0.0 {
                        *next_loop = ctx.now + *loop_delay;
                    }
                    return Status::Running;
                }
                *next_loop = 0.0;
                result
            }
            Kind::Parallel {
                children,
                opts,
                ready_at,
            } => {
                let (mut succeeded, mut failed) = (0, 0);
                let mut primary = Status::Running;
                let n = children.len();
                for (i, child) in children.iter_mut().enumerate() {
                    if i > 0
                        && opts.repeat_secondary
                        && opts.repeat_secondary_delay > 0.0
                        && ready_at.get(i).is_some_and(|&t| t > ctx.now)
                    {
                        continue;
                    }
                    let result = child.tick(ctx);
                    if ctx.cancelled {
                        return Status::Failure;
                    }
                    if i == 0 {
                        primary = result;
                        match result {
                            Status::Success => succeeded += 1,
                            Status::Failure => failed += 1,
                            Status::Running => {}
                        }
                    } else if opts.repeat_secondary && result != Status::Running {
                        child.reset(ctx);
                        if opts.repeat_secondary_delay > 0.0 {
                            if ready_at.len() <= i {
                                ready_at.resize(i + 1, 0.0);
                            }
                            ready_at[i] = ctx.now + opts.repeat_secondary_delay;
                        }
                    } else {
                        match result {
                            Status::Success => succeeded += 1,
                            Status::Failure => failed += 1,
                            Status::Running => {}
                        }
                    }
                }
                if opts.finish_on_primary && primary != Status::Running {
                    for child in children.iter_mut().skip(1) {
                        child.reset(ctx);
                    }
                    return primary;
                }
                let fail = if opts.failure_any {
                    failed > 0
                } else {
                    primary == Status::Failure
                };
                if fail {
                    return Status::Failure;
                }
                let success = if opts.success_all {
                    succeeded == n
                } else {
                    primary == Status::Success
                };
                if success {
                    Status::Success
                } else {
                    Status::Running
                }
            }
            Kind::Decorator {
                cond,
                invert,
                child,
                child_active,
                registered,
                last_poll,
                ..
            } => {
                let result = if *child_active {
                    child.tick(ctx)
                } else {
                    let pass = cond.check(ctx) != *invert;
                    // tg registers its observers here; the poll compares against this result.
                    if !*registered {
                        *registered = true;
                        *last_poll = Some(pass);
                    }
                    if pass {
                        child.tick(ctx)
                    } else {
                        Status::Failure
                    }
                };
                if ctx.cancelled {
                    *child_active = false;
                    return Status::Failure;
                }
                *child_active = result == Status::Running;
                result
            }
            Kind::TimeLimit {
                child,
                duration,
                started,
                succeed_on_timeout,
            } => {
                let start = *started.get_or_insert(ctx.now);
                if ctx.now - start >= *duration {
                    child.reset(ctx);
                    *started = None;
                    return if *succeed_on_timeout {
                        Status::Success
                    } else {
                        Status::Failure
                    };
                }
                let result = child.tick(ctx);
                if result != Status::Running {
                    *started = None;
                }
                result
            }
            Kind::Leaf {
                behavior,
                running,
                next_perform,
                failed_last,
            } => {
                if *next_perform > ctx.now {
                    if !*running && *failed_last {
                        return Status::Failure;
                    }
                    ctx.active = index;
                    return Status::Running;
                }
                if !*running {
                    if !behavior.setup(ctx) {
                        return Status::Failure;
                    }
                    *running = true;
                }
                let Perform { delay, outcome } = behavior.perform(ctx);
                if delay {
                    *next_perform = ctx.now + behavior.cooldown();
                }
                match outcome {
                    Some(true) => {
                        *failed_last = false;
                        *running = false;
                        behavior.finish(ctx, true);
                        Status::Success
                    }
                    Some(false) => {
                        *failed_last = true;
                        *running = false;
                        behavior.finish(ctx, false);
                        Status::Failure
                    }
                    None => {
                        ctx.active = index;
                        Status::Running
                    }
                }
            }
        }
    }

    /// tg `reset_tick_state` of this node and its subtree.
    fn reset(&mut self, ctx: &mut AiCtx) {
        match &mut self.kind {
            Kind::Selector { children, running } | Kind::Sequence { children, running } => {
                *running = 0;
                for c in children {
                    c.reset(ctx);
                }
            }
            Kind::Subplan {
                child, next_loop, ..
            } => {
                *next_loop = 0.0;
                child.reset(ctx);
            }
            Kind::Parallel {
                children, ready_at, ..
            } => {
                ready_at.clear();
                for c in children {
                    c.reset(ctx);
                }
            }
            Kind::Decorator {
                child,
                child_active,
                registered,
                last_poll,
                ..
            } => {
                *child_active = false;
                *registered = false;
                *last_poll = None;
                child.reset(ctx);
            }
            Kind::TimeLimit { child, started, .. } => {
                *started = None;
                child.reset(ctx);
            }
            Kind::Leaf {
                behavior, running, ..
            } => {
                if *running {
                    *running = false;
                    behavior.finish(ctx, false);
                }
            }
        }
    }

    /// tg `poll_condition` of every registered decorator with an observer abort.
    fn poll(&mut self, ctx: &mut AiCtx) {
        let (index, last) = (self.index, self.last);
        match &mut self.kind {
            Kind::Selector { children, .. }
            | Kind::Sequence { children, .. }
            | Kind::Parallel { children, .. } => {
                for c in children {
                    c.poll(ctx);
                }
            }
            Kind::Subplan { child, .. } | Kind::TimeLimit { child, .. } => child.poll(ctx),
            Kind::Decorator {
                cond,
                invert,
                abort,
                child,
                registered,
                last_poll,
                ..
            } => {
                if *registered && *abort != Abort::None {
                    let current = cond.check(ctx) != *invert;
                    match *last_poll {
                        None => *last_poll = Some(current),
                        Some(previous) if previous != current => {
                            *last_poll = Some(current);
                            // tg `on_observed_change`.
                            let active = ctx.active;
                            if !current && abort.has_self() && active >= index && active <= last {
                                ctx.cancelled = true;
                            }
                            if current && abort.has_lower() && (active == 0 || active > last) {
                                ctx.cancelled = true;
                            }
                        }
                        Some(_) => {}
                    }
                }
                child.poll(ctx);
            }
            Kind::Leaf { .. } => {}
        }
    }
}

/// Selector (first non-failure wins) and sequence (first non-success wins), resuming at the
/// child that last ran.
fn tick_ordered(
    children: &mut [Node],
    running: &mut usize,
    ctx: &mut AiCtx,
    selector: bool,
) -> Status {
    let stop_on = if selector {
        Status::Failure
    } else {
        Status::Success
    };
    let start = (*running).max(1);
    for i in start..=children.len() {
        let result = children[i - 1].tick(ctx);
        if ctx.cancelled {
            return Status::Failure;
        }
        if result != stop_on {
            *running = if result == Status::Running { i } else { 0 };
            return result;
        }
    }
    *running = 0;
    stop_on
}

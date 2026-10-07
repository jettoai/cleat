//! The single decision thread. Listener procs and the config watcher only send `Event`s here;
//! timers (one pending per key, rescheduling replaces) are kept on this thread too, on the
//! engine's clock.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, SystemTime};

use super::Engine;
use crate::audio::ListenerKind;
use crate::model::MicrophonePermission;
use crate::outvol::writer_log::VolumeWriterEvent;
use crate::reclaim::RouteResponse;

pub enum Event {
    Listener { kind: ListenerKind, received: SystemTime },
    ConfigTouched { received: SystemTime },
    /// The microphone dialog's answer.
    Microphone(MicrophonePermission),
    /// A silence detector changed its verdict.
    LivenessFlip { uid: String, name: String, live: bool },
    /// The routing daemon answered a reclaim request.
    RouteAnswered { name: String, address: String, response: RouteResponse },
    /// A device's output volume changed (the hold's listener, which names the device).
    OutputVolumeChanged { device: u32 },
    /// A line from coreaudiod's writer log.
    VolumeWriter(VolumeWriterEvent),
    /// The writer log ended with this status.
    VolumeWriterExited(i32),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Timer {
    Reconcile { delay_ms: u64 },
    ConfigReload,
    /// The output volume hold's judgement of an open writer streak; never rescheduled by beats.
    StreakJudge,
    /// The writer log's restart after it stopped.
    SourceRestart,
}

/// Which event scheduled a pass, for the trace's latency columns.
#[derive(Clone, Copy, Debug)]
pub struct Origin {
    pub label: &'static str,
    pub received: SystemTime,
    pub delay_ms: u64,
}

#[derive(Default)]
pub struct Scheduler {
    pending: HashMap<Timer, (Duration, Origin)>,
}

impl Scheduler {
    pub fn schedule(&mut self, timer: Timer, deadline: Duration, origin: Origin) {
        self.pending.insert(timer, (deadline, origin));
    }

    pub fn next_deadline(&self) -> Option<Duration> {
        self.pending.values().map(|(d, _)| *d).min()
    }

    pub fn is_pending(&self, timer: Timer) -> bool {
        self.pending.contains_key(&timer)
    }

    pub fn cancel(&mut self, timer: Timer) {
        self.pending.remove(&timer);
    }

    /// Due timers, earliest first.
    pub fn take_due(&mut self, now: Duration) -> Vec<(Timer, Origin)> {
        let mut due: Vec<(Duration, Timer, Origin)> =
            self.pending.iter().filter(|(_, (d, _))| *d <= now).map(|(t, (d, o))| (*d, *t, *o)).collect();
        due.sort_by_key(|(d, _, _)| *d);
        for (_, t, _) in &due {
            self.pending.remove(t);
        }
        due.into_iter().map(|(_, t, o)| (t, o)).collect()
    }
}

impl Engine {
    /// The earliest pending timer, on the engine's clock.
    pub fn next_deadline(&self) -> Option<Duration> {
        self.scheduler.next_deadline()
    }

    /// Whether a reconcile `delay_ms` out is waiting (Swift `pendingReconciles[delay]`).
    pub fn reconcile_pending(&self, delay_ms: u64) -> bool {
        self.scheduler.is_pending(Timer::Reconcile { delay_ms })
    }

    pub fn cancel_reconcile(&mut self, delay_ms: u64) {
        self.scheduler.cancel(Timer::Reconcile { delay_ms });
    }

    /// Fires every timer that is due now.
    pub fn run_due(&mut self) {
        for (timer, origin) in self.scheduler.take_due(self.clock.mono()) {
            self.fire(timer, origin);
        }
    }
}

pub fn run(engine: &mut Engine, rx: &Receiver<Event>) {
    loop {
        let msg = match engine.next_deadline() {
            Some(d) => rx.recv_timeout(d.saturating_sub(engine.clock.mono())),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match msg {
            Ok(ev) => engine.handle(ev),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        engine.run_due();
    }
}

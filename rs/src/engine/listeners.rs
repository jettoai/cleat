//! Listener wiring and event handling. Runs on the engine thread, like the rest of the engine.

use super::run_loop::{Event, Origin, Timer};
use super::Engine;
use crate::audio::{ListenTarget, ListenerKind};
use crate::state::clock::clock_ms;
use std::time::Duration;

/// "The device list settled"; this beat and later ones consume arrivals.
pub const SETTLE_MS: u64 = 500;
pub const RETRY_MS: [u64; 3] = [1000, 3000, 6000];
/// How long a balance change is left to settle before it is judged (Swift `balanceSettle`, fa89ada).
/// Apps change the volume one channel at a time (Parallels 238ms apart) and a device without a
/// main volume element reports balance from the two channels. Below SETTLE_MS so it never spends
/// an arrival; the output volume hold's 300ms is kept below this so a volume revert lands first.
pub const BALANCE_SETTLE_MS: u64 = 400;

/// Whether a scheduled beat is late enough to spend an arrival: the settle beat and later.
pub fn consumes_arrivals(delay_ms: u64) -> bool {
    delay_ms >= SETTLE_MS
}

impl Engine {
    /// Registered once at startup and never removed.
    pub fn attach_system_listeners(&mut self) {
        for kind in [ListenerKind::Devices, ListenerKind::DefaultInput, ListenerKind::DefaultOutput] {
            if let Some(t) = self.system.add_listener(ListenTarget::System(kind)) {
                self.system_tokens.push(t);
            }
        }
    }

    /// Points every per-device listener at what the config and device list now say.
    pub fn rebind_devices(&mut self) {
        for t in self.device_tokens.drain(..) {
            self.system.remove_listener(t);
        }
        let snap = self.system.snapshot(&self.config);
        let mut targets = vec![];
        if let Some(out) = snap.default_output {
            if self.config.balance.is_some() {
                targets.push(ListenTarget::Device { device: out, kind: ListenerKind::Balance, element: 0 });
            }
            if !self.config.reclaim_active().is_empty() {
                targets.push(ListenTarget::Device { device: out, kind: ListenerKind::Running, element: 0 });
            }
        }
        if let Some(out) = snap.default_output {
            // AirPods Max have no main volume element and report each channel instead.
            if !self.config.hold_against_active().is_empty() {
                for element in [0, 1, 2] {
                    targets.push(ListenTarget::Device { device: out, kind: ListenerKind::OutputVolume, element });
                }
            }
        }
        let mut held = self.config.input_volume_devices(&snap.devices);
        held.sort_by_key(|d| d.id);
        for d in held {
            for element in [0, 1, 2] {
                targets.push(ListenTarget::Device { device: d.id, kind: ListenerKind::Volume, element });
            }
        }
        let mut attached = vec![];
        let mut failed = vec![];
        for t in targets {
            let ListenTarget::Device { device, kind, element } = t else { continue };
            let name = format!("{}@{device}.{element}", kind.label());
            match self.system.add_listener(t) {
                Some(tok) => {
                    self.device_tokens.push(tok);
                    attached.push(name);
                }
                None => failed.push(name),
            }
        }
        // Logged only when it changes, so a burst of device events writes it once.
        let line = format!("listeners: [{}] failed: [{}]", attached.join(", "), failed.join(", "));
        if self.listener_summary.as_deref() != Some(line.as_str()) {
            if self.trace {
                self.note(&line);
            }
            self.listener_summary = Some(line);
        }
        // One enumeration serves the listeners and the detectors (Swift `rebindDevices`).
        self.sync_liveness_detectors(&snap);
        self.sync_output_volume_source();
    }

    pub fn handle(&mut self, ev: Event) {
        match ev {
            Event::Listener { kind, received } => {
                if self.trace {
                    self.note(&format!("event: {} (recv {})", kind.label(), clock_ms(received)));
                }
                let origin = |delay_ms| Origin { label: kind.label(), received, delay_ms };
                match kind {
                    ListenerKind::Devices => {
                        self.note("devices: list changed");
                        self.check_returns_now();
                        self.rebind_devices();
                        for d in std::iter::once(SETTLE_MS).chain(RETRY_MS) {
                            self.schedule(d, origin(d));
                        }
                    }
                    ListenerKind::DefaultInput => self.schedule(SETTLE_MS, origin(SETTLE_MS)),
                    ListenerKind::DefaultOutput => {
                        self.check_returns_now();
                        self.rebind_devices();
                        for d in RETRY_MS {
                            self.schedule(d, origin(d));
                        }
                    }
                    ListenerKind::Balance => {
                        self.balance_changed_at = Some(self.clock.mono());
                        self.schedule(BALANCE_SETTLE_MS, origin(BALANCE_SETTLE_MS));
                    }
                    ListenerKind::Running | ListenerKind::Volume => self.schedule(0, origin(0)),
                    ListenerKind::OutputVolume => {}
                }
            }
            Event::ConfigTouched { received } => {
                if self.trace {
                    self.note(&format!("event: config (recv {})", clock_ms(received)));
                }
                let o = Origin { label: "config", received, delay_ms: 300 };
                let deadline = self.clock.mono() + Duration::from_millis(300);
                self.scheduler.schedule(Timer::ConfigReload, deadline, o);
            }
            Event::Microphone(permission) => self.update_microphone(permission),
            Event::LivenessFlip { uid, name, live } => self.liveness_flipped(&uid, &name, live),
            Event::RouteAnswered { name, address, response } => self.route_answered(&name, &address, &response),
            Event::OutputVolumeChanged { device } => self.output_volume_changed(device),
            Event::VolumeWriter(e) => self.volume_writer_event(e),
            Event::VolumeWriterExited(status) => self.volume_writer_exited(status),
        }
    }

    /// Times a headset coming back the moment the output moves, not on the next beat.
    fn check_returns_now(&mut self) {
        let snap = self.system.snapshot(&self.config);
        self.check_reclaim_returns(&snap);
    }

    fn schedule(&mut self, delay_ms: u64, origin: Origin) {
        let deadline = self.clock.mono() + Duration::from_millis(delay_ms);
        self.scheduler.schedule(Timer::Reconcile { delay_ms }, deadline, origin);
    }

    /// One pending reconcile per delay, as a listener would ask for it.
    pub fn schedule_reconcile(&mut self, delay_ms: u64) {
        let received = self.clock.wall();
        self.schedule(delay_ms, Origin { label: "scheduled", received, delay_ms });
    }

    pub fn fire(&mut self, timer: Timer, origin: Origin) {
        match timer {
            Timer::Reconcile { delay_ms } => self.reconcile(consumes_arrivals(delay_ms), Some(origin)),
            Timer::ConfigReload => self.config_file_changed(origin),
            Timer::StreakJudge => self.reconcile(false, Some(origin)),
            Timer::SourceRestart => self.source_restart_due(),
        }
    }
}

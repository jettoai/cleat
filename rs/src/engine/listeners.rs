//! Listener wiring and event handling. Runs on the engine thread, like the rest of the engine.

use super::run_loop::{Event, Origin, Timer};
use super::Engine;
use crate::audio::{ListenTarget, ListenerKind};
use crate::state::clock::clock_ms;

const SETTLE_MS: u64 = 500;
const RETRY_MS: [u64; 3] = [1000, 3000, 6000];

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
            if !self.config.reclaim.is_empty() {
                targets.push(ListenTarget::Device { device: out, kind: ListenerKind::Running, element: 0 });
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
            self.note(&line);
            self.listener_summary = Some(line);
        }
    }

    pub fn handle(&mut self, ev: Event) {
        match ev {
            Event::Listener { kind, received } => {
                self.note(&format!("event: {} (recv {})", kind.label(), clock_ms(received)));
                let origin = |delay_ms| Origin { label: kind.label(), received, delay_ms };
                match kind {
                    ListenerKind::Devices => {
                        self.note("devices: list changed");
                        self.rebind_devices();
                        for d in std::iter::once(SETTLE_MS).chain(RETRY_MS) {
                            self.schedule(d, origin(d));
                        }
                    }
                    ListenerKind::DefaultInput => self.schedule(SETTLE_MS, origin(SETTLE_MS)),
                    ListenerKind::DefaultOutput => {
                        self.rebind_devices();
                        for d in RETRY_MS {
                            self.schedule(d, origin(d));
                        }
                    }
                    ListenerKind::Balance | ListenerKind::Running | ListenerKind::Volume => self.schedule(0, origin(0)),
                }
            }
            Event::ConfigTouched { received } => {
                self.note(&format!("event: config (recv {})", clock_ms(received)));
                let o = Origin { label: "config", received, delay_ms: 300 };
                self.scheduler.schedule(Timer::ConfigReload, 300, o);
            }
        }
    }

    fn schedule(&mut self, delay_ms: u64, origin: Origin) {
        self.scheduler.schedule(Timer::Reconcile { delay_ms }, delay_ms, origin);
    }

    pub fn fire(&mut self, timer: Timer, origin: Origin) {
        match timer {
            Timer::Reconcile { delay_ms } => self.reconcile(delay_ms >= SETTLE_MS, Some(origin)),
            Timer::ConfigReload => self.config_file_changed(origin),
        }
    }
}

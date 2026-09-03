import AudioToolbox
import CoreAudio
import Foundation

/// CoreAudio listener wiring and the reconcile scheduler. Split out of Engine.swift to keep both
/// files short; every function here runs on `Engine.queue`, same as the rest of the engine.
extension Engine {

    // MARK: - System listeners

    /// Registered once at startup and never removed: these three properties exist for as long as
    /// the process does.
    func attachSystemListeners() {
        systemTokens.append(system.addSystemListener(
            selector: kAudioHardwarePropertyDevices, queue: queue
        ) { [weak self] in self?.devicesChanged() })

        systemTokens.append(system.addSystemListener(
            selector: kAudioHardwarePropertyDefaultInputDevice, queue: queue
        ) { [weak self] in self?.defaultInputChanged() })

        systemTokens.append(system.addSystemListener(
            selector: kAudioHardwarePropertyDefaultOutputDevice, queue: queue
        ) { [weak self] in self?.defaultOutputChanged() })
    }

    private func devicesChanged() {
        note("devices: list changed")
        rebindDevices()
        scheduleReconcile(after: Engine.settleBeat)
        Engine.retryBeats.forEach(scheduleReconcile(after:))
    }

    private func defaultInputChanged() {
        // Half a second, because a device that has just appeared is not immediately selectable -
        // setting the default input on it returns success and then quietly does not stick.
        scheduleReconcile(after: Engine.settleBeat)
    }

    private func defaultOutputChanged() {
        // The balance listener is bound to a specific device, so it moves with the default output.
        rebindDevices()
        Engine.retryBeats.forEach(scheduleReconcile(after:))
    }

    // MARK: - Per-device listeners

    /// Points every per-device listener and every silence detector at whatever the config and the
    /// device list now say. Called whenever either could have changed.
    ///
    /// One enumeration serves both: reading the device list is synchronous IPC to coreaudiod, and
    /// a flapping cable delivers these events in bursts.
    func rebindDevices() {
        let snapshot = system.snapshot(config: config)
        attachDeviceListeners(snapshot)
        syncLivenessDetectors(snapshot)
    }

    private func attachDeviceListeners(_ snapshot: DeviceSnapshot) {
        for token in deviceTokens { system.removeListener(token) }
        deviceTokens.removeAll()

        if config.balance != nil, let output = snapshot.defaultOutput {
            deviceTokens.append(system.addDeviceListener(
                device: output,
                selector: kAudioHardwareServiceDeviceProperty_VirtualMainBalance,
                scope: kAudioObjectPropertyScopeOutput,
                element: kAudioObjectPropertyElementMain,
                queue: queue
            ) { [weak self] in self?.scheduleReconcile(after: 0) })
        }

        // Every device the volume rule has a target for, which with a `"*"` wildcard is every
        // input device present: a gain nobody listens to is only pulled back on the next
        // unrelated event, which is exactly the drift this rule exists to catch.
        let held = snapshot.devices
            .filter { $0.hasInput && config.inputVolumeTarget(for: $0) != nil }
            .sorted { $0.id < $1.id }
        for device in held {
            // Main plus both channels: a device that has no main volume element reports changes
            // per channel instead, and registering for a property a device lacks is a no-op.
            // Several callbacks for one change collapse into a single reconcile below.
            for element in [kAudioObjectPropertyElementMain, 1, 2] {
                deviceTokens.append(system.addDeviceListener(
                    device: device.id,
                    selector: kAudioDevicePropertyVolumeScalar,
                    scope: kAudioObjectPropertyScopeInput,
                    element: AudioObjectPropertyElement(element),
                    queue: queue
                ) { [weak self] in self?.scheduleReconcile(after: 0) })
            }
        }
    }

    // MARK: - Liveness

    /// Brings the running silence detectors in line with the config and the device list: one
    /// detector per configured device that is actually plugged in, none for anything else.
    ///
    /// A device is keyed by UID rather than by id because ids are reassigned on replug - the same
    /// receiver coming back with a different id has to get a new detector, not keep pointing an
    /// IOProc at a number that now means something else.
    ///
    /// Safe to call as often as there are events: a device that already has the right detector is
    /// not touched, so `reconcile` can use this as the retry for a `start()` that failed.
    func syncLivenessDetectors(_ snapshot: DeviceSnapshot) {
        guard microphone.isGranted else {
            // Without the microphone permission an IOProc reads silence forever, which would look
            // exactly like a transmitter that is switched off. Better to measure nothing and let
            // the input rule behave as it did before liveness existed.
            livenessUnavailable.removeAll()
            stopAllLivenessDetectors(reason: "no microphone permission")
            return
        }

        var wanted: [String: (device: AudioDevice, zeroSeconds: Double)] = [:]
        for (entry, settings) in config.liveness {
            guard let device = snapshot.device(matching: entry, input: true) else { continue }
            wanted[device.uid] = (device, settings.zeroSeconds)
        }
        // A device that is no longer asked for starts its next spell with a clean slate, so an
        // unplug and replug reports a failure again rather than failing silently.
        livenessUnavailable.formIntersection(wanted.keys)

        for (uid, detector) in livenessDetectors {
            // Keep only detectors that still match what is asked for, down to the threshold: a
            // device id changes on replug and zeroSeconds changes on a config edit, and neither
            // can be applied to a detector that is already running.
            if let wanted = wanted[uid],
               wanted.device.id == detector.deviceID,
               wanted.zeroSeconds == detector.zeroSeconds {
                continue
            }
            detector.stop()
            livenessDetectors[uid] = nil
            livenessState[uid] = nil
            note("liveness: \(detector.name) -> stopped")
        }

        for (uid, entry) in wanted.sorted(by: { $0.key < $1.key }) where livenessDetectors[uid] == nil {
            let name = entry.device.name
            let detector = makeDetector(
                entry.device,
                system.nominalSampleRate(entry.device.id) ?? 48_000,
                entry.zeroSeconds,
                queue
            ) { [weak self] isLive in
                self?.livenessFlipped(uid: uid, name: name, isLive: isLive)
            }

            if detector.start() {
                livenessDetectors[uid] = detector
                livenessState[uid] = .measuring
                livenessUnavailable.remove(uid)
                note("liveness: \(name) -> measuring")
            } else if livenessUnavailable.insert(uid).inserted {
                // Once per device, not once per beat: the next reconcile tries again, and a
                // receiver that stays shut would otherwise write this line all day.
                note("liveness: \(name) -> unavailable (could not open input)")
            }
        }
    }

    private func stopAllLivenessDetectors(reason: String) {
        guard !livenessDetectors.isEmpty else { return }
        for (uid, detector) in livenessDetectors {
            detector.stop()
            livenessState[uid] = nil
            note("liveness: \(detector.name) -> stopped (\(reason))")
        }
        livenessDetectors.removeAll()
    }

    private func livenessFlipped(uid: String, name: String, isLive: Bool) {
        livenessState[uid] = isLive ? .live : .silent
        note("liveness: \(name) -> \(isLive ? "live" : "silent")")
        reconcile()
    }

    // MARK: - Scheduling

    /// One pending reconcile per delay. A plug-in event fires several listeners at once and each
    /// asks for the same beats; without this the engine would run the same pass four times.
    func scheduleReconcile(after delay: TimeInterval) {
        pendingReconciles[delay]?.cancel()

        let work = DispatchWorkItem { [weak self] in
            guard let self else { return }
            pendingReconciles[delay] = nil
            reconcile(consumingArrivals: Engine.consumesArrivals(after: delay))
        }
        pendingReconciles[delay] = work
        queue.asyncAfter(deadline: .now() + delay, execute: work)
    }
}

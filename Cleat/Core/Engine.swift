import CoreAudio
import Foundation
import ServiceManagement

/// The one place decisions are made.
///
/// Every CoreAudio listener, the config watcher and the silence detectors deliver onto
/// `queue`, a serial queue, and nothing touches engine state from anywhere else. That is what lets
/// the rules be pure functions of a snapshot: by the time they run, no other event is halfway
/// through changing what they are looking at.
final class Engine: @unchecked Sendable {

    /// Beats after a device change. The 0.5s one is "the device list settled"; 1/3/6 are for
    /// balance, which reads back nil for a second or more after a Bluetooth reconnect. Carried
    /// over from the Hammerspoon version, where they were measured rather than guessed.
    static let settleBeat: TimeInterval = 0.5
    static let retryBeats: [TimeInterval] = [1, 3, 6]

    /// How many log lines `cleat status` shows.
    private static let recentEventLimit = 20

    let queue = DispatchQueue(label: "ai.jetto.cleat.engine")
    let system: AudioSystem
    private let log: EventLog

    // Everything below is touched on `queue` only. Internal rather than private so the listener
    // wiring can live in Engine+Listeners.swift without a second queue-discipline story.
    var config = Config.disabled
    var configState = "missing"
    var microphone = MicrophonePermission.pending
    /// Silence verdict per device UID, owned here because the detectors are.
    var livenessState: [String: Liveness] = [:]
    var livenessDetectors: [String: any LivenessDetecting] = [:]
    /// UIDs whose detector could not be started. Membership is what keeps the retry quiet: the
    /// failure is logged when the device joins this set, not on every beat that tries again.
    var livenessUnavailable: Set<String> = []
    /// The device UIDs the previous reconcile saw, or nil before the first one. What makes
    /// `snapshot.arrived` an edge rather than a state.
    private var previousDeviceUIDs: Set<String>?
    var systemTokens: [ListenerToken] = []
    var deviceTokens: [ListenerToken] = []
    var pendingReconciles: [TimeInterval: DispatchWorkItem] = [:]
    private var recentEvents: [String] = []
    private var watcher: ConfigWatcher?

    // Reclaim bookkeeping, all keyed by Bluetooth address. It lives here rather than in the rule
    // because it is memory of what was asked and when, which a pure function must not have.
    /// The earliest the next request for this headset may go out.
    var reclaimNextAttempt: [String: Date] = [:]
    /// Requests sent and not yet answered. One per headset at a time, which is what bounds the
    /// client's parked-request table.
    var reclaimInFlight: Set<String> = []
    /// Headsets whose "the phone has it" line is already in the log. Cleared when the headset
    /// comes back, so the next spell reports for itself.
    var reclaimHeldLogged: Set<String> = []
    /// Whether the "no routing service" line has been written. Once is enough.
    var reclaimUnavailableLogged = false

    private let configURL: URL
    private let statusURL: URL
    let makeDetector: LivenessDetectorFactory
    let routing: any RouteRequesting
    let bluetooth: any BluetoothInventory
    /// Injectable so the throttle and the backoff can be tested without waiting a minute.
    let now: () -> Date

    /// The detector the daemon uses. Injectable so the engine's detector bookkeeping can be tested
    /// without opening a real input.
    static let liveDetector: LivenessDetectorFactory = { device, sampleRate, zeroSeconds, queue, onFlip in
        ZeroSignalDetector(
            device: device.id,
            uid: device.uid,
            name: device.name,
            sampleRate: sampleRate,
            zeroSeconds: zeroSeconds,
            queue: queue,
            onFlip: onFlip
        )
    }

    init(
        system: AudioSystem = CoreAudioSystem(),
        log: EventLog = EventLog(),
        configURL: URL = Paths.configURL,
        statusURL: URL = Paths.statusURL,
        makeDetector: @escaping LivenessDetectorFactory = Engine.liveDetector,
        routing: any RouteRequesting = SmartRoutingClient(),
        bluetooth: any BluetoothInventory = IOBluetoothPairings(),
        now: @escaping () -> Date = Date.init
    ) {
        self.system = system
        self.log = log
        self.configURL = configURL
        self.statusURL = statusURL
        self.makeDetector = makeDetector
        self.routing = routing
        self.bluetooth = bluetooth
        self.now = now
    }

    // MARK: - Lifecycle

    func start(microphone permission: MicrophonePermission) {
        queue.async { [self] in
            microphone = permission
            loadConfig()
            note("engine started (config: \(configState), microphone: \(permission.label))")
            syncLaunchAtLogin()
            attachSystemListeners()
            rebindDevices()

            let watcher = ConfigWatcher(url: configURL, queue: queue) { [weak self] in
                self?.configFileChanged()
            }
            self.watcher = watcher
            watcher.start()

            // The first pass is the baseline: it records what was already plugged in, so nothing
            // present at launch counts as having just arrived.
            reconcile(consumingArrivals: true)
        }
    }

    /// The answer to the microphone dialog, which arrives after the engine is already running: the
    /// four rules that need no microphone must not wait behind a dialog nobody has looked at yet.
    /// Silence detection is the only thing the permission gates, so this is where its detectors are
    /// attached or torn down. An answer that changes nothing is not worth a line in the log.
    func updateMicrophone(_ permission: MicrophonePermission) {
        queue.async { [self] in
            guard permission != microphone else { return }
            microphone = permission
            note("microphone: \(permission.label)")

            rebindDevices()
            reconcile()
        }
    }

    // MARK: - Config

    /// Reads the config file. A file that is malformed or out of range never replaces a good one:
    /// the rule that was in force stays in force, and the reason lands in `configState` where
    /// `cleat status` shows it. Before any successful load that means everything is off. A file
    /// that is *missing* is different: removing the config is how a user says "enforce nothing",
    /// so every rule goes off (design.md section 3).
    private func loadConfig() {
        guard FileManager.default.fileExists(atPath: configURL.path) else {
            configState = "missing"
            config = .disabled
            return
        }
        do {
            config = try Config.load(from: configURL)
            configState = "ok"
        } catch {
            let reason = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
            configState = "invalid (\(reason))"
        }
    }

    private func configFileChanged() {
        let previous = config
        let previousState = configState
        loadConfig()

        guard config != previous || configState != previousState else { return }
        note("config: reloaded (\(configState))")

        syncLaunchAtLogin()
        rebindDevices()
        reconcile()
    }

    /// The dev build never touches login items: it shares no bundle id with the release build, so
    /// registering it would leave a second Cleat starting at login on the developer's machine.
    private func syncLaunchAtLogin() {
        guard Bundle.main.bundleIdentifier?.hasSuffix(".dev") != true else { return }
        guard configState == "ok" else { return }

        let service = SMAppService.mainApp
        let wanted = config.launchAtLogin
        guard wanted != (service.status == .enabled) else { return }

        do {
            if wanted { try service.register() } else { try service.unregister() }
            note("launchAtLogin: \(wanted ? "registered" : "unregistered")")
        } catch {
            note("launchAtLogin: \(wanted ? "register" : "unregister") failed (\(error.localizedDescription))")
        }
    }

    // MARK: - Reconcile

    /// `consumingArrivals` is what keeps a device arrival from being spent before the device can
    /// be selected. A zero-delay beat - the volume and balance listeners ask for one - can land
    /// while a device that has just appeared is still not selectable, and `setDefaultOutput` on
    /// such a device returns success without sticking. Those beats see the arrival and may act on
    /// it, but do not mark it as seen, so the 0.5s settle beat gets the same arrival and the same
    /// chance.
    ///
    /// Not consuming is the default because everything that reconciles for a reason of its own -
    /// a liveness flip, the permission answer, a config reload - can land inside that same window
    /// and has no idea whether the device list has settled. Only the scheduler knows how long it
    /// waited, so it is the one that passes `Engine.consumesArrivals(after:)`. `start` is the
    /// exception: it consumes to lay down the first mark, because until there is one, every
    /// arrival compares against nothing and is dropped.
    func reconcile(consumingArrivals: Bool = false) {
        var snapshot = system.snapshot(config: config)
        snapshot.arrived = arrivals(in: snapshot, consuming: consumingArrivals)
        // Detectors are brought in line here as well as on a device change, because `start()` can
        // fail on a device that is listed but not yet ready to be opened. The beats that already
        // follow every device event are the retry: devices that are measuring are left alone, so a
        // steady state costs one dictionary walk and no HAL call.
        syncLivenessDetectors(snapshot)
        snapshot.liveness = livenessForRules(snapshot)

        let inputActions = InputPinRule.reconcile(snapshot, config)
        // Headphones that just connected outrank the priority list, and the question asked here is
        // "did a headset arrive", not "did the takeover rule write something". The rule stays quiet
        // when macOS has already moved the output to the arriving headset, and reading that silence
        // as permission to run the priority list is how the output ends up back on the speakers
        // half a second after it reached the headphones.
        let outputActions = HeadphonesTakeoverRule.hasEligibleArrival(snapshot, config)
            ? HeadphonesTakeoverRule.reconcile(snapshot, config)
            : OutputPinRule.reconcile(snapshot, config)
        // Balance belongs to a specific device, and this pass may be about to move the default
        // output somewhere else. Writing it now would set the balance of the device being left;
        // the 'dOut' listener brings us straight back here against the new one.
        let balanceActions = outputActions.isEmpty ? BalanceRule.reconcile(snapshot, config) : []
        let volumeActions = InputVolumeRule.reconcile(snapshot, config)
        // Last, and not a CoreAudio write at all: a headset that is not in the device list cannot
        // be pinned, taken over or balanced, so this is the one rule with nothing to say about the
        // devices the four above just decided on.
        let reclaimActions = reclaimRequests(snapshot)

        for action in inputActions + outputActions + balanceActions + volumeActions + reclaimActions {
            apply(action)
        }
        writeStatus(snapshot)
    }

    /// Which devices are new since the last pass, and the bookkeeping that decides it.
    ///
    /// A consuming pass moves the mark: the next pass compares against what this one saw, so one
    /// plug-in produces one arrival across the 0.5/1/3/6 beats that follow it rather than four. A
    /// pass that is not consuming reports the same arrival and leaves the mark where it was, which
    /// is how a beat that lands before the device is selectable gets to try without spending the
    /// only chance (see `reconcile(consumingArrivals:)`).
    ///
    /// Two consequences worth naming. Headphones already connected when Cleat starts never arrive:
    /// the first pass has nothing to compare against, and a daemon that grabs the output every
    /// time it is restarted is worse than one that waits for the next reconnect. And once a
    /// consuming pass has been through, a failed `setDefaultOutput` is not retried - no write in
    /// Cleat is - so the next connect is the next chance.
    private func arrivals(in snapshot: DeviceSnapshot, consuming: Bool) -> Set<String> {
        let present = Set(snapshot.devices.map(\.uid))
        defer { if consuming { previousDeviceUIDs = present } }
        guard let previous = previousDeviceUIDs else { return [] }
        return present.subtracting(previous)
    }

    /// Whether a scheduled beat is late enough to spend an arrival. The settle beat is defined as
    /// "the device list has settled", so it and everything after it consume; the zero-delay beats
    /// the volume and balance listeners ask for come too early to be the only attempt.
    static func consumesArrivals(after delay: TimeInterval) -> Bool { delay >= settleBeat }

    /// What the rules are told about silence, which is not always what the detectors know.
    ///
    /// While the dialog is unanswered no detector has run, so every configured device would be
    /// missing from `livenessState` - and a missing key means "present" to the input rule, which
    /// would pin the input to a receiver that may well be switched off, then move it back three
    /// seconds after the permission lands. That is the startup double switch the measuring gate
    /// exists to prevent, so an unmeasured device reads as `measuring` until there is an answer:
    /// it may keep the input it already holds, but nothing switches to it. A refusal is different -
    /// it is a decision that liveness is off, and the devices go back to being untracked.
    private func livenessForRules(_ snapshot: DeviceSnapshot) -> [String: Liveness] {
        guard microphone == .pending else { return livenessState }

        var liveness = livenessState
        for entry in config.liveness.keys {
            guard let device = snapshot.device(matching: entry, input: true) else { continue }
            if liveness[device.uid] == nil { liveness[device.uid] = .measuring }
        }
        return liveness
    }

    /// Writes one action back. A failure is logged with its `OSStatus` and dropped - the next
    /// event reconciles again, and a retry loop against a device that is disappearing is how a
    /// daemon ends up fighting the system.
    private func apply(_ action: Action) {
        // The one action that is not a write: it is a question for another daemon, and what
        // happened is only known when the answer arrives, which is where its log line is written.
        if case .requestRoute(let name, let address, let reason) = action {
            requestRoute(name: name, address: address, reason: reason)
            return
        }

        let status: OSStatus
        switch action {
        case .setDefaultInput(let device, _):
            status = system.setDefaultInput(device)
        case .setDefaultOutput(let device, _):
            status = system.setDefaultOutput(device)
        case .setBalance(let device, let value, _):
            status = system.setBalance(device, value)
        case .setInputVolume(let device, let value, _):
            status = system.setInputVolume(device, value)
        case .requestRoute:
            return  // handled above, before there was an OSStatus to talk about
        }

        if status == noErr {
            note("\(action.label): \(action.reason)")
        } else {
            note("\(action.label): \(action.reason) failed (OSStatus \(status))")
        }
    }

    /// Appends to the event log and to the ring `cleat status` prints.
    func note(_ message: String) {
        let line = log.append(message)
        recentEvents.append(line)
        if recentEvents.count > Self.recentEventLimit {
            recentEvents.removeFirst(recentEvents.count - Self.recentEventLimit)
        }
    }

    // MARK: - Status

    private func writeStatus(_ snapshot: DeviceSnapshot) {
        StatusStore.write(Status(
            configState: configState,
            microphone: microphone.label,
            defaultInput: snapshot.defaultInput.flatMap { snapshot.device(id: $0)?.name },
            defaultOutput: snapshot.defaultOutput.flatMap { snapshot.device(id: $0)?.name },
            rules: ruleSummaries(snapshot),
            liveness: livenessSummaries(snapshot),
            recentEvents: recentEvents
        ), to: statusURL)
    }

    private func ruleSummaries(_ snapshot: DeviceSnapshot) -> [String: String] {
        var rules: [String: String] = [:]

        rules["inputPin"] = Self.pinSummary(config.input, blocked: config.blockedInput)
        rules["outputPin"] = Self.pinSummary(config.output, blocked: config.blockedOutput)

        rules["headphones"] = config.headphonesTakeOver
            ? "on (bluetooth output takes over when it connects)"
            : "off"

        rules["reclaim"] = reclaimSummary()

        if let balance = config.balance {
            let current = snapshot.outputBalance.map { String(format: "%.2f", $0) } ?? "unreadable"
            rules["balance"] = String(format: "on (target %.2f, now %@)", balance, current)
        } else {
            rules["balance"] = "off"
        }

        if config.inputVolume.isEmpty {
            rules["inputVolume"] = "off"
        } else if let wildcard = config.inputVolume[Config.inputVolumeWildcard] {
            rules["inputVolume"] = wildcardVolumeSummary(snapshot, default: wildcard)
        } else {
            let parts = config.inputVolume.sorted { $0.key < $1.key }.map { entry, wanted -> String in
                guard let device = snapshot.device(matching: entry, input: true),
                      let current = snapshot.inputVolumes[device.id] else {
                    return String(format: "%@ %.0f%% (absent)", entry, wanted)
                }
                return String(format: "%@ %.0f%% (now %.0f%%)", entry, wanted, Double(current) * 100)
            }
            rules["inputVolume"] = "on (\(parts.joined(separator: ", ")))"
        }

        return rules
    }

    /// The `inputPin` and `outputPin` lines, which say the same two things on both sides: the
    /// priority list, and the blocked list when there is one. An empty priority list is the rule
    /// being off.
    private static func pinSummary(_ priority: [String], blocked: [String]) -> String {
        guard !priority.isEmpty else {
            // An empty priority list turns the whole rule off, blocked list and all. Saying only
            // "off" would leave someone who wrote a blocked list believing it is in force.
            return blocked.isEmpty ? "off" : "off (blocked list needs a priority list)"
        }
        var summary = "on (\(priority.joined(separator: ", ")))"
        if !blocked.isEmpty {
            summary += ", blocked: \(blocked.joined(separator: ", "))"
        }
        return summary
    }

    /// The `inputVolume` line when a wildcard is in force: the default first, then every input
    /// device present, in the order the rule acts on them, and last the named entries that match
    /// no present device.
    ///
    /// Each device is asked the same question the rule asks - `inputVolumeTarget` - so what the
    /// status says is what is being enforced. Walking the devices rather than the config entries
    /// is what makes that true when one name covers two devices: both are held to the override,
    /// and both are reported that way.
    private func wildcardVolumeSummary(_ snapshot: DeviceSnapshot, default wildcard: Double) -> String {
        var parts = snapshot.devices
            .filter(\.hasInput)
            .sorted(by: AudioDevice.byName)
            .compactMap { device -> String? in
                guard let wanted = config.inputVolumeTarget(for: device) else { return nil }
                guard let current = snapshot.inputVolumes[device.id] else {
                    // Here, but its gain could not be read this pass, so nothing is being held to
                    // the target. That is not the same as not being plugged in.
                    return String(format: "%@ %.0f%% (unreadable)", device.name, wanted)
                }
                return String(
                    format: "%@ %.0f%% (now %.0f%%)", device.name, wanted, Double(current) * 100
                )
            }

        parts += config.inputVolume
            .filter {
                $0.key != Config.inputVolumeWildcard
                    && snapshot.device(matching: $0.key, input: true) == nil
            }
            .sorted { $0.key < $1.key }
            .map { String(format: "%@ %.0f%% (absent)", $0.key, $0.value) }

        guard !parts.isEmpty else { return String(format: "on (default %.0f%%)", wildcard) }
        return String(format: "on (default %.0f%%; %@)", wildcard, parts.joined(separator: ", "))
    }

    private func livenessSummaries(_ snapshot: DeviceSnapshot) -> [String: String] {
        var summaries: [String: String] = [:]
        for entry in config.liveness.keys {
            guard let device = snapshot.device(matching: entry, input: true) else {
                summaries[entry] = "absent"
                continue
            }
            guard microphone.isGranted else {
                summaries[entry] = microphone == .pending
                    ? "awaiting microphone permission"
                    : "disabled (no microphone permission)"
                continue
            }
            switch livenessState[device.uid] {
            case .measuring: summaries[entry] = "measuring"
            case .live: summaries[entry] = "live"
            case .silent: summaries[entry] = "silent"
            // A listed, plugged-in device with no detector means the input could not be
            // opened; the rule treats it as present.
            case nil: summaries[entry] = "unavailable"
            }
        }
        return summaries
    }
}

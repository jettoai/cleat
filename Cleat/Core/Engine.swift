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
    var microphone = "not determined"
    var microphoneGranted = false
    /// Silence verdict per device UID, owned here because the detectors are.
    var livenessState: [String: Liveness] = [:]
    var livenessDetectors: [String: ZeroSignalDetector] = [:]
    var systemTokens: [ListenerToken] = []
    var deviceTokens: [ListenerToken] = []
    var pendingReconciles: [TimeInterval: DispatchWorkItem] = [:]
    private var recentEvents: [String] = []
    private var watcher: ConfigWatcher?

    init(system: AudioSystem = CoreAudioSystem(), log: EventLog = EventLog()) {
        self.system = system
        self.log = log
    }

    // MARK: - Lifecycle

    func start(microphoneGranted: Bool, microphoneState: String) {
        queue.async { [self] in
            self.microphoneGranted = microphoneGranted
            microphone = microphoneState
            loadConfig()
            note("engine started (config: \(configState), microphone: \(microphone))")
            syncLaunchAtLogin()
            attachSystemListeners()
            rebindDevices()

            let watcher = ConfigWatcher(queue: queue) { [weak self] in self?.configFileChanged() }
            self.watcher = watcher
            watcher.start()

            reconcile()
        }
    }

    // MARK: - Config

    /// Reads the config file. A file that is missing, malformed or out of range never replaces a
    /// good one: the rule that was in force stays in force, and the reason lands in `configState`
    /// where `cleat status` shows it. Before any successful load that means everything is off.
    private func loadConfig() {
        guard FileManager.default.fileExists(atPath: Paths.configURL.path) else {
            configState = "missing"
            config = .disabled
            return
        }
        do {
            config = try Config.load(from: Paths.configURL)
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

    func reconcile() {
        var snapshot = system.snapshot(config: config)
        snapshot.liveness = livenessState

        let inputActions = InputPinRule.reconcile(snapshot, config)
        let outputActions = OutputPinRule.reconcile(snapshot, config)
        // Balance belongs to a specific device, and this pass may be about to move the default
        // output somewhere else. Writing it now would set the balance of the device being left;
        // the 'dOut' listener brings us straight back here against the new one.
        let balanceActions = outputActions.isEmpty ? BalanceRule.reconcile(snapshot, config) : []
        let volumeActions = InputVolumeRule.reconcile(snapshot, config)

        for action in inputActions + outputActions + balanceActions + volumeActions {
            apply(action)
        }
        writeStatus(snapshot)
    }

    /// Writes one action back. A failure is logged with its `OSStatus` and dropped - the next
    /// event reconciles again, and a retry loop against a device that is disappearing is how a
    /// daemon ends up fighting the system.
    private func apply(_ action: Action) {
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
            microphone: microphone,
            defaultInput: snapshot.defaultInput.flatMap { snapshot.device(id: $0)?.name },
            defaultOutput: snapshot.defaultOutput.flatMap { snapshot.device(id: $0)?.name },
            rules: ruleSummaries(snapshot),
            liveness: livenessSummaries(snapshot),
            recentEvents: recentEvents
        ))
    }

    private func ruleSummaries(_ snapshot: DeviceSnapshot) -> [String: String] {
        var rules: [String: String] = [:]

        if config.input.isEmpty {
            rules["inputPin"] = "off"
        } else {
            var summary = "on (\(config.input.joined(separator: ", ")))"
            if !config.blockedInput.isEmpty {
                summary += ", blocked: \(config.blockedInput.joined(separator: ", "))"
            }
            rules["inputPin"] = summary
        }

        rules["outputPin"] = config.output.isEmpty ? "off" : "on (\(config.output.joined(separator: ", ")))"

        if let balance = config.balance {
            let current = snapshot.outputBalance.map { String(format: "%.2f", $0) } ?? "unreadable"
            rules["balance"] = String(format: "on (target %.2f, now %@)", balance, current)
        } else {
            rules["balance"] = "off"
        }

        if config.inputVolume.isEmpty {
            rules["inputVolume"] = "off"
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

    private func livenessSummaries(_ snapshot: DeviceSnapshot) -> [String: String] {
        var summaries: [String: String] = [:]
        for entry in config.liveness.keys {
            guard let device = snapshot.device(matching: entry, input: true) else {
                summaries[entry] = "absent"
                continue
            }
            guard microphoneGranted else {
                summaries[entry] = "disabled (no microphone permission)"
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

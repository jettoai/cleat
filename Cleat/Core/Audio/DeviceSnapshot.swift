import CoreAudio

/// One audio device as the rules see it.
struct AudioDevice: Equatable, Sendable {
    let id: AudioDeviceID
    let name: String
    /// CoreAudio's stable identifier. Survives renames and tells two identically named devices
    /// apart, which is why a config entry may name a device either way.
    let uid: String
    let hasInput: Bool
    let hasOutput: Bool

    init(id: AudioDeviceID, name: String, uid: String, hasInput: Bool, hasOutput: Bool) {
        self.id = id
        self.name = name
        self.uid = uid
        self.hasInput = hasInput
        self.hasOutput = hasOutput
    }

    /// True when any of these config entries names this device.
    func isListed(in entries: [String]) -> Bool {
        entries.contains { DeviceName.matches(entry: $0, name: name, uid: uid) }
    }

    /// Ordering for anything that walks the device list: by name, id breaking the tie, because
    /// two devices can share a name and the order CoreAudio hands them back is not promised.
    /// Shared so the action list and the status line agree on it.
    static func byName(_ lhs: AudioDevice, _ rhs: AudioDevice) -> Bool {
        lhs.name == rhs.name ? lhs.id < rhs.id : lhs.name < rhs.name
    }
}

/// What silence detection currently says about one device. A missing key is not a fourth case: it
/// means the device is not tracked at all (no `liveness` entry, or no microphone permission), which
/// the input rule reads as present.
enum Liveness: Equatable, Sendable {
    /// A detector is running but has not seen a buffer either way yet.
    case measuring
    case live
    case silent
}

/// Everything the rules are allowed to know: one reading of the audio system, taken on the engine
/// queue before any rule runs. The rules are pure functions of this value and the config, so a
/// decision table can be tested without a sound card.
struct DeviceSnapshot: Equatable, Sendable {
    var devices: [AudioDevice]
    var defaultInput: AudioDeviceID?
    var defaultOutput: AudioDeviceID?
    /// Virtual main balance of the default output, 0.0 (left) - 1.0 (right). `nil` when the device
    /// is not ready or does not support it.
    var outputBalance: Float?
    /// Input volume scalars (0.0-1.0) for the devices the config asks about.
    var inputVolumes: [AudioDeviceID: Float]
    /// Silence detection verdict per device UID. A missing key means "not tracked", which the
    /// input rule reads as present - the same behaviour as before liveness existed.
    var liveness: [String: Liveness]

    init(
        devices: [AudioDevice] = [],
        defaultInput: AudioDeviceID? = nil,
        defaultOutput: AudioDeviceID? = nil,
        outputBalance: Float? = nil,
        inputVolumes: [AudioDeviceID: Float] = [:],
        liveness: [String: Liveness] = [:]
    ) {
        self.devices = devices
        self.defaultInput = defaultInput
        self.defaultOutput = defaultOutput
        self.outputBalance = outputBalance
        self.inputVolumes = inputVolumes
        self.liveness = liveness
    }

    /// The present device a config entry names, or nil. `input` picks which side must exist:
    /// a headset appears once with both, an interface may have only one.
    func device(matching entry: String, input: Bool) -> AudioDevice? {
        devices.first { device in
            guard input ? device.hasInput : device.hasOutput else { return false }
            return DeviceName.matches(entry: entry, name: device.name, uid: device.uid)
        }
    }

    func device(id: AudioDeviceID) -> AudioDevice? {
        devices.first { $0.id == id }
    }
}

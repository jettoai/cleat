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
    /// `kAudioDevicePropertyTransportType`, the bus the device is on. Read for one question only -
    /// is this a pair of wireless headphones - and defaulted to unknown so a device the HAL will
    /// not answer for behaves as it did before this field existed.
    let transport: UInt32

    init(
        id: AudioDeviceID,
        name: String,
        uid: String,
        hasInput: Bool,
        hasOutput: Bool,
        transport: UInt32 = kAudioDeviceTransportTypeUnknown
    ) {
        self.id = id
        self.name = name
        self.uid = uid
        self.hasInput = hasInput
        self.hasOutput = hasOutput
        self.transport = transport
    }

    /// Both Bluetooth transports count: classic carries AirPods and most headsets, LE is what
    /// newer devices negotiate, and the takeover rule wants either.
    var isBluetooth: Bool {
        transport == kAudioDeviceTransportTypeBluetooth
            || transport == kAudioDeviceTransportTypeBluetoothLE
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
    /// Whether something is playing through the default output right now
    /// (`kAudioDevicePropertyDeviceIsRunningSomewhere`). Read only when the reclaim rule is on,
    /// and false when there is no default output at all, so a config that does not ask for
    /// reclaim behaves as it did before this field existed.
    var outputRunning: Bool
    /// Input volume scalars (0.0-1.0) for the devices the config asks about.
    var inputVolumes: [AudioDeviceID: Float]
    /// Silence detection verdict per device UID. A missing key means "not tracked", which the
    /// input rule reads as present - the same behaviour as before liveness existed.
    var liveness: [String: Liveness]
    /// UIDs of the devices that appeared on this pass and were not here on the last one. Filled in
    /// by the engine, which is the only thing that remembers what the previous pass saw. Empty on
    /// the first pass after launch: everything already plugged in was not "just connected".
    var arrived: Set<String>

    init(
        devices: [AudioDevice] = [],
        defaultInput: AudioDeviceID? = nil,
        defaultOutput: AudioDeviceID? = nil,
        outputBalance: Float? = nil,
        outputRunning: Bool = false,
        inputVolumes: [AudioDeviceID: Float] = [:],
        liveness: [String: Liveness] = [:],
        arrived: Set<String> = []
    ) {
        self.devices = devices
        self.defaultInput = defaultInput
        self.defaultOutput = defaultOutput
        self.outputBalance = outputBalance
        self.outputRunning = outputRunning
        self.inputVolumes = inputVolumes
        self.liveness = liveness
        self.arrived = arrived
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

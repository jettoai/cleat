import CoreAudio
@testable import Cleat

/// The devices that actually sit on Albert's desk, plus the two virtual ones that must never be
/// taken over. Ids are arbitrary but stable so a failure names a device, not a number.
///
/// Transports are the real ones: the takeover rule reads them, so a fixture that lied about the
/// bus would make its tests agree with each other and with nothing else.
enum Fixture {
    static let wireless = AudioDevice(
        id: 10,
        name: "Wireless microphone",
        uid: "AppleUSBAudioEngine:Shenzhen Hollyland Technology Co.,Ltd:Wireless microphone:952X2D2Q952:3",
        hasInput: true,
        hasOutput: false,
        transport: kAudioDeviceTransportTypeUSB
    )
    static let brio = AudioDevice(
        id: 20, name: "Brio 100", uid: "AppleUSBAudioEngine:Brio 100:B",
        hasInput: true, hasOutput: false, transport: kAudioDeviceTransportTypeUSB
    )
    static let airPods = AudioDevice(
        id: 30, name: "AirPods Max", uid: "AirPodsMax-UID",
        hasInput: true, hasOutput: true, transport: kAudioDeviceTransportTypeBluetooth
    )
    static let zoom = AudioDevice(
        id: 40, name: "ZoomAudioDevice", uid: "ZoomAudioDevice",
        hasInput: true, hasOutput: true, transport: kAudioDeviceTransportTypeVirtual
    )
    static let maono = AudioDevice(
        // The name carries a NO-BREAK SPACE, exactly as CoreAudio reports it.
        id: 50, name: "Maono\u{00A0}AI Microphone", uid: "Maono-UID",
        hasInput: true, hasOutput: false, transport: kAudioDeviceTransportTypeUSB
    )
    static let displaySpeakers = AudioDevice(
        id: 60, name: "Studio Display Speakers", uid: "StudioDisplay-UID",
        hasInput: false, hasOutput: true, transport: kAudioDeviceTransportTypeDisplayPort
    )
    static let macSpeakers = AudioDevice(
        id: 70, name: "MacBook Pro Speakers", uid: "BuiltInSpeakerDevice",
        hasInput: false, hasOutput: true, transport: kAudioDeviceTransportTypeBuiltIn
    )
    /// The Mac Studio's own speakers, named the way CoreAudio reports them - no space around the
    /// Chinese possessive.
    static let macStudioSpeakers = AudioDevice(
        id: 80, name: "Mac Studio的揚聲器", uid: "BuiltInSpeakerDevice-MacStudio",
        hasInput: false, hasOutput: true, transport: kAudioDeviceTransportTypeBuiltIn
    )
    /// The 3.5mm jack on the back of the Mac Studio: headphones, but wired, so the takeover rule
    /// is not interested in them.
    static let wiredHeadphones = AudioDevice(
        id: 90, name: "外接耳機", uid: "BuiltInHeadphoneOutputDevice",
        hasInput: false, hasOutput: true, transport: kAudioDeviceTransportTypeBuiltIn
    )

    /// The config Albert actually runs.
    static let pinnedInput = Config(
        input: ["Wireless microphone", "Brio 100"],
        blockedInput: ["AirPods Max"],
        inputVolume: [:],
        liveness: ["Wireless microphone": LivenessConfig(zeroSeconds: 3)]
    )
}

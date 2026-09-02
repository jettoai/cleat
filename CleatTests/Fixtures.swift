import CoreAudio
@testable import Cleat

/// The devices that actually sit on Albert's desk, plus the two virtual ones that must never be
/// taken over. Ids are arbitrary but stable so a failure names a device, not a number.
enum Fixture {
    static let wireless = AudioDevice(
        id: 10,
        name: "Wireless microphone",
        uid: "AppleUSBAudioEngine:Shenzhen Hollyland Technology Co.,Ltd:Wireless microphone:952X2D2Q952:3",
        hasInput: true,
        hasOutput: false
    )
    static let brio = AudioDevice(
        id: 20, name: "Brio 100", uid: "AppleUSBAudioEngine:Brio 100:B", hasInput: true, hasOutput: false
    )
    static let airPods = AudioDevice(
        id: 30, name: "AirPods Max", uid: "AirPodsMax-UID", hasInput: true, hasOutput: true
    )
    static let zoom = AudioDevice(
        id: 40, name: "ZoomAudioDevice", uid: "ZoomAudioDevice", hasInput: true, hasOutput: true
    )
    static let maono = AudioDevice(
        // The name carries a NO-BREAK SPACE, exactly as CoreAudio reports it.
        id: 50, name: "Maono\u{00A0}AI Microphone", uid: "Maono-UID", hasInput: true, hasOutput: false
    )
    static let displaySpeakers = AudioDevice(
        id: 60, name: "Studio Display Speakers", uid: "StudioDisplay-UID", hasInput: false, hasOutput: true
    )
    static let macSpeakers = AudioDevice(
        id: 70, name: "MacBook Pro Speakers", uid: "BuiltInSpeakerDevice", hasInput: false, hasOutput: true
    )

    /// The config Albert actually runs.
    static let pinnedInput = Config(
        input: ["Wireless microphone", "Brio 100"],
        blockedInput: ["AirPods Max"],
        inputVolume: [:],
        liveness: ["Wireless microphone": LivenessConfig(zeroSeconds: 3)]
    )
}

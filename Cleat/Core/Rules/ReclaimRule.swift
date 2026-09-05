import Foundation

/// Rule 7: ask a headset back from the phone that took it.
///
/// AirPods paired to both a Mac and an iPhone belong to whichever one last asked for them. The
/// phone asks by playing something; when it stops, nothing on the Mac asks again, so the headset
/// stays with the phone - connected over Bluetooth, absent from CoreAudio - and the Mac's sound
/// comes out of the speakers for the rest of the day. macOS only reclaims the headset when an app
/// on this side *starts* playing, and by then the sound has already gone somewhere else.
///
/// The four conditions below are the whole rule, and each one is there to keep it from asking when
/// asking would be wrong. `outputRunning` is the important one: it is what makes this a reply to
/// something the user is doing rather than a daemon fighting a phone over an idle headset. The
/// request itself carries a score that says "this machine is playing", so a phone that is playing
/// too outranks it and keeps the headset - which is the behaviour anyone would want, and is
/// arbitrated by the system rather than by this rule.
///
/// A headset that is here in CoreAudio needs nothing: either it already holds the output, or one
/// of the output rules owns that decision. This rule is only about the window where there is no
/// audio device to point at.
enum ReclaimRule {

    /// The reason string that travels with the request. It ends up in the system's own routing
    /// log next to the arbitration that answers it, which is where this rule gets debugged.
    static func requestReason(for headset: BluetoothHeadset) -> String {
        "cleat: \(headset.name) connected but not an audio device, Mac is playing"
    }

    static func reconcile(
        _ snapshot: DeviceSnapshot, _ headsets: [BluetoothHeadset], _ config: Config
    ) -> [Action] {
        // Nothing is asked for while the Mac is silent: a Mac that is not playing has no claim to
        // make, and taking the headset off a phone that is using it would be exactly the rudeness
        // this rule is trying to undo.
        guard !config.reclaim.isEmpty, snapshot.outputRunning else { return [] }

        // One request at a time. Only one device can hold the output, so asking for two headsets
        // on the same pass could only start a fight between them; the name order is the same tie
        // break every other list in Cleat uses.
        guard let target = candidates(snapshot, headsets, config).first else { return [] }
        return [.requestRoute(
            name: target.name, address: target.address, reason: requestReason(for: target)
        )]
    }

    /// The listed headsets that are connected to this Mac and have no CoreAudio device here, in
    /// the order the rule would act on them. Shared with `cleat reclaim`, so the manual button and
    /// the daemon can never disagree about which headset the config means.
    static func candidates(
        _ snapshot: DeviceSnapshot, _ headsets: [BluetoothHeadset], _ config: Config
    ) -> [BluetoothHeadset] {
        headsets
            .filter { headset in
                headset.isConnected
                    && headset.isListed(in: config.reclaim)
                    && !isAudioDevice(headset, in: snapshot)
            }
            .sorted { $0.name == $1.name ? $0.address < $1.address : $0.name < $1.name }
    }

    /// Whether this headset is already an output device the Mac can see. Matched on the Bluetooth
    /// name, which is the name CoreAudio gives the device - not on the config entry, which may be
    /// an address that no audio device carries.
    private static func isAudioDevice(_ headset: BluetoothHeadset, in snapshot: DeviceSnapshot) -> Bool {
        snapshot.devices.contains { device in
            device.hasOutput && DeviceName.matches(entry: headset.name, name: device.name, uid: device.uid)
        }
    }
}

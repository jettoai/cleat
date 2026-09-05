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

    /// `excluding` is the engine's memory of what has been asked for lately: headsets it is still
    /// waiting on an answer about, and headsets a backoff is holding down. They are taken out
    /// before the pick rather than after it, so a headset that cannot be asked for right now does
    /// not spend this pass's one request while another listed headset goes unasked.
    static func reconcile(
        _ snapshot: DeviceSnapshot, _ headsets: [BluetoothHeadset], _ config: Config,
        excluding blocked: Set<String> = []
    ) -> [Action] {
        // Nothing is asked for while the Mac is silent: a Mac that is not playing has no claim to
        // make, and taking the headset off a phone that is using it would be exactly the rudeness
        // this rule is trying to undo.
        guard !config.reclaim.isEmpty, snapshot.outputRunning else { return [] }

        // One request at a time. Only one device can hold the output, so asking for two headsets
        // on the same pass could only start a fight between them; the name order is the same tie
        // break every other list in Cleat uses.
        let target = candidates(snapshot, headsets, config).first { !blocked.contains($0.address) }
        guard let target else { return [] }
        return [.requestRoute(
            name: target.name, address: target.address, reason: requestReason(for: target)
        )]
    }

    /// The listed headsets that are connected to this Mac and have no CoreAudio device here, in
    /// the order the rule would act on them. `cleat reclaim` cannot use this list - it has no
    /// CoreAudio snapshot to leave devices out by - but it picks from the listed and connected
    /// headsets in the same `inRuleOrder`, and sends the same `requestReason`, so a config entry
    /// that matches two headsets means the same one to the button and to the daemon.
    static func candidates(
        _ snapshot: DeviceSnapshot, _ headsets: [BluetoothHeadset], _ config: Config
    ) -> [BluetoothHeadset] {
        inRuleOrder(
            headsets.filter { headset in
                headset.isConnected
                    && headset.isListed(in: config.reclaim)
                    && !isAudioDevice(headset, in: snapshot)
            }
        )
    }

    /// Name first, address as the tie break: the order every list in Cleat is read in, and the
    /// order both this rule and `cleat reclaim` act on headsets in.
    static func inRuleOrder(_ headsets: [BluetoothHeadset]) -> [BluetoothHeadset] {
        headsets.sorted { $0.name == $1.name ? $0.address < $1.address : $0.name < $1.name }
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

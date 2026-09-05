import Foundation
import IOBluetooth

/// One paired Bluetooth device, as the reclaim rule sees it.
///
/// This is deliberately not an `AudioDevice`: the whole point of the rule is the window where the
/// headset is connected over Bluetooth and has no CoreAudio device at all, because a phone is
/// holding the audio. During that window this is the only place the headset shows up.
struct BluetoothHeadset: Equatable, Sendable {
    let name: String
    /// Colon separated and upper case (`70:F9:4A:B6:0C:C9`), which is the form the routing
    /// request wants. `IOBluetoothDevice` reports dashes in lower case, so it is normalised on
    /// the way in and every other file can take the format for granted.
    let address: String
    let isConnected: Bool

    init(name: String, address: String, isConnected: Bool) {
        self.name = name
        self.address = address
        self.isConnected = isConnected
    }

    /// `70-f9-4a-b6-0c-c9` (what IOBluetooth reports) to `70:F9:4A:B6:0C:C9` (what the routing
    /// daemon expects). Anything already in that form is returned unchanged.
    static func canonicalAddress(_ value: String) -> String {
        value.replacingOccurrences(of: "-", with: ":").uppercased()
    }

    /// True when one of these config entries names this headset, by name or by address.
    ///
    /// The same two-way match every other rule uses: `DeviceName.matches` compares a name after
    /// normalisation and a UID exactly, and the address plays the part of the UID here - it is
    /// exact by construction, and it is what tells two headsets with the same name apart. An
    /// address is additionally matched through `canonicalAddress`, so writing it the way the
    /// Bluetooth pane shows it (dashes, lower case) works too. Names keep their case, exactly as
    /// `DeviceName` intends: "airpods max" is a typo, not a match.
    func isListed(in entries: [String]) -> Bool {
        entries.contains { entry in
            DeviceName.matches(entry: entry, name: name, uid: address)
                || Self.canonicalAddress(entry) == address
        }
    }
}

/// The paired Bluetooth devices. A protocol so the engine and its tests can be given a list
/// without a radio.
protocol BluetoothInventory: AnyObject {
    func pairedHeadsets() -> [BluetoothHeadset]
}

/// The real IOBluetooth pairing list.
///
/// Only three fields are read, and none of them opens a connection: `pairedDevices` is a local
/// query against the pairing database, so this is cheap enough to call on a reconcile beat.
final class IOBluetoothPairings: BluetoothInventory, @unchecked Sendable {

    func pairedHeadsets() -> [BluetoothHeadset] {
        guard let paired = IOBluetoothDevice.pairedDevices() else { return [] }
        return paired.compactMap { entry in
            guard let device = entry as? IOBluetoothDevice,
                  let name = device.name,
                  let address = device.addressString
            else { return nil }
            return BluetoothHeadset(
                name: name,
                address: BluetoothHeadset.canonicalAddress(address),
                isConnected: device.isConnected()
            )
        }
    }
}

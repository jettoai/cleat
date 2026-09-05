import Foundation

/// One paired Bluetooth device, as the reclaim rule sees it.
///
/// This is deliberately not an `AudioDevice`: the whole point of the rule is the window where the
/// headset is connected over Bluetooth and has no CoreAudio device at all, because a phone is
/// holding the audio. During that window this is the only place the headset shows up.
struct BluetoothHeadset: Equatable, Sendable {
    let name: String
    /// Colon separated and upper case (`70:F9:4A:B6:0C:C9`), which is the form the routing
    /// request wants. Every address is normalised on the way in, so every other file can take
    /// the format for granted whatever the source spelled it like.
    let address: String
    let isConnected: Bool

    /// `70-f9-4a-b6-0c-c9` (how the Bluetooth pane writes an address, and how someone copying
    /// from it writes one into the config) to `70:F9:4A:B6:0C:C9` (what the routing daemon
    /// expects). Anything already in that form is returned unchanged.
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

/// The paired Bluetooth devices, read from `system_profiler`.
///
/// Deliberately not `+[IOBluetoothDevice pairedDevices]`. That call builds
/// `IOBluetoothCoreBluetoothCoordinator` on first use, and the initialiser waits on a semaphore
/// that never gets signalled in a process holding no Bluetooth privacy decision - which is every
/// launch of Cleat as an app, because nothing in that path ever raises the prompt that would
/// create one. It is what killed 0.3.0: the first reconcile beat called it on `Engine.queue`, the
/// queue stopped draining, and the daemon spent the rest of its life wedged. Run from a terminal
/// the same call returns at once, because the decision it finds is the terminal's, which is why
/// `cleat reclaim` never showed the bug and the daemon always did.
///
/// `system_profiler SPBluetoothDataType` asks a different daemon for the same list, needs no
/// privacy decision of its own, and answers in about two tenths of a second - measured from an
/// app bundle with no Bluetooth entry, the context that hangs.
final class SystemProfilerPairings: BluetoothInventory, @unchecked Sendable {

    private static let toolURL = URL(fileURLWithPath: "/usr/sbin/system_profiler")

    /// How long a reconcile beat will wait for the list. The measured answer is far inside this;
    /// the limit is for the answer that never comes, which has to cost the engine one beat rather
    /// than the daemon's life. That is the whole lesson of 0.3.0.
    static let timeout: TimeInterval = 5

    /// How long an answer is reused before the tool is run again. Tied to the request throttle on
    /// purpose: a headset cannot be asked for twice inside that window, so a list fresher than it
    /// could not change what the rule does - it would only spawn another process per beat, and
    /// beats are cheap to come by, a liveness flip on a microphone in use being one.
    static let reuseWindow: TimeInterval = Engine.reclaimInterval

    private let now: @Sendable () -> Date
    private let read: @Sendable () -> [BluetoothHeadset]?

    /// The last answer and when it was asked for. Read from `Engine.queue` in the daemon and from
    /// the calling thread in the CLI, so the lock is what makes "is it still fresh" and "here is a
    /// new one" indivisible.
    private let lock = NSLock()
    private var answer: (headsets: [BluetoothHeadset], asked: Date)?

    /// The clock and the query are arguments so the reuse window can be tested without a radio, a
    /// subprocess, or thirty seconds of waiting.
    init(
        now: @escaping @Sendable () -> Date = { Date() },
        read: @escaping @Sendable () -> [BluetoothHeadset]? = { SystemProfilerPairings.query() }
    ) {
        self.now = now
        self.read = read
    }

    func pairedHeadsets() -> [BluetoothHeadset] {
        let moment = now()
        if let fresh = reusable(at: moment) { return fresh }

        // Asked for outside the lock: this spawns a process, and holding a lock across it would
        // hand every other caller the wait the reuse window exists to remove.
        guard let headsets = read() else {
            // A failed read is not an answer. Remembering it would turn one bad reading into
            // thirty seconds of telling the rule this Mac has no paired headsets.
            return []
        }

        lock.lock()
        answer = (headsets, moment)
        lock.unlock()
        return headsets
    }

    /// The last answer while it is still worth reusing. A clock that has gone backwards makes the
    /// answer stale rather than fresh forever: the cheap direction to be wrong in is asking again.
    private func reusable(at moment: Date) -> [BluetoothHeadset]? {
        lock.lock()
        defer { lock.unlock() }
        guard let answer else { return nil }
        let age = moment.timeIntervalSince(answer.asked)
        guard age >= 0, age < Self.reuseWindow else { return nil }
        return answer.headsets
    }

    /// One reading of the pairing list, or nil when the tool failed, had to be stopped, or printed
    /// something that is not the report Cleat asked for.
    static func query() -> [BluetoothHeadset]? {
        guard let data = readPairingList() else { return nil }
        return parse(data)
    }

    /// The two lists `system_profiler` reports, and what each says about a device.
    private static let sections = [("device_connected", true), ("device_not_connected", false)]

    /// One `system_profiler SPBluetoothDataType -json` document, as headsets. A device without an
    /// address is skipped: the address is what a routing request is addressed to, so a device
    /// Cleat cannot address is a device it cannot ask for.
    static func parse(_ data: Data) -> [BluetoothHeadset] {
        guard let root = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let report = (root["SPBluetoothDataType"] as? [[String: Any]])?.first
        else { return [] }

        return sections.flatMap { key, isConnected -> [BluetoothHeadset] in
            let entries = report[key] as? [[String: Any]] ?? []
            // Each entry is a one-key dictionary: the device name maps to its fields.
            return entries.compactMap { entry -> BluetoothHeadset? in
                guard let (name, fields) = entry.first,
                      let address = (fields as? [String: Any])?["device_address"] as? String
                else { return nil }
                return BluetoothHeadset(
                    name: name,
                    address: BluetoothHeadset.canonicalAddress(address),
                    isConnected: isConnected
                )
            }
        }
    }

    /// Runs the tool and hands back what it printed, or nil if it failed or had to be stopped.
    private static func readPairingList() -> Data? {
        let task = Process()
        task.executableURL = toolURL
        task.arguments = ["SPBluetoothDataType", "-json"]
        let pipe = Pipe()
        task.standardOutput = pipe
        task.standardError = FileHandle.nullDevice
        guard (try? task.run()) != nil else { return nil }

        // The read below is what the calling queue is parked on, and only the child closing its
        // end releases it - so the deadline has to act on the child, not on the read.
        let watchdog = ChildProcess(task)
        DispatchQueue.global().asyncAfter(deadline: .now() + timeout) { watchdog.stop() }

        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        task.waitUntilExit()
        watchdog.settle()
        return task.terminationStatus == 0 ? data : nil
    }
}

/// A child process two queues share: the one waiting for its output, and the one holding the
/// deadline. The lock is what keeps "it is still running" and "stop it" from being separated by
/// the child exiting in between.
private final class ChildProcess: @unchecked Sendable {
    private let lock = NSLock()
    private let task: Process
    private var reaped = false

    init(_ task: Process) { self.task = task }

    /// The deadline fired. Stops the child unless it has already finished on its own.
    func stop() {
        lock.lock()
        defer { lock.unlock() }
        guard !reaped, task.isRunning else { return }
        task.terminate()
    }

    /// The child finished and has been waited for; the deadline must not touch it again.
    func settle() {
        lock.lock()
        reaped = true
        lock.unlock()
    }
}

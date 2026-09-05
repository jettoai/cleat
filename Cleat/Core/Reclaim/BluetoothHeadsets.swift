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

    /// How long a reconcile beat will wait for the list before giving up on it. The measured
    /// answer is far inside this; the limit is for the answer that never comes, which has to cost
    /// the engine one beat rather than the daemon's life. That is the whole lesson of 0.3.0.
    ///
    /// It is a limit on this side's wait, not on the tool's life - `readPairingList` says why
    /// nothing done to the child can be relied on to end a read.
    static let timeout: TimeInterval = 5

    /// How long an answer worth keeping is reused before the tool is run again. Tied to the
    /// request throttle on purpose: a headset already asked for cannot be asked for again inside
    /// that window, so re-reading the list on its account would only spawn another process per
    /// beat, and beats are cheap to come by, a liveness flip on a microphone in use being one.
    /// What the window does cost is a headset that pairs or connects part way through it: that
    /// one stays out of the rule's sight until the window is up. That is the trade, and it is
    /// only taken for a reading that found something - see `pairedHeadsets`.
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
        guard let headsets = read(), !headsets.isEmpty else {
            // Neither a failed reading nor an empty one is an answer to keep. Remembering either
            // would turn one bad reading - a tool that would not start, one that did not answer
            // in time, or a document with nothing recognisable in it - into half a minute of
            // telling the rule this Mac has no paired headsets. A Mac that really has none pays
            // one subprocess per beat instead, and only while something is playing and reclaim is
            // configured, which is the only state that asks for this list at all.
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

    /// One reading of the pairing list, or nil when the tool failed to start, exited badly, or
    /// did not answer inside the deadline.
    ///
    /// A document that is not the report Cleat asked for does not come back as nil: `parse`
    /// returns what it could recognise, and recognising nothing is an empty list, which is
    /// indistinguishable from a Mac with no pairings. `pairedHeadsets` is where the two failure
    /// shapes meet the same fate - neither is cached, so the next beat asks again.
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

    /// Runs the tool and hands back what it printed, or nil if it would not start, exited badly,
    /// or did not answer inside `timeout`.
    private static func readPairingList() -> Data? {
        let task = Process()
        task.executableURL = toolURL
        task.arguments = ["SPBluetoothDataType", "-json"]
        let pipe = Pipe()
        task.standardOutput = pipe
        task.standardError = FileHandle.nullDevice
        guard (try? task.run()) != nil else { return nil }

        // Reading to EOF ends when every writer has let go of the pipe, and the writers are not
        // only the process Cleat started: `system_profiler` forks a worker of its own that
        // inherits the same fd. Signals are a poor way to end a read even so. Measured with this
        // exact Process and Pipe against a 5s deadline: a child that ignores SIGTERM keeps the
        // reader parked to 12.60s, a signal it ignores having cost it nothing; and a child that
        // forks a grandchild and exits cleanly keeps it parked to 9.01s while reporting status 0,
        // so a truncated document would have been taken for an answer. Foundation does give the
        // child a process group of its own and `terminate()` reaches all of it, but that only
        // buries what is still in the group and still answering to a pid the system has not
        // handed on since. A good effort, and not a deadline.
        //
        // The deadline therefore belongs on this side of the pipe. The read runs on a queue that
        // is allowed to stay parked; `Engine.queue` waits here only as long as it agreed to.
        let child = ChildProcess(task)
        child.readInBackground(pipe.fileHandleForReading)
        return child.answer(within: timeout)
    }
}

/// A tool held at arm's length: its output is read on a queue that is allowed to stay parked for
/// as long as the tool likes, while the caller waits on a semaphore that is not.
///
/// The lock keeps "it is still running" and "signal it" from being separated by the child exiting
/// and being waited for in between, which would leave a signal aimed at a process id the system is
/// free to have given to somebody else.
private final class ChildProcess: @unchecked Sendable {
    private let lock = NSLock()
    private let finished = DispatchSemaphore(value: 0)
    private let task: Process
    private var reaped = false
    private var output: Data?

    init(_ task: Process) { self.task = task }

    /// Reads to EOF, waits for the child, and keeps what a clean run printed.
    ///
    /// Both halves have to hold for an answer. EOF alone only says every writer let go, which a
    /// tool that printed half a document and exited also does - and it exits 0 while doing it.
    func readInBackground(_ handle: FileHandle) {
        DispatchQueue.global().async { [self] in
            let data = handle.readDataToEndOfFile()
            task.waitUntilExit()
            let clean = task.terminationStatus == 0
            lock.lock()
            reaped = true
            output = clean ? data : nil
            lock.unlock()
            finished.signal()
        }
    }

    /// What the tool printed, or nil if it failed or is not done in time. The wait ends at the
    /// deadline whatever the child is doing: a run that outlives it is abandoned, never awaited.
    func answer(within timeout: TimeInterval) -> Data? {
        guard finished.wait(timeout: .now() + timeout) == .success else {
            abandon()
            return nil
        }
        lock.lock()
        defer { lock.unlock() }
        return output
    }

    /// Nobody is waiting for this run any more. Ask it to leave, insist a second later, and let
    /// the reading queue see EOF whenever it comes - what it reads then goes nowhere.
    ///
    /// This is a burial, not the deadline: it is what keeps an abandoned run from holding a queue
    /// and a descriptor for as long as it likes, and the deadline above holds whether it works.
    private func abandon() {
        send(SIGTERM)
        DispatchQueue.global().asyncAfter(deadline: .now() + 1) { [self] in send(SIGKILL) }
    }

    /// Signals the run, group and all, unless it has already been waited for.
    ///
    /// The group is the point: the tool forks a worker that inherits the pipe, so signalling the
    /// one process Cleat started leaves the reading queue parked on a grandchild that nobody is
    /// going to stop. Foundation gives the child a process group of its own - measured: the
    /// child's pid is its own pgid, and neither is Cleat's - and that is what makes a group signal
    /// both worth sending and safe to send. The check is the whole safety argument, so it fails
    /// towards the narrow signal: a child that is somehow not its own group leader is in Cleat's
    /// group, and signalling that group would take the daemon down with the tool.
    private func send(_ number: Int32) {
        lock.lock()
        defer { lock.unlock() }
        guard !reaped, task.isRunning else { return }
        let pid = task.processIdentifier
        kill(getpgid(pid) == pid ? -pid : pid, number)
    }
}

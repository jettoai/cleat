import Foundation

/// What the output volume hold remembers between events: the value the user last set per device,
/// the writer lines since the last judgement, a listed writer's pending write, and the
/// tug-of-war brake. A value type driven by explicit times so a whole day can be replayed.
struct OutputVolumeLedger: Sendable {

    /// How long after a change it is judged. Parallels writes left and right up to 249ms apart
    /// and the log line arrives within 17ms, so 0.3s sees the whole write and its writer. Kept
    /// below `Engine.balanceSettle` (0.4): the revert lands before the balance rule looks.
    static let judgeDelay: TimeInterval = 0.3
    /// A non-listed write this soon after a listed one, starting from the listed write's result,
    /// is the system reacting to it (Control Center's HUD did, 16ms later), not a person.
    static let echoGrace: TimeInterval = 0.1
    /// The only writer seen echoing a listed write. Anyone else that soon is a person.
    static let echoWriter = "ControlCenter"
    /// A line that arrives this late may describe a change somebody has already changed again.
    static let staleAfter: TimeInterval = 1.0
    static let tugWindow: TimeInterval = 60
    static let tugLimit = 3
    static let tugPause: TimeInterval = 600
    /// Bluetooth volume changes in a row with no writer line at all before the source is called
    /// blind. The crown and reconnects legitimately have none, so this is high.
    static let blindAfter = 10
    static let selfWriter = "cleat"
    /// A writer line and the output change it caused are at most this far apart. The log line
    /// carries the write's own time, the listener the time it ran; both trail the write by ms.
    static let attributionWindow: TimeInterval = 0.15
    /// Output changes older than this can no longer be paired with a line (lines older than
    /// `staleAfter` are dropped, and a streak is judged `judgeDelay` after it opens).
    static let changeMemory: TimeInterval = 2

    struct Streak: Sendable {
        var uid: String
        var startedAt: Date
        var lastForeignAt: Date
        /// Every listed line of the streak, whichever control it wrote.
        var listed: [VolumeWrite]
        /// Control Center lines that continue a listed result.
        var echoes: [VolumeWrite]
        var overruled: Bool
        var priorWriter: String?
    }

    /// One channel of the default output moving, as the volume listener saw it.
    struct OutputChange: Equatable, Sendable {
        var at: Date
        var from: Float
        var to: Float
    }

    struct Revert: Equatable, Sendable {
        var at: Date
        var from: Float
        var to: Float
        var writer: String
    }

    struct Judgement: Equatable, Sendable {
        var verdict: OutputVolumeHoldRule.Verdict
        /// This judgement is the one that tripped the tug-of-war brake.
        var startedPause: Bool
    }

    private(set) var held: [String: Float] = [:]
    private(set) var streak: Streak?
    private(set) var lastWriter: String?
    private(set) var lastChangeAt: Date?
    private(set) var revertTimes: [Date] = []
    private(set) var pausedUntil: Date?
    private(set) var pausedBy: String?
    private(set) var lastRevert: Revert?
    private(set) var unattributedInARow = 0
    private(set) var unrecognised: String?
    /// The default output's channels at the last reading, and how they moved recently.
    private(set) var observed: [Float] = []
    private(set) var outputChanges: [OutputChange] = []

    /// Takes one writer line. Returns when the streak it opened should be judged, or nil when it
    /// opened none.
    mutating func record(
        _ write: VolumeWrite, listed: Bool, ownPID: Int32, outputUID: String?, now: Date
    ) -> Date? {
        // Any line that parsed proves the source is alive and still speaks the expected wording.
        unattributedInARow = 0
        unrecognised = nil
        guard now.timeIntervalSince(write.at) <= Self.staleAfter else { return nil }
        guard write.changesValue else { return nil }
        if write.pid == ownPID {
            lastWriter = Self.selfWriter
            return nil
        }

        if listed {
            guard let uid = outputUID else { return nil }
            if var open = streak {
                open.lastForeignAt = write.at
                open.listed.append(write)
                streak = open
                return nil
            }
            streak = Streak(uid: uid, startedAt: write.at, lastForeignAt: write.at,
                            listed: [write], echoes: [], overruled: false, priorWriter: lastWriter)
            return write.at + Self.judgeDelay
        }

        if var open = streak {
            let isEcho = write.writer == Self.echoWriter
                && write.at.timeIntervalSince(open.lastForeignAt) <= Self.echoGrace
                && (open.listed + open.echoes).contains { abs($0.to - write.from) <= OutputVolumeHoldRule.tolerance }
            if isEcho {
                open.echoes.append(write)
                streak = open
                return nil
            }
            open.overruled = true
            streak = open
        }
        lastWriter = write.writer
        return nil
    }

    /// True the first time since the last parsed line, so the engine logs it once.
    mutating func noteUnrecognised(_ message: String) -> Bool {
        defer { unrecognised = message }
        return unrecognised == nil
    }

    mutating func volumeChanged(at: Date) {
        lastChangeAt = at
    }

    /// One reading of the default output's channels. A channel that differs from the last reading
    /// is an output change; a different channel count (another device) only starts over.
    mutating func observe(_ values: [Float], at: Date) {
        if values.count == observed.count {
            for (old, new) in zip(observed, values) where abs(old - new) > OutputVolumeHoldRule.tolerance {
                outputChanges.append(OutputChange(at: at, from: old, to: new))
            }
        }
        observed = values
        outputChanges.removeAll { at.timeIntervalSince($0.at) > Self.changeMemory }
    }

    /// The line moved the output: some channel went from its `from` to its `to` around its time.
    /// Equal values alone prove nothing (a microphone write can land on the output's value).
    func movedOutput(_ write: VolumeWrite) -> Bool {
        let tolerance = OutputVolumeHoldRule.tolerance
        return outputChanges.contains {
            abs($0.from - write.from) <= tolerance && abs($0.to - write.to) <= tolerance
                && abs($0.at.timeIntervalSince(write.at)) <= Self.attributionWindow
        }
    }

    /// The streak as the rule sees it: only its lines that moved the output. None did: nothing
    /// for the rule to undo.
    private func foreign(_ open: Streak) -> OutputVolumeHoldRule.Foreign? {
        let output = open.listed.filter(movedOutput)
        guard let first = output.min(by: { $0.at < $1.at }) else { return nil }
        let results = (output + open.echoes.filter(movedOutput)).map(\.to)
        return .init(restore: first.from, writer: first.writer, results: results,
                     overruled: open.overruled, priorWriter: open.priorWriter)
    }

    /// A streak that is due is judged even while later changes keep arriving: Control Center's
    /// echo pushes the listener's settle beat back, and waiting for it would let the user's next
    /// press land first.
    func isSettling(now: Date) -> Bool {
        if let streak { return now < streak.startedAt + Self.judgeDelay }
        guard let lastChangeAt else { return false }
        return lastChangeAt + Self.judgeDelay > now
    }

    /// `sourceDown`: the writer source is not delivering, so a pending listed write is not trusted.
    mutating func judge(device: AudioDevice, current: [Float], now: Date, sourceDown: Bool = false) -> Judgement {
        let idle = Judgement(verdict: .none, startedPause: false)
        guard !current.isEmpty, !isSettling(now: now) else { return idle }
        if let open = streak, open.uid != device.uid { streak = nil }
        if let until = pausedUntil, now >= until {
            pausedUntil = nil
            pausedBy = nil
        }
        revertTimes.removeAll { now.timeIntervalSince($0) > Self.tugWindow }

        var verdict = OutputVolumeHoldRule.decide(.init(
            current: current, held: held[device.uid], foreign: streak.flatMap(foreign),
            lastWriter: lastWriter, paused: pausedUntil != nil || sourceDown || blind != nil
        ))
        var startedPause = false
        if case .revert(let from, _, let writer, _) = verdict, revertTimes.count >= Self.tugLimit {
            pausedUntil = now + Self.tugPause
            pausedBy = writer
            startedPause = true
            verdict = .kept(from: held[device.uid] ?? from, to: from, writer: writer, note: "paused")
        }

        switch verdict {
        case .adopt(let value), .kept(_, let value, _, _):
            held[device.uid] = value
        case .revert(let from, let restore, let writer, _):
            held[device.uid] = restore
            revertTimes.append(now)
            lastRevert = Revert(at: now, from: from, to: restore, writer: writer)
        case .none:
            break
        }
        if device.isBluetooth, case .kept(_, _, "unknown", _) = verdict {
            unattributedInARow += 1
        }

        streak = nil
        lastWriter = nil
        return Judgement(verdict: verdict, startedPause: startedPause)
    }

    /// Why the writer source cannot be trusted right now, or nil.
    var blind: String? {
        if unrecognised != nil { return "unrecognised writer line" }
        if unattributedInARow >= Self.blindAfter {
            return "\(unattributedInARow) Bluetooth volume changes without a writer line"
        }
        return nil
    }
}

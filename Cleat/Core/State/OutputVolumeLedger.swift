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

    struct Streak: Sendable {
        var uid: String
        var startedAt: Date
        var lastForeignAt: Date
        var foreign: OutputVolumeHoldRule.Foreign
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
                open.foreign.results.append(write.to)
                streak = open
                return nil
            }
            streak = Streak(
                uid: uid, startedAt: write.at, lastForeignAt: write.at,
                foreign: .init(restore: write.from, writer: write.writer, results: [write.to],
                               overruled: false, priorWriter: lastWriter)
            )
            return write.at + Self.judgeDelay
        }

        if var open = streak {
            let isEcho = write.writer == Self.echoWriter
                && write.at.timeIntervalSince(open.lastForeignAt) <= Self.echoGrace
                && open.foreign.results.contains { abs($0 - write.from) <= OutputVolumeHoldRule.tolerance }
            if isEcho {
                open.foreign.results.append(write.to)
                streak = open
                return nil
            }
            open.foreign.overruled = true
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
            current: current, held: held[device.uid], foreign: streak?.foreign,
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

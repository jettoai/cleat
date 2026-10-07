import CoreAudio
import Foundation

/// Engine-side bookkeeping for the output volume hold that is not the ledger: the writer source
/// and its restarts. Lives in one stored property on Engine so Engine.swift stays short.
struct OutputVolumeHold {
    var ledger = OutputVolumeLedger()
    var makeSource: () -> any VolumeWriterSource = { VolumeWriterLog() }
    var source: (any VolumeWriterSource)?
    /// Why the source is not delivering, or nil when it is (or when the hold is off).
    var sourceProblem: String?
    var restartAttempt = 0
    var restartWork: DispatchWorkItem?
    var streakWork: DispatchWorkItem?
    /// UID of the default output the last pass judged, so a writer line can be pinned to a device
    /// without a fresh snapshot.
    var outputUID: String?
}

/// The `outputVolume` block of status.json, which the settings screen reads.
struct OutputVolumeStatus: Codable, Equatable {
    /// The same state the `outputVolume` status line starts with.
    var state: String
    /// e.g. "13:40 拉回 Parallels Desktop 改的音量 36% → 50%". Absent until the first revert.
    var lastRevert: String?
}

extension Engine {

    /// See `OutputVolumeLedger.judgeDelay`; named here beside `balanceSettle` (0.4), which it must
    /// stay below.
    static let outputVolumeSettle: TimeInterval = OutputVolumeLedger.judgeDelay
    private static let restartBackoff: [TimeInterval] = [1, 2, 4, 8, 16, 32, 60]

    // MARK: - Writer source

    /// Starts or stops the log reader to match the config. Called from `rebindDevices`, which runs
    /// often, so a reader that is already running is left alone.
    func syncOutputVolumeSource() {
        guard !config.outputVolumeHoldAgainst.isEmpty else {
            let wasOn = outputVolume.source != nil || outputVolume.restartWork != nil
            outputVolume.source?.stop()
            outputVolume.source = nil
            outputVolume.restartWork?.cancel()
            outputVolume.restartWork = nil
            outputVolume.streakWork?.cancel()
            outputVolume.streakWork = nil
            outputVolume.sourceProblem = nil
            outputVolume.restartAttempt = 0
            if wasOn { note("outputVolume: off") }
            return
        }
        guard outputVolume.source == nil, outputVolume.restartWork == nil else { return }
        startOutputVolumeSource()
    }

    private func startOutputVolumeSource() {
        let source = outputVolume.makeSource()
        let started = source.start(
            queue: queue,
            onEvent: { [weak self] in self?.volumeWriterEvent($0) },
            onExit: { [weak self] in self?.volumeWriterExited($0) }
        )
        guard started else {
            outputVolume.sourceProblem = "log stream could not start"
            note("outputVolume: writer log could not start, not reverting")
            scheduleSourceRestart()
            return
        }
        outputVolume.source = source
        outputVolume.sourceProblem = nil
        note("outputVolume: reading writers from the system log")
    }

    private func volumeWriterExited(_ status: Int32) {
        outputVolume.source = nil
        guard !config.outputVolumeHoldAgainst.isEmpty else { return }
        outputVolume.sourceProblem = "log stream exited (\(status))"
        let delay = scheduleSourceRestart()
        note("outputVolume: writer log stopped (exit \(status)), restarting in \(Int(delay))s")
    }

    @discardableResult
    private func scheduleSourceRestart() -> TimeInterval {
        let backoff = Self.restartBackoff
        let delay = backoff[min(outputVolume.restartAttempt, backoff.count - 1)]
        outputVolume.restartAttempt += 1
        let work = DispatchWorkItem { [weak self] in
            guard let self else { return }
            outputVolume.restartWork = nil
            guard !config.outputVolumeHoldAgainst.isEmpty else { return }
            startOutputVolumeSource()
        }
        outputVolume.restartWork = work
        queue.asyncAfter(deadline: .now() + delay, execute: work)
        return delay
    }

    private func volumeWriterEvent(_ event: VolumeWriterEvent) {
        switch event {
        case .unrecognised(let message):
            if outputVolume.ledger.noteUnrecognised(message) {
                note("outputVolume: unrecognised writer line, not reverting: \(message.prefix(120))")
            }
        case .write(var write):
            outputVolume.restartAttempt = 0
            let uid = outputVolume.outputUID ?? currentOutputUID()
            // Named from here on by the entry it matched, so the log says what the config says.
            let entry = OutputVolumeHoldRule.listedEntry(path: write.writer, in: config.outputVolumeHoldAgainst)
            write.writer = entry ?? URL(fileURLWithPath: write.writer).lastPathComponent
            if let deadline = outputVolume.ledger.record(
                write, listed: entry != nil, ownPID: getpid(), outputUID: uid, now: now()
            ) {
                scheduleStreakJudge(at: deadline)
            }
        }
    }

    private func currentOutputUID() -> String? {
        let snapshot = system.snapshot(config: config)
        return snapshot.defaultOutput.flatMap { snapshot.device(id: $0)?.uid }
    }

    /// A listed write is judged on its own clock, not the listener's: later changes must not push
    /// it back.
    private func scheduleStreakJudge(at deadline: Date) {
        outputVolume.streakWork?.cancel()
        let work = DispatchWorkItem { [weak self] in
            guard let self else { return }
            outputVolume.streakWork = nil
            reconcile()
        }
        outputVolume.streakWork = work
        queue.asyncAfter(deadline: .now() + max(0, deadline.timeIntervalSince(now())), execute: work)
    }

    // MARK: - Listener and rule

    /// The default output's volume moved, whoever moved it.
    func outputVolumeChanged() {
        outputVolume.ledger.volumeChanged(at: now())
        scheduleReconcile(after: Engine.outputVolumeSettle)
    }

    func outputVolumeActions(_ snapshot: DeviceSnapshot) -> [Action] {
        guard !config.outputVolumeHoldAgainst.isEmpty,
              let id = snapshot.defaultOutput, let device = snapshot.device(id: id) else { return [] }
        outputVolume.outputUID = device.uid

        let judgement = outputVolume.ledger.judge(device: device, current: snapshot.outputVolumes, now: now())
        // One line per judged change, the reconciliation record: who moved it and what we did.
        switch judgement.verdict {
        case .kept(let from, let to, let writer, let remark):
            note(Self.percentLine(device.name, from, to) + " writer=\(writer) kept"
                 + (remark.map { " (\($0))" } ?? ""))
        case .revert(let from, let restore, let writer, let prior):
            if let prior {
                note(Self.percentLine(device.name, prior.from, prior.to) + " writer=\(prior.writer) kept")
            }
            // What it was before the listed write is what goes back (a prior change ends there too).
            note(Self.percentLine(device.name, restore, from) + " writer=\(writer) reverted")
        case .none, .adopt:
            break
        }
        if judgement.startedPause, let until = outputVolume.ledger.pausedUntil {
            let writer = outputVolume.ledger.pausedBy ?? "unknown"
            note("outputVolume: paused until \(Self.clock(until, seconds: false)), \(writer) changed the volume "
                 + "back \(OutputVolumeLedger.tugLimit + 1) times in a minute; turning off Parallels' "
                 + "\"Sync volume with Mac\" stops it")
        }
        return OutputVolumeHoldRule.actions(judgement.verdict, device: device)
    }

    private static func percentLine(_ name: String, _ from: Float, _ to: Float) -> String {
        String(format: "outputVolume: %@ %.0f%% -> %.0f%%", name, Double(from) * 100, Double(to) * 100)
    }

    // MARK: - Status

    /// The status line's state, before any "last revert" suffix.
    private func outputVolumeState() -> String {
        let list = config.outputVolumeHoldAgainst
        guard !list.isEmpty else { return "off" }
        if let reason = outputVolume.sourceProblem ?? outputVolume.ledger.blind {
            return "paused: cannot read system log writer (\(reason))"
        }
        if let until = outputVolume.ledger.pausedUntil {
            let writer = outputVolume.ledger.pausedBy ?? "unknown"
            return "paused: tug-of-war with \(writer), resumes \(Self.clock(until, seconds: false))"
        }
        return "on (holding against \(list.joined(separator: ", ")))"
    }

    func outputVolumeSummary() -> String {
        var summary = outputVolumeState()
        if let last = outputVolume.ledger.lastRevert {
            summary += String(format: "; last revert %@ %.0f%% -> %.0f%% (%@)",
                              Self.clock(last.at, seconds: true),
                              Double(last.from) * 100, Double(last.to) * 100, last.writer)
        }
        return summary
    }

    func outputVolumeStatus() -> OutputVolumeStatus {
        let lastRevert = outputVolume.ledger.lastRevert.map { last in
            String(format: "%@ 拉回 %@ 改的音量 %.0f%% → %.0f%%",
                   Self.clock(last.at, seconds: false),
                   last.writer,
                   Double(last.from) * 100, Double(last.to) * 100)
        }
        return OutputVolumeStatus(state: outputVolumeState(), lastRevert: lastRevert)
    }

    /// Local wall-clock time for people: status is read by a person, not parsed.
    private static func clock(_ date: Date, seconds: Bool) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = seconds ? "HH:mm:ss" : "HH:mm"
        return formatter.string(from: date)
    }
}

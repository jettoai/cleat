import XCTest
@testable import Cleat

/// 2026-10-07, 10:19 to 12:56, replayed through the ledger on the log's own clock. Parallels
/// pulled the AirPods Max down five times; Control Center, the Digital Crown and Jetto moved it
/// 68 more. Only the five may be undone, and the remembered value must follow the other 68.
final class OutputVolumeReplayTests: XCTestCase {

    private enum Kind {
        case write(pid: Int32, control: Int, from: Float, to: Float)
        /// No writer line for these two: both channels move together.
        case crown(Float)
        case reconnect(Float)
    }

    private struct Event {
        let at: Date
        let kind: Kind
    }

    private struct Revert {
        let at: Date
        let streakStart: Date
        let writer: String
        let restore: Float
    }

    private struct Group {
        var at: Date
        var end: Date
        var writer: String
        var to: Float
        var channels: Set<Int>
    }

    private static let device = Fixture.airPods

    private static func events() -> [Event] {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(secondsFromGMT: 8 * 3600)
        formatter.dateFormat = "yyyy-MM-dd HH:mm:ss.SSS"
        return OutputVolumeReplayFixture.events.split(separator: "\n").map { line in
            let field = line.split(separator: " ").map(String.init)
            let at = formatter.date(from: "\(field[0]) \(field[1])")!
            switch field[2] {
            case "write":
                return Event(at: at, kind: .write(pid: Int32(field[3])!, control: Int(field[4])!,
                                                  from: Float(field[5])!, to: Float(field[6])!))
            case "crown":
                return Event(at: at, kind: .crown(Float(field[3])!))
            default:
                XCTAssertEqual(field[2], "reconnect")
                return Event(at: at, kind: .reconnect(Float(field[3])!))
            }
        }
    }

    private static func writer(_ pid: Int32) -> String {
        OutputVolumeReplayFixture.writers[pid] ?? "pid \(pid)"
    }

    /// The steps of the plan's section 4.3: every due judgement before the next event, our own
    /// write fires the listener, a line arrives 5ms after it was logged.
    ///
    /// The captures have no listener readings, so the output changes are synthesised: every time
    /// channel 262 or 263 moves (a write line to those controls, the crown, a reconnect, our own
    /// revert) the listener is taken to read both channels at that instant. That holds for this
    /// day's AirPods Max, whose output controls are 262/263; on a real machine the readings come
    /// from the listener, at its own time (named blind spot: the replay cannot show listener lag).
    private static func replay() -> (reverts: [Revert], timeline: [(at: Date, held: Float?)]) {
        var ledger = OutputVolumeLedger()
        var channels: [Int: Float] = [:]
        var judged = Set<Date>()
        var reverts: [Revert] = []
        var timeline: [(at: Date, held: Float?)] = []

        func listener(at: Date) {
            ledger.observe([262, 263].compactMap { channels[$0] }, at: at)
        }

        func runDue(upTo limit: Date?) {
            while true {
                var due: [Date] = []
                if let streak = ledger.streak { due.append(streak.startedAt + OutputVolumeLedger.judgeDelay) }
                if let change = ledger.lastChangeAt { due.append(change + OutputVolumeLedger.judgeDelay) }
                guard let mark = due.filter({ date in !judged.contains(date) && (limit.map { date <= $0 } ?? true) }).min()
                else { return }
                judged.insert(mark)
                let streakStart = ledger.streak?.startedAt
                let current = [262, 263].compactMap { channels[$0] }
                let judgement = ledger.judge(device: device, current: current, now: mark)
                if case .revert(_, let restore, let writer, _) = judgement.verdict {
                    channels[262] = restore
                    channels[263] = restore
                    listener(at: mark)
                    ledger.volumeChanged(at: mark)
                    reverts.append(Revert(at: mark, streakStart: streakStart ?? .distantPast,
                                          writer: writer, restore: restore))
                }
                timeline.append((mark, ledger.held[device.uid]))
            }
        }

        for event in events() {
            runDue(upTo: event.at)
            switch event.kind {
            case .write(let pid, let control, let from, let to):
                let old = channels[control]
                channels[control] = to
                if old.map({ abs($0 - to) > 1e-9 }) ?? true {
                    listener(at: event.at)
                    ledger.volumeChanged(at: event.at)
                }
                let name = writer(pid)
                let line = VolumeWrite(at: event.at, pid: pid, writer: name, control: control, from: from, to: to)
                _ = ledger.record(line, listed: name == "prl_vm_app", ownPID: 1,
                                  outputUID: device.uid, now: event.at + 0.005)
            case .crown(let value), .reconnect(let value):
                let changed = [262, 263].contains { channels[$0].map { abs($0 - value) > 1e-9 } ?? true }
                if changed { ledger.volumeChanged(at: event.at) }
                channels[262] = value
                channels[263] = value
                if changed { listener(at: event.at) }
            }
        }
        runDue(upTo: nil)
        return (reverts, timeline.sorted { $0.at < $1.at })
    }

    /// The same grouping as the day's classification (replay.py): one write is one writer's lines
    /// within 300ms on different channels; one crown turn is clicks under a second apart.
    private static func groups() -> [Group] {
        var writes: [Group] = []
        var crowns: [Group] = []
        for event in events() {
            switch event.kind {
            case .write(let pid, let control, let from, let to):
                guard abs(from - to) > 1e-6 else { continue }
                let name = writer(pid)
                if var last = writes.last, last.writer == name,
                   event.at.timeIntervalSince(last.end) < 0.3, !last.channels.contains(control) {
                    last.end = event.at
                    last.to = to
                    last.channels.insert(control)
                    writes[writes.count - 1] = last
                } else {
                    writes.append(Group(at: event.at, end: event.at, writer: name, to: to, channels: [control]))
                }
            case .reconnect:
                continue
            case .crown(let value):
                if var last = crowns.last, event.at.timeIntervalSince(last.end) < 1.0 {
                    last.end = event.at
                    last.to = value
                    crowns[crowns.count - 1] = last
                } else {
                    crowns.append(Group(at: event.at, end: event.at, writer: "Crown", to: value, channels: []))
                }
            }
        }
        return (writes + crowns).sorted { $0.at < $1.at }
    }

    // MARK: - Tests

    func testFixtureHoldsTheDaysWrites() {
        let counts = Dictionary(grouping: Self.groups(), by: \.writer).mapValues(\.count)
        XCTAssertEqual(counts, ["prl_vm_app": 5, "ControlCenter": 17, "Crown": 1, "Jetto": 50])
    }

    func testEveryParallelsWriteIsRevertedWithinHalfASecond() {
        let (reverts, _) = Self.replay()
        let parallels = Self.groups().filter { $0.writer == "prl_vm_app" }

        XCTAssertEqual(reverts.count, 5)
        XCTAssertEqual(reverts.map(\.writer), Array(repeating: "prl_vm_app", count: 5))
        XCTAssertEqual(reverts.map(\.streakStart), parallels.map(\.at))
        for revert in reverts {
            XCTAssertEqual(revert.at.timeIntervalSince(revert.streakStart), 0.3, accuracy: 0.001)
        }
        let restores = reverts.map(\.restore)
        for (got, want) in zip(restores, [Float(0.488189), 0.213605, 0.625000, 0.156250, 0.312500]) {
            XCTAssertEqual(got, want, accuracy: 1e-6)
        }
    }

    /// No revert lands on anyone else's change, and the remembered value follows each of them.
    ///
    /// Each change is looked up at the first judgement after it: either the remembered value is
    /// now its value, or a later change came in before that judgement and took over.
    func testTheRememberedValueFollowsEveryoneElse() {
        let (_, timeline) = Self.replay()
        let groups = Self.groups()
        var outcome: [String: Int] = [:]
        var supersededBy: [String] = []
        var echo: [Group] = []

        for (index, group) in groups.enumerated() where group.writer != "prl_vm_app" {
            guard let judged = timeline.first(where: { $0.at >= group.end })?.at else {
                XCTFail("no judgement after \(group.at)")
                continue
            }
            let held = timeline.last { $0.at <= judged }?.held
            if let later = groups[(index + 1)...].first(where: { $0.at < judged }) {
                outcome["\(group.writer) superseded", default: 0] += 1
                supersededBy.append("\(group.writer)>\(later.writer)")
                // Taken over by Parallels: the revert puts back exactly this change's value.
                if later.writer == "prl_vm_app" {
                    let after = timeline.first { $0.at >= later.at + OutputVolumeLedger.judgeDelay }?.held
                    XCTAssertEqual(after ?? -1, group.to, accuracy: OutputVolumeHoldRule.tolerance)
                }
            } else if let held, abs(held - group.to) <= OutputVolumeHoldRule.tolerance {
                outcome["\(group.writer) held", default: 0] += 1
            } else {
                echo.append(group)
            }
        }

        XCTAssertEqual(outcome, [
            "Jetto held": 49, "Jetto superseded": 1,
            "Crown held": 1,
            "ControlCenter held": 10, "ControlCenter superseded": 6,
        ])
        XCTAssertEqual(supersededBy.filter { $0 == "ControlCenter>prl_vm_app" }.count, 1)
        XCTAssertEqual(supersededBy.filter { $0 == "ControlCenter>ControlCenter" }.count, 5)
        XCTAssertEqual(supersededBy.filter { $0 == "Jetto>Jetto" }.count, 1)
        // The one Control Center write that does not survive is its 16ms echo of Parallels'
        // write at 11:44:31.271, which goes back with it.
        XCTAssertEqual(echo.count, 1)
        XCTAssertEqual(echo.first?.writer, "ControlCenter")
        XCTAssertEqual(echo.first?.to ?? 0, 0.5625, accuracy: 1e-6)
        if let echo = echo.first {
            let held = timeline.last { $0.at <= echo.end + 0.3 }?.held
            XCTAssertEqual(held ?? 0, 0.625, accuracy: 1e-6)
        }
    }
}

import XCTest
@testable import Cleat

/// The output volume hold's decision (the pure rule) and its memory (the ledger), driven by
/// explicit times.
final class OutputVolumeHoldRuleTests: XCTestCase {

    private let device = Fixture.airPods
    private let t0 = Date(timeIntervalSince1970: 1_791_339_595.713)

    private func write(_ writer: String, at offset: TimeInterval, _ from: Float, _ to: Float,
                       pid: Int32 = 8484, control: Int = 262) -> VolumeWrite {
        VolumeWrite(at: t0 + offset, pid: pid, writer: writer, control: control, from: from, to: to)
    }

    private func foreign(_ restore: Float, _ results: [Float], overruled: Bool = false) -> OutputVolumeHoldRule.Foreign {
        .init(restore: restore, writer: "prl_vm_app", results: results, overruled: overruled, priorWriter: nil)
    }

    /// A ledger that has already adopted `level` for the device.
    private func ledger(holding level: Float) -> OutputVolumeLedger {
        var ledger = OutputVolumeLedger()
        _ = ledger.judge(device: device, current: [level, level], now: t0 - 10)
        return ledger
    }

    /// Records a listed write, lands it on both channels and judges at its deadline.
    private func parallels(_ ledger: inout OutputVolumeLedger, at offset: TimeInterval,
                           _ from: Float, _ to: Float) -> OutputVolumeLedger.Judgement {
        let line = write("prl_vm_app", at: offset, from, to)
        let deadline = ledger.record(line, listed: true, ownPID: 1, outputUID: device.uid, now: line.at)
        XCTAssertNotNil(deadline)
        ledger.volumeChanged(at: line.at)
        return ledger.judge(device: device, current: [to, to], now: deadline ?? line.at)
    }

    // MARK: - Rule

    func testListedWriteStillInPlaceIsReverted() {
        let verdict = OutputVolumeHoldRule.decide(.init(
            current: [0.427210, 0.427210], held: 0.488189, foreign: foreign(0.488189, [0.427210]),
            lastWriter: nil, paused: false
        ))
        XCTAssertEqual(verdict, .revert(from: 0.427210, restore: 0.488189, writer: "prl_vm_app", prior: nil))
        XCTAssertEqual(OutputVolumeHoldRule.actions(verdict, device: device),
                       [.setOutputVolume(device.id, 0.488189, reason: "AirPods Max 43% -> 49% (reverted prl_vm_app)")])
    }

    func testOverruledOrMovedOnWritesAreKept() {
        let overruled = OutputVolumeHoldRule.decide(.init(
            current: [0.5, 0.5], held: 0.625, foreign: foreign(0.625, [0.53], overruled: true),
            lastWriter: "ControlCenter", paused: false
        ))
        XCTAssertEqual(overruled, .kept(from: 0.625, to: 0.5, writer: "ControlCenter", note: nil))

        // The crown turned after it, or what it wrote was the microphone: the output is not its value.
        let crown = OutputVolumeHoldRule.decide(.init(
            current: [0.6, 0.6], held: 0.625, foreign: foreign(0.625, [0.53]), lastWriter: nil, paused: false
        ))
        XCTAssertEqual(crown, .kept(from: 0.625, to: 0.6, writer: "prl_vm_app", note: nil))
        XCTAssertEqual(OutputVolumeHoldRule.actions(crown, device: device), [])
    }

    func testAdoptAndNothing() {
        XCTAssertEqual(OutputVolumeHoldRule.decide(.init(current: [0.4, 0.6], held: nil, foreign: nil, lastWriter: nil, paused: false)),
                       .adopt(0.5))
        XCTAssertEqual(OutputVolumeHoldRule.decide(.init(current: [0.3, 0.3], held: 0.5, foreign: nil, lastWriter: "cleat", paused: false)),
                       .adopt(0.3))
        XCTAssertEqual(OutputVolumeHoldRule.decide(.init(current: [0.5, 0.5], held: 0.5, foreign: nil, lastWriter: "Jetto", paused: false)),
                       .none)
        XCTAssertEqual(OutputVolumeHoldRule.decide(.init(current: [], held: 0.5, foreign: nil, lastWriter: nil, paused: false)),
                       .none)
    }

    // MARK: - Ledger

    /// Left first, right 249ms later (the 12:56 shape): one streak, one revert.
    func testChannelsWrittenApartAreOneRevert() {
        var ledger = ledger(holding: 0.5)
        let left = write("prl_vm_app", at: 0, 0.5, 0.43)
        let right = write("prl_vm_app", at: 0.249, 0.5, 0.43, control: 263)
        let deadline = ledger.record(left, listed: true, ownPID: 1, outputUID: device.uid, now: left.at)
        XCTAssertEqual(deadline, t0 + 0.3)
        XCTAssertNil(ledger.record(right, listed: true, ownPID: 1, outputUID: device.uid, now: right.at))
        ledger.volumeChanged(at: right.at)

        XCTAssertEqual(ledger.judge(device: device, current: [0.43, 0.5], now: t0 + 0.25).verdict, .none)
        let judgement = ledger.judge(device: device, current: [0.43, 0.43], now: t0 + 0.3)
        XCTAssertEqual(judgement.verdict, .revert(from: 0.43, restore: 0.5, writer: "prl_vm_app", prior: nil))
        XCTAssertEqual(ledger.revertTimes.count, 1)
        XCTAssertEqual(ledger.held[device.uid], 0.5)
    }

    /// 11:44:31: Control Center re-writes Parallels' result 16ms later. That is part of the
    /// write; a change 0.2s later is somebody's own.
    func testEchoJoinsTheWriteButALaterChangeOverrulesIt() {
        var echoed = ledger(holding: 0.625)
        _ = echoed.record(write("prl_vm_app", at: 0, 0.625, 0.531160), listed: true, ownPID: 1, outputUID: device.uid, now: t0)
        _ = echoed.record(write("ControlCenter", at: 0.016, 0.531160, 0.5625, pid: 531), listed: false, ownPID: 1,
                          outputUID: device.uid, now: t0 + 0.016)
        XCTAssertEqual(echoed.judge(device: device, current: [0.5625, 0.5625], now: t0 + 0.3).verdict,
                       .revert(from: 0.5625, restore: 0.625, writer: "prl_vm_app", prior: nil))

        var overruled = ledger(holding: 0.625)
        _ = overruled.record(write("prl_vm_app", at: 0, 0.625, 0.531160), listed: true, ownPID: 1, outputUID: device.uid, now: t0)
        _ = overruled.record(write("ControlCenter", at: 0.2, 0.531160, 0.5, pid: 531), listed: false, ownPID: 1,
                             outputUID: device.uid, now: t0 + 0.2)
        XCTAssertEqual(overruled.judge(device: device, current: [0.5, 0.5], now: t0 + 0.5).verdict,
                       .kept(from: 0.625, to: 0.5, writer: "ControlCenter", note: nil))
    }

    func testStaleLineOpensNoStreak() {
        var ledger = ledger(holding: 0.5)
        let line = write("prl_vm_app", at: 0, 0.5, 0.43)
        XCTAssertNil(ledger.record(line, listed: true, ownPID: 1, outputUID: device.uid, now: t0 + 1.5))
        XCTAssertNil(ledger.streak)
    }

    /// Three reverts in a minute are allowed; the fourth pauses the hold for ten minutes.
    func testTugOfWarPausesAndResumes() {
        var ledger = ledger(holding: 0.5)
        for round in 0..<3 {
            let judgement = parallels(&ledger, at: Double(round) * 10, 0.5, 0.43)
            XCTAssertEqual(judgement.verdict, .revert(from: 0.43, restore: 0.5, writer: "prl_vm_app", prior: nil))
            XCTAssertFalse(judgement.startedPause)
        }
        let fourth = parallels(&ledger, at: 30, 0.5, 0.43)
        XCTAssertEqual(fourth.verdict, .kept(from: 0.5, to: 0.43, writer: "prl_vm_app", note: "paused"))
        XCTAssertTrue(fourth.startedPause)
        XCTAssertEqual(ledger.pausedUntil?.timeIntervalSince(t0) ?? 0, 630.3, accuracy: 0.001)
        XCTAssertEqual(ledger.pausedBy, "prl_vm_app")

        let during = parallels(&ledger, at: 40, 0.43, 0.36)
        XCTAssertEqual(during.verdict, .kept(from: 0.43, to: 0.36, writer: "prl_vm_app", note: "paused"))
        XCTAssertEqual(ledger.held[device.uid], 0.36)

        let after = parallels(&ledger, at: 700, 0.36, 0.3)
        XCTAssertEqual(after.verdict, .revert(from: 0.3, restore: 0.36, writer: "prl_vm_app", prior: nil))
        XCTAssertNil(ledger.pausedUntil)
    }

    func testTenBluetoothChangesWithoutAWriterLineIsBlindUntilALineParses() {
        var ledger = ledger(holding: 0.5)
        for step in 1...10 {
            let level = 0.5 - Float(step) * 0.01
            ledger.volumeChanged(at: t0 + Double(step))
            _ = ledger.judge(device: device, current: [level, level], now: t0 + Double(step) + 0.3)
            XCTAssertEqual(ledger.blind != nil, step >= 10, "step \(step)")
        }
        _ = ledger.record(write("Jetto", at: 20, 0.4, 0.2, pid: 72504), listed: false, ownPID: 1,
                          outputUID: device.uid, now: t0 + 20)
        XCTAssertNil(ledger.blind)

        var speakers = OutputVolumeLedger()
        for step in 0...12 {
            let level = 0.5 - Float(step) * 0.01
            _ = speakers.judge(device: Fixture.displaySpeakers, current: [level], now: t0 + Double(step))
        }
        XCTAssertNil(speakers.blind)
    }

    /// The output moved to another device between the write and its judgement.
    func testStreakForAnotherDeviceIsDropped() {
        var ledger = ledger(holding: 0.5)
        _ = ledger.record(write("prl_vm_app", at: 0, 0.5, 0.43), listed: true, ownPID: 1, outputUID: "other-UID", now: t0)
        let judgement = ledger.judge(device: device, current: [0.43, 0.43], now: t0 + 0.3)
        XCTAssertEqual(judgement.verdict, .kept(from: 0.5, to: 0.43, writer: "unknown", note: nil))
        XCTAssertNil(ledger.streak)
    }
}

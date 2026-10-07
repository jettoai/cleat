import Foundation

/// How long a listed headset takes to come back to the Mac (B-1173). Temporary measurement for the
/// Swift/Rust comparison: the lines match cleat-rs word for word, and this file goes when the
/// comparison is done.
extension Engine {

    /// An accepted hijack whose headset is not the output this long after the request is logged
    /// as not back.
    static let returnTimeout: TimeInterval = 10
    /// A macOS switch back slower than this is a headset taken out of its case, not a return.
    static let macosReturnLimit: TimeInterval = 30

    /// A request on its way, recorded before it is sent: the fake and the real client may both
    /// answer before `request` returns.
    func noteRequestSent(name: String, address: String) {
        reclaimWatch.returns[address] = ReturnTiming(name: name, sent: now())
    }

    /// Anything but `routed` ends the timing without a line.
    func noteRouteAnswer(address: String, routed: Bool) {
        guard routed else {
            reclaimWatch.returns[address] = nil
            return
        }
        acceptReturn(address)
    }

    /// The hijack was accepted: log now if the headset already arrived, otherwise wait for it.
    private func acceptReturn(_ address: String) {
        guard var timing = reclaimWatch.returns[address] else { return }
        timing.accepted = true
        if let at = timing.arrived {
            reclaimWatch.returns[address] = nil
            note("reclaim: \(timing.name) back on the Mac in \(Engine.seconds(at.timeIntervalSince(timing.sent))) s")
        } else {
            reclaimWatch.returns[address] = timing
            let left = timing.sent.addingTimeInterval(Engine.returnTimeout).timeIntervalSince(now())
            scheduleReconcile(after: max(left, 0))
        }
    }

    /// Times a headset coming back the moment the output moves, not on the next beat.
    func checkReturnsNow() {
        checkReclaimReturns(system.snapshot(config: config))
    }

    /// Called at the end of every pass and on every device or default-output event.
    func checkReclaimReturns(_ snapshot: DeviceSnapshot) {
        timeMacosReturn(snapshot)
        guard !reclaimWatch.returns.isEmpty else { return }
        let moment = now()
        var lines: [String] = []
        for (address, var timing) in reclaimWatch.returns {
            let headset = BluetoothHeadset(name: timing.name, address: address, isConnected: true)
            if timing.arrived == nil, ReclaimRule.isDefaultOutput(headset, in: snapshot) {
                timing.arrived = moment
            }
            if timing.accepted, let at = timing.arrived {
                lines.append("reclaim: \(timing.name) back on the Mac in \(Engine.seconds(at.timeIntervalSince(timing.sent))) s")
                reclaimWatch.returns[address] = nil
            } else if moment.timeIntervalSince(timing.sent) >= Engine.returnTimeout {
                if timing.accepted {
                    let place: String
                    if ReclaimRule.isAudioDevice(headset, in: snapshot) {
                        let output = snapshot.defaultOutput.flatMap(snapshot.device(id:))?.name ?? "nothing"
                        place = "in CoreAudio, the output is \(output)"
                    } else {
                        place = "not in CoreAudio"
                    }
                    lines.append("reclaim: \(timing.name) not back on the Mac \(Int(Engine.returnTimeout)) s after the request (\(place))")
                }
                reclaimWatch.returns[address] = nil
            } else {
                reclaimWatch.returns[address] = timing
            }
        }
        lines.forEach(note)
    }

    /// macOS bringing a listed headset back by itself: timed from the first check that saw the Mac
    /// playing with the headset elsewhere. Arrival is judged before the reset, so a playback that
    /// stops in the same move still counts. Cleat's own request or a hand-picked output voids it.
    private func timeMacosReturn(_ snapshot: DeviceSnapshot) {
        guard !config.reclaim.isEmpty else {
            reclaimWatch.macosReturn.removeAll()
            return
        }
        let moment = now()
        var lines: [String] = []
        for (address, name) in reclaimNames.sorted(by: { $0.key < $1.key }) {
            let voided = reclaimWatch.returns[address] != nil || reclaimWatch.userChose.contains(address)
            let headset = BluetoothHeadset(name: name, address: address, isConnected: true)
            if ReclaimRule.isDefaultOutput(headset, in: snapshot) {
                let start = reclaimWatch.macosReturn.removeValue(forKey: address)
                if let start, !voided {
                    let took = moment.timeIntervalSince(start)
                    if took <= Engine.macosReturnLimit {
                        lines.append("reclaim: \(name) back on the Mac (macOS) in \(Engine.seconds(took)) s")
                    }
                }
            } else if voided || !snapshot.outputRunning {
                reclaimWatch.macosReturn[address] = nil
            } else if reclaimWatch.macosReturn[address] == nil {
                reclaimWatch.macosReturn[address] = moment
            }
        }
        lines.forEach(note)
    }

    private static func seconds(_ interval: TimeInterval) -> String {
        String(format: "%.1f", max(interval, 0))
    }
}

/// A request on its way: when it went out, whether it was accepted, when the headset arrived.
struct ReturnTiming {
    let name: String
    let sent: Date
    var accepted = false
    var arrived: Date?
}

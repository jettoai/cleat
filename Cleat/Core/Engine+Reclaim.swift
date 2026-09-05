import Foundation

/// Rule 7's engine side: when to ask, how often, and what to do with the answer. Split out of
/// Engine.swift to keep both files short; every function here runs on `Engine.queue`, same as the
/// rest of the engine, including the response handler, which the routing client delivers there.
extension Engine {

    /// What the request tells the arbitration this machine is doing. 201 is the score macOS gives
    /// a general playback session; media playback is 301 and a call is 501. The daemon refuses
    /// only when the remote score is strictly higher, so at 201 a phone that is idle (100) gives
    /// the headset up, a tie goes to whoever asked, and a phone playing media or on a call keeps
    /// what it has.
    ///
    /// Asking with 301 would win against a phone that is playing media - and then lose it again
    /// the next time that phone asks, which is a headset bouncing between two devices rather than
    /// the problem this rule exists to fix. The design is that the phone comes first: Cleat asks
    /// for a headset the phone is not using, and does not compete for one it is.
    static let reclaimScore: Int32 = 201

    /// Slowest sensible retry against the same headset. Every reconcile beat asks, and a headset
    /// the phone is quietly holding would otherwise be asked for several times a second while
    /// music plays.
    static let reclaimInterval: TimeInterval = 30

    /// How long to leave a headset alone after the phone has won it. Long enough that a call is
    /// not interrupted every half minute, short enough that hanging up is noticed.
    static let reclaimBackoff: TimeInterval = 60

    /// The requests this pass should send: what the rule asked for, minus anything the throttle or
    /// a backoff is still holding down.
    func reclaimRequests(_ snapshot: DeviceSnapshot) -> [Action] {
        guard !config.reclaim.isEmpty else { return [] }
        guard routing.isAvailable else {
            // Fail closed and say so once. A macOS that has moved or renamed the routing class is
            // not an error to retry; it is this rule being off.
            if !reclaimUnavailableLogged {
                reclaimUnavailableLogged = true
                note("reclaim: unavailable on this macOS")
            }
            return []
        }

        return ReclaimRule.reconcile(snapshot, bluetooth.pairedHeadsets(), config).filter { action in
            guard case .requestRoute(_, let address, _) = action else { return false }
            guard !reclaimInFlight.contains(address) else { return false }
            guard let next = reclaimNextAttempt[address] else { return true }
            return now() >= next
        }
    }

    /// Sends one request. The throttle is set here rather than when the answer comes back, so a
    /// reply that never arrives still cannot turn into a request per beat.
    func requestRoute(name: String, address: String, reason: String) {
        reclaimInFlight.insert(address)
        reclaimNextAttempt[address] = now().addingTimeInterval(Engine.reclaimInterval)

        routing.request(
            address: address, score: Engine.reclaimScore, reason: reason, queue: queue
        ) { [weak self] response in
            guard let self else { return }
            routeAnswered(name: name, address: address, response: response)
        }
    }

    /// What the daemon said. Nothing here writes to CoreAudio: a granted hijack moves the default
    /// output by itself, and if it does not, the device arriving is an ordinary arrival that
    /// `HeadphonesTakeoverRule` already knows what to do with.
    private func routeAnswered(name: String, address: String, response: RouteResponse) {
        reclaimInFlight.remove(address)

        switch response.outcome {
        case .routed:
            reclaimHeldLogged.remove(address)
            note("reclaim: \(name) <- remote device (hijack accepted)")
            // The audio device appears a moment after the answer. These are the same beats a
            // device change would schedule, and they are what lets the takeover rule see the
            // arrival if macOS has not already moved the output itself.
            Engine.retryBeats.forEach(scheduleReconcile(after:))

        case .alreadyRouted:
            // It was here all along. Nothing changed, so nothing is logged.
            reclaimHeldLogged.remove(address)

        case .heldByRemote(let detail):
            reclaimNextAttempt[address] = now().addingTimeInterval(Engine.reclaimBackoff)
            noteHeld("reclaim: \(name) held by remote device (\(detail))", address: address)

        case .busy:
            // A previous hijack of ours is still running. Not news, and not a reason to wait: the
            // next beat is the retry.
            reclaimNextAttempt[address] = nil

        case .refused(let detail):
            reclaimNextAttempt[address] = now().addingTimeInterval(Engine.reclaimBackoff)
            noteHeld("reclaim: \(name) refused (\(detail))", address: address)
        }
    }

    /// One line per spell, not one per attempt: a headset a phone keeps all afternoon is worth
    /// saying once, and the set is cleared the moment it comes back.
    private func noteHeld(_ message: String, address: String) {
        guard reclaimHeldLogged.insert(address).inserted else { return }
        note(message)
    }

    /// The `reclaim` line in `cleat status`.
    func reclaimSummary() -> String {
        guard !config.reclaim.isEmpty else { return "off" }
        guard routing.isAvailable else {
            return "unavailable (no routing service on this macOS) (\(config.reclaim.joined(separator: ", ")))"
        }
        return "on (\(config.reclaim.joined(separator: ", ")))"
    }
}

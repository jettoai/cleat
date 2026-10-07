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

    /// A refusal that is about to stop being true (the buds go back in, the screen unlocks) is
    /// retried this soon, for at most `reclaimRetrySpan` from the first one. The retry has to
    /// schedule its own beat: putting the buds in is not a CoreAudio event, so nothing else would.
    static let reclaimRetryDelay: TimeInterval = 8
    static let reclaimRetrySpan: TimeInterval = 180

    private static func isShortLivedRefusal(_ detail: String) -> Bool {
        let detail = detail.lowercased()
        return detail.contains("out of ear") || detail.contains("screen locked")
    }

    /// The requests this pass should send: what the rule asks for once the headsets the throttle
    /// and the backoffs are holding down have been taken off its list.
    func reclaimRequests(_ snapshot: DeviceSnapshot) -> [Action] {
        guard !config.reclaim.isEmpty else { return [] }
        forgetHeadsetsThatCameBack(snapshot)
        guard routing.isAvailable else {
            // Fail closed and say so once. A macOS that has moved or renamed the routing class is
            // not an error to retry; it is this rule being off.
            if !reclaimUnavailableLogged {
                reclaimUnavailableLogged = true
                note("reclaim: unavailable on this macOS")
            }
            return []
        }
        // The rule asks this too, and has to: a Mac that is not playing has no claim to make. It
        // is asked again here, in front of the rule, because reading the pairing list costs a
        // subprocess and this answer costs nothing. Beats are cheap to come by - a liveness flip
        // on a microphone in use is one, several times a minute - and every beat that reaches the
        // rule while the Mac is silent would have paid for a list it then ignores.
        guard snapshot.outputRunning else {
            // The Mac stopped playing: the next refusal starts a fresh run of short retries, and
            // the next playback asks again for a headset that is here but not chosen.
            reclaimRetryWindow.removeAll()
            reclaimAskedThisPlayback.removeAll()
            return []
        }

        return ReclaimRule.reconcile(
            snapshot, bluetooth.pairedHeadsets(), config,
            excluding: heldDownHeadsets().union(presentHeadsetsAlreadyAsked(snapshot))
        )
    }

    /// A headset macOS moved back by itself never gets a `routed` answer, so its becoming the
    /// default output is what ends the spell: the next refusal is logged and retried as a new one.
    /// Merely being in CoreAudio is not enough - a headset that is here but not chosen is still
    /// being asked for, and ending its spell every beat would make the short retries endless.
    private func forgetHeadsetsThatCameBack(_ snapshot: DeviceSnapshot) {
        for (address, name) in reclaimNames {
            let headset = BluetoothHeadset(name: name, address: address, isConnected: true)
            if ReclaimRule.isDefaultOutput(headset, in: snapshot) { endSpell(address) }
        }
    }

    /// Headsets in CoreAudio, not the output, and already asked for in this playback. Whatever the
    /// answer was, the user has had the chance to be switched over; if they then pick another
    /// output, that choice stands until the next playback. A headset absent from CoreAudio keeps
    /// the old cadence, since there is no device for anyone to have chosen.
    private func presentHeadsetsAlreadyAsked(_ snapshot: DeviceSnapshot) -> Set<String> {
        reclaimAskedThisPlayback.filter { address in
            guard let name = reclaimNames[address] else { return false }
            let headset = BluetoothHeadset(name: name, address: address, isConnected: true)
            return ReclaimRule.isAudioDevice(headset, in: snapshot)
        }
    }

    /// The headsets this pass must not ask for: one asked for less than an interval ago, and one a
    /// backoff is holding down. The rule is told about them before it picks rather than being
    /// filtered after, so a headset that cannot be asked for now steps aside for the next one on
    /// the list instead of spending the single request a pass allows.
    ///
    /// Both readings come off `reclaimNextAttempt` because `requestRoute` sets it as the request
    /// goes out: an answer that never comes holds its headset down for `reclaimInterval` and no
    /// longer, leaving nothing behind that a later beat cannot get past.
    private func heldDownHeadsets() -> Set<String> {
        let moment = now()
        return Set(reclaimNextAttempt.filter { moment < $0.value }.keys)
    }

    /// Sends one request. The throttle is set here rather than when the answer comes back, so a
    /// reply that never arrives still cannot turn into a request per beat.
    func requestRoute(name: String, address: String, reason: String) {
        reclaimNextAttempt[address] = now().addingTimeInterval(Engine.reclaimInterval)
        reclaimNames[address] = name
        reclaimAskedThisPlayback.insert(address)

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
        switch response.outcome {
        case .routed:
            endSpell(address)
            note("reclaim: \(name) <- remote device (hijack accepted)")
            // The audio device appears a moment after the answer. These are the same beats a
            // device change would schedule, and they are what lets the takeover rule see the
            // arrival if macOS has not already moved the output itself.
            Engine.retryBeats.forEach(scheduleReconcile(after:))

        case .alreadyRouted:
            // It was here all along. Nothing changed, so nothing is logged.
            endSpell(address)

        case .heldByRemote(let detail):
            reclaimNextAttempt[address] = now().addingTimeInterval(Engine.reclaimBackoff)
            // A call outranks the 201 this Mac asks with, on purpose: see `reclaimScore`.
            let why = detail.contains("Remote Category 501")
                ? ": the phone is on a call, Cleat yields by design" : ""
            noteHeld("reclaim: \(name) held by remote device (\(detail))\(why)", address: address)

        case .busy:
            // A previous hijack of ours is still running. Not news, and not a reason to wait: the
            // next beat is the retry.
            reclaimNextAttempt[address] = nil
            reclaimAskedThisPlayback.remove(address)

        case .refused(let detail):
            if Engine.isShortLivedRefusal(detail), retryWindowOpen(address) {
                reclaimNextAttempt[address] = now().addingTimeInterval(Engine.reclaimRetryDelay)
                reclaimAskedThisPlayback.remove(address)
                scheduleReconcile(after: Engine.reclaimRetryDelay)
            } else {
                reclaimNextAttempt[address] = now().addingTimeInterval(Engine.reclaimBackoff)
            }
            noteHeld("reclaim: \(name) refused (\(detail))", address: address)
        }
    }

    /// The headset is the output: the next refusal is logged and retried as a new one.
    private func endSpell(_ address: String) {
        reclaimHeldLogged[address] = nil
        reclaimRetryWindow[address] = nil
    }

    /// Opens the window on the first short-lived refusal and says whether it is still open. A
    /// spent window stays spent until the headset becomes the output or the Mac stops playing.
    private func retryWindowOpen(_ address: String) -> Bool {
        let start = reclaimRetryWindow[address] ?? now()
        reclaimRetryWindow[address] = start
        return now() < start.addingTimeInterval(Engine.reclaimRetrySpan)
    }

    /// One line per reason, not one per attempt: a headset a phone keeps all afternoon is worth
    /// saying once, a new reason is worth saying again, and it all resets when the headset is back.
    private func noteHeld(_ message: String, address: String) {
        guard reclaimHeldLogged[address] != message else { return }
        reclaimHeldLogged[address] = message
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

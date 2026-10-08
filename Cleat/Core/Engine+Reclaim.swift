import CoreAudio
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

    /// Someone touched the keyboard, mouse or trackpad this recently: they are at the Mac, and the
    /// Mac playing is them listening. Longer than this and they are looking at something else -
    /// most likely the phone the headset went to.
    static let userActiveWindow: TimeInterval = 30

    /// An output change this close to the last input event was made by hand. It is judged on the
    /// first pass after the change, which the default-output listener schedules a second later. A
    /// phone taking the headset comes after the user has picked the phone up, unlocked it and
    /// pressed play, which takes longer than this; misreading that as a choice would leave the
    /// headset with the phone for the rest of the playback.
    static let manualChoiceWindow: TimeInterval = 5

    /// How long Cleat's own default-output write counts as the cause of the next output change.
    static let ownOutputWriteWindow: TimeInterval = 10

    private static func isShortLivedRefusal(_ detail: String) -> Bool {
        let detail = detail.lowercased()
        return detail.contains("out of ear") || detail.contains("screen locked")
    }

    /// The requests this pass should send: what the rule asks for once the headsets the throttle
    /// and the backoffs are holding down have been taken off its list.
    func reclaimRequests(_ snapshot: DeviceSnapshot) -> [Action] {
        guard !config.reclaim.isEmpty else { return [] }
        forgetHeadsetsThatCameBack(snapshot)
        noteOutputDeparture(snapshot)
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
            reclaimWatch.userChose.removeAll()
            return []
        }

        let headsets = bluetooth.pairedHeadsets()
        learnNames(headsets)
        let candidates = ReclaimRule.candidates(snapshot, headsets, config)
        guard let first = candidates.first else {
            // Nothing listed is away from the output: whatever was holding it back is over, and
            // the next reason is news.
            reclaimWatch.notAskingLogged = nil
            return []
        }

        // What this playback has settled: the user moved the output off the headset by hand, or
        // the daemon said the Mac already has it. Neither is asked for again until it ends.
        let chose = presentHeadsets(reclaimWatch.userChose, snapshot)
        let settled = presentHeadsets(reclaimAskedThisPlayback, snapshot)
        guard let next = candidates.first(where: {
            !chose.contains($0.address) && !settled.contains($0.address)
        }) else {
            let output = snapshot.defaultOutput.flatMap(snapshot.device(id:))?.name ?? "another output"
            noteNotAsking(first, chose.contains(first.address)
                ? "the user picked \(output)" : "already asked this playback")
            return []
        }

        guard isUserActive(snapshot) else {
            noteNotAsking(next, "nobody is using the Mac")
            // Coming back to the keyboard is not a CoreAudio event, so nothing else would bring
            // the next pass: one beat per retry delay, the cadence a short refusal already uses.
            scheduleReconcile(after: Engine.reclaimRetryDelay)
            return []
        }

        return ReclaimRule.reconcile(
            snapshot, headsets, config,
            excluding: heldDownHeadsets().union(chose).union(settled)
        )
    }

    /// Remembers the name each listed headset goes by, so a pass can recognise it in CoreAudio
    /// and in the default output before it has ever been asked for.
    private func learnNames(_ headsets: [BluetoothHeadset]) {
        for headset in headsets where headset.isListed(in: config.reclaim) {
            reclaimNames[headset.address] = headset.name
        }
    }

    /// The output moving off a listed headset that is still in CoreAudio, by hand or not. By hand
    /// is an input event a moment before and a move Cleat did not make itself; anything else - a
    /// phone taking the headset while it stays listed, macOS falling back - leaves it wanted.
    /// A headset that has left CoreAudio was taken, not passed over, and is not judged here.
    private func noteOutputDeparture(_ snapshot: DeviceSnapshot) {
        let current = snapshot.defaultOutput.flatMap(snapshot.device(id:))
        defer { reclaimWatch.lastOutput = current }
        guard let left = reclaimWatch.lastOutput, left.uid != current?.uid,
              let entry = reclaimNames.first(where: {
                  DeviceName.matches(entry: $0.value, name: left.name, uid: left.uid)
              })
        else { return }
        let headset = BluetoothHeadset(name: entry.value, address: entry.key, isConnected: true)
        guard ReclaimRule.isAudioDevice(headset, in: snapshot) else { return }

        let byCleat = reclaimWatch.ownOutputWrite.map {
            $0.device == current?.id && now().timeIntervalSince($0.at) < Engine.ownOutputWriteWindow
        } ?? false
        // No reading is the old behaviour: a switch away from a headset in use stood.
        let byHand = snapshot.inputIdle.map { $0 < Engine.manualChoiceWindow } ?? true
        if byHand && !byCleat { reclaimWatch.userChose.insert(entry.key) }
    }

    /// At the keyboard in the last half minute, or watching something that keeps the display on.
    /// A reading that could not be taken counts as someone being there, so the rule then behaves
    /// as it did before it knew about the user.
    private func isUserActive(_ snapshot: DeviceSnapshot) -> Bool {
        guard let idle = snapshot.inputIdle else { return true }
        return idle < Engine.userActiveWindow || snapshot.displayHeldAwake
    }

    /// One line per reason, written when the reason changes: a Mac left playing to an empty room
    /// is one line, not one every beat.
    private func noteNotAsking(_ headset: BluetoothHeadset, _ reason: String) {
        let message = "reclaim: \(headset.name) not asked (\(reason))"
        guard reclaimWatch.notAskingLogged != message else { return }
        reclaimWatch.notAskingLogged = message
        note(message)
    }

    /// A headset macOS moved back by itself never gets a `routed` answer, so its becoming the
    /// default output is what ends the spell: the next refusal is logged and retried as a new one.
    /// Merely being in CoreAudio is not enough - a headset that is here but not chosen is still
    /// being asked for, and ending its spell every beat would make the short retries endless.
    private func forgetHeadsetsThatCameBack(_ snapshot: DeviceSnapshot) {
        for (address, name) in reclaimNames {
            let headset = BluetoothHeadset(name: name, address: address, isConnected: true)
            if ReclaimRule.isDefaultOutput(headset, in: snapshot) {
                endSpell(address)
                // Back as the output: whatever this playback had settled about it is over, and
                // the next time it leaves, the leaving is judged afresh.
                reclaimAskedThisPlayback.remove(address)
                reclaimWatch.userChose.remove(address)
            }
        }
    }

    /// The headsets in this set that are in CoreAudio. A headset absent from CoreAudio keeps the
    /// usual cadence whatever this playback has settled, since there is no device for anyone to
    /// have chosen.
    private func presentHeadsets(_ addresses: Set<String>, _ snapshot: DeviceSnapshot) -> Set<String> {
        addresses.filter { address in
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
        noteRequestSent(name: name, address: address)

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
        if response.outcome != .routed { forgetReturn(address) }
        switch response.outcome {
        case .routed:
            endSpell(address)
            reclaimAskedThisPlayback.insert(address)
            note("reclaim: \(name) <- remote device (hijack accepted)")
            acceptReturn(address)
            // The audio device appears a moment after the answer. These are the same beats a
            // device change would schedule, and they are what lets the takeover rule see the
            // arrival if macOS has not already moved the output itself.
            Engine.retryBeats.forEach(scheduleReconcile(after:))

        case .alreadyRouted:
            // It was here all along: nothing changed, nothing is logged, and nothing is asked
            // again this playback.
            endSpell(address)
            reclaimAskedThisPlayback.insert(address)

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

        case .refused(let detail):
            if Engine.isShortLivedRefusal(detail), retryWindowOpen(address) {
                reclaimNextAttempt[address] = now().addingTimeInterval(Engine.reclaimRetryDelay)
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

/// What reclaim remembers between passes about the output and the person in front of the Mac.
struct ReclaimWatch {
    /// The default output the previous pass saw: how a pass tells the output just left a headset.
    var lastOutput: AudioDevice?
    /// Headsets the user moved the output off by hand during this playback, by address.
    var userChose: Set<String> = []
    /// Cleat's own last default-output write, so a pin moving the output is not read as the user's.
    var ownOutputWrite: (device: AudioDeviceID, at: Date)?
    /// The last "not asked" line, so a reason that has not changed is not written every beat.
    var notAskingLogged: String?
    /// Requests whose return is being timed, and macOS returns being timed (Engine+ReturnTiming.swift).
    var returns: [String: ReturnTiming] = [:]
    var macosReturn: [String: Date] = [:]
}

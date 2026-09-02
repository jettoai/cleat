/// The state machine behind silence detection, kept separate from CoreAudio so it can be tested
/// without a device.
///
/// A wireless receiver whose transmitter is off is still a device and still delivers audio - it
/// delivers exact digital zero. A real microphone never does: even a quiet room reads as noise a
/// few dB above nothing. So "every sample in this buffer was zero, for N seconds running" is the
/// signal, and this type is the counter.
struct ZeroStreak {

    enum Flip: Equatable {
        case becameSilent
        case becameLive
    }

    private enum State {
        /// Nothing measured yet. The first buffer either way is worth reporting, so that status
        /// stops saying "measuring" the moment a verdict exists.
        case unknown
        case live
        case silent
    }

    /// Consecutive all-zero frames needed before a device counts as silent.
    let thresholdFrames: Int

    private var state: State = .unknown
    private(set) var zeroFrames = 0

    init(thresholdFrames: Int) {
        self.thresholdFrames = max(1, thresholdFrames)
    }

    var isSilent: Bool { state == .silent }

    /// Feeds one buffer. Returns a flip only when the verdict changes, so the caller can wake the
    /// engine on transitions rather than twelve times a second.
    mutating func feed(frames: Int, allZero: Bool) -> Flip? {
        guard allZero else {
            zeroFrames = 0
            guard state != .live else { return nil }
            state = .live
            return .becameLive
        }

        zeroFrames += frames
        guard zeroFrames >= thresholdFrames, state != .silent else { return nil }
        state = .silent
        return .becameSilent
    }
}

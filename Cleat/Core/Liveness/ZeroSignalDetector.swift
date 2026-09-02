import CoreAudio
import Foundation

/// What the engine needs from a silence detector. A protocol so the engine's bookkeeping - which
/// device gets a detector, and when a failed start is tried again - can be tested without opening a
/// real input. `ZeroSignalDetector` is the only implementation that ships.
protocol LivenessDetecting: AnyObject, Sendable {
    var deviceID: AudioDeviceID { get }
    var name: String { get }
    /// The threshold baked in at init, so the engine can tell a running detector that still matches
    /// the config from one that has to be replaced.
    var zeroSeconds: Double { get }

    /// False when the input could not be opened; see `ZeroSignalDetector.start()`.
    @discardableResult
    func start() -> Bool
    func stop()
}

/// How the engine makes one. `AudioDevice` carries the id, uid and name; the sample rate is read
/// from the audio system by the caller, which is the only part of this that needs the HAL.
typealias LivenessDetectorFactory = @Sendable (
    _ device: AudioDevice,
    _ sampleRate: Double,
    _ zeroSeconds: Double,
    _ queue: DispatchQueue,
    _ onFlip: @escaping @Sendable (Bool) -> Void
) -> LivenessDetecting

/// Watches one input device for exact digital silence, using a HAL IOProc.
///
/// This is the cheapest place to ask the question: the HAL is already moving these frames, so the
/// whole cost is one wakeup per buffer and a scan for a non-zero byte. The buffer is pushed to
/// 4096 frames, which at 48 kHz is about twelve wakeups a second - the Hammerspoon version paid a
/// permanently resident ffmpeg (1.8% of a core) for the same answer.
///
/// Realtime discipline: the IOProc block allocates nothing, takes no lock and calls nothing that
/// might. It writes one word and, only on a change of verdict, signals a user-data dispatch source
/// - Apple's own channel out of a realtime thread.
final class ZeroSignalDetector: @unchecked Sendable {

    /// Frames per callback. Larger means fewer wakeups; 4096 is the ceiling most devices allow.
    private static let preferredBufferFrames: UInt32 = 4096

    let deviceID: AudioDeviceID
    let uid: String
    let name: String
    /// Kept so the engine can tell a config change that only moved the threshold from one that
    /// changed nothing: the threshold is baked into the streak at init, so a new value needs a new
    /// detector rather than a running one that silently keeps the old number.
    let zeroSeconds: Double

    private let queue: DispatchQueue
    private let onFlip: (Bool) -> Void

    private var procID: AudioDeviceIOProcID?
    private var flipSource: DispatchSourceUserDataAdd?
    private var started = false

    /// Touched by the IOProc thread only, which is serial with itself.
    private var streak: ZeroStreak
    private var bytesPerSample: Int

    /// The verdict, handed from the realtime thread to `queue`. A single aligned word written
    /// before `add(data:)` and read in the handler that the signal wakes, so the dispatch source
    /// itself provides the ordering.
    private let silentFlag: UnsafeMutablePointer<Int32>

    init(
        device: AudioDeviceID,
        uid: String,
        name: String,
        sampleRate: Double,
        zeroSeconds: Double,
        queue: DispatchQueue,
        onFlip: @escaping (Bool) -> Void
    ) {
        self.deviceID = device
        self.uid = uid
        self.name = name
        self.zeroSeconds = zeroSeconds
        self.queue = queue
        self.onFlip = onFlip
        self.streak = ZeroStreak(thresholdFrames: Int(sampleRate * zeroSeconds))
        self.bytesPerSample = MemoryLayout<Float32>.size
        self.silentFlag = UnsafeMutablePointer<Int32>.allocate(capacity: 1)
        self.silentFlag.initialize(to: 0)
    }

    deinit {
        stop()
        silentFlag.deinitialize(count: 1)
        silentFlag.deallocate()
    }

    // MARK: - Lifecycle

    /// Returns false when the device cannot be opened - no microphone permission, or it went away
    /// between the snapshot and here. The caller leaves liveness unmeasured, which the input rule
    /// reads as present.
    @discardableResult
    func start() -> Bool {
        guard !started else { return true }

        applyPreferredBufferSize()
        readStreamFormat()

        let source = DispatchSource.makeUserDataAddSource(queue: queue)
        source.setEventHandler { [weak self] in
            guard let self else { return }
            onFlip(silentFlag.pointee == 0)
        }
        flipSource = source

        var proc: AudioDeviceIOProcID?
        let createStatus = AudioDeviceCreateIOProcIDWithBlock(&proc, deviceID, nil) {
            [weak self] _, inputData, _, _, _ in
            self?.consume(inputData)
        }
        guard createStatus == noErr, let proc else {
            flipSource = nil
            return false
        }
        procID = proc

        guard AudioDeviceStart(deviceID, proc) == noErr else {
            AudioDeviceDestroyIOProcID(deviceID, proc)
            procID = nil
            flipSource = nil
            return false
        }

        source.resume()
        started = true
        return true
    }

    func stop() {
        if let procID {
            AudioDeviceStop(deviceID, procID)
            AudioDeviceDestroyIOProcID(deviceID, procID)
            self.procID = nil
        }
        flipSource?.cancel()
        flipSource = nil
        started = false
    }

    // MARK: - Realtime path

    /// Runs on the HAL's realtime thread. Nothing in here allocates, locks or dispatches.
    private func consume(_ inputData: UnsafePointer<AudioBufferList>) {
        let list = UnsafeMutableAudioBufferListPointer(
            UnsafeMutablePointer(mutating: inputData)
        )
        guard let first = list.first, first.mNumberChannels > 0 else { return }

        // Frames, not bytes: interleaved devices report one buffer of N*channels samples,
        // non-interleaved report one buffer per channel.
        let frameBytes = bytesPerSample * Int(first.mNumberChannels)
        guard frameBytes > 0 else { return }
        let frames = Int(first.mDataByteSize) / frameBytes

        var allZero = true
        for buffer in list {
            guard let data = buffer.mData, buffer.mDataByteSize > 0 else { continue }
            if Self.containsNonZeroByte(data, count: Int(buffer.mDataByteSize)) {
                allZero = false
                break
            }
        }

        guard let flip = streak.feed(frames: frames, allZero: allZero) else { return }
        silentFlag.pointee = (flip == .becameSilent) ? 1 : 0
        flipSource?.add(data: 1)
    }

    /// Any non-zero byte means signal. Comparing bytes rather than decoded samples makes this
    /// independent of the stream's sample format, and errs the safe way: a float -0.0 would read
    /// as signal, which falls back to the behaviour Cleat had before liveness existed.
    private static func containsNonZeroByte(_ data: UnsafeMutableRawPointer, count: Int) -> Bool {
        let raw = UnsafeRawBufferPointer(start: data, count: count)
        let wordSize = MemoryLayout<UInt64>.size
        var offset = 0
        while offset + wordSize <= count {
            if raw.loadUnaligned(fromByteOffset: offset, as: UInt64.self) != 0 { return true }
            offset += wordSize
        }
        while offset < count {
            if raw[offset] != 0 { return true }
            offset += 1
        }
        return false
    }

    // MARK: - Device setup

    /// Ask for the largest buffer the device allows, up to 4096 frames. Fewer, bigger callbacks is
    /// the entire performance argument for doing this in-process.
    private func applyPreferredBufferSize() {
        let rangeAddress = AudioProperty.address(
            kAudioDevicePropertyBufferFrameSizeRange, scope: kAudioObjectPropertyScopeInput
        )
        var wanted = Self.preferredBufferFrames
        if let range = AudioProperty.value(deviceID, rangeAddress, as: AudioValueRange.self) {
            wanted = min(max(wanted, UInt32(range.mMinimum)), UInt32(range.mMaximum))
        }
        let sizeAddress = AudioProperty.address(
            kAudioDevicePropertyBufferFrameSize, scope: kAudioObjectPropertyScopeInput
        )
        _ = AudioProperty.setValue(deviceID, sizeAddress, wanted)
    }

    /// The sample width the buffers will actually arrive in. Only used to turn bytes into frames;
    /// if the device will not say, Float32 is the HAL's virtual format and the right assumption.
    private func readStreamFormat() {
        let address = AudioProperty.address(
            kAudioDevicePropertyStreamFormat, scope: kAudioObjectPropertyScopeInput
        )
        guard let format = AudioProperty.value(
            deviceID, address, as: AudioStreamBasicDescription.self
        ), format.mBitsPerChannel >= 8 else { return }
        bytesPerSample = Int(format.mBitsPerChannel) / 8
    }
}

/// Every member is already there with the right shape; the conformance only names it.
extension ZeroSignalDetector: LivenessDetecting {}

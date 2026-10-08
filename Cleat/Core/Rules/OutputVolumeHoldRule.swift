import CoreAudio
import Foundation

/// Rule 7: undo writes to the default output's volume made by the listed programs.
///
/// Which program wrote is not something CoreAudio says; the ledger gets it from coreaudiod's log
/// and hands this rule the facts. Everything that is not provably a listed writer - Control
/// Center, the Digital Crown, a reconnect, Jetto ducking for a recording - is the user's, and the
/// value the rule remembers follows it.
enum OutputVolumeHoldRule {

    /// Closer than this is the same volume.
    static let tolerance: Float = 0.0005

    /// A listed writer's write still waiting to be judged.
    struct Foreign: Equatable, Sendable {
        /// The value right before the listed writer touched the output, from the earliest of its
    /// lines that moved the output.
        var restore: Float
        var writer: String
        /// Every value the write (and any echo of it) left on a channel.
        var results: [Float]
        /// Somebody else wrote after it; it is theirs now.
        var overruled: Bool
        /// The non-listed writer of the change just before the listed write, for the log.
        var priorWriter: String?
    }

    struct Input: Equatable, Sendable {
        var current: [Float]
        var held: Float?
        var foreign: Foreign?
        /// Writer of the last non-listed line since the previous judgement, "cleat" for our own.
        var lastWriter: String?
        var paused: Bool
    }

    /// A user change the listed write landed on top of before it was ever judged.
    struct Prior: Equatable, Sendable {
        var from: Float
        var to: Float
        var writer: String
    }

    enum Verdict: Equatable, Sendable {
        /// Nothing changed, or nothing readable.
        case none
        /// Remember silently: the first reading of a device, or our own write.
        case adopt(Float)
        /// The user's (or an unlisted program's) change: remember it.
        case kept(from: Float, to: Float, writer: String, note: String?)
        /// A listed writer's change: write `restore` back. `prior` is logged first so the record
        /// has no gap.
        case revert(from: Float, restore: Float, writer: String, prior: Prior?)
    }

    /// The config entry a writer's executable path matches, or nil. An entry names either the
    /// executable (`prl_vm_app`) or an app the executable lives inside (`Parallels Desktop` matches
    /// any `Parallels Desktop.app` on the path, so one entry covers an app's helpers too).
    static func listedEntry(path: String, in entries: [String]) -> String? {
        let components = path.split(separator: "/").map(String.init)
        guard let executable = components.last else { return nil }
        return entries.first { entry in
            !entry.isEmpty && (executable == entry || components.contains(entry + ".app"))
        }
    }

    static func level(_ values: [Float]) -> Float? {
        values.isEmpty ? nil : values.reduce(0, +) / Float(values.count)
    }

    static func decide(_ input: Input) -> Verdict {
        guard let level = level(input.current) else { return .none }
        guard let held = input.held else { return .adopt(level) }

        if let foreign = input.foreign {
            let stillTheirs = input.current.allSatisfy { value in
                foreign.results.contains { abs($0 - value) <= tolerance }
            }
            // The ledger hands over only lines that moved the output, so `held` is not consulted:
            // a user change not yet judged can sit between it and `restore`.
            if stillTheirs, !foreign.overruled, abs(level - foreign.restore) > tolerance {
                if input.paused {
                    return .kept(from: held, to: level, writer: foreign.writer, note: "paused")
                }
                let prior = abs(held - foreign.restore) > tolerance
                    ? Prior(from: held, to: foreign.restore, writer: foreign.priorWriter ?? "unknown") : nil
                return .revert(from: level, restore: foreign.restore, writer: foreign.writer, prior: prior)
            }
        }

        guard abs(level - held) > tolerance else { return .none }
        if input.lastWriter == OutputVolumeLedger.selfWriter { return .adopt(level) }
        let writer = input.lastWriter ?? input.foreign?.writer ?? "unknown"
        return .kept(from: held, to: level, writer: writer, note: nil)
    }

    /// The write a `revert` asks for. Left and right get the same value, which also puts the
    /// balance back in the middle.
    static func actions(_ verdict: Verdict, device: AudioDevice) -> [Action] {
        guard case .revert(let from, let restore, let writer, _) = verdict else { return [] }
        let reason = String(format: "%@ %.0f%% -> %.0f%% (reverted %@)",
                            device.name, Double(from) * 100, Double(restore) * 100, writer)
        return [.setOutputVolume(device.id, restore, reason: reason)]
    }
}

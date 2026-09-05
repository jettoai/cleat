import Darwin
import Foundation
import ObjectiveC

/// What `audioaccessoryd` answered.
///
/// The three fields are exactly what `BTAudioRoutingResponse` carries, kept raw so the meaning is
/// worked out in one place - `outcome` - which is a pure function and therefore testable without
/// a phone, a headset, or the private framework being present at all.
struct RouteResponse: Equatable, Sendable {
    /// `1` is Route. Nothing else is documented, and nothing else is treated as success.
    let action: Int?
    let reason: String?
    let error: String?

    init(action: Int? = nil, reason: String? = nil, error: String? = nil) {
        self.action = action
        self.reason = reason
        self.error = error
    }

    /// The action value that means the daemon moved the audio.
    static let routeAction = 1

    /// What the answer means for the engine. Five cases, because they need five different
    /// responses: one is worth logging, one is worth nothing at all, and three differ only in how
    /// long to wait before asking again.
    enum Outcome: Equatable, Sendable {
        /// The hijack went through and the headset is coming back.
        case routed
        /// It was already here. Nothing changed, so nothing is logged.
        case alreadyRouted
        /// The phone outscored us. Worth saying once, and worth backing off from.
        case heldByRemote(String)
        /// A previous request of ours is still running; the next beat is soon enough.
        case busy
        /// Anything else, including an error where the reason is empty.
        case refused(String)
    }

    var outcome: Outcome {
        let text = reason ?? ""
        // Matched case insensitively, folded once: the daemon's wording is not promised to keep
        // its capitalisation. The detail below is cut from `text`, which keeps its own.
        let folded = text.lowercased()
        // "Already routed" wins over a Route action: whatever the daemon reports having done,
        // the headset was here before we asked, so nothing about this pass changed anything.
        if folded.contains("already routed") { return .alreadyRouted }
        if action == Self.routeAction { return .routed }
        if folded.contains("remote category") {
            // The reason reads "Rejected, Remote Category 301 > Local Category 200, ..."; the
            // "Rejected" half is already said by the log line this ends up in.
            let detail = text.hasPrefix("Rejected, ") ? String(text.dropFirst("Rejected, ".count)) : text
            return .heldByRemote(detail)
        }
        // "Previous hijack hasn't finished". Matched on two words rather than on the apostrophe,
        // which is the character most likely to be spelled differently in another build.
        if folded.contains("hijack"), folded.contains("finished") { return .busy }

        if !text.isEmpty { return .refused(text) }
        return .refused(error ?? "no reason given")
    }
}

/// Asking for a headset back. A protocol so the engine can be tested without the private
/// framework, and so an unavailable one is a value rather than a crash.
protocol RouteRequesting: AnyObject {
    /// False when this system has no routing request to make - the class is gone, renamed, or has
    /// lost one of the properties Cleat sets. Nothing is attempted when this is false.
    var isAvailable: Bool { get }

    /// Sends one request. `completion` is called on `queue`, once, and only if a response arrives:
    /// the engine's throttle, not a timer here, is what keeps a lost answer from wedging the rule.
    func request(
        address: String,
        score: Int32,
        reason: String,
        queue: DispatchQueue,
        completion: @escaping @Sendable (RouteResponse) -> Void
    )
}

/// The private Smart Routing path, as observed on macOS 26.
///
/// `audioaccessoryd` arbitrates who owns a pair of AirPods by score: the request carries what this
/// machine is doing (100 idle, 201 a playback session, 301 media, 501 a call) and the accessory
/// reports what the other device is doing. The request is refused only when the remote score is
/// strictly higher, so a tie goes to whoever asked. That is the same request the system itself
/// sends when an app starts audio, which is why a Mac that is playing gets the headset back from
/// an idle phone. There is no entitlement on it; there is also no promise it exists in the next macOS,
/// so every lookup below is checked and a missing piece turns the whole rule off rather than
/// crashing the daemon.
final class SmartRoutingClient: RouteRequesting, @unchecked Sendable {

    private static let frameworkPath =
        "/System/Library/PrivateFrameworks/AudioAccessoryServices.framework/AudioAccessoryServices"
    private static let requestClassName = "BTAudioRoutingRequest"
    /// Flags value 1 is Hijack: "take this device from whoever has it", as opposed to a plain
    /// request for a device nobody is using.
    static let hijackFlag = 1

    /// Every property Cleat sets. All of them are checked before any of them is set, because a
    /// `setValue(_:forKey:)` against a key the class does not have raises an Objective-C exception,
    /// which Swift cannot catch: an unavailable framework has to be found out about by asking, not
    /// by trying.
    private static let requiredKeys = [
        "deviceAddress", "appBundleID", "audioScore", "flags", "reason",
        "dispatchQueue", "responseHandler"
    ]

    /// The callbacks land here first and are then handed to the caller's queue. Keeping the
    /// daemon's own delivery queue separate from the engine queue means the engine never waits on
    /// a reply that is being delivered onto the very queue it is running on.
    private let deliveryQueue = DispatchQueue(label: "ai.jetto.cleat.routing")

    private let requestClass: NSObject.Type?

    /// The request objects waiting for an answer, one per address. A request owns the XPC
    /// connection its answer comes back on, so something has to hold it until then - and it must
    /// not be the response handler itself, which the request owns in turn: that cycle would leave
    /// a connection open for every request ever made. Keyed by address rather than counted,
    /// because the engine never has two requests out for the same headset, so a reply that never
    /// arrives parks one object and not a growing pile.
    private let lock = NSLock()
    private var pending: [String: NSObject] = [:]

    init() {
        requestClass = Self.resolveRequestClass()
    }

    var isAvailable: Bool { requestClass != nil }

    func request(
        address: String,
        score: Int32,
        reason: String,
        queue: DispatchQueue,
        completion: @escaping @Sendable (RouteResponse) -> Void
    ) {
        guard let requestClass else { return }

        let request = requestClass.init()
        request.setValue(address, forKey: "deviceAddress")
        request.setValue(Bundle.main.bundleIdentifier ?? "ai.jetto.cleat", forKey: "appBundleID")
        request.setValue(NSNumber(value: score), forKey: "audioScore")
        request.setValue(NSNumber(value: Self.hijackFlag), forKey: "flags")
        request.setValue(reason, forKey: "reason")
        request.setValue(deliveryQueue, forKey: "dispatchQueue")

        // The handler captures no reference to the request - `pending` is what keeps it alive -
        // and the release happens on the caller's queue, after this block has returned, rather
        // than out from under the block while it is still running.
        let handler: @convention(block) (AnyObject?) -> Void = { [weak self] raw in
            guard let self else { return }
            let response = Self.read(raw)
            queue.async {
                self.finish(address)
                completion(response)
            }
        }
        request.setValue(handler, forKey: "responseHandler")

        lock.lock()
        pending[address] = request
        lock.unlock()

        _ = request.perform(NSSelectorFromString("activate"))
    }

    private func finish(_ address: String) {
        lock.lock()
        pending[address] = nil
        lock.unlock()
    }

    /// Reads the three fields off a `BTAudioRoutingResponse`. Every one of them is optional here
    /// because they are optional there: a response that carries an error carries no reason.
    private static func read(_ raw: AnyObject?) -> RouteResponse {
        guard let response = raw as? NSObject else {
            return RouteResponse(error: "no response object")
        }
        return RouteResponse(
            action: (response.value(forKey: "action") as? NSNumber)?.intValue,
            reason: response.value(forKey: "reason") as? String,
            error: describeError(response.value(forKey: "error"))
        )
    }

    /// The `error` field is a code, and a successful response carries zero rather than nothing.
    /// Reporting that zero as an error would put "error 0" in the log next to every hijack that
    /// worked.
    private static func describeError(_ value: Any?) -> String? {
        switch value {
        case let number as NSNumber:
            return number.intValue == 0 ? nil : "code \(number.intValue)"
        case let error as NSError:
            return error.localizedDescription
        case let text as String:
            return text.isEmpty ? nil : text
        default:
            return nil
        }
    }

    /// The class, if this system has it with every property Cleat needs. Anything short of that is
    /// nil, and `isAvailable` reports the rule as off.
    private static func resolveRequestClass() -> NSObject.Type? {
        guard dlopen(frameworkPath, RTLD_NOW) != nil else { return nil }
        guard let candidate = NSClassFromString(requestClassName) as? NSObject.Type else { return nil }
        guard candidate.instancesRespond(to: NSSelectorFromString("activate")) else { return nil }

        // KVC finds a setter before it looks for an ivar, and every key here is a declared
        // property, so "does it respond to the setter" is the same question as "can this key be
        // set" - asked without setting anything.
        let missing = requiredKeys.contains { key in
            !candidate.instancesRespond(to: NSSelectorFromString(setterName(for: key)))
        }
        return missing ? nil : candidate
    }

    /// `deviceAddress` to `setDeviceAddress:`, which is how Objective-C names the setter KVC will
    /// look for.
    private static func setterName(for key: String) -> String {
        "set" + key.prefix(1).uppercased() + key.dropFirst() + ":"
    }
}

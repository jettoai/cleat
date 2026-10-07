import Foundation
import IOKit
import IOKit.pwr_mgt

/// Whether someone is using this Mac, read from two places that need no permission: the HID
/// system's idle counter, which `ioreg` shows to any user, and the power assertions, which
/// `pmset -g assertions` shows to any user. Measured on 2026-10-07 at 7µs and 55µs a read.
enum UserActivity {

    /// Seconds since the last keyboard, mouse or trackpad event, or nil when the registry does
    /// not answer.
    static func inputIdleSeconds() -> TimeInterval? {
        let service = IOServiceGetMatchingService(kIOMainPortDefault, IOServiceMatching("IOHIDSystem"))
        guard service != 0 else { return nil }
        defer { IOObjectRelease(service) }
        guard let value = IORegistryEntryCreateCFProperty(
            service, "HIDIdleTime" as CFString, kCFAllocatorDefault, 0
        )?.takeRetainedValue() as? NSNumber else { return nil }
        return value.doubleValue / 1_000_000_000  // nanoseconds
    }

    /// Whether any app is holding the display awake. A video playing in a browser or a player
    /// does, which is how someone watching without touching anything still counts as here.
    /// Music does not.
    static func displayHeldAwake() -> Bool {
        var status: Unmanaged<CFDictionary>?
        guard IOPMCopyAssertionsStatus(&status) == kIOReturnSuccess,
              let levels = status?.takeRetainedValue() as? [String: Any] else { return false }
        return ((levels["PreventUserIdleDisplaySleep"] as? NSNumber)?.intValue ?? 0) > 0
    }
}

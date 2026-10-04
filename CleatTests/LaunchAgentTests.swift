import ServiceManagement
import XCTest
@testable import Cleat

final class LaunchAgentTests: XCTestCase {

    func testOnlyAnEnabledRegistrationWithNoJobNeedsRegisteringAgain() {
        let statuses: [SMAppService.Status] = [.enabled, .notRegistered, .notFound, .requiresApproval]
        for status in statuses {
            for loaded in [true, false] {
                let onlyTrueCell = status == .enabled && !loaded
                XCTAssertEqual(
                    LaunchAgent.needsReregistration(status: status, jobLoaded: loaded),
                    onlyTrueCell,
                    "status \(status.rawValue), loaded \(loaded)"
                )
            }
        }
    }
}

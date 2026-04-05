// ABOUTME: Basic smoke tests for the SproutMobile Swift wrapper around the Rust core.
// ABOUTME: Full integration tests live in the sprout-mobile Rust crate; these just verify linkage.
import XCTest
@testable import SproutMobile

final class SproutMobileTests: XCTestCase {
    func testPackageLoads() throws {
        // If this compiles and runs, the XCFramework linked successfully.
        XCTAssertTrue(true)
    }
}

import XCTest

@testable import FedimintSdk

/// Offline tests for the generated bindings.
///
/// Everything here runs against the XCFramework's macOS slice under plain
/// `swift test`, with no federation, no network and no simulator. That is the
/// deliberate boundary: anything that needs a live federation belongs in the
/// demo app (`ios/Demo`), which is the iOS counterpart of running
/// `android/app` on a device.
///
/// The point of these is not to re-test the Rust — the crate has its own suite —
/// but to prove the *binding* works end to end: that values cross in both
/// directions, that a Rust error arrives as a catchable `SdkError` carrying its
/// `ErrorCode`, and that the async exports run on a runtime that does not abort
/// the process the first time the client touches storage.
final class FedimintSdkTests: XCTestCase {

    /// A real invite code with a valid bech32m checksum, lifted from the crate's
    /// own `types/invite.rs` tests so both sides parse the same fixture. Nothing
    /// is reachable at the guardian URL it names, which is exactly why it suits
    /// an offline test: parsing never contacts it.
    private static let inviteCode =
        "fed11qgqpqrnhwden5te0vehk7tnzv9ez7qqpyq4z52329g4z52329g4z52329g4z"
        + "52329g4z52329g4z52329g4z5wa8phk"

    /// The federation id that code invites to: 32 bytes of 0x2a.
    private static let inviteFederationId = String(repeating: "2a", count: 32)

    // MARK: - Mnemonic

    func testGenerateProducesTwelveWords() throws {
        let words = try Mnemonic.generate().words()
        XCTAssertEqual(words.count, 12, "BIP-39 with 128 bits of entropy is a 12-word phrase")
        XCTAssertFalse(words.contains(where: \.isEmpty))
    }

    /// The round trip is what proves `Vec<String>` crosses correctly in both
    /// directions — out through `words()` and back in through `fromWords`.
    func testMnemonicRoundTripsThroughWords() throws {
        let original = try Mnemonic.generate().words()
        let restored = try Mnemonic.fromWords(words: original).words()
        XCTAssertEqual(restored, original)
    }

    func testMalformedMnemonicThrowsInvalidInput() {
        XCTAssertThrowsError(try Mnemonic.fromWords(words: ["not", "a", "seed"])) { error in
            assertCode(error, .invalidInput)
        }
    }

    // MARK: - InviteCode

    func testInviteCodeParsesAndRoundTrips() throws {
        let invite = try InviteCode.parse(code: Self.inviteCode)
        XCTAssertEqual(invite.federationId(), Self.inviteFederationId)
        // `display()` is the only way back out to a string, and it has to be
        // the same code: an invite is a bearer credential the binding never
        // hands over implicitly.
        XCTAssertEqual(invite.display(), Self.inviteCode)
    }

    func testMalformedInviteCodeThrowsInvalidInput() {
        XCTAssertThrowsError(try InviteCode.parse(code: "fed11-not-a-real-code")) { error in
            assertCode(error, .invalidInput)
        }
    }

    // MARK: - Notes

    func testMalformedNotesThrowInvalidInput() {
        XCTAssertThrowsError(try Notes.parse(notes: "definitely not ecash")) { error in
            assertCode(error, .invalidInput)
        }
    }

    // MARK: - Sdk

    /// The one test that exercises the async path end to end.
    ///
    /// `createFedimintSdk` is `#[uniffi::export(async_runtime = "tokio")]`, so it
    /// runs inside `async-compat`'s process-wide fallback runtime. That runtime
    /// is current-thread unless `async-compat/multi-thread` is on, and the first
    /// `fedimint-rocksdb` transaction on a current-thread scheduler aborts the
    /// process on `tokio::task::block_in_place`. If that feature ever comes off
    /// in `rust/fedimint-sdk/Cargo.toml`, this test does not fail — the test
    /// runner dies — which is the loudest possible signal and the reason this
    /// runs at all.
    func testOpensStorageAndExportsTheSeedItWasGiven() async throws {
        let dataDir = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dataDir) }

        let mnemonic = try Mnemonic.generate()
        let expected = mnemonic.words()

        var sdk: Sdk? = try await withTimeout {
            try await createFedimintSdk(dataDir: dataDir.path, mnemonic: mnemonic)
        }
        XCTAssertEqual(sdk?.exportMnemonic().words(), expected)
        XCTAssertEqual(sdk?.federations().count, 0, "nothing has been joined yet")

        try await sdk?.shutdown()
        sdk = nil
    }

    /// Reopening the same directory has to return the seed already stored there,
    /// not generate a new one — this is what a second app launch does.
    ///
    /// Note the `first = nil`, which is load bearing rather than tidy. `shutdown()`
    /// releases the storage lock, but the crate documents that the underlying
    /// store stays open until *every* handle over it has actually been dropped,
    /// and that a build against the same location before then is left waiting.
    /// In Swift a handle is dropped by ARC, not by `shutdown()`, so holding
    /// `first` in scope across the second `createFedimintSdk` deadlocks — it does
    /// not throw. Callers reopening a directory in one process have to do the
    /// same thing.
    func testReopeningLoadsTheStoredSeed() async throws {
        let dataDir = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dataDir) }

        var first: Sdk? = try await withTimeout {
            try await createFedimintSdk(dataDir: dataDir.path, mnemonic: nil)
        }
        let seed = first?.exportMnemonic().words()
        XCTAssertEqual(seed?.count, 12)
        try await first?.shutdown()
        first = nil

        var second: Sdk? = try await withTimeout {
            try await createFedimintSdk(dataDir: dataDir.path, mnemonic: nil)
        }
        XCTAssertEqual(second?.exportMnemonic().words(), seed)
        try await second?.shutdown()
        second = nil
    }

    /// A second opener while the first still holds the lock is refused rather
    /// than queued. This is the error an app has to show the user rather than
    /// retry — and it is the case that must *not* behave like the reopen above,
    /// where waiting is correct.
    func testSecondOpenOfTheSameDirectoryIsRefused() async throws {
        let dataDir = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dataDir) }

        var held: Sdk? = try await withTimeout {
            try await createFedimintSdk(dataDir: dataDir.path, mnemonic: nil)
        }

        do {
            _ = try await withTimeout {
                try await createFedimintSdk(dataDir: dataDir.path, mnemonic: nil)
            }
            XCTFail("a second open of a locked directory should have been refused")
        } catch {
            assertCode(error, .storageInUse)
        }

        try await held?.shutdown()
        held = nil
    }

    // MARK: - Packaging

    /// Cheap guard against the one file in `Sources/FedimintSdk` that is hand
    /// written drifting from the crate it describes.
    func testVersionMatchesTheCrate() {
        XCTAssertEqual(fedimintSdkVersion, "0.1.0-alpha.1")
    }

    // MARK: - Helpers

    /// Fails the test instead of hanging the whole run.
    ///
    /// Not belt and braces: the storage tests above are exactly the ones whose
    /// failure mode is a deadlock rather than a wrong value — `createFedimintSdk`
    /// waits for the store when a handle is still open. Without this, a
    /// regression costs CI its full timeout and reports nothing useful.
    private func withTimeout<T: Sendable>(
        seconds: UInt64 = 60,
        _ work: @escaping @Sendable () async throws -> T
    ) async throws -> T {
        try await withThrowingTaskGroup(of: T.self) { group in
            group.addTask { try await work() }
            group.addTask {
                try await Task.sleep(nanoseconds: seconds * 1_000_000_000)
                throw TimedOut(seconds: seconds)
            }
            guard let first = try await group.next() else { throw TimedOut(seconds: seconds) }
            group.cancelAll()
            return first
        }
    }

    private struct TimedOut: Swift.Error, CustomStringConvertible {
        let seconds: UInt64
        var description: String { "timed out after \(seconds)s — most likely a deadlock" }
    }

    private func makeTemporaryDirectory() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("fedimint-sdk-tests")
            .appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    /// Asserts the thrown value is an `SdkError` carrying `expected`.
    ///
    /// Written out rather than inlined because the interesting half is the cast:
    /// every fallible export throws the crate's own error interface, so a test
    /// that only checked `XCTAssertThrowsError` would pass even if the binding
    /// lost the error type entirely.
    private func assertCode(
        _ error: Swift.Error,
        _ expected: ErrorCode,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        guard let error = error as? SdkError else {
            XCTFail("expected an SdkError, got \(type(of: error))", file: file, line: line)
            return
        }
        XCTAssertEqual(error.code(), expected, error.reason(), file: file, line: line)
    }
}

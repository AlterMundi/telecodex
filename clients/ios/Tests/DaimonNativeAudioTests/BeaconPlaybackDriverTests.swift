import Foundation
import XCTest
@testable import DaimonNativeAudio

@MainActor private final class BeaconOutputFixture: PCMOutputDevice {
    var completions: [@Sendable () -> Void] = [], buffers: [Data] = []
    var starts = 0, stops = 0, pauses = 0, failSchedule = false
    func prepare() throws {}
    func schedule(_ data: Data, completion: @escaping @Sendable () -> Void) throws {
        if failSchedule { throw BeaconPlaybackDriver.Refusal.stalled }
        buffers.append(data); completions.append(completion)
    }
    func start() throws { starts += 1 }
    func pause() { pauses += 1 }
    func stop() { stops += 1 }
}

final class BeaconPlaybackDriverTests: XCTestCase {
    @MainActor private func settle() async {
        await withCheckedContinuation { next in DispatchQueue.main.async { next.resume() } }
    }
    private func frame(expires: Double, frequency: Double = 40.4, level: Double = 0.2) throws -> BeaconPlaybackDriver.Frame {
        try .init(frequencies:[frequency,80.8,121.2,161.6,282.8,444.4],pulses:Array(repeating:0,count:6),
                  weights:[1,0,0,0,0,0],depth:0,level:level,transitionMilliseconds:20,expiresAtUptime:expires)
    }
    @MainActor func testOnlyContiguousPlayedCallbacksReportSoundAndQueueIsBounded() async throws {
        let device = BeaconOutputFixture(); var played: [Int] = [], acquisitions = 0
        let driver = try BeaconPlaybackDriver(device:device,activate:{acquisitions += 1},deactivate:{},clock:{0}) {
            if case .playing(let n,_) = $0 { played.append(n) }
        }
        defer { driver.close() }
        try driver.frame(frame(expires:20))
        XCTAssertEqual(device.completions.count,2); XCTAssertEqual(played,[]); XCTAssertEqual(acquisitions,1)
        XCTAssertTrue(device.buffers.allSatisfy { $0.count == 1920 })
        for _ in 0..<100 { try driver.level(0.2) }
        XCTAssertEqual(device.completions.count,2,"Control refresh cannot grow the scheduled queue")
        device.completions[1](); await settle(); XCTAssertEqual(played,[])
        device.completions[0](); await settle(); XCTAssertEqual(played,[1920])
        XCTAssertEqual(device.completions.count,4)
        device.completions[0](); await settle(); XCTAssertEqual(played,[1920])
    }
    @MainActor func testGainCannotRefreshExpiredFactsAndFreshFrameCanResume() async throws {
        let device = BeaconOutputFixture(); var now = 0.0, quiet = 0, releases = 0, acquisitions = 0
        let driver = try BeaconPlaybackDriver(device:device,activate:{acquisitions += 1},deactivate:{releases += 1},clock:{now}) {
            if case .quiet = $0 { quiet += 1 }
        }
        defer { driver.close() }
        try driver.frame(frame(expires:0))
        XCTAssertEqual(device.completions.count,0); XCTAssertEqual(acquisitions,0,"Already stale data cannot claim playback")
        try driver.frame(frame(expires:0.1))
        now = 0.11; try driver.level(0.3,milliseconds:0)
        var cursor = 0
        for _ in 0..<40 {
            if cursor < device.completions.count {
                now += 0.04; device.completions[cursor](); cursor += 1; await settle()
            }
        }
        XCTAssertEqual(releases,1); XCTAssertGreaterThan(quiet,0)
        let before = device.completions.count
        try driver.level(0.4,milliseconds:0)
        XCTAssertEqual(device.completions.count,before,"Gain cannot revive stale weights")
        try driver.frame(frame(expires:now+1))
        XCTAssertEqual(acquisitions,2); XCTAssertEqual(device.completions.count,before+2)
    }
    @MainActor func testSilentOwnerReleasesOnlyItsOwnSharedSessionHolder() async throws {
        let device = BeaconOutputFixture()
        let sessionDevice = SharedBeaconSessionFixture(), session = AudioSessionOwnership(device:sessionDevice)
        let speech = UUID(), beacon = UUID(); var now = 0.0
        try session.acquire(speech,cancelled:{})
        let driver = try BeaconPlaybackDriver(device:device,activate:{try session.acquire(beacon,cancelled:{})},
            deactivate:{_ = session.release(beacon)},clock:{now},emit:{_ in})
        defer { driver.close() }
        try driver.frame(frame(expires:10)); try driver.level(0,milliseconds:0)
        device.completions[0](); device.completions[1](); await settle()
        XCTAssertEqual(session.count,1); XCTAssertEqual(sessionDevice.stops,0)
        try driver.frame(frame(expires:10)); XCTAssertEqual(session.count,2)
        now = 0.01; driver.close(); XCTAssertEqual(session.count,1)
        session.release(speech); XCTAssertEqual(sessionDevice.stops,1)
    }
    @MainActor func testAbsoluteLifetimeAndCloseFenceLateCallbacksWithoutReplay() async throws {
        let device = BeaconOutputFixture(); var now = 0.0, closed = 0, releases = 0
        let driver = try BeaconPlaybackDriver(device:device,activate:{},deactivate:{releases += 1},clock:{now}) {
            if case .closed = $0 { closed += 1 }
        }
        try driver.frame(frame(expires:20)); let old = device.completions[0]
        now = 7199; try driver.frame(frame(expires:7219))
        now = 7200; driver.tick(); old(); await settle()
        XCTAssertEqual(closed,1); XCTAssertEqual(releases,1)
        XCTAssertThrowsError(try driver.frame(frame(expires:7220)))
        XCTAssertEqual(device.completions.count,2); driver.close(); XCTAssertEqual(closed,1)
    }
    @MainActor func testInvalidVocabularyDeviceFailureAndStallDoNotRetry() async throws {
        let device = BeaconOutputFixture(); var now = 0.0, unavailable = 0
        let driver = try BeaconPlaybackDriver(device:device,activate:{},deactivate:{},clock:{now}) {
            if case .unavailable = $0 { unavailable += 1 }
        }
        XCTAssertThrowsError(try driver.frame(frame(expires:21)))
        XCTAssertEqual(device.completions.count,0)
        try driver.frame(frame(expires:20))
        now = 2; driver.tick()
        XCTAssertEqual(unavailable,1); XCTAssertThrowsError(try driver.level(0.2))
        let failed = BeaconOutputFixture(); failed.failSchedule = true
        let next = try BeaconPlaybackDriver(device:failed,activate:{},deactivate:{},clock:{0},emit:{_ in})
        XCTAssertThrowsError(try next.frame(frame(expires:20)))
        XCTAssertThrowsError(try next.frame(frame(expires:20))); XCTAssertEqual(failed.completions.count,0)
        let changed = try BeaconPlaybackDriver(device:BeaconOutputFixture(),activate:{},deactivate:{},clock:{0},emit:{_ in})
        try changed.frame(frame(expires:20))
        XCTAssertThrowsError(try changed.frame(frame(expires:20,frequency:41)))
        XCTAssertThrowsError(try changed.level(0.2))
    }
}
@MainActor private final class SharedBeaconSessionFixture: AudioSessionDevice {
    var starts = 0, stops = 0
    func activate() throws { starts += 1 }
    func deactivate() throws { stops += 1 }
}

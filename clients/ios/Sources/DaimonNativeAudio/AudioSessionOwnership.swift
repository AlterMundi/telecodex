import Foundation

enum AudioSessionMode { case spokenOutput, duplexVoice }

@MainActor protocol AudioSessionDevice: AnyObject {
    func activate(mode: AudioSessionMode) throws
    func deactivate() throws
}

/// One app audio session with at most two explicit speech/Beacon holders.
/// An unconfirmed release is retained; neither a new owner nor a stale callback
/// can hide it by creating another activation or retrying deactivation.
@MainActor final class AudioSessionOwnership {
    enum Refusal: Error { case unavailable, capacity }
    private let device: AudioSessionDevice
    private var holders: [UUID: () -> Void] = [:]
    private var cancelling = false
    private(set) var mode: AudioSessionMode?
    private(set) var unconfirmedRelease: UUID?
    var count: Int { holders.count }
    init(device: AudioSessionDevice) { self.device = device }

    func acquire(_ owner: UUID, mode requested: AudioSessionMode = .spokenOutput, cancelled: @escaping () -> Void) throws {
        guard !cancelling, unconfirmedRelease == nil else { throw Refusal.unavailable }
        if holders[owner] != nil {
            guard mode == requested else { throw Refusal.unavailable }; return
        }
        guard holders.isEmpty || mode == requested else { throw Refusal.unavailable }
        guard holders.count < 2 else { throw Refusal.capacity }
        if holders.isEmpty {
            do { try device.activate(mode: requested) }
            catch {
                // Clean only this attempted activation; failure remains visible.
                do { try device.deactivate() } catch { unconfirmedRelease = owner }
                throw error
            }
        }
        mode = requested
        holders[owner] = cancelled
    }
    @discardableResult func release(_ owner: UUID) -> Bool {
        guard holders.removeValue(forKey: owner) != nil else { return unconfirmedRelease == nil }
        if holders.isEmpty {
            mode = nil
            do { try device.deactivate() } catch { unconfirmedRelease = owner; return false }
        }
        return true
    }
    func cancelOwned() {
        guard !cancelling else { return }
        cancelling = true
        let snapshot = holders
        for (owner, cancel) in snapshot { cancel(); release(owner) }
        cancelling = false
    }
    /// The host may expose this only as an explicit recovery action for the
    /// exact retained failed owner. Ordinary close/refresh never calls it.
    func recoverRelease(_ owner: UUID) -> Bool {
        guard holders.isEmpty, unconfirmedRelease == owner else { return false }
        do { try device.deactivate(); unconfirmedRelease = nil; return true } catch { return false }
    }
}

import Foundation

/// One reschedulable monotonic timer, not a new queued work item per refresh.
/// Its main-actor box lets the driver arrange cancellation safely at deinit.
@MainActor private final class BeaconWake {
    private let source = DispatchSource.makeTimerSource(queue:.main)
    func start(_ tick: @escaping @MainActor () -> Void) {
        source.setEventHandler { Task { @MainActor in tick() } }
        source.activate()
    }
    func after(_ seconds: TimeInterval) {
        source.schedule(deadline:.now()+max(0.001,seconds),leeway:.milliseconds(5))
    }
    func cancel() { source.cancel() }
}

/// Foreground six-carrier output. The host supplies only its own session holder.
/// New controls cannot replenish the absolute lifetime or revive expired facts.
@MainActor public final class BeaconPlaybackDriver {
    public enum Refusal: Error { case closed, expired, clockChanged, invalidFrame, configurationChanged, stalled }
    public enum Event {
        case ready
        case playing(playedFrames: Int, control: UInt64)
        case quiet(control: UInt64)
        case closed, unavailable
    }
    public struct Frame {
        let frequencies: [Double], pulses: [Double], weights: [Double]
        let depth: Double, level: Double, transition: Int, expires: TimeInterval
        public init(frequencies: [Double], pulses: [Double], weights: [Double],
                    depth: Double, level: Double, transitionMilliseconds: Int,
                    expiresAtUptime: TimeInterval) throws {
            guard frequencies.count == 6, pulses.count == 6, weights.count == 6,
                  frequencies.allSatisfy({ $0.isFinite && (20...16384).contains($0) }),
                  pulses.allSatisfy({ $0.isFinite && (0...64).contains($0) }),
                  weights.allSatisfy({ $0.isFinite && (0...1).contains($0) }),
                  zip(frequencies,weights).allSatisfy({ pair in pair.1 == 0 || pair.0 < 10800 }),
                  weights.reduce(0,+) <= 1+1e-9,
                  depth.isFinite, (0...0.5).contains(depth), level.isFinite, (0...0.8).contains(level),
                  (20...2000).contains(transitionMilliseconds),
                  expiresAtUptime.isFinite, expiresAtUptime >= 0 else { throw Refusal.invalidFrame }
            self.frequencies = frequencies; self.pulses = pulses; self.weights = weights
            self.depth = depth; self.level = level; transition = transitionMilliseconds; expires = expiresAtUptime
        }
        func sameConfiguration(_ other: Frame) -> Bool {
            frequencies == other.frequencies && pulses == other.pulses && depth == other.depth
        }
    }
    private struct Ticket { let id: UUID; let control: UInt64; var done = false }
    private let device: PCMOutputDevice
    private let activate: @MainActor () throws -> Void
    private let deactivate: @MainActor () -> Void
    private let emit: (Event) -> Void
    private let clock: () -> TimeInterval
    private let expires: TimeInterval
    private let wake: BeaconWake
    private var lastClock: TimeInterval
    private var lastProgress: TimeInterval
    private var configuration: Frame?
    private var synth: BeaconSynth?
    private var tickets: [Ticket] = []
    private var epoch = UUID()
    private var control: UInt64 = 0
    private var validUntil: TimeInterval = 0
    private var playedFrames = 0, producedFrames = 0
    private var active = false, closed = false, stale = true, reportedPlaying = false, reportedQuiet = false
    private var lifetimeFade = false
    private let quantum = 960 // 40ms at24kHz; at most two buffers/80ms scheduled.
    private var samples = Array(repeating:Int16(0),count:960)

    public convenience init(activate: @escaping @MainActor () throws -> Void,
                            deactivate: @escaping @MainActor () -> Void,
                            emit: @escaping (Event) -> Void) throws {
        try self.init(device:ApplePCMOutput(),activate:activate,deactivate:deactivate,
                      clock:{ ProcessInfo.processInfo.systemUptime },emit:emit)
    }
    init(device: PCMOutputDevice, activate: @escaping @MainActor () throws -> Void,
         deactivate: @escaping @MainActor () -> Void, clock: @escaping () -> TimeInterval,
         emit: @escaping (Event) -> Void) throws {
        let now = clock()
        guard now.isFinite, now >= 0 else { throw Refusal.clockChanged }
        self.device = device; self.activate = activate; self.deactivate = deactivate
        self.clock = clock; self.emit = emit; expires = now+7200; lastClock = now; lastProgress = now
        try device.prepare()
        wake = BeaconWake()
        wake.start { [weak self] in self?.tick() }
        wake.after(7200); emit(.ready)
    }
    private func check() throws -> TimeInterval {
        guard !closed else { throw Refusal.closed }
        let now = clock()
        guard now.isFinite, now >= lastClock else { fail(); throw Refusal.clockChanged }
        lastClock = now
        guard now < expires else { close(); throw Refusal.expired }
        return now
    }
    @discardableResult public func frame(_ value: Frame) throws -> UInt64 {
        let now = try check()
        guard value.expires <= now+20 else { throw Refusal.invalidFrame }
        if let previous = configuration, !previous.sameConfiguration(value) {
            fail(); throw Refusal.configurationChanged
        }
        if synth == nil {
            synth = try BeaconSynth(frequencies:value.frequencies,pulses:value.pulses,depth:value.depth)
            configuration = value
        }
        let fresh = value.expires > now
        try synth?.weights(fresh ? value.weights : Array(repeating:0,count:6),milliseconds:fresh ? value.transition : 1000)
        if now < expires-0.35 && !lifetimeFade { try synth?.level(value.level,milliseconds:120) }
        control += 1; validUntil = value.expires; stale = !fresh; reportedQuiet = false
        do { try pump(now); reschedule(); return control } catch { fail(); throw error }
    }
    @discardableResult public func level(_ value: Double, milliseconds: Int = 180) throws -> UInt64 {
        let now = try check()
        guard value.isFinite, (0...0.8).contains(value), (0...2000).contains(milliseconds) else { throw Refusal.invalidFrame }
        if now < expires-0.35 && !lifetimeFade { try synth?.level(value,milliseconds:milliseconds) }
        control += 1; reportedQuiet = false // Never update validUntil.
        do { try pump(now); reschedule(); return control } catch { fail(); throw error }
    }
    private func applyExpiry(_ now: TimeInterval) throws {
        if !lifetimeFade && now >= expires-0.35 {
            lifetimeFade = true
            try synth?.level(0,milliseconds:max(1,Int((expires-now)*1000)))
        }
        if now >= validUntil && !stale { stale = true; try synth?.weights(Array(repeating:0,count:6),milliseconds:1000) }
    }
    private func pump(_ now: TimeInterval) throws {
        guard !closed else { return }
        try applyExpiry(now)
        guard let synth else { return }
        while tickets.count < 2 && !synth.silent {
            guard producedFrames <= 172800000-quantum else { close(); throw Refusal.expired }
            if !active { try activate(); active = true; lastProgress = now }
            try synth.render(into:&samples,count:quantum); producedFrames += quantum
            var data = Data(); data.reserveCapacity(quantum*2)
            for sample in samples {
                let bits = UInt16(bitPattern:sample)
                data.append(UInt8(truncatingIfNeeded:bits)); data.append(UInt8(truncatingIfNeeded:bits>>8))
            }
            let id = UUID(), generation = epoch
            tickets.append(Ticket(id:id,control:control))
            try device.schedule(data) { [weak self] in
                DispatchQueue.main.async { self?.completed(id,generation:generation) }
            }
            try device.start()
        }
        if tickets.isEmpty && synth.silent {
            device.pause(); release(); reportedPlaying = false
            guard !closed else { return }
            if !reportedQuiet { reportedQuiet = true; emit(.quiet(control:control)) }
            if lifetimeFade { close() }
        }
    }
    private func completed(_ id: UUID, generation: UUID) {
        guard !closed, epoch == generation, let index = tickets.firstIndex(where:{$0.id == id}) else { return }
        do {
            let now = try check()
            tickets[index].done = true
            var last: UInt64?
            while tickets.first?.done == true {
                last = tickets.removeFirst().control; playedFrames += quantum; lastProgress = now
            }
            if let last, !reportedPlaying {
                reportedPlaying = true; emit(.playing(playedFrames:playedFrames,control:last))
            }
            guard !closed, epoch == generation else { return }
            try pump(now); reschedule()
        } catch { if !closed { fail() } }
    }
    /// Existing timer or synthetic qualification clock only; no new observations.
    func tick() {
        guard !closed else { return }
        do {
            let now = try check()
            if !tickets.isEmpty && now-lastProgress >= 2 { fail(); return }
            try pump(now); reschedule()
        } catch { if !closed { fail() } }
    }
    private func reschedule() {
        guard !closed, let now = try? check() else { return }
        var next = lifetimeFade ? expires : expires-0.35
        if !stale { next = min(next,validUntil) }
        if !tickets.isEmpty { next = min(next,lastProgress+2) }
        wake.after(next-now)
    }
    private func release() { if active { active = false; deactivate() } }
    private func fail() { guard !closed else { return }; close(); emit(.unavailable) }
    public func close() {
        guard !closed else { return }
        closed = true; epoch = UUID(); wake.cancel(); tickets.removeAll()
        device.stop(); release(); emit(.closed)
    }
    deinit {
        let timer = wake, output = device, release = active ? deactivate : nil
        DispatchQueue.main.async { timer.cancel(); output.stop(); release?() }
    }
}

import CoreFoundation
import Foundation

/// Closed PCM/Beacon wire contracts shared with the existing native-audio.js.
struct PCMBridgeMessage {
    enum Refusal: Error { case invalid }
    let command: String
    let document: String
    let id: String?
    let fields: [String: Any]
    init(_ raw: String) throws {
        guard raw.utf8.count <= 45000, let data = raw.data(using: .utf8),
              let fields = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let command = fields["command"] as? String, let document = fields["document_id"] as? String,
              UUID(uuidString: document)?.uuidString.lowercased() == document else { throw Refusal.invalid }
        let extras: Set<String>
        switch command {
        case "hello": extras = []
        case "session":
            extras = ["enabled"]
            guard let enabled = fields["enabled"] as? NSNumber,
                  CFGetTypeID(enabled) == CFBooleanGetTypeID() else { throw Refusal.invalid }
        case "beacon_begin", "beacon_stop": extras = ["id"]
        case "beacon_frame": extras = ["id", "frequencies", "pulses", "weights", "depth", "level", "transition_ms", "expires_at_ms"]
        case "beacon_level": extras = ["id", "level", "transition_ms"]
        case "pcm_begin": extras = ["id", "seconds"]
        case "pcm_chunk": extras = ["id", "sequence", "utterance", "end", "data", "sha256"]
        case "pcm_flush", "pause", "play", "stop": extras = ["id"]
        default: throw Refusal.invalid
        }
        guard Set(fields.keys) == extras.union(["command", "document_id"]) else { throw Refusal.invalid }
        let id = fields["id"] as? String
        if extras.contains("id") {
            guard let id else { throw Refusal.invalid }
            if command.hasPrefix("beacon_") {
                guard raw.utf8.count <= 4096, id.hasPrefix("beacon_"), id.utf8.count == 43,
                      UUID(uuidString:String(id.dropFirst(7)))?.uuidString.lowercased() == String(id.dropFirst(7)) else { throw Refusal.invalid }
            } else if id.range(of:"^pcm_[1-9][0-9]{0,15}$",options:.regularExpression) == nil { throw Refusal.invalid }
        }
        self.command = command; self.document = document; self.id = id; self.fields = fields
    }
    func integer(_ key: String, _ range: ClosedRange<Int>) throws -> Int {
        guard let value = fields[key] as? NSNumber, CFGetTypeID(value) != CFBooleanGetTypeID(),
              value.doubleValue.isFinite, value.doubleValue.rounded() == value.doubleValue,
              value.doubleValue >= Double(range.lowerBound), value.doubleValue <= Double(range.upperBound)
        else { throw Refusal.invalid }
        return value.intValue
    }
}


extension PCMBridgeMessage {
    func number(_ key: String, _ range: ClosedRange<Double>) throws -> Double {
        guard let value = fields[key] as? NSNumber, CFGetTypeID(value) != CFBooleanGetTypeID(),
              value.doubleValue.isFinite, range.contains(value.doubleValue) else { throw Refusal.invalid }
        return value.doubleValue
    }
    private func vector(_ key: String, _ range: ClosedRange<Double>) throws -> [Double] {
        guard let values = fields[key] as? [Any], values.count == 6 else { throw Refusal.invalid }
        return try values.map { raw in
            guard let value = raw as? NSNumber, CFGetTypeID(value) != CFBooleanGetTypeID(),
                  value.doubleValue.isFinite, range.contains(value.doubleValue) else { throw Refusal.invalid }
            return value.doubleValue
        }
    }
    func beaconFrame() throws -> BeaconPlaybackDriver.Frame {
        guard command == "beacon_frame" else { throw Refusal.invalid }
        return try .init(frequencies:vector("frequencies",20...16384),pulses:vector("pulses",0...64),
                         weights:vector("weights",0...1),depth:number("depth",0...0.5),level:number("level",0...0.8),
                         transitionMilliseconds:integer("transition_ms",20...2000),
                         expiresAtUptime:Double(integer("expires_at_ms",0...9007199254740991))/1000)
    }
}

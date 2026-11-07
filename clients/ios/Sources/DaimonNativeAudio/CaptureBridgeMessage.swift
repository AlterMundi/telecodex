import Foundation

/// Session ownership only. No recorder, file, URL or permission command.
struct CaptureBridgeMessage {
    enum Refusal: Error { case invalid }
    let command: String
    let document: String
    let owner: UUID
    init(_ raw: String) throws {
        guard raw.utf8.count <= 512, let data = raw.data(using:.utf8),
              let value = try JSONSerialization.jsonObject(with:data) as? [String:Any],
              Set(value.keys) == ["command","document_id","owner"],
              let command = value["command"] as? String, ["capture_begin","capture_end"].contains(command),
              let document = value["document_id"] as? String,
              UUID(uuidString:document)?.uuidString.lowercased() == document,
              let text = value["owner"] as? String, let owner = UUID(uuidString:text),
              owner.uuidString.lowercased() == text else { throw Refusal.invalid }
        self.command = command; self.document = document; self.owner = owner
    }
}

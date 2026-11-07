import Foundation
import XCTest
@testable import DaimonNativeAudio

final class InteractionBridgeTests: XCTestCase {
    private let document = UUID().uuidString.lowercased()
    private func message(_ fields: [String: Any]) throws -> PCMBridgeMessage {
        var fields = fields; fields["document_id"] = document
        return try PCMBridgeMessage(String(data: JSONSerialization.data(withJSONObject: fields), encoding: .utf8)!)
    }
    func testHomeSelectionAndNativePagesDoNotWidenToLoginOrOtherOrigins() throws {
        let home = try InteractionHome(" https://Example.org:443/interaction/ ")
        XCTAssertEqual(home.origin.absoluteString, "https://example.org")
        XCTAssertEqual(home.conversation.absoluteString, "https://example.org/interaction/?view=conversation&client=standalone")
        XCTAssertTrue(home.allowsPage(home.conversation), "URL path=\(home.conversation.path), components path=\(URLComponents(url:home.conversation,resolvingAgainstBaseURL:true)?.path ?? "missing")")
        for value in ["http://example.org", "https://user:secret@example.org", "https://example.org:8443",
                      "https://example.org/?token=secret", "https://example.org/#other", "https://example.org/works/"] {
            XCTAssertThrowsError(try InteractionHome(value))
        }
        for value in ["https://other.example/interaction/", "https://auth.dmn.homes/interaction/",
                      "https://example.org/interaction-evil/", "https://example.org:8443/interaction/",
                      "https://example.org/interaction/../works/", "https://example.org/interaction/%2e%2e/works/",
                      "https://user@example.org/interaction/"] {
            XCTAssertFalse(home.allowsPage(try XCTUnwrap(URL(string: value))), value)
        }
    }
    func testOnlyTheClosedPCMContractAndCanonicalDocumentCanEnter() throws {
        XCTAssertEqual(try message(["command":"hello"]).document, document)
        XCTAssertEqual(try message(["command":"pcm_begin","id":"pcm_1","seconds":600]).integer("seconds",300...600),600)
        let invalid: [[String: Any]] = [
            ["command":"beacon_begin","id":"pcm_1"], ["command":"begin","id":"pcm_1"],
            ["command":"pcm_begin","id":"pcm_1","seconds":300,"url":"https://example.org"],
            ["command":"stop","id":"pcm_0"], ["command":"stop","id":"foreign"],
            ["command":"session","enabled":1], ["command":"stop"]
        ]
        for fields in invalid { XCTAssertThrowsError(try message(fields)) }
        XCTAssertThrowsError(try PCMBridgeMessage("{\"command\":\"hello\",\"document_id\":\"invalid\"}"))
        XCTAssertThrowsError(try PCMBridgeMessage(String(repeating:" ",count:45001)))
    }
    func testBooleanFractionAndOversizedDurationCannotBecomeNativeIntegers() throws {
        let invalid: [Any] = [true, 300.5, -1, 601, "600", NSNull()]
        for seconds in invalid {
            let value = try message(["command":"pcm_begin","id":"pcm_1","seconds":seconds])
            XCTAssertThrowsError(try value.integer("seconds",300...600))
        }
        XCTAssertEqual(try message(["command":"session","enabled":false]).command,"session")
    }
    func testCaptureOwnershipCommandsAreClosedAndDoNotGrantMediaOrAcceptOtherDocuments() throws {
        let owner = UUID().uuidString.lowercased()
        func raw(_ value: [String:Any]) throws -> String {
            String(data:try JSONSerialization.data(withJSONObject:value),encoding:.utf8)!
        }
        let fields: [String:Any] = ["command":"capture_begin","document_id":document,"owner":owner]
        XCTAssertEqual(try CaptureBridgeMessage(raw(fields)).owner.uuidString.lowercased(),owner)
        var end = fields; end["command"] = "capture_end"
        XCTAssertEqual(try CaptureBridgeMessage(raw(end)).command,"capture_end")
        for (key,value) in [("command","grant"),("owner","other"),("document_id","invalid")] {
            var changed = fields; changed[key] = value
            XCTAssertThrowsError(try CaptureBridgeMessage(raw(changed)))
        }
        var extra = fields; extra["seconds"] = 601
        XCTAssertThrowsError(try CaptureBridgeMessage(raw(extra)))
        XCTAssertThrowsError(try CaptureBridgeMessage(String(repeating:" ",count:513)))
    }

}

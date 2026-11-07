#if QUALIFICATION
import Foundation
import Security
import WebKit

/// Only the disposable qualification bundle compiles this file. Normal apps
/// retain WebKit's system trust handling and never load fixture anchors.
@MainActor enum QualificationTLS {
    static var evidence: [String:Bool] = [:]
    static func anchor(_ name: String) -> SecCertificate? {
        let arguments = ProcessInfo.processInfo.arguments, flag = "--\(name)"
        let indices = arguments.indices.filter { arguments[$0] == flag }
        guard indices.count == 1, let index = indices.first, index+1 < arguments.count,
              arguments[index+1].utf8.count <= 21848,
              let data = Data(base64Encoded:arguments[index+1]), (1...16384).contains(data.count) else { return nil }
        return SecCertificateCreateWithData(nil,data as CFData)
    }
    static func evaluate(_ trust: SecTrust, host: String, anchor: SecCertificate) -> Bool {
        guard SecTrustSetPolicies(trust,SecPolicyCreateSSL(true,host as CFString)) == errSecSuccess,
              SecTrustSetAnchorCertificates(trust,[anchor] as CFArray) == errSecSuccess,
              SecTrustSetAnchorCertificatesOnly(trust,true) == errSecSuccess,
              SecTrustSetNetworkFetchAllowed(trust,false) == errSecSuccess else { return false }
        return SecTrustEvaluateWithError(trust,nil)
    }
    static func copy(_ trust: SecTrust) -> SecTrust? {
        guard let chain = SecTrustCopyCertificateChain(trust) else { return nil }
        var result: SecTrust?
        guard SecTrustCreateWithCertificates(chain,SecPolicyCreateBasicX509(),&result) == errSecSuccess else { return nil }
        return result
    }
}

extension InteractionWebViewController {
    public func webView(_ webView: WKWebView, didReceive challenge: URLAuthenticationChallenge,
                        completionHandler: @escaping (URLSession.AuthChallengeDisposition,URLCredential?) -> Void) {
        // This override exists only in the separately identified test bundle.
        // Real homes/SSO keep ordinary handling even in that test build.
        guard challenge.protectionSpace.host == "localhost",
              challenge.protectionSpace.port == 443,
              challenge.protectionSpace.authenticationMethod == NSURLAuthenticationMethodServerTrust else {
            completionHandler(.performDefaultHandling,nil); return
        }
        guard challenge.previousFailureCount == 0,
              let trust = challenge.protectionSpace.serverTrust,
              let root = QualificationTLS.anchor("qualification-ca"),
              let alien = QualificationTLS.anchor("qualification-alien-ca"),
              let wrongHost = QualificationTLS.copy(trust), let wrongRoot = QualificationTLS.copy(trust),
              !QualificationTLS.evaluate(wrongHost,host:"not-localhost.invalid",anchor:root),
              !QualificationTLS.evaluate(wrongRoot,host:"localhost",anchor:alien),
              QualificationTLS.evaluate(trust,host:"localhost",anchor:root) else {
            completionHandler(.cancelAuthenticationChallenge,nil); return
        }
        QualificationTLS.evidence = ["valid_chain":true,"wrong_host_refused":true,"wrong_anchor_refused":true,
                                     "system_trust_modified":false]
        completionHandler(.useCredential,URLCredential(trust:trust))
    }
}
#endif

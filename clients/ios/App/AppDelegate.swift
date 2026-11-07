import CryptoKit
import Foundation
import UIKit
import WebKit

#if QUALIFICATION
@MainActor private func qualificationStage(_ stage: String) {
    NSLog("DaimonQualification %@", stage)
    // Fixture-only bounded, nonsecret startup observations, readable even when
    // WebKit has not reached the loopback HTTPS server. Never present in pilot.
    let url = URL(fileURLWithPath:NSHomeDirectory()).appendingPathComponent("Documents/qualification-stage.txt")
    try? Data(stage.prefix(512).utf8).write(to:url,options:.atomic)
}
#endif

@main @MainActor final class AppDelegate: UIResponder, UIApplicationDelegate {
    let audioSession = IOSSpokenAudioSession()
    func application(_ application: UIApplication, configurationForConnecting session: UISceneSession,
                     options: UIScene.ConnectionOptions) -> UISceneConfiguration {
        let config = UISceneConfiguration(name: "Conversation", sessionRole: session.role)
        config.delegateClass = SceneDelegate.self; return config
    }
}

@MainActor final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?
    func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options: UIScene.ConnectionOptions) {
        guard let scene = scene as? UIWindowScene else { return }
        let window = UIWindow(windowScene: scene)
        window.rootViewController = UINavigationController(rootViewController: HomeController())
        self.window = window; window.makeKeyAndVisible()
        #if QUALIFICATION
        qualificationStage("scene_connected")
        #endif
    }
    func sceneDidBecomeActive(_ scene: UIScene) {
        (UIApplication.shared.delegate as? AppDelegate)?.audioSession.enterForeground()
    }
    func sceneDidEnterBackground(_ scene: UIScene) {
        (UIApplication.shared.delegate as? AppDelegate)?.audioSession.leaveForeground()
    }
}

@MainActor final class HomeController: UIViewController {
    private let address = UITextField()
    private let error = UILabel()
    override func viewDidLoad() {
        super.viewDidLoad(); title = "Daimon"; view.backgroundColor = .systemBackground
        let description = UILabel(); description.numberOfLines = 0
        description.font = .preferredFont(forTextStyle: .body); description.adjustsFontForContentSizeCategory = true
        description.text = "Enter your daimon’s HTTPS home address. Sign in with the existing shared login."
        address.placeholder = "https://your-daimon.example"; address.keyboardType = .URL
        address.autocapitalizationType = .none; address.autocorrectionType = .no; address.borderStyle = .roundedRect
        address.text = UserDefaults.standard.string(forKey: "home")
        address.accessibilityLabel = "Daimon home address"
        let open = UIButton(type: .system); open.setTitle("Open my daimon", for: .normal)
        open.addTarget(self, action: #selector(openHome), for: .touchUpInside)
        error.numberOfLines = 0; error.textColor = .systemRed
        let stack = UIStackView(arrangedSubviews: [description,address,open,error]); stack.axis = .vertical; stack.spacing = 20
        stack.translatesAutoresizingMaskIntoConstraints = false; view.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo:view.safeAreaLayoutGuide.leadingAnchor,constant:24),
            stack.trailingAnchor.constraint(equalTo:view.safeAreaLayoutGuide.trailingAnchor,constant:-24),
            stack.topAnchor.constraint(equalTo:view.safeAreaLayoutGuide.topAnchor,constant:32),
            address.heightAnchor.constraint(greaterThanOrEqualToConstant:44),open.heightAnchor.constraint(greaterThanOrEqualToConstant:44)
        ])
    }
    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        #if QUALIFICATION
        let args = ProcessInfo.processInfo.arguments
        if let index = args.firstIndex(of: "--qualification-home"), index + 1 < args.count {
            qualificationStage("opening_fixture_home")
            address.text = args[index + 1]; openHome()
        }
        #endif
    }
    @objc private func openHome() {
        do {
            let home = try InteractionHome(address.text ?? "")
            guard let url = Bundle.main.url(forResource:"native-audio",withExtension:"js"),
                  let expected = Bundle.main.object(forInfoDictionaryKey:"NativeAdapterSHA256") as? String,
                  let session = (UIApplication.shared.delegate as? AppDelegate)?.audioSession else { throw InteractionHome.Refusal.invalidHome }
            let bytes = try Data(contentsOf:url)
            guard PCMPlaybackQueue.digest(bytes) == expected, let source = String(data:bytes,encoding:.utf8) else { throw InteractionHome.Refusal.invalidHome }
            let controller = try InteractionWebViewController(home:home,session:session,adapterSource:source)
            controller.title = "Conversation"
            navigationController?.pushViewController(controller,animated:false)
            UserDefaults.standard.set(home.origin.absoluteString,forKey:"home"); error.text = nil
            #if QUALIFICATION
            qualificationStage("controller_created")
            QualificationStart.run(controller)
            #endif
        } catch {
            self.error.text = "Use an HTTPS home address without credentials, query or fragment."
            #if QUALIFICATION
            qualificationStage("home_open_refused: \(error)")
            #endif
        }
    }
}

#if QUALIFICATION
/// Test-only start after observed foreground + actual page readiness. It never
/// enters the normal application build or simulates a human voice original.
@MainActor private enum QualificationStart {
    static func run(_ controller: UIViewController, expires: TimeInterval = ProcessInfo.processInfo.systemUptime+40) {
        guard ProcessInfo.processInfo.systemUptime < expires else { qualificationStage("page_readiness_expired"); return }
        guard UIApplication.shared.applicationState == .active, let web = controller.view as? WKWebView else {
            DispatchQueue.main.asyncAfter(deadline:.now()+0.1) { run(controller,expires:expires) }; return
        }
        web.evaluateJavaScript("typeof window.runQualification==='function'") { ready,error in
            guard ProcessInfo.processInfo.systemUptime < expires else { qualificationStage("page_readiness_expired"); return }
            if (ready as? Bool) == true {
                qualificationStage("fixture_start_submitted")
                guard let session = (UIApplication.shared.delegate as? AppDelegate)?.audioSession else { return }
                NativeBeaconProbe.run(session) { observed in
                    var result = observed; result["tls_fixture"] = QualificationTLS.evidence
                    let data = try! JSONSerialization.data(withJSONObject:result)
                    let text = String(data:data,encoding:.utf8)!
                    web.evaluateJavaScript("window.__nativeBeaconQualification=\(text);window.runQualification()")
                }
            }
            else { DispatchQueue.main.asyncAfter(deadline:.now()+0.1) { run(controller,expires:expires) } }
        }
    }
}
/// Actual native device/session qualification only. No WebKit microphone grant,
/// provider, human original, operational observer or production activation.
@MainActor private final class NativeBeaconProbe {
    private static var active: NativeBeaconProbe?
    private let session: IOSSpokenAudioSession
    private let completion: ([String:Any]) -> Void
    private var output: BeaconPlaybackDriver?
    private var timeout: DispatchWorkItem?
    private var phase = 0, played = 0
    private var phases: [String] = []
    private var finished = false
    private init(_ session: IOSSpokenAudioSession, completion: @escaping ([String:Any]) -> Void) {
        self.session = session; self.completion = completion
    }
    static func run(_ session: IOSSpokenAudioSession, completion: @escaping ([String:Any]) -> Void) {
        guard active == nil else { return }
        let probe = NativeBeaconProbe(session,completion:completion); active = probe
        probe.start()
    }
    private func frame() throws -> BeaconPlaybackDriver.Frame {
        try .init(frequencies:[40.4,80.8,121.2,161.6,282.8,444.4],pulses:Array(repeating:0,count:6),
                  weights:[1,0,0,0,0,0],depth:0,level:0.2,transitionMilliseconds:20,
                  expiresAtUptime:ProcessInfo.processInfo.systemUptime+0.3)
    }
    private func start() {
        let deadline = DispatchWorkItem { [weak self] in
            Task { @MainActor in self?.finish(false,reason:"native_probe_timeout") }
        }
        timeout = deadline; DispatchQueue.main.asyncAfter(deadline:.now()+12,execute:deadline)
        do {
            output = try session.makeBeacon { [weak self] event in self?.receive(event) }
            try output?.frame(frame())
        } catch { finish(false,reason:"native_start_failed") }
    }
    private func receive(_ event: BeaconPlaybackDriver.Event) {
        guard !finished else { return }
        do {
            switch event {
            case .ready: break
            case .playing(let count,_):
                played = count
                if phase == 0 { phases.append("playing"); phase = 1 }
                else if phase == 2 {
                    phases.append("fresh_playing"); phase = 3; try output?.level(0,milliseconds:80)
                } else if phase == 4 {
                    phases.append("restored_playing"); phase = 5; session.leaveForeground()
                }
            case .quiet:
                if phase == 1 {
                    phases.append("stale_quiet"); phase = 2; try output?.frame(frame())
                } else if phase == 3 {
                    phases.append("duck_quiet"); phase = 4; try output?.frame(frame())
                }
            case .closed:
                if phase == 5 { phases.append("foreground_closed"); finish(true,reason:"qualified_native_output") }
                else { finish(false,reason:"unexpected_native_close") }
            case .unavailable: finish(false,reason:"native_output_unavailable")
            }
        } catch { finish(false,reason:"native_control_failed") }
    }
    private func finish(_ passed: Bool, reason: String) {
        guard !finished else { return }; finished = true
        timeout?.cancel(); timeout = nil; output?.close(); output = nil; session.enterForeground()
        let result: [String:Any] = ["state":passed ? "passed" : "failed","reason":reason,
                                   "phase":phase,"phases":phases,"played_frames":played,"microphone_requested":false]
        Self.active = nil
        // Allow an in-progress host cancellation to finish before the WebView
        // reserves another output; never restart this failed owner or probe.
        DispatchQueue.main.async { self.completion(result) }
    }
}

#endif

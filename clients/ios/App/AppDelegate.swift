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
            guard let captureURL = Bundle.main.url(forResource:"native-capture",withExtension:"js"),
                  let captureHash = Bundle.main.object(forInfoDictionaryKey:"NativeCaptureSHA256") as? String else { throw InteractionHome.Refusal.invalidHome }
            let captureBytes = try Data(contentsOf:captureURL)
            guard PCMPlaybackQueue.digest(captureBytes) == captureHash,
                  let captureSource = String(data:captureBytes,encoding:.utf8) else { throw InteractionHome.Refusal.invalidHome }
            let bytes = try Data(contentsOf:url)
            guard PCMPlaybackQueue.digest(bytes) == expected, let source = String(data:bytes,encoding:.utf8) else { throw InteractionHome.Refusal.invalidHome }
            let controller = try InteractionWebViewController(home:home,session:session,adapterSource:source,captureSource:captureSource)
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
    static func run(_ controller: UIViewController, attempt: Int = 0) {
        guard attempt < 200 else { qualificationStage("page_readiness_expired"); return }
        guard UIApplication.shared.applicationState == .active, let web = controller.view as? WKWebView else {
            DispatchQueue.main.asyncAfter(deadline:.now()+0.1) { run(controller,attempt:attempt+1) }; return
        }
        web.evaluateJavaScript("typeof window.runQualification==='function'") { ready,error in
            if (ready as? Bool) == true {
                qualificationStage("fixture_start_submitted")
                web.evaluateJavaScript("window.runQualification()")
            }
            else { DispatchQueue.main.asyncAfter(deadline:.now()+0.1) { run(controller,attempt:attempt+1) } }
        }
    }
}
#endif

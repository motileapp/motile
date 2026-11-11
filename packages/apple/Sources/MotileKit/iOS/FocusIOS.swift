#if os(iOS)
import GameController
import UIKit
import UIKit.UIGestureRecognizerSubclass

/// Whether the user is in the iOS app: one of its scenes is in front and active, with nothing of
/// the system's, like Control Center or the app switcher, over it. And when they last touched it,
/// typed in it, or pressed a key of a keyboard, a shortcut too.
final class FocusWatch {
    private(set) var lastInput: Date?
    private var changed: () -> Void = {}

    var focused: Bool {
        UIApplication.shared.connectedScenes.contains { $0.activationState == .foregroundActive }
    }

    func start(changed: @escaping () -> Void) {
        self.changed = changed
        let scenes = [
            UIScene.didActivateNotification, UIScene.willDeactivateNotification, UIScene.didEnterBackgroundNotification,
            UIScene.willEnterForegroundNotification, UIScene.didDisconnectNotification,
        ]
        for name in scenes {
            NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                self?.watchWindows()
                self?.changed()
            }
        }
        NotificationCenter.default.addObserver(forName: UIWindow.didBecomeKeyNotification, object: nil, queue: .main) { [weak self] _ in
            self?.watchWindows()
        }
        for name in [UITextView.textDidChangeNotification, UITextField.textDidChangeNotification] {
            NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in self?.lastInput = Date() }
        }
        NotificationCenter.default.addObserver(forName: .GCKeyboardDidConnect, object: nil, queue: .main) { [weak self] note in
            self?.watch(note.object as? GCKeyboard)
        }
        watch(GCKeyboard.coalesced)
        watchWindows()
    }

    /// Every key of a keyboard with keys, whatever takes it after: text, a shortcut, a menu's command.
    private func watch(_ keyboard: GCKeyboard?) {
        keyboard?.keyboardInput?.keyChangedHandler = { [weak self] _, _, _, pressed in
            guard pressed else { return }
            DispatchQueue.main.async { self?.lastInput = Date() }
        }
    }

    /// Puts a watcher of touches and keys on every key window that has none.
    private func watchWindows() {
        for case let scene as UIWindowScene in UIApplication.shared.connectedScenes {
            guard let window = scene.keyWindow, !(window.gestureRecognizers ?? []).contains(where: { $0 is InputWatcher }) else { continue }
            window.addGestureRecognizer(InputWatcher { [weak self] in self?.lastInput = Date() })
        }
    }
}

/// Sees every touch in its window and never recognizes, so that what the window's views do with
/// them is left as it is.
private final class InputWatcher: UIGestureRecognizer, UIGestureRecognizerDelegate {
    private let used: () -> Void

    init(used: @escaping () -> Void) {
        self.used = used
        super.init(target: nil, action: nil)
        cancelsTouchesInView = false
        delaysTouchesBegan = false
        delaysTouchesEnded = false
        requiresExclusiveTouchType = false
        delegate = self
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        used()
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
        used()
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        used()
        state = .failed
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
        state = .failed
    }

    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
        true
    }
}
#endif

#if os(iOS)
import SwiftUI
import UIKit

/// Drives the client without a finger, for looking at it in the simulator: `MOTILE_DEMO_SCRIPT` names
/// a file of steps, one a line, that is read again whenever it changes.
///
///     sidebar open | sidebar close      shows or hides the sidebar
///     thread <title>                    opens the thread with that title
///     new                               starts a new thread
///     type <text>                       writes in the composer
///     send                              sends what is written
///     panel diff|files|agents|close     opens a tab of the panel, or closes the panel
///     file <path>                       opens a file in the panel
///     change <path>                     opens what the latest turn changed in a file
///     sheet commands|projects|settings|server|thread|close
///     appearance light|dark|system
///     answer <allow|refuse>             answers the first approval
///     view                              opens the thread's images in the viewer
///     branches                          opens the branch picker in the thread's settings
///     top                               scrolls the transcript to its start
///     focus | blur                      gives the composer the keyboard, or takes it away
///     access <supervised|…>             sets how much the agent may do without asking
enum DemoDriver {
    private static var watcher: Timer?
    private static var done = 0

    static func startIfRequested(store: AppStore, drawer: Drawer) {
        let environment = ProcessInfo.processInfo.environment
        if let email = environment["MOTILE_DEMO_SIGN_IN"] {
            signIn(email, store: store)
        }
        guard let script = environment["MOTILE_DEMO_SCRIPT"] else { return }
        watcher = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { _ in
            guard let text = try? String(contentsOfFile: script, encoding: .utf8) else { return }
            let steps = text.split(separator: "\n").map(String.init)
            guard steps.count > done else { return }
            for step in steps[done...] { run(step, store: store, drawer: drawer) }
            done = steps.count
        }
    }

    private static func first<Found: UIView>(_ kind: Found.Type, in view: UIView?) -> Found? {
        guard let view else { return nil }
        if let found = view as? Found { return found }
        return view.subviews.lazy.compactMap { first(kind, in: $0) }.first
    }

    /// Signs in on an auth server with the dev login, once the core has said who it is.
    private static func signIn(_ email: String, store: AppStore) {
        guard store.ready else {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { signIn(email, store: store) }
            return
        }
        guard !store.account.signedIn else { return }
        store.devSignIn(email: email)
    }

    private static func run(_ step: String, store: AppStore, drawer: Drawer) {
        let parts = step.split(separator: " ", maxSplits: 1).map(String.init)
        guard let verb = parts.first else { return }
        let rest = parts.count > 1 ? parts[1] : ""
        switch verb {
        case "sidebar": drawer.isOpen = rest == "open"
        case "thread":
            guard let thread = store.threads.values.first(where: { $0.title.localizedCaseInsensitiveContains(rest) }) else { return }
            store.select(.thread(thread.id))
        case "new": store.startNewThread(in: store.composerProject ?? store.projects.first)
        case "type": store.draft = rest
        case "send": store.send()
        case "stop": store.stop()
        case "panel":
            switch rest {
            case "diff": store.sidePanel.showDiff()
            case "files": store.sidePanel.open(.files)
            case "agents": store.sidePanel.open(.agents)
            case "max": store.sidePanel.toggleMaximized()
            case "open": store.sidePanel.isOpen = true
            default: store.sidePanel.isOpen = false
            }
        case "file": store.sidePanel.open(.file(rest))
        case "change":
            guard let turn = store.sidePanel.turns.last else { return }
            store.sidePanel.showChange(turn: turn.id, path: rest)
        case "sheet":
            switch rest {
            case "commands": store.openPanel(.commands)
            case "projects": store.openPanel(.projects)
            case "threads": store.openPanel(.threads)
            case "add": store.addProject()
            case "settings": store.showsSettings = true
            case "server": store.showsAddServer = true
            case "thread": store.showsThreadSettings = true
            case "commit":
                guard let project = store.gitProject, let item = project.gitControl?.menu.first(where: { $0.action == "commit" }) else { return }
                store.chooseGit(item, in: project)
            default:
                store.closePanel()
                store.showsSettings = false
                store.showsAddServer = false
                store.showsThreadSettings = false
                store.committingProject = nil
            }
        case "appearance":
            UserDefaults.standard.set(rest, forKey: "appearance")
            (Appearance(rawValue: rest) ?? .system).apply()
        case "answer":
            guard let approval = store.activity.approvals.first else { return }
            store.answer(approval, allow: rest == "allow")
        case "done": store.toggleDone()
        case "drop": store.dropTargeted = rest == "on"
        case "top":
            let window = UIApplication.shared.connectedScenes.compactMap { ($0 as? UIWindowScene)?.keyWindow }.first
            (first(TranscriptScroller.self, in: window)?.subviews.first as? UIScrollView)?.setContentOffset(.zero, animated: false)
        case "flash":
            let window = UIApplication.shared.connectedScenes.compactMap { ($0 as? UIWindowScene)?.keyWindow }.first
            let scroll = first(TranscriptScroller.self, in: window)?.subviews.first as? UIScrollView
            if rest == "end", let scroll { scroll.setContentOffset(CGPoint(x: 0, y: scroll.contentSize.height - scroll.bounds.height), animated: false) }
            scroll?.flashScrollIndicators()
            print("FLASH", scroll?.verticalScrollIndicatorInsets as Any, scroll?.safeAreaInsets as Any, scroll?.automaticallyAdjustsScrollIndicatorInsets as Any)
        case "focus":
            let window = UIApplication.shared.connectedScenes.compactMap { ($0 as? UIWindowScene)?.keyWindow }.first
            first(ComposerUITextView.self, in: window)?.becomeFirstResponder()
        case "search":
            let window = UIApplication.shared.connectedScenes.compactMap { ($0 as? UIWindowScene)?.keyWindow }.first
            first(UITextField.self, in: window)?.becomeFirstResponder()
        case "blur": Platform.endEditing()
        case "access": store.setAccess(Access(rawValue: rest) ?? .full)
        case "view":
            let shown = store.transcript.rows.compactMap { row -> ViewedMedia? in
                guard case .media(let media) = row.kind else { return nil }
                return ViewedMedia(name: media.name, video: media.video, source: .media(media.id))
            }
            store.view(shown, at: 0)
        case "branches":
            guard let project = store.composerProject else { return }
            store.showsThreadSettings = true
            store.showBranches(of: project)
        default: break
        }
    }
}
#endif

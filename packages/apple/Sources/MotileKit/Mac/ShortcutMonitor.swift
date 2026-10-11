#if os(macOS)
import AppKit
import SwiftUI

/// Runs the keyboard shortcuts of the window, before its menus and the view with the keys see
/// them, and shows the threads' keys in the sidebar while the keys that open them are held.
final class ShortcutMonitor {
    static let shared = ShortcutMonitor()

    /// A key is being recorded for a shortcut, so every key is that.
    var recording = false
    private weak var store: AppStore?
    private var monitors: [Any] = []
    private var hint: DispatchWorkItem?
    /// How long the threads' keys wait before they show, so a quick ⌘C doesn't flash them.
    private static let hintDelay = 0.2
    /// The commands that run again while their keys are held.
    private static let repeating: Set<String> = ["thread.previous", "thread.next", "rightPanel.nextTab", "rightPanel.previousTab"]

    func start(_ store: AppStore) {
        guard monitors.isEmpty else { return }
        self.store = store
        let keys = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            self?.handle(event) == true ? nil : event
        }
        let flags = NSEvent.addLocalMonitorForEvents(matching: .flagsChanged) { [weak self] event in
            self?.held(event.modifierFlags)
            return event
        }
        monitors = [keys, flags].compactMap { $0 }
        NotificationCenter.default.addObserver(forName: NSApplication.didResignActiveNotification, object: nil, queue: .main) { [weak self] _ in
            self?.held([])
        }
    }

    private func handle(_ event: NSEvent) -> Bool {
        guard let store, !recording, store.viewing == nil, NSApp.modalWindow == nil else { return false }
        guard let window = event.window, !(window is NSPanel), window.attachedSheet == nil, window.sheetParent == nil else { return false }
        let responder = window.firstResponder
        let editing = (responder as? NSTextView)?.isEditable == true
        let composing = (responder as? ComposerNSTextView)?.sends == true
        let flags = event.modifierFlags
        let command = store.shortcuts.command(
            for: KeyNames.names(of: event),
            command: flags.contains(.command), control: flags.contains(.control), option: flags.contains(.option), shift: flags.contains(.shift),
            in: store.shortcutContext(composerFocus: composing, editableFocus: editing)
        )
        guard let command else { return false }
        // The command panel takes its own keys, the digits of its rows among them.
        guard store.panel == nil || command == "commandPalette.toggle" || command == "threadPicker.toggle" else { return false }
        if event.isARepeat, !Self.repeating.contains(command) { return true }
        return store.perform(command)
    }

    private func held(_ flags: NSEvent.ModifierFlags) {
        hint?.cancel()
        hint = nil
        guard let store else { return }
        let down = flags.intersection([.command, .control, .option, .shift])
        guard !down.isEmpty, store.panel == nil, store.settings == nil, let keys = store.shortcuts.effective("thread.jump.1").first?.keys,
            down == keys.modifierFlags
        else {
            if store.showsJumpHints { store.showsJumpHints = false }
            return
        }
        let show = DispatchWorkItem { [weak store] in store?.showsJumpHints = true }
        hint = show
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.hintDelay, execute: show)
    }
}

extension ShortcutKeys {
    var modifierFlags: NSEvent.ModifierFlags {
        var flags: NSEvent.ModifierFlags = []
        if command { flags.insert(.command) }
        if control { flags.insert(.control) }
        if option { flags.insert(.option) }
        if shift { flags.insert(.shift) }
        return flags
    }
}

/// The names a key press goes by in the rules. A letter is the one the keyboard's layout types,
/// and a digit or a sign the key where it is on a US keyboard, so ⌘[ is the same key in every
/// layout and with Shift.
enum KeyNames {
    private static let named: [UInt16: String] = [
        36: "enter", 76: "enter", 48: "tab", 49: "space", 51: "backspace", 117: "delete", 53: "escape",
        123: "arrowleft", 124: "arrowright", 125: "arrowdown", 126: "arrowup",
        115: "home", 119: "end", 116: "pageup", 121: "pagedown",
        122: "f1", 120: "f2", 99: "f3", 118: "f4", 96: "f5", 97: "f6", 98: "f7", 100: "f8", 101: "f9", 109: "f10", 103: "f11", 111: "f12",
    ]
    private static let signs: [UInt16: String] = [
        29: "0", 18: "1", 19: "2", 20: "3", 21: "4", 23: "5", 22: "6", 26: "7", 28: "8", 25: "9",
        27: "-", 24: "=", 33: "[", 30: "]", 42: "\\", 41: ";", 39: "'", 43: ",", 47: ".", 44: "/", 50: "`",
    ]
    private static let letters: [UInt16: String] = [
        0: "a", 11: "b", 8: "c", 2: "d", 14: "e", 3: "f", 5: "g", 4: "h", 34: "i", 38: "j", 40: "k", 37: "l", 46: "m",
        45: "n", 31: "o", 35: "p", 12: "q", 15: "r", 1: "s", 17: "t", 32: "u", 9: "v", 13: "w", 7: "x", 16: "y", 6: "z",
    ]

    /// The one name the key is recorded by.
    static func name(of event: NSEvent) -> String? {
        if let name = named[event.keyCode] { return name }
        let typed = layoutKey(event)
        if let typed, typed.count == 1, typed.first?.isASCII == true, typed.first?.isLetter == true { return typed }
        return signs[event.keyCode] ?? letters[event.keyCode] ?? typed
    }

    /// Every name the key matches: its own, and the letter of its place when the layout types no letter there.
    static func names(of event: NSEvent) -> Set<String> {
        var names: Set<String> = []
        if let name = name(of: event) { names.insert(name) }
        if let typed = layoutKey(event) { names.insert(typed) }
        if let letter = letters[event.keyCode], !names.contains(where: { $0.count == 1 && $0.first?.isASCII == true && $0.first?.isLetter == true }) {
            names.insert(letter)
        }
        return names
    }

    private static func layoutKey(_ event: NSEvent) -> String? {
        guard let typed = event.charactersIgnoringModifiers?.lowercased(), !typed.isEmpty else { return nil }
        return typed
    }

    /// The key as the rules write it, as in `mod+shift+k`, from a press while one is recorded.
    static func rule(of event: NSEvent) -> String? {
        guard let name = name(of: event) else { return nil }
        let flags = event.modifierFlags
        let held = [(flags.contains(.command), "mod"), (flags.contains(.control), "ctrl"), (flags.contains(.option), "alt"), (flags.contains(.shift), "shift")]
        return (held.filter(\.0).map(\.1) + [name]).joined(separator: "+")
    }
}

extension AppStore {
    /// The commands whose doing is the Mac's own: the sidebar, the window, and the composer's menus.
    func performOnThisSystem(_ command: String) -> Bool {
        switch command {
        case "sidebar.toggle":
            let hidden = UserDefaults.standard.bool(forKey: MainView.sidebarHiddenKey)
            UserDefaults.standard.set(!hidden, forKey: MainView.sidebarHiddenKey)
            return true
        case "rightPanel.close":
            if settings != nil || showsUsage {
                closeRoute()
            } else if !sidePanel.closeActive() {
                NSApp.keyWindow?.performClose(nil)
            }
            return true
        case "modelPicker.toggle", "composer.effort", "composer.access", "composer.workspace":
            return ShortcutTargets.press(command)
        default:
            return false
        }
    }
}

/// The controls a shortcut presses as a click would: the composer's menus.
enum ShortcutTargets {
    fileprivate static var views: [String: [WeakTarget]] = [:]

    fileprivate struct WeakTarget {
        weak var view: TargetView?
    }

    /// Clicks the control of the command, if it is on screen.
    static func press(_ command: String) -> Bool {
        views[command] = views[command]?.filter { $0.view != nil }
        let shown = views[command]?.compactMap(\.view).first { view in
            !view.away && view.window?.isKeyWindow == true && !view.isHiddenOrHasHiddenAncestor && view.bounds.width > 0
        }
        guard let view = shown, let window = view.window else { return false }
        let center = view.convert(NSPoint(x: view.bounds.midX, y: view.bounds.midY), to: nil)
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            guard let click = NSEvent.mouseEvent(
                with: type, location: center, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: type == .leftMouseDown ? 1 : 0
            ) else { return false }
            NSApp.postEvent(click, atStart: false)
        }
        return true
    }
}

fileprivate final class TargetView: NSView {
    var command = ""
    var away = false

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard window != nil, ShortcutTargets.views[command]?.contains(where: { $0.view === self }) != true else { return }
        ShortcutTargets.views[command, default: []].append(ShortcutTargets.WeakTarget(view: self))
    }
}

private struct ShortcutTarget: NSViewRepresentable {
    let command: String

    func makeNSView(context: Context) -> TargetView {
        let view = TargetView()
        view.command = command
        return view
    }

    func updateNSView(_ view: TargetView, context: Context) {
        view.away = context.environment.putAway
    }
}

extension View {
    /// Makes the control the one the command's shortcut presses.
    func shortcutTarget(_ command: String) -> some View {
        background(ShortcutTarget(command: command))
    }
}
#endif

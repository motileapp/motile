#if os(macOS)
import AppKit
import SwiftUI

/// The keys of a shortcut, on a button that records new ones: pressed, it takes the next keys
/// pressed with their modifiers. Esc or a click elsewhere stops it without a change.
struct ShortcutField: View {
    private let caps: [String]
    private let placeholder: String
    private let size: ControlSize
    private let record: (String) -> Void
    @State private var recording = false
    @State private var monitors: [Any] = []

    init(_ caps: [String], placeholder: String = "Record Shortcut", size: ControlSize = .large, record: @escaping (String) -> Void) {
        self.caps = caps
        self.placeholder = placeholder
        self.size = size
        self.record = record
    }

    var body: some View {
        Button(action: start) {
            label
                .padding(.horizontal, recording || caps.isEmpty ? size.padding : (size.height - KeyCaps.height) / 2)
                .frame(minWidth: size.height, minHeight: size.height)
        }
        .buttonStyle(ControlButtonStyle(look: ControlLook(variant: .outline, size: size, selected: recording)))
        .help(recording ? "Press the keys, or Esc to stop" : "Record new keys")
        .onDisappear(perform: stop)
    }

    @ViewBuilder private var label: some View {
        if recording {
            Text("Press keys")
                .font(size.font)
                .foregroundStyle(Color.themePrimary)
        } else if caps.isEmpty {
            Text(placeholder)
                .font(size.font)
                .foregroundStyle(Color.themeMutedForeground)
        } else {
            KeyCaps(caps)
                .foregroundStyle(Color.themeForeground)
        }
    }

    private func start() {
        guard !recording else { return }
        recording = true
        ShortcutMonitor.shared.recording = true
        let keys = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            let escape = event.keyCode == 53 && event.modifierFlags.intersection([.command, .control, .option, .shift]).isEmpty
            let key = escape ? nil : KeyNames.rule(of: event)
            stop()
            if let key { record(key) }
            return nil
        }
        // After the click, so that a click on the field itself records again.
        let clicks = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { event in
            stop()
            return event
        }
        monitors = [keys, clicks].compactMap { $0 }
    }

    private func stop() {
        monitors.forEach(NSEvent.removeMonitor)
        monitors = []
        guard recording else { return }
        recording = false
        ShortcutMonitor.shared.recording = false
    }
}
#endif

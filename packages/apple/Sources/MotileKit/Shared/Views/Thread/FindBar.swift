import SwiftUI

/// Finding in the open thread, as a browser finds in a page: a card in the thread's top corner,
/// over the toast, with the field, how many matches there are and which one is current, and the
/// buttons that go to the one before and the next and close it. Its last button's corner runs
/// along the card's.
struct FindBar: View {
    @Environment(AppStore.self) private var store
    @FocusState private var focused: Bool

    private static let radius = Radius.lg
    private static let margin = radius - ControlSize.regular.radius
    /// How far it stands from the top of the thread.
    static let gap: CGFloat = 6
    static let height = ControlSize.regular.height + 2 * margin
    /// What it covers of the top of the transcript, with the room under it.
    static let room = gap + height + 8

    var body: some View {
        let find = store.find
        HStack(spacing: 0) {
            InputField("Find in thread", text: Binding(get: { find.query }, set: { store.setFindQuery($0) }), icon: .search, variant: .bare, focus: $focused)
                .onSubmit { store.stepFind(back: Self.shiftHeld) }
            if !find.label.isEmpty {
                Text(find.label)
                    .font(.ui(size: ControlSize.regular.textSize))
                    .monospacedDigit()
                    .foregroundStyle(find.count == 0 ? Color.themeMutedStrongerForeground : Color.themeMutedForeground)
                    .padding(.horizontal, ControlSize.regular.gap)
                    .fixedSize()
            }
            Rectangle()
                .fill(Color.themeBorderPopover)
                .frame(width: 1, height: ControlSize.regular.height - 2 * Self.margin)
                .padding(.horizontal, ControlSize.regular.gap)
            ActionButton(icon: .chevronUp, help: store.shortcuts.help("Previous match", "find.previous", in: ["findOpen"])) {
                store.stepFind(back: true)
            }
            .disabled(find.count == 0)
            ActionButton(icon: .chevronDown, help: store.shortcuts.help("Next match", "find.next", in: ["findOpen"])) {
                store.stepFind(back: false)
            }
            .disabled(find.count == 0)
            ActionButton(icon: .x, help: "Close") { store.closeFind() }
        }
        .frame(height: ControlSize.regular.height)
        .padding(Self.margin)
        .frame(maxWidth: Platform.scale > 1 ? .infinity : 380)
        .background {
            RoundedRectangle(cornerRadius: Self.radius, style: .continuous).fill(Color.themePopover)
        }
        .overlay {
            RoundedRectangle(cornerRadius: Self.radius, style: .continuous).strokeBorder(Color.themeBorderPopover, lineWidth: 1)
        }
        .environment(\.surface, .popover)
        .onChange(of: find.focusRequests, initial: true) {
            FieldFocus.take { focused = true }
        }
    }

    private static var shiftHeld: Bool {
        #if os(macOS)
        NSApp.currentEvent?.modifierFlags.contains(.shift) == true
        #else
        false
        #endif
    }
}

/// Gives a field the keys with what it has selected, so that what is typed replaces it, as a
/// browser's find does. A field that has just come in takes the keys only once it is in the
/// window, so it is asked a moment later, and what it selects waits for that.
enum FieldFocus {
    static func take(_ focus: @escaping () -> Void) {
        DispatchQueue.main.async {
            focus()
            #if os(macOS)
            DispatchQueue.main.async { NSApp.sendAction(#selector(NSText.selectAll(_:)), to: nil, from: nil) }
            #endif
        }
    }
}

/// The find bar and the toast in the thread's top corner, the bar first, on one layer that casts
/// one shadow behind both, so that neither shades the other.
struct ThreadCorner: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        VStack(alignment: .trailing, spacing: FindBar.gap) {
            if store.find.shown, store.selectedThread != nil {
                FindBar()
                    .appearing(.opacity.combined(with: .offset(y: -6)))
            }
            if let toast = store.openToast {
                ToastView(toast: toast)
                    .appearing(.opacity.combined(with: .offset(y: -6)))
            }
        }
        .compositingGroup()
        .shadow(.lg, .shadowStronger)
        .padding(.top, FindBar.gap)
        .padding(.horizontal, 14)
        .animation(.easeOut(duration: 0.15), value: store.toast)
        .animation(.easeOut(duration: 0.15), value: store.find.shown)
    }
}

extension AppStore {
    /// The text selected in the transcript, on one line, to find.
    var selectedTranscriptText: String? {
        #if os(macOS)
        guard let view = NSApp.keyWindow?.firstResponder as? RowTextView else { return nil }
        let range = view.selectedRange()
        guard range.length > 0, range.length <= 200 else { return nil }
        let text = (view.string as NSString).substring(with: range)
        let line = text.split(whereSeparator: \.isNewline).first.map(String.init) ?? ""
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        return trimmed.isEmpty ? nil : trimmed
        #else
        return nil
        #endif
    }
}

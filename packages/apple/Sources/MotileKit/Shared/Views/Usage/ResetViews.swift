import SwiftUI

/// The limit resets an account holds, under its windows: how many and when the first expires, and the
/// menu that uses one. On iOS it is a row of the system's list that opens the menu.
struct ResetsRow: View {
    /// How far the menu stands from the row's top, bottom and trailing edge.
    private static let around: CGFloat = 12

    #if os(iOS)
    @Environment(AppStore.self) private var store
    #endif
    let resets: LimitResets
    /// After a reset was used.
    var used: (() -> Void)?

    var body: some View {
        #if os(iOS)
        Menu {
            ResetOptions(resets: resets, used: used)
        } label: {
            HStack {
                Label {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Use Reset")
                            .foregroundStyle(Color.themePrimary)
                        Text([resets.title, resets.detail].compactMap { $0 }.joined(separator: " · "))
                            .font(.footnote)
                            .foregroundStyle(Color.themeMutedForeground)
                    }
                } icon: {
                    Image(.ticket, size: .xl)
                        .foregroundStyle(Color.themePrimary)
                }
                Spacer()
                if store.usingReset(of: resets) { Spinner(size: .xl) }
            }
        }
        .disabled(store.usingReset(of: resets))
        #else
        HStack(spacing: 10) {
            Image(.ticket, size: .md)
                .foregroundStyle(Color.themeForeground)
            VStack(alignment: .leading, spacing: 2) {
                Text(resets.title)
                    .font(.ui(.sm, weight: .medium))
                    .foregroundStyle(Color.themeForeground)
                if let detail = resets.detail {
                    Text(detail)
                        .font(.ui(.xs))
                        .foregroundStyle(Color.themeMutedForeground)
                }
            }
            .lineLimit(1)
            Spacer(minLength: 8)
            ResetMenu(resets: resets, size: .large, used: used)
        }
        .padding(.leading, LimitRow.inset)
        .padding([.trailing, .vertical], Self.around)
        #endif
    }
}

/// The items of the system's menu that uses one of an account's resets.
struct ResetOptions: View {
    @Environment(AppStore.self) private var store
    let resets: LimitResets
    /// The thread its limit stopped.
    var thread: ThreadInfo?
    var used: (() -> Void)?

    var body: some View {
        ForEach(resets.options) { option in
            Button { store.useLimitReset(option, of: resets, done: used) } label: {
                Text(option.title)
                Text(option.shown(at: thread != nil))
            }
            .disabled(!option.usable(at: thread != nil))
        }
    }
}

extension LimitResets.Option {
    /// It can be used now, or where `atLimit`, in a thread its limit stopped.
    func usable(at limit: Bool) -> Bool {
        usable || (limit && atLimit)
    }

    /// What it refills, or why it can't be used now.
    func shown(at limit: Bool) -> String {
        usable(at: limit) ? detail : note ?? detail
    }
}

/// The menu that uses one of an account's limit resets: each kind with what it refills, how many are
/// left and when they expire. One that waits for a limit is offered at one, in a thread its limit
/// stopped. The Mac draws it as its other dropdowns; iOS shows the system's menu.
struct ResetMenu: View {
    @Environment(AppStore.self) private var store
    let resets: LimitResets
    var variant = ButtonVariant.outline
    var size = ControlSize.regular
    var margin = EdgeInsets()
    /// The thread its limit stopped.
    var thread: ThreadInfo?
    /// It opens above its button, as what the composer's buttons open does.
    var up = false
    var used: (() -> Void)?
    #if os(macOS)
    @State private var open = false
    #endif

    private static let title = "Use Reset"
    private static let help = "Refill this account's limits now with one of its resets"

    var body: some View {
        let pending = store.usingReset(of: resets)
        #if os(macOS)
        ActionButton(Self.title, help: Self.help, variant: variant, size: size, pending: pending, selected: open, opens: true, margin: margin) {
            open.toggle()
        }
        .disabled(pending)
        .background {
            Color.clear
                .padding(margin)
                .anchorPreference(key: OpenDropdown.self, value: .bounds) { anchor in
                    guard open else { return nil }
                    return OpenDropdown.Shown(anchor: anchor, content: AnyView(card.environment(store)), up: up, trailing: true) {
                        open = false
                    }
                }
        }
        #else
        ActionMenu(Self.title, help: Self.help, variant: variant, size: size, pending: pending, margin: margin) {
            ResetOptions(resets: resets, thread: thread, used: used)
        }
        .disabled(pending)
        #endif
    }

    #if os(macOS)
    private var card: some View {
        VStack(spacing: 0) {
            ForEach(resets.options) { option in
                VStack(alignment: .leading, spacing: 1) {
                    Text(option.title)
                        .font(DropdownList.font)
                        .foregroundStyle(Color.themeForeground)
                    Text(detail(of: option))
                        .font(.ui(.xs))
                        .foregroundStyle(Color.themeMutedForeground)
                }
                .lineLimit(1)
                .padding(.vertical, 6)
                .frame(maxWidth: .infinity, alignment: .leading)
                .dropdownRow()
                .opacity(.disabled, when: !usable(option))
                .button(.dropdownLight()) {
                    open = false
                    use(option)
                }
                .disabled(!usable(option))
            }
        }
        .padding(DropdownList.padding)
        .fixedSize()
        .dropdownCard()
    }

    private func use(_ option: LimitResets.Option) {
        store.useLimitReset(option, of: resets, done: used)
    }

    private func detail(of option: LimitResets.Option) -> String {
        option.shown(at: thread != nil)
    }

    private func usable(_ option: LimitResets.Option) -> Bool {
        option.usable(at: thread != nil)
    }
    #endif
}

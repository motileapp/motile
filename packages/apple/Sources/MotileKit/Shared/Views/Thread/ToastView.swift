import SwiftUI

/// What the last git action or project add did, or what was refused, at the top of the thread,
/// whose shadow `ThreadCorner` casts.
/// What follows is one click away: the push after a commit, the pull request after a push, the
/// thread in a project that was cloned.
struct ToastView: View {
    @Environment(AppStore.self) private var store
    let toast: Toast

    private static let radius = Radius.lg
    private static let padding: CGFloat = 14
    private static let closeSize = ControlSize.regular.height
    private static let titleHeight: CGFloat = scaled(16)
    /// Tucks the close button into the corner, its own corner running along the toast's.
    private static let closeMargin = radius - ControlSize.regular.radius
    private static let trailing = closeMargin + closeSize + 4

    var body: some View {
        let url = toast.url.flatMap { URL(string: $0) }
        HStack(alignment: .top, spacing: 10) {
            Group {
                if toast.pending {
                    Spinner(size: .md)
                } else {
                    Image(toast.failed ? .circleAlert : .circleCheck, size: .md)
                        .foregroundStyle(toast.failed ? Color.themeDestructive : Color.themeSuccess)
                }
            }
            .frame(height: Self.titleHeight)
            VStack(alignment: .leading, spacing: 4) {
                Text(toast.title)
                    .font(.ui(.sm, weight: .medium))
                    .foregroundStyle(Color.themeForeground)
                    .frame(minHeight: Self.titleHeight)
                if let description = toast.description {
                    if toast.failed {
                        output(description)
                    } else {
                        Text(description)
                            .font(.ui(.xs))
                            .foregroundStyle(Color.themeMutedForeground)
                            .lineLimit(2)
                            .textSelection(.enabled)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                let buttons = actions(url: url)
                if !buttons.isEmpty {
                    HStack(spacing: 0) {
                        ForEach(Array(buttons.enumerated()), id: \.offset) { index, button in
                            ActionButton(
                                button.title, variant: button.prominent ? .primary : .secondary,
                                margin: .sharing(Self.actionGap, at: index, of: buttons.count), action: button.run
                            )
                        }
                    }
                    .padding(.top, 6)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.vertical, Self.padding)
        .padding(.leading, Self.padding)
        .padding(.trailing, Self.trailing)
        .frame(maxWidth: Platform.scale > 1 ? 440 : 320)
        .background {
            RoundedRectangle(cornerRadius: Self.radius, style: .continuous).fill(Color.themePopover)
        }
        .overlay {
            RoundedRectangle(cornerRadius: Self.radius, style: .continuous).strokeBorder(Color.themeBorderPopover, lineWidth: 1)
        }
        .overlay(alignment: .topTrailing) {
            ActionButton(icon: .x, help: "Close", symbolSize: ControlSize.regular.smallSymbol) { store.dismissToast() }
                .padding(Self.closeMargin)
        }
        .environment(\.surface, .popover)
    }

    /// The room between the buttons' lights, which their presses share.
    private static let actionGap: CGFloat = 8

    private func actions(url: URL?) -> [(title: String, prominent: Bool, run: () -> Void)] {
        var actions: [(title: String, prominent: Bool, run: () -> Void)] = []
        if let url {
            actions.append(("View PR", toast.nextLabel == nil, { store.showPullRequest(url) }))
        }
        if let next = toast.nextLabel {
            actions.append((next, true, { store.runNextGit() }))
        }
        if toast.projectID != nil {
            actions.append(("Open project", true, { store.openToastProject() }))
        }
        return actions
    }

    private static let outputFont = PlatformFont.uiMono(.xs)
    private static let outputPadding: CGFloat = scaled(8)
    /// Eight lines of output and the room around them.
    private static let outputHeight = (outputFont.textLineHeight + 2) * 8 + outputPadding * 2

    /// What git printed, scrolled to its end, where a hook says what it found.
    private func output(_ text: String) -> some View {
        FadingScroll(maxHeight: Self.outputHeight, anchor: .bottom, padding: Self.outputPadding) {
            Text(text)
                .font(.ui(.xs, design: .monospaced))
                .foregroundStyle(Color.themeMutedForeground)
                .lineSpacing(2)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .box(in: RoundedRectangle(cornerRadius: Radius.md, style: .continuous), bordered: true)
        .padding(.top, 4)
        .padding(.trailing, Self.padding - Self.trailing)
    }
}

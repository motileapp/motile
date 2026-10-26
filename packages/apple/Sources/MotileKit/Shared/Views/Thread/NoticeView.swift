import SwiftUI

/// What the last git action or project add did, or what was refused, at the top of the thread.
/// What follows is one click away: the push after a commit, the pull request after a push, the
/// thread in a project that was cloned.
struct NoticeView: View {
    @Environment(AppStore.self) private var store
    let notice: Notice

    private static let radius = Radius.lg
    private static let padding: CGFloat = 14
    private static let closeSize = ControlSize.regular.height
    private static let titleHeight: CGFloat = scaled(16)
    /// Tucks the close button into the corner, its own corner running along the notice's.
    private static let closeMargin = radius - ControlSize.regular.radius
    private static let trailing = closeMargin + closeSize + 4

    var body: some View {
        let url = notice.url.flatMap { URL(string: $0) }
        HStack(alignment: .top, spacing: 10) {
            Group {
                if notice.pending {
                    Spinner(size: .md)
                } else {
                    Image(notice.failed ? .circleAlert : .circleCheck, size: .md)
                        .foregroundStyle(notice.failed ? Color.themeDestructive : Color.themeSuccess)
                }
            }
            .frame(height: Self.titleHeight)
            VStack(alignment: .leading, spacing: 4) {
                Text(notice.title)
                    .font(.ui(size: 13, weight: .medium))
                    .foregroundStyle(Color.themeForeground)
                    .frame(minHeight: Self.titleHeight)
                if let description = notice.description {
                    if notice.failed {
                        output(description)
                    } else {
                        Text(description)
                            .font(.ui(size: 12.5))
                            .foregroundStyle(Color.themeMutedForeground)
                            .lineLimit(2)
                            .textSelection(.enabled)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                if url != nil || notice.nextLabel != nil || notice.projectID != nil {
                    HStack(spacing: 8) {
                        if let url {
                            action("View PR", prominent: notice.nextLabel == nil) { store.showPullRequest(url) }
                        }
                        if let next = notice.nextLabel {
                            action(next, prominent: true) { store.runNextGit() }
                        }
                        if notice.projectID != nil {
                            action("Open project", prominent: true) { store.openNoticeProject() }
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
            RoundedRectangle(cornerRadius: Self.radius, style: .continuous)
                .fill(Color.themePopover)
                .shadow(.md)
        }
        .overlay {
            RoundedRectangle(cornerRadius: Self.radius, style: .continuous).strokeBorder(Color.themeBorderPopover, lineWidth: 1)
        }
        .overlay(alignment: .topTrailing) {
            ActionButton(icon: .x, help: "Close", symbolSize: ControlSize.regular.smallSymbol) { store.dismissNotice() }
                .padding(Self.closeMargin)
        }
        .environment(\.surface, .popover)
    }

    private func action(_ title: String, prominent: Bool, run: @escaping () -> Void) -> some View {
        ActionButton(title, variant: prominent ? .primary : .secondary, action: run)
    }

    private static let outputFont = PlatformFont.uiMono(11.5)
    private static let outputPadding: CGFloat = scaled(8)
    /// Eight lines of output and the room around them.
    private static let outputHeight = (outputFont.textLineHeight + 2) * 8 + outputPadding * 2

    /// What git printed, scrolled to its end, where a hook says what it found.
    private func output(_ text: String) -> some View {
        FadingScroll(maxHeight: Self.outputHeight, anchor: .bottom, padding: Self.outputPadding) {
            Text(text)
                .font(.ui(size: 11.5, design: .monospaced))
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

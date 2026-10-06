import SwiftUI

extension View {
    /// Out of sight and out of reach, but still there: it comes back as it was left.
    func putAway(_ away: Bool) -> some View {
        opacity(away ? 0 : 1)
            .allowsHitTesting(!away)
            .accessibilityHidden(away)
            .disabled(away)
            .transformEnvironment(\.putAway) { $0 = $0 || away }
    }
}

extension EnvironmentValues {
    /// Whether the view is put away. The AppKit views in it hide, so that they keep no cursor,
    /// hover or keys of their own under what covers them.
    @Entry var putAway = false
}

#if os(macOS)
extension NSView {
    /// Hides the view under a put-away one. What has the keys in it gives them to the window
    /// first, as hiding it would have AppKit look for the next view to take them while SwiftUI
    /// updates, which SwiftUI can't answer.
    func putAway(_ away: Bool) {
        if away, let responder = window?.firstResponder as? NSView, responder.isDescendant(of: self) {
            window?.makeFirstResponder(nil)
        }
        isHidden = away
    }
}
#endif

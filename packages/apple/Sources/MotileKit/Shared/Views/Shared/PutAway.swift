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

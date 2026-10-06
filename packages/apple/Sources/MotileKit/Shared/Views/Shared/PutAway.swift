import SwiftUI

extension View {
    /// Out of sight and out of reach, but still there: it comes back as it was left.
    func putAway(_ away: Bool) -> some View {
        opacity(away ? 0 : 1)
            .allowsHitTesting(!away)
            .accessibilityHidden(away)
            .disabled(away)
    }
}

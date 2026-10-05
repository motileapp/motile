import SwiftUI

extension View {
    /// Comes and goes as one piece: what changes inside on the way, like a button under a click,
    /// keeps its place in it.
    func appearing(_ transition: AnyTransition = .opacity) -> some View {
        geometryGroup().transition(transition)
    }
}

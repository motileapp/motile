import SwiftUI

/// The row the arrow keys are on. An object, since a field keeps the key handler of an earlier
/// render at times, which would read the row as it was then: this one it reads as it is.
@Observable
final class Highlight {
    private(set) var index = 0
    /// The row the list brings into sight: the one the keys moved to, never one under the pointer.
    private(set) var shown: Int?

    func key(_ index: Int) {
        self.index = index
        shown = index
    }

    func point(_ index: Int) {
        self.index = index
        shown = nil
    }
}

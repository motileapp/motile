import SwiftUI

/// The row the arrow keys are on. An object, since a field keeps the key handler of an earlier
/// render at times, which would read the row as it was then: this one it reads as it is.
@Observable
final class Highlight {
    var index = 0
}

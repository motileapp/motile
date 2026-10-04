import Foundation

enum Version {
    /// Whether `version` comes before `other`, comparing their numbers from the left.
    static func isOlder(_ version: String, than other: String?) -> Bool {
        guard let other, !version.isEmpty, !other.isEmpty else { return false }
        let numbers: (String) -> [Int] = { $0.split(separator: ".").map { Int($0) ?? 0 } }
        let (ours, theirs) = (numbers(version), numbers(other))
        for index in 0..<max(ours.count, theirs.count) {
            let left = index < ours.count ? ours[index] : 0
            let right = index < theirs.count ? theirs[index] : 0
            if left != right { return left < right }
        }
        return false
    }
}

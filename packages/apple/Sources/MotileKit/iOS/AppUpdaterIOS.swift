#if os(iOS)
import Foundation
import Observation

/// Finds out about new releases, which the servers are compared with. The app itself is
/// updated by TestFlight and the App Store.
@Observable
final class AppUpdater {
    private static let latestRelease = URL(string: "https://github.com/motileapp/motile/releases/latest")!
    private static let checkEvery: TimeInterval = 6 * 3600

    /// The newest release's version.
    private(set) var latest: String?
    let current = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? ""

    @ObservationIgnored private var timer: Timer?

    /// Looks for a new release now and every few hours.
    func start() {
        check()
        timer = Timer.scheduledTimer(withTimeInterval: Self.checkEvery, repeats: true) { [weak self] _ in self?.check() }
    }

    func check() {
        var request = URLRequest(url: Self.latestRelease)
        request.httpMethod = "HEAD"
        request.cachePolicy = .reloadIgnoringLocalCacheData
        URLSession.shared.dataTask(with: request) { [weak self] _, response, _ in
            // The address redirects to the release's own page, whose last part is its tag.
            let tag = response?.url?.lastPathComponent ?? ""
            guard tag.hasPrefix("v") else { return }
            DispatchQueue.main.async { self?.latest = String(tag.dropFirst()) }
        }.resume()
    }
}
#endif

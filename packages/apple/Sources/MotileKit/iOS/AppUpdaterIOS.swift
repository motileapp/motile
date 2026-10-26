#if os(iOS)
import Foundation
import Observation

/// Finds out about new releases, which the servers are compared with. The client itself is
/// updated by TestFlight and the App Store.
@Observable
final class AppUpdater {
    private static let latestRelease = URL(string: "https://github.com/motileapp/motile/releases/latest")!
    private static let checkEvery: TimeInterval = 5 * 60

    /// The newest release's version.
    private(set) var latest: String?
    let current = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? ""

    /// Called with each release found, and whether the user asked for it.
    @ObservationIgnored var onFound: ((_ asked: Bool) -> Void)?
    @ObservationIgnored private var timer: Timer?
    @ObservationIgnored private var lastCheck: Date?

    /// Looks for a new release now and every few minutes.
    func start() {
        check()
        timer = Timer.scheduledTimer(withTimeInterval: Self.checkEvery, repeats: true) { [weak self] _ in self?.check() }
    }

    /// Looks again if the timer missed its turn while the client was in the back.
    func checkIfDue() {
        guard let lastCheck, Date().timeIntervalSince(lastCheck) >= Self.checkEvery else { return }
        check()
    }

    func check() {
        lastCheck = Date()
        var request = URLRequest(url: Self.latestRelease)
        request.httpMethod = "HEAD"
        request.cachePolicy = .reloadIgnoringLocalCacheData
        URLSession.shared.dataTask(with: request) { [weak self] _, response, _ in
            // The address redirects to the release's own page, whose last part is its tag.
            let tag = response?.url?.lastPathComponent ?? ""
            guard tag.hasPrefix("v") else { return }
            DispatchQueue.main.async {
                self?.latest = String(tag.dropFirst())
                self?.onFound?(false)
            }
        }.resume()
    }
}
#endif

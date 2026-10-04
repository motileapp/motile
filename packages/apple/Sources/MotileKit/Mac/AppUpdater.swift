#if os(macOS)
import AppKit
import Foundation
import Observation
import Security

/// Finds out about new releases and replaces this app with one: it downloads the release's
/// app, checks that it is signed by whoever signed this one, puts it in this one's place and
/// starts it.
@Observable
final class AppUpdater: NSObject, URLSessionDownloadDelegate {
    enum State: Equatable {
        case idle
        /// The user asked and the answer isn't here yet.
        case checking
        case upToDate
        case available(String)
        case downloading(String, Double)
        case installing(String)
        /// Installed; the running app is still the old one.
        case ready(String)
        case failed(String)
    }

    struct Failure: LocalizedError {
        let message: String
        var errorDescription: String? { message }
    }

    private static let latestRelease = URL(string: "https://github.com/motileapp/motile/releases/latest")!
    private static let checkEvery: TimeInterval = 6 * 3600

    private(set) var state = State.idle
    /// The newest release's version. The servers are compared with it as well.
    private(set) var latest: String?
    let current = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? ""

    @ObservationIgnored private var timer: Timer?
    @ObservationIgnored private var downloadingVersion = ""
    @ObservationIgnored private lazy var session = URLSession(configuration: .default, delegate: self, delegateQueue: .main)

    /// Looks for a new release now and every few hours.
    func start() {
        check()
        timer = Timer.scheduledTimer(withTimeInterval: Self.checkEvery, repeats: true) { [weak self] _ in self?.check() }
    }

    /// `asked` is for when the user wants to know: finding nothing new is then said too.
    func check(asked: Bool = false) {
        switch state {
        case .checking, .downloading, .installing, .ready: return
        default: break
        }
        if asked { state = .checking }
        var request = URLRequest(url: Self.latestRelease)
        request.httpMethod = "HEAD"
        request.cachePolicy = .reloadIgnoringLocalCacheData
        URLSession.shared.dataTask(with: request) { [weak self] _, response, _ in
            // The address redirects to the release's own page, whose last part is its tag.
            let tag = response?.url?.lastPathComponent ?? ""
            let version = tag.hasPrefix("v") ? String(tag.dropFirst()) : nil
            DispatchQueue.main.async { self?.found(version, asked: asked) }
        }.resume()
    }

    private func found(_ version: String?, asked: Bool) {
        guard let version else {
            if asked { state = .failed("GitHub couldn’t be reached to look for a new version.") }
            return
        }
        latest = version
        if Version.isOlder(current, than: version) {
            state = .available(version)
        } else if asked {
            state = .upToDate
            DispatchQueue.main.asyncAfter(deadline: .now() + 4) { [weak self] in
                if self?.state == .upToDate { self?.state = .idle }
            }
        }
    }

    /// Why this copy can't replace itself, if it can't.
    var obstacle: String? {
        if Bundle.main.bundlePath.contains("/AppTranslocation/") {
            return "Move Motile to the Applications folder and open it from there to update it."
        }
        if Self.team(of: Self.runningCode()) == nil {
            return "This copy of Motile wasn’t signed by its developer, so it can’t update itself. Download the new version from motile.app."
        }
        return nil
    }

    func install() {
        guard case .available(let version) = state else { return }
        if let obstacle {
            state = .failed(obstacle)
            return
        }
        guard let url = URL(string: "https://github.com/motileapp/motile/releases/download/v\(version)/Motile.zip") else { return }
        downloadingVersion = version
        state = .downloading(version, 0)
        session.downloadTask(with: url).resume()
    }

    /// Back to offering the update, after it failed.
    func retry() {
        state = .idle
        check(asked: true)
    }

    // MARK: Downloading

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData bytesWritten: Int64, totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64) {
        guard totalBytesExpectedToWrite > 0 else { return }
        state = .downloading(downloadingVersion, Double(totalBytesWritten) / Double(totalBytesExpectedToWrite))
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
        let version = downloadingVersion
        let status = (downloadTask.response as? HTTPURLResponse)?.statusCode ?? 0
        guard status == 200 else {
            state = .failed("The new version couldn’t be downloaded (\(status)).")
            return
        }
        // The file is gone once this returns, so it is moved first.
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("motile-update-\(UUID().uuidString)")
        let archive = folder.appendingPathComponent("Motile.zip")
        do {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            try FileManager.default.moveItem(at: location, to: archive)
        } catch {
            state = .failed("The download couldn’t be saved: \(error.localizedDescription)")
            return
        }
        state = .installing(version)
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let result = Result { try AppUpdater.replaceThisApp(with: archive, in: folder) }
            try? FileManager.default.removeItem(at: folder)
            DispatchQueue.main.async {
                switch result {
                case .success: self?.state = .ready(version)
                case .failure(let error): self?.state = .failed(error.localizedDescription)
                }
            }
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        guard let error else { return }
        state = .failed("The new version couldn’t be downloaded: \(error.localizedDescription)")
    }

    // MARK: Installing

    private static func replaceThisApp(with archive: URL, in folder: URL) throws {
        let unpack = Process()
        unpack.executableURL = URL(fileURLWithPath: "/usr/bin/ditto")
        unpack.arguments = ["-x", "-k", archive.path, folder.path]
        try unpack.run()
        unpack.waitUntilExit()
        let newApp = folder.appendingPathComponent("Motile.app")
        guard unpack.terminationStatus == 0, FileManager.default.fileExists(atPath: newApp.path) else {
            throw Failure(message: "The download isn’t a copy of Motile.")
        }
        try verify(newApp)
        do {
            _ = try FileManager.default.replaceItemAt(Bundle.main.bundleURL, withItemAt: newApp)
        } catch {
            throw Failure(message: "Motile couldn’t be replaced where it is: \(error.localizedDescription)")
        }
    }

    private static func runningCode() -> SecStaticCode? {
        var running: SecCode?
        guard SecCodeCopySelf([], &running) == errSecSuccess, let running else { return nil }
        var code: SecStaticCode?
        guard SecCodeCopyStaticCode(running, [], &code) == errSecSuccess else { return nil }
        return code
    }

    /// The developer team that signed the code, or `nil` for code signed by nobody in particular.
    private static func team(of code: SecStaticCode?) -> String? {
        guard let code else { return nil }
        var information: CFDictionary?
        guard SecCodeCopySigningInformation(code, SecCSFlags(rawValue: kSecCSSigningInformation), &information) == errSecSuccess else {
            return nil
        }
        return (information as? [String: Any])?[kSecCodeInfoTeamIdentifier as String] as? String
    }

    /// The new app has to be intact and meet this app's own requirement: the same identifier,
    /// signed by the same developer.
    private static func verify(_ app: URL) throws {
        let notOurs = Failure(message: "The download isn’t signed by Motile’s developer, so it wasn’t installed.")
        guard let running = runningCode() else { throw notOurs }
        var requirement: SecRequirement?
        guard SecCodeCopyDesignatedRequirement(running, [], &requirement) == errSecSuccess, let requirement else { throw notOurs }
        var downloaded: SecStaticCode?
        guard SecStaticCodeCreateWithPath(app as CFURL, [], &downloaded) == errSecSuccess, let downloaded else { throw notOurs }
        let strict = SecCSFlags(rawValue: kSecCSCheckAllArchitectures | kSecCSCheckNestedCode | kSecCSStrictValidate)
        guard SecStaticCodeCheckValidity(downloaded, strict, requirement) == errSecSuccess else { throw notOurs }
    }

    /// Starts the installed version once this one has quit.
    func relaunch() {
        let waitThenOpen = Process()
        waitThenOpen.executableURL = URL(fileURLWithPath: "/bin/sh")
        let script = "while kill -0 \(ProcessInfo.processInfo.processIdentifier) 2>/dev/null; do sleep 0.1; done; /usr/bin/open \"$0\""
        waitThenOpen.arguments = ["-c", script, Bundle.main.bundlePath]
        try? waitThenOpen.run()
        NSApp.terminate(nil)
    }

    /// Shows a state without anything happening, for the demo's screenshots.
    func show(_ state: State, latest: String) {
        self.state = state
        self.latest = latest
    }
}
#endif

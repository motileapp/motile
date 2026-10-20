import SnapshotTesting
import XCTest

@testable import MotileKit

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// Every program of the corpus drawn dark at 390 and 720 points as the transcript draws it, as
/// build/programs/<mac|ios>/<program>-<width>.png for the sheets `scripts/compare-frames.sh` makes.
@MainActor
final class ProgramPicturesTests: XCTestCase {
    #if os(macOS)
    private static let output = Fixtures.repository.appendingPathComponent("build/programs/mac")
    #else
    private static let output = Fixtures.repository.appendingPathComponent("build/programs/ios")
    #endif

    private final class Host: ProgramHost {
        let rowID = "picture"
        func send(_ node: String, _ event: JSON) {}
        func media(_ id: String, done: @escaping (URL?) -> Void) { done(nil) }
        func resized(_ node: String) {}
    }

    func testEveryProgramOfTheCorpusIsPictured() throws {
        try FileManager.default.createDirectory(at: Self.output, withIntermediateDirectories: true)
        let host = Host()
        for (name, tree) in try Fixtures.programs() {
            let node = try XCTUnwrap(UiNode(json: tree), name)
            for width: CGFloat in [390, 720] {
                let box = ProgramLayout.layout(node, width: width, env: ProgramEnv(column: width, window: CGSize(width: 1200, height: 800)))
                let size = CGSize(width: width, height: max(1, box.size.height.rounded(.up)))
                let program = ProgramViews.make(for: box.node)
                program.frame = CGRect(origin: .zero, size: box.size)
                let data = try XCTUnwrap(Self.picture(program, size: size) { program.show(box, host: host) }, name)
                try data.write(to: Self.output.appendingPathComponent("\(name.dropLast(5))-\(Int(width)).png"))
            }
        }
    }

    #if os(macOS)
    private static func picture(_ program: NSView, size: CGSize, show: () -> Void) -> Data? {
        let dark = NSAppearance(named: .darkAqua)!
        let page = FlippedView(frame: CGRect(origin: .zero, size: size))
        page.appearance = dark
        page.wantsLayer = true
        page.addSubview(program)
        var data: Data?
        dark.performAsCurrentDrawingAppearance {
            page.layer?.backgroundColor = Theme.background.cgColor
            show()
            Snapshotting<NSView, NSImage>.image.snapshot(page).run { image in
                data = Snapshotting<NSView, NSImage>.image.diffing.toData(image)
            }
        }
        return data
    }
    #else
    private static func picture(_ program: UIView, size: CGSize, show: () -> Void) -> Data? {
        let page = FlippedView(frame: CGRect(origin: .zero, size: size))
        page.overrideUserInterfaceStyle = .dark
        page.backgroundColor = Theme.background
        page.addSubview(program)
        show()
        var data: Data?
        let traits = UITraitCollection(traitsFrom: [UITraitCollection(userInterfaceStyle: .dark), UITraitCollection(displayScale: 2)])
        let strategy = Snapshotting<UIView, UIImage>.image(traits: traits)
        strategy.snapshot(page).run { image in data = strategy.diffing.toData(image) }
        return data
    }
    #endif
}

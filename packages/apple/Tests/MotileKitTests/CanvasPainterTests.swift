import CoreGraphics
import ImageIO
import SnapshotTesting
import XCTest

@testable import MotileKit

#if os(macOS)
import AppKit
typealias TestImage = NSImage
#else
import UIKit
typealias TestImage = UIImage
#endif

/// Paints the core's reference scenes as the core's reference painter does, and compares the
/// pictures with its own outside the labels' boxes, where the typefaces differ.
final class CanvasPainterTests: XCTestCase {
    private struct Case: Decodable {
        let name: String
        let hover: Int?
        let scale: CGFloat
        let masks: [[CGFloat]]
    }

    /// A pixel differs when a channel differs by more than this, and a picture when more than
    /// `allowed` of its pixels do.
    private static let tolerance = 40
    private static let allowed = 0.01

    func testScenesArePaintedAsTheReferenceDoes() throws {
        let cases = try JSONDecoder().decode([Case].self, from: Data(contentsOf: Fixtures.references.appendingPathComponent("cases.json")))
        XCTAssertFalse(cases.isEmpty)
        try FileManager.default.createDirectory(at: Fixtures.output, withIntermediateDirectories: true)
        var worst: [String: (name: String, share: Double)] = [:]
        for test in cases {
            let scene = try XCTUnwrap(Fixtures.scene(test.name), test.name)
            for (theme, dark) in [("dark", true), ("light", false)] {
                let name = "\(test.name)-\(theme)"
                let picture = Self.render(scene, dark: dark, scale: test.scale, hover: test.hover)
                let image = Self.platformImage(picture)
                try Self.diffing.toData(image).write(to: Fixtures.output.appendingPathComponent("\(name).png"))
                let reference = try XCTUnwrap(Self.load(Fixtures.references.appendingPathComponent("\(name).png")), name)
                XCTAssertEqual([reference.width, reference.height], [picture.width, picture.height], name)
                let share = Self.differing(picture, reference, masks: test.masks, scale: test.scale)
                print(String(format: "canvas %@: %.3f%% differs", name, share * 100))
                if share > worst[theme]?.share ?? -1 { worst[theme] = (name, share) }
                guard share > Self.allowed else { continue }
                XCTFail(String(format: "%@: %.2f%% of its pixels differ from the reference", name, share * 100))
                let referenceImage = Self.platformImage(reference)
                guard let (_, attachments) = Self.diffing.diff(referenceImage, image) else { continue }
                for attachment in attachments { add(attachment) }
            }
        }
        for (theme, worst) in worst.sorted(by: { $0.key < $1.key }) {
            print(String(format: "canvas worst %@: %@ %.3f%%", theme, worst.name, worst.share * 100))
        }
    }

    #if os(macOS)
    private static let diffing = Diffing<NSImage>.image(precision: Float(1 - allowed), perceptualPrecision: 0.84)

    private static func platformImage(_ image: CGImage) -> NSImage {
        NSImage(cgImage: image, size: CGSize(width: image.width, height: image.height))
    }
    #else
    private static let diffing = Diffing<UIImage>.image(precision: Float(1 - allowed), perceptualPrecision: 0.84, scale: 1)

    private static func platformImage(_ image: CGImage) -> UIImage {
        UIImage(cgImage: image, scale: 1, orientation: .up)
    }
    #endif

    /// The scene on the transcript's background, `scale` pixels to the point, with `hover` lit.
    static func render(_ scene: CanvasScene, dark: Bool, scale: CGFloat, hover: Int?) -> CGImage {
        let width = Int(max(1, ceil(scene.width * scale)))
        let height = Int(max(1, ceil(scene.height * scale)))
        let context = bitmap(width: width, height: height)
        context.translateBy(x: 0, y: CGFloat(height))
        context.scaleBy(x: scale, y: -scale)
        let palette = CanvasPalette(surface: .background, dark: dark)
        context.setFillColor(palette.color(.surface(.base)))
        context.fill(CGRect(x: 0, y: 0, width: CGFloat(width) / scale, height: CGFloat(height) / scale))
        CanvasPainter.paint(scene, in: context, palette: palette, lit: hover)
        return context.makeImage()!
    }

    private static func bitmap(width: Int, height: Int) -> CGContext {
        CGContext(
            data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    }

    private static func load(_ url: URL) -> CGImage? {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
        return CGImageSourceCreateImageAtIndex(source, 0, nil)
    }

    /// The image's pixels as sRGB bytes, four to a pixel.
    private static func pixels(_ image: CGImage) -> [UInt8] {
        let context = bitmap(width: image.width, height: image.height)
        context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        let bytes = context.data!.assumingMemoryBound(to: UInt8.self)
        return Array(UnsafeBufferPointer(start: bytes, count: image.width * image.height * 4))
    }

    /// The share of the pixels that differ, with every mask painted over in both pictures.
    static func differing(_ picture: CGImage, _ reference: CGImage, masks: [[CGFloat]], scale: CGFloat) -> Double {
        guard picture.width == reference.width, picture.height == reference.height else { return 1 }
        let (width, height) = (picture.width, picture.height)
        var masked = [Bool](repeating: false, count: width * height)
        for mask in masks where mask.count == 4 {
            let left = max(0, Int((mask[0] * scale).rounded(.down)))
            let top = max(0, Int((mask[1] * scale).rounded(.down)))
            let right = min(width, Int(((mask[0] + mask[2]) * scale).rounded(.up)))
            let bottom = min(height, Int(((mask[1] + mask[3]) * scale).rounded(.up)))
            guard left < right, top < bottom else { continue }
            for y in top..<bottom { for x in left..<right { masked[y * width + x] = true } }
        }
        let (mine, theirs) = (pixels(picture), pixels(reference))
        var differ = 0
        for index in 0..<(width * height) where !masked[index] {
            let at = index * 4
            if (0..<4).contains(where: { abs(Int(mine[at + $0]) - Int(theirs[at + $0])) > tolerance }) { differ += 1 }
        }
        return Double(differ) / Double(width * height)
    }
}

enum Fixtures {
    static let repository = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .deletingLastPathComponent()
    static let references = repository.appendingPathComponent("crates/core/fixtures/canvas/reference")
    static let output = repository.appendingPathComponent("build/canvas/apple")

    static let trees = repository.appendingPathComponent("crates/core/fixtures/openui")

    /// The golden trees of the corpus, by their file's name.
    static func programs() throws -> [(String, JSON)] {
        let names = try FileManager.default.contentsOfDirectory(atPath: trees.path).filter { $0.hasSuffix(".json") }.sorted()
        return try names.map { name in
            let data = try Data(contentsOf: trees.appendingPathComponent(name))
            return (name, try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? JSON))
        }
    }

    static func scene(_ name: String) -> CanvasScene? {
        guard let data = try? Data(contentsOf: references.appendingPathComponent("\(name).json")) else { return nil }
        return CanvasLayout.decode(data)
    }
}

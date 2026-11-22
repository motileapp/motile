import CoreGraphics
import XCTest

@testable import MotileKit

/// How a reply's image is boxed, and the colour an SVG's `currentColor` takes.
final class PicturesTests: XCTestCase {
    private func media(width: Int, height: Int) -> MediaContent {
        MediaContent(json: ["media": "a.png", "width": width, "height": height])
    }

    func testASmallPictureLiesInTheSmallestBoxAtItsOwnSize() {
        let icon = media(width: 24, height: 16)
        XCTAssertEqual(icon.box(width: 600), CGSize(width: 96, height: 96))
        XCTAssertEqual(icon.drawn(width: 600), CGSize(width: 24, height: 16))
    }

    func testALargePictureFillsItsBoxAndFitsTheColumn() {
        let shot = media(width: 1200, height: 300)
        XCTAssertEqual(shot.box(width: 600), CGSize(width: 600, height: 150))
        XCTAssertEqual(shot.drawn(width: 600), CGSize(width: 600, height: 150))
        XCTAssertEqual(media(width: 40, height: 400).box(width: 600), CGSize(width: 96, height: 400))
    }

    func testCurrentColorTakesTheGivenColourUnlessTheRootHasOne() {
        let svg = #"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" fill="currentColor"><rect/></svg>"#
        let inked = String(decoding: Pictures.inked(Data(svg.utf8), color: "#d0d2d9"), as: UTF8.self)
        XCTAssertEqual(inked, ##"<?xml version="1.0"?><svg color="#d0d2d9" xmlns="http://www.w3.org/2000/svg" fill="currentColor"><rect/></svg>"##)

        let own = #"<svg xmlns="http://www.w3.org/2000/svg" color="red"><rect/></svg>"#
        XCTAssertEqual(String(decoding: Pictures.inked(Data(own.utf8), color: "#d0d2d9"), as: UTF8.self), own)
    }
}

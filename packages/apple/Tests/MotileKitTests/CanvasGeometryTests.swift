import CoreGraphics
import XCTest

@testable import MotileKit

/// What the canvas view works out from a scene: the hit under a point, its size with a legend,
/// and where the flip and the confetti are at a moment.
final class CanvasGeometryTests: XCTestCase {
    func testABarBandHoldsThePointsOverIt() throws {
        let scene = try XCTUnwrap(Fixtures.scene("chart-bar-720"))
        guard case .rect(let band) = scene.hits[3].region else { return XCTFail("a bar's band is a rectangle") }
        XCTAssertEqual(scene.hit(at: CGPoint(x: band.midX, y: 100)), 3)
        XCTAssertEqual(scene.hit(at: CGPoint(x: band.minX, y: band.maxY)), 3)
        XCTAssertNil(scene.hit(at: CGPoint(x: band.midX, y: band.maxY + 1)))
    }

    func testADonutsSectorHoldsItsRingOnly() throws {
        let scene = try XCTUnwrap(Fixtures.scene("chart-donut-720"))
        let center = CGPoint(x: 100, y: 100)
        func at(angle: CGFloat, distance: CGFloat) -> CGPoint {
            CGPoint(x: center.x + cos(angle) * distance, y: center.y + sin(angle) * distance)
        }
        XCTAssertEqual(scene.hit(at: at(angle: 0, distance: 80)), 0)
        XCTAssertEqual(scene.hit(at: at(angle: 2.2, distance: 80)), 1)
        XCTAssertEqual(scene.hit(at: at(angle: -2, distance: 80)), 3, "up and to the left is past π, where the last slice ends")
        XCTAssertNil(scene.hit(at: at(angle: 0, distance: 40)), "the hole")
        XCTAssertNil(scene.hit(at: at(angle: 0, distance: 101)))
    }

    func testASectorPastAFullTurnHoldsTheAnglesItWrapsTo() {
        let sector = CanvasHit.Region.sector(center: .zero, inner: 0, outer: 10, start: 5.5, end: 7)
        func at(_ angle: CGFloat) -> CGPoint { CGPoint(x: cos(angle) * 5, y: sin(angle) * 5) }
        XCTAssertTrue(sector.contains(at(0.3)))
        XCTAssertTrue(sector.contains(at(-0.5)))
        XCTAssertFalse(sector.contains(at(1)))
    }

    func testTheLastHitThatHoldsThePointWins() throws {
        let json = """
            {"width": 100, "height": 100, "items": [], "hits": [
              {"shape": "rect", "x": 0, "y": 0, "width": 100, "height": 100, "at": 0, "tip": {"x": 0, "y": 0, "title": "under"}},
              {"shape": "circle", "cx": 50, "cy": 50, "r": 10, "at": 0, "tip": {"x": 0, "y": 0, "title": "over"}}
            ]}
            """
        let scene = try XCTUnwrap(CanvasLayout.decode(Data(json.utf8)))
        XCTAssertEqual(scene.hit(at: CGPoint(x: 52, y: 50)), 1)
        XCTAssertEqual(scene.hit(at: CGPoint(x: 5, y: 5)), 0)
    }

    func testAChartsLegendLiesUnderItAndWraps() throws {
        let scene = try XCTUnwrap(Fixtures.scene("chart-bar-grouped-390"))
        XCTAssertEqual(CanvasContent.height(scene, width: 390), 240 + 10 + scaled(18))
        XCTAssertEqual(CanvasContent.height(scene, width: 60), 240 + 10 + scaled(18) + 6 + scaled(18))
        XCTAssertEqual(CanvasContent(scene: scene, width: 390).size.width, 390)
    }

    func testAPiesLegendLiesBesideItsDiscOrUnderIt() throws {
        let scene = try XCTUnwrap(Fixtures.scene("chart-pie-720"))
        let column = 4 * scaled(20) + 3 * 9
        let beside = CanvasContent(scene: scene, width: 720)
        XCTAssertEqual(beside.size, CGSize(width: 720, height: 200))
        XCTAssertEqual(beside.legend.first?.dot.minX, 224)
        XCTAssertEqual(beside.legend.first?.dot.minY, ((200 - column) / 2).rounded() + ((scaled(20) - 8) / 2).rounded())
        let legend = try XCTUnwrap(scene.legend)
        let under = CanvasScene(
            width: scene.width, height: scene.height, baseline: nil, label: scene.label, items: scene.items, hits: scene.hits,
            legend: CanvasLegend(beside: false, entries: legend.entries), motion: nil)
        XCTAssertEqual(CanvasContent.height(under, width: 390), 200 + 24 + column)
    }

    func testASceneWithoutALegendKeepsItsOwnSize() throws {
        let scene = try XCTUnwrap(Fixtures.scene("math-integral"))
        XCTAssertEqual(CanvasContent(scene: scene, width: 720).size, scene.size)
    }

    func testTheFlipSquashesTheFaceToNothingWhereTheFacesSwap() throws {
        let scene = try XCTUnwrap(CanvasLayout.scene(kind: "Flashcard", props: [:], width: 400))
        guard case .flip(let flip) = scene.motion else { return XCTFail("a flashcard's scene is its flip") }
        XCTAssertEqual(flip.scale(at: 0).scaleX, 1, accuracy: 1e-4)
        XCTAssertFalse(flip.scale(at: 0).pastSwap)
        let swap = flip.scale(at: flip.swapAt * flip.duration)
        XCTAssertEqual(swap.scaleX, 0, accuracy: 1e-3)
        XCTAssertTrue(swap.pastSwap)
        XCTAssertEqual(flip.scale(at: flip.duration).scaleX, 1, accuracy: 1e-4)
        XCTAssertTrue(flip.scale(at: flip.duration * 2).pastSwap)
    }

    func testAConfettiPieceFallsTurnsAndFades() throws {
        let scene = try XCTUnwrap(CanvasLayout.scene(kind: "Celebration", props: [:], width: 400))
        guard case .confetti(let confetti) = scene.motion else { return XCTFail("a celebration's scene is its confetti") }
        let window = CGSize(width: 1000, height: 800)
        let piece = confetti.pieces[1]
        let start = confetti.frames(in: window, at: 0)[1]
        XCTAssertEqual(start.rect.origin.x, piece.x * 1000, accuracy: 1e-3)
        XCTAssertEqual(start.rect.origin.y, confetti.top, accuracy: 1e-3)
        XCTAssertEqual(start.rect.size, CGSize(width: confetti.width, height: confetti.height))
        XCTAssertEqual(start.angle, 0)
        XCTAssertEqual(start.opacity, 1)

        let middle = confetti.frames(in: window, at: piece.delay + piece.duration / 2)[1]
        let eased = Self.cssEase(confetti.curve, at: 0.5)
        XCTAssertEqual(middle.rect.origin.x, piece.x * 1000 + piece.drift * eased, accuracy: 0.01)
        XCTAssertEqual(middle.rect.origin.y, confetti.top + confetti.fall * 800 * eased, accuracy: 0.05)
        XCTAssertEqual(middle.angle, piece.turn * eased * .pi / 180, accuracy: 1e-3)

        let end = confetti.frames(in: window, at: piece.delay + piece.duration)[1]
        XCTAssertEqual(end.rect.origin.x, piece.x * 1000 + piece.drift, accuracy: 1e-3)
        XCTAssertEqual(end.rect.origin.y, confetti.top + confetti.fall * 800, accuracy: 1e-2)
        XCTAssertEqual(end.angle, piece.turn * .pi / 180, accuracy: 1e-4)
        XCTAssertEqual(end.opacity, confetti.endOpacity, accuracy: 1e-4)
        XCTAssertEqual(confetti.frames(in: window, at: confetti.duration)[1], end)
    }

    /// A CSS timing function's progress, found by walking its curve finely.
    private static func cssEase(_ curve: [CGFloat], at x: CGFloat) -> CGFloat {
        func bezier(_ a: CGFloat, _ b: CGFloat, _ t: CGFloat) -> CGFloat {
            3 * (1 - t) * (1 - t) * t * a + 3 * (1 - t) * t * t * b + t * t * t
        }
        let t = stride(from: CGFloat(0), through: 1, by: 1e-5).min { abs(bezier(curve[0], curve[2], $0) - x) < abs(bezier(curve[0], curve[2], $1) - x) }!
        return bezier(curve[1], curve[3], t)
    }
}

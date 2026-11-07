import XCTest

@testable import MotileKit

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// The box around code inside a list holds all of its code, wherever in the text the code is.
final class CodeBoxTests: XCTestCase {
    private func system(_ text: String, code: String) -> TextSystem {
        let result = NSMutableAttributedString(string: text, attributes: [.font: Theme.proseFont])
        let range = (text as NSString).range(of: code)
        let style = NSMutableParagraphStyle()
        style.minimumLineHeight = Theme.codeLineHeight
        style.maximumLineHeight = Theme.codeLineHeight
        let box = CodeBoxes.make(language: "bash", index: 0, indent: 22, style: style)
        result.addAttributes([.paragraphStyle: style, .font: Theme.codeFont, .motileCode: box], range: range)
        let system = TextSystem()
        system.update(to: result)
        return system
    }

    private func check(_ system: TextSystem, code: String, _ message: String) throws {
        let height = system.height(forWidth: 360)
        let box = try XCTUnwrap(system.codeBoxes(origin: .zero).first, message)
        let range = (system.storage.string as NSString).range(of: code)
        let glyph = system.layout.glyphIndexForCharacter(at: NSMaxRange(range) - 1)
        let line = system.layout.lineFragmentUsedRect(forGlyphAt: glyph, effectiveRange: nil)
        XCTAssertGreaterThan(line.height, 0, message)
        XCTAssertEqual(box.frame.maxY, line.maxY + CodeBox.paddingBottom, accuracy: 0.5, message)
        XCTAssertLessThanOrEqual(box.frame.maxY, height, message)
    }

    func testABoxThatEndsTheTextHoldsItsCode() throws {
        let code = "~/.local/bin/motile status"
        try check(system("Power-cut test. Then check:\n\(code)", code: code), code: code, "last")
    }

    func testABoxWithTextAfterItHoldsItsCode() throws {
        let code = "~/.local/bin/motile status"
        try check(system("Then check:\n\(code)\nAnd go on.", code: code), code: code, "middle")
    }
}

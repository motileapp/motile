import XCTest

@testable import MotileKit

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A code block's view is exactly as tall as its trimmed lines, and still shows every one.
final class CodeLinesTests: XCTestCase {
    func testEveryLineIsLaidOutInAViewAsTallAsTheCode() {
        let content = CodeContent(language: "bash", code: "sudo systemctl stop uupd.timer\nsudo bootc upgrade", spans: nil)
        let height = LineRoom.height(lines: content.lineCount, font: Theme.codeFont, lineHeight: Theme.codeLineHeight)
        let view = RowTextView.make(wraps: false)
        view.content = content.attributed
        view.frame = CGRect(x: 0, y: 0, width: 400, height: height)
        // UIKit sizes the container to the view; AppKit leaves it alone.
        #if os(macOS)
        view.textContainer!.size = view.frame.size
        let layout = view.layoutManager!
        #else
        view.textContainer.size = view.frame.size
        let layout = view.layoutManager
        #endif

        var bottoms: [CGFloat] = []
        layout.enumerateLineFragments(forGlyphRange: NSRange(location: 0, length: layout.numberOfGlyphs)) { rect, _, _, _, _ in
            bottoms.append(rect.maxY)
        }
        XCTAssertEqual(bottoms.count, 2)
        XCTAssertEqual(bottoms.last ?? 0, height, accuracy: 0.5)
    }
}

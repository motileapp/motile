#if os(macOS)
import AppKit
import XCTest

@testable import MotileKit

/// What is typed in a line on the Mac reaches its delegate, secure or not.
@MainActor
final class LineFieldTests: XCTestCase {
    private final class Typed: NSObject, NSTextFieldDelegate {
        var text: String?

        func controlTextDidChange(_ notification: Notification) {
            text = (notification.object as? NSTextField)?.stringValue
        }
    }

    private func typed(secure: Bool) throws -> String? {
        let field = LineTextField.make(secure: secure)
        let typed = Typed()
        field.delegate = typed
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 300, height: 100), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }
        field.frame = NSRect(x: 10, y: 10, width: 200, height: 22)
        window.contentView?.addSubview(field)
        XCTAssertTrue(window.makeFirstResponder(field))
        let editor = try XCTUnwrap(field.currentEditor() as? NSTextView)
        editor.insertText("demokey", replacementRange: NSRange(location: NSNotFound, length: 0))
        return typed.text
    }

    func testTextReachesTheDelegate() throws {
        XCTAssertEqual(try typed(secure: false), "demokey")
    }

    func testSecureTextReachesTheDelegate() throws {
        XCTAssertEqual(try typed(secure: true), "demokey")
    }
}
#endif

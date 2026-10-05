#if os(iOS)
import UIKit

extension NSAttributedString.Key {
    /// The paragraph keeps the place of a table; the value is its `TableContent`.
    static let motileTable = NSAttributedString.Key("motile.table")
}

/// A table of a reply, laid out once: its columns are as wide as their widest cell, up to a
/// limit past which a cell wraps. It is drawn over a line of the text that is as tall as it is,
/// and scrolls sideways when it is wider than the column.
final class TableContent: NSObject {
    private static let maxColumn = scaled(240)
    private static let minColumn: CGFloat = 44
    static let sidePadding: CGFloat = 10
    static let rowPadding: CGFloat = 7

    let cells: [[NSAttributedString]]
    let headers: Int
    let columnWidths: [CGFloat]
    let rowHeights: [CGFloat]
    let size: CGSize
    /// The table as it is copied: its rows as lines, its cells apart by tabs.
    let words: String

    init(cells: [[NSAttributedString]], headers: Int) {
        self.cells = cells
        self.headers = headers
        let columns = cells.map(\.count).max() ?? 0
        var widths = [CGFloat](repeating: Self.minColumn, count: columns)
        for row in cells {
            for (column, cell) in row.enumerated() {
                let natural = ceil(cell.bounds(width: Self.maxColumn).width) + 2 * Self.sidePadding + 1
                widths[column] = max(widths[column], min(natural, Self.maxColumn + 2 * Self.sidePadding))
            }
        }
        columnWidths = widths
        rowHeights = cells.map { row in
            let tallest = row.enumerated().map { column, cell in
                ceil(cell.bounds(width: widths[column] - 2 * Self.sidePadding).height)
            }.max() ?? 0
            return tallest + 2 * Self.rowPadding + 1
        }
        size = CGSize(width: widths.reduce(0, +), height: rowHeights.reduce(0, +))
        words = cells.map { $0.map(\.string).joined(separator: "\t") }.joined(separator: "\n")
    }

    // The same table typeset again is equal, so streamed text keeps the layout before it.
    override func isEqual(_ object: Any?) -> Bool {
        guard let other = object as? TableContent else { return false }
        return other.words == words && other.size == size
    }

    override var hash: Int { words.hashValue }
}

/// Takes the cells of the tables out of a reply's text and leaves one line for each table.
struct TableSetter {
    private struct Cell {
        let row: Int
        let column: Int
        let range: NSRange
        let header: Bool
    }

    private var tables: [Int: [Cell]] = [:]

    mutating func set(_ para: JSON, style: NSMutableParagraphStyle) {
        let range = NSRange(location: para.int("start"), length: para.int("len"))
        let cell = Cell(row: para.int("row"), column: para.int("column"), range: range, header: para.bool("header"))
        tables[para.int("table"), default: []].append(cell)
    }

    func finished(_ text: NSMutableAttributedString, paras: [JSON]) -> NSAttributedString {
        // The last table first, so that the places of the ones before it stay as they are.
        for cells in tables.values.sorted(by: { ($0.first?.range.location ?? 0) > ($1.first?.range.location ?? 0) }) {
            guard let first = cells.first, let last = cells.last, NSMaxRange(last.range) <= text.length else { continue }
            let rows = (cells.map(\.row).max() ?? 0) + 1
            let columns = (cells.map(\.column).max() ?? 0) + 1
            var grid = [[NSAttributedString]](repeating: [NSAttributedString](repeating: NSAttributedString(), count: columns), count: rows)
            for cell in cells {
                grid[cell.row][cell.column] = Self.trimmed(text.attributedSubstring(from: cell.range))
            }
            let table = TableContent(cells: grid, headers: Set(cells.filter(\.header).map(\.row)).count)
            let whole = NSRange(location: first.range.location, length: NSMaxRange(last.range) - first.range.location)
            let endsLine = (text.string as NSString).substring(with: whole).hasSuffix("\n")
            let style = NSMutableParagraphStyle()
            style.minimumLineHeight = table.size.height
            style.maximumLineHeight = table.size.height
            let place = NSAttributedString(
                string: endsLine ? "\u{200B}\n" : "\u{200B}",
                attributes: [.paragraphStyle: style, .font: Theme.smallFont, .motileTable: table])
            text.replaceCharacters(in: whole, with: place)
        }
        return text
    }

    private static func trimmed(_ cell: NSAttributedString) -> NSAttributedString {
        guard cell.string.hasSuffix("\n") else { return cell }
        return cell.attributedSubstring(from: NSRange(location: 0, length: cell.length - 1))
    }

    static func words(of text: NSAttributedString) -> String {
        let result = NSMutableString(string: text.string)
        var places: [(NSRange, String)] = []
        text.enumerateAttribute(.motileTable, in: NSRange(location: 0, length: text.length)) { value, range, _ in
            guard let table = value as? TableContent else { return }
            let endsLine = (text.string as NSString).substring(with: range).hasSuffix("\n")
            places.append((range, table.words + (endsLine ? "\n" : "")))
        }
        for (range, words) in places.reversed() { result.replaceCharacters(in: range, with: words) }
        return result as String
    }
}

/// A table of a reply, over the line of the text that keeps its place.
final class TableView: FlippedView {
    private let scroll = SidewaysClipView()
    private let grid = Grid()

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(scroll)
        scroll.addSubview(grid)
        menuActions = { [weak self] in
            guard let table = self?.grid.table else { return [] }
            return [MenuAction(title: "Copy Table", symbol: .copy) { Platform.copy(table.words) }]
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    /// Puts a table view over every place a table has in `text`, reusing the ones there are.
    static func place(_ boxes: [(frame: CGRect, table: TableContent)], in text: UIView, reusing views: [TableView]) -> [TableView] {
        var views = views
        while views.count < boxes.count {
            let view = TableView()
            text.addSubview(view)
            views.append(view)
        }
        for (index, view) in views.enumerated() {
            view.isHidden = index >= boxes.count
            guard index < boxes.count else { continue }
            view.frame = boxes[index].frame
            view.show(boxes[index].table)
        }
        return views
    }

    private func show(_ table: TableContent) {
        scroll.frame = bounds
        scroll.setContent(grid, size: table.size)
        guard grid.table !== table else { return }
        grid.table = table
        grid.redraw()
    }

    private final class Grid: FlippedView {
        var table: TableContent?

        override func draw(_ rect: CGRect) {
            guard let table else { return }
            var y: CGFloat = 0
            for (row, cells) in table.cells.enumerated() {
                var x: CGFloat = 0
                for (column, cell) in cells.enumerated() {
                    let width = table.columnWidths[column]
                    let box = CGRect(
                        x: x + TableContent.sidePadding, y: y + TableContent.rowPadding,
                        width: width - 2 * TableContent.sidePadding, height: table.rowHeights[row] - 2 * TableContent.rowPadding)
                    cell.draw(with: box, options: [.usesLineFragmentOrigin], context: nil)
                    x += width
                }
                y += table.rowHeights[row]
                (row < table.headers ? Theme.borderSecondary : Theme.border).setFill()
                CGRect(x: 0, y: y - 1, width: table.size.width, height: 1).fillCurrent()
            }
        }
    }
}
#endif

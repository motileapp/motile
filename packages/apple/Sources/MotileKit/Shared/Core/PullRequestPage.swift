import SwiftUI

/// The pull request tab as the core worked it out, its text set for drawing. It is read off the
/// main thread.
struct PullRequestPage {
    enum Tone: String {
        case success, danger, warning, pending, neutral, merged
    }

    struct Status: Identifiable {
        let kind: String
        let tone: Tone
        let title: String
        let detail: String?
        let at: Double?
        let buttons: [PullRequestButton]

        var id: String { kind }
    }

    struct Check: Identifiable {
        let id: Int
        let name: String
        let workflow: String?
        let label: String
        let tone: Tone
        let description: String?
        let url: URL?
        /// The prompt that has the agent fix it.
        let fix: String?
    }

    struct Entry: Identifiable {
        let id: Int
        let kind: String
        let author: String
        let said: String
        let tone: Tone
        let at: Double
        let body: [PullRequestText]
        let commits: [(oid: String, headline: String)]
        let url: URL?
    }

    let number: Int
    let title: String
    let url: URL?
    let state: PullRequest.State
    let byline: String
    let base: String
    let head: String
    let files: Int
    let additions: Int
    let deletions: Int
    let statuses: [Status]
    let checks: [Check]
    let primary: PullRequestButton?
    /// How it merges, as the core spells it: "squash".
    let method: String?
    let methods: [PullRequestChoice]
    let menu: [PullRequestButton]
    let activity: [Entry]
    let verdicts: [PullRequestChoice]
    let withComment: PullRequestChoice?
    /// Something is still being worked out, so it is read again in a while.
    let settling: Bool

    init(json: JSON) {
        number = json.int("number")
        title = json.string("title")
        url = URL(string: json.string("url"))
        state = switch json.string("state") {
        case "merged": .merged
        case "closed": .closed
        case "draft": .draft
        default: .open
        }
        byline = json.string("byline")
        base = json.string("base")
        head = json.string("head")
        files = json.int("files")
        additions = json.int("additions")
        deletions = json.int("deletions")
        statuses = json.objects("statuses").map { status in
            Status(
                kind: status.string("kind"), tone: Tone(json: status), title: status.string("title"),
                detail: status.optionalString("detail"), at: status.optionalDouble("at"),
                buttons: status.objects("buttons").map(PullRequestButton.init))
        }
        checks = json.objects("checks").enumerated().map { index, check in
            Check(
                id: index, name: check.string("name"), workflow: check.optionalString("workflow"), label: check.string("label"),
                tone: Tone(json: check), description: check.optionalString("description"),
                url: check.optionalString("url").flatMap(URL.init(string:)), fix: check.optionalString("fix"))
        }
        primary = json.object("primary").map(PullRequestButton.init)
        method = json.optionalString("method")
        methods = json.objects("methods").map(PullRequestChoice.init)
        menu = json.objects("menu").map(PullRequestButton.init)
        activity = json.objects("activity").enumerated().map { index, entry in
            Entry(
                id: index, kind: entry.string("kind"), author: entry.string("author"), said: entry.string("said"),
                tone: Tone(json: entry), at: entry.double("at"), body: PullRequestText.blocks(entry.objects("body")),
                commits: entry.objects("commits").map { ($0.string("oid"), $0.string("headline")) },
                url: entry.optionalString("url").flatMap(URL.init(string:)))
        }
        verdicts = json.objects("verdicts").map(PullRequestChoice.init)
        withComment = json.object("with_comment").map(PullRequestChoice.init)
        settling = json.bool("settling")
    }
}

extension PullRequestPage.Tone {
    init(json: JSON) {
        self = Self(rawValue: json.string("tone")) ?? .neutral
    }
}

/// Something the tab offers: an action on the pull request, or a prompt for the thread's agent.
struct PullRequestButton: Identifiable {
    let label: String
    let action: String?
    let method: String?
    let prompt: String?
    let style: String
    let confirm: (title: String, message: String, button: String)?

    var id: String { label }

    init(json: JSON) {
        label = json.string("label")
        action = json.optionalString("action")
        method = json.optionalString("method")
        prompt = json.optionalString("prompt")
        style = json.string("style")
        confirm = json.object("confirm").map { ($0.string("title"), $0.string("message"), $0.string("button")) }
    }
}

struct PullRequestChoice: Identifiable {
    let label: String
    let action: String
    let method: String?

    var id: String { label }

    init(json: JSON) {
        label = json.string("label")
        action = json.string("action")
        method = json.optionalString("method")
    }
}

/// A stretch of Markdown from the pull request, set like a reply in the transcript, a little
/// smaller to sit in the panel.
enum PullRequestText {
    case prose(NSAttributedString)
    case code(NSAttributedString)

    static let size: CGFloat = 13 * Platform.scale

    static func blocks(_ json: [JSON]) -> [PullRequestText] {
        json.map { block in
            if block.string("kind") == "code" {
                let code = NSMutableAttributedString(
                    attributedString: Typesetter.code(block.string("code"), spans: block["spans"] as? [NSNumber] ?? []))
                // The panel is narrow: long lines wrap rather than run out of it.
                code.enumerateAttribute(.paragraphStyle, in: NSRange(location: 0, length: code.length)) { value, range, _ in
                    guard let style = (value as? NSParagraphStyle)?.mutableCopy() as? NSMutableParagraphStyle else { return }
                    style.lineBreakMode = .byCharWrapping
                    code.addAttribute(.paragraphStyle, value: style, range: range)
                }
                return .code(code)
            }
            return .prose(resized(Typesetter.prose(block)))
        }
    }

    /// The text with every font made smaller by as much as the panel's text is.
    private static func resized(_ text: NSAttributedString) -> NSAttributedString {
        let factor = size / Theme.proseSize
        let result = NSMutableAttributedString(attributedString: text)
        result.enumerateAttribute(.font, in: NSRange(location: 0, length: result.length)) { value, range, _ in
            guard let font = value as? PlatformFont else { return }
            result.addAttribute(.font, value: font.withSize(font.pointSize * factor), range: range)
        }
        return result
    }
}

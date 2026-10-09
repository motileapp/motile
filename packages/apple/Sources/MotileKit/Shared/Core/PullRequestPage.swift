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
        let commits: [Commit]
        let url: URL?
        /// Names the comment or the review, to react to it.
        let subject: String?
        let reactions: [Reaction]
        let thread: Thread?
    }

    struct Commit: Identifiable {
        let oid: String
        let headline: String
        let sha: String

        var id: String { sha.isEmpty ? oid : sha }
    }

    /// A conversation on a line.
    struct Thread: Identifiable {
        let id: String
        let path: String
        let line: Int?
        /// "left" for a line as it was, "right" as the pull request makes it.
        let side: String
        let resolved: Bool
        let outdated: Bool
        /// The last lines of the diff it was written under.
        let hunk: [String]
        let comments: [Comment]
        let fix: String?
        let canResolve: Bool

        init(json: JSON) {
            id = json.string("id")
            path = json.string("path")
            line = (json["line"] as? NSNumber)?.intValue
            side = json.string("side")
            resolved = json.bool("resolved")
            outdated = json.bool("outdated")
            hunk = json.strings("hunk")
            comments = json.objects("comments").map(Comment.init)
            fix = json.optionalString("fix")
            canResolve = json.bool("can_resolve")
        }
    }

    struct Comment: Identifiable {
        let id: String
        let author: String
        let at: Double
        let body: [PullRequestText]
        let url: URL?
        let reactions: [Reaction]

        init(json: JSON) {
            id = json.string("id")
            author = json.string("author")
            at = json.double("at")
            body = PullRequestText.blocks(json.objects("body"))
            url = json.optionalString("url").flatMap(URL.init(string:))
            reactions = json.objects("reactions").map(Reaction.init)
        }
    }

    struct Reaction: Identifiable, Equatable {
        /// As the protocol spells it: "thumbs_up".
        let kind: String
        let count: Int
        let mine: Bool

        var id: String { kind }

        init(json: JSON) {
            kind = json.string("kind")
            count = json.int("count")
            mine = json.bool("mine")
        }

        /// GitHub's reactions, in the order it offers them.
        static let all: [(kind: String, emoji: String)] = [
            ("thumbs_up", "👍"), ("thumbs_down", "👎"), ("laugh", "😄"), ("hooray", "🎉"),
            ("confused", "😕"), ("heart", "❤️"), ("rocket", "🚀"), ("eyes", "👀"),
        ]

        var emoji: String { Self.all.first { $0.kind == kind }?.emoji ?? "" }
    }

    struct Toggle: Identifiable {
        let name: String
        let color: Color?
        let on: Bool

        var id: String { name }
    }

    struct Reviewer: Identifiable {
        let name: String
        let label: String
        let tone: Tone

        var id: String { name }
    }

    struct StackLayer: Identifiable {
        let number: Int
        let title: String
        let state: PullRequest.State
        let current: Bool

        var id: Int { number }
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
    /// The description as it was written, for editing it.
    let body: String
    let canEdit: Bool
    let labels: [(name: String, color: Color)]
    let labelChoices: [Toggle]
    let reviewers: [Reviewer]
    let reviewerChoices: [Toggle]
    /// Which of its files the user has marked as viewed, by path.
    let viewed: [String: Bool]
    let canReviewLines: Bool
    let threads: [Thread]
    let stack: (url: URL?, base: String, layers: [StackLayer])?
    let stackedOn: (number: Int, title: String, state: PullRequest.State)?
    let watchable: Bool

    init(json: JSON) {
        number = json.int("number")
        title = json.string("title")
        url = URL(string: json.string("url"))
        state = Self.state(json.string("state"))
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
                commits: entry.objects("commits").map { Commit(oid: $0.string("oid"), headline: $0.string("headline"), sha: $0.string("sha")) },
                url: entry.optionalString("url").flatMap(URL.init(string:)), subject: entry.optionalString("id"),
                reactions: entry.objects("reactions").map(Reaction.init), thread: entry.object("thread").map(Thread.init))
        }
        verdicts = json.objects("verdicts").map(PullRequestChoice.init)
        withComment = json.object("with_comment").map(PullRequestChoice.init)
        settling = json.bool("settling")
        body = json.string("body")
        canEdit = json.bool("can_edit")
        labels = json.objects("labels").map { ($0.string("name"), Self.color(hex: $0.string("color"))) }
        labelChoices = json.objects("label_choices").map {
            Toggle(name: $0.string("name"), color: $0.optionalString("color").map(Self.color(hex:)), on: $0.bool("on"))
        }
        reviewers = json.objects("reviewers").map { Reviewer(name: $0.string("name"), label: $0.string("label"), tone: Tone(json: $0)) }
        reviewerChoices = json.objects("reviewer_choices").map { Toggle(name: $0.string("name"), color: nil, on: $0.bool("on")) }
        viewed = Dictionary(json.objects("viewed").map { ($0.string("path"), $0.bool("viewed")) }) { first, _ in first }
        canReviewLines = json.bool("can_review_lines")
        threads = json.objects("threads").map(Thread.init)
        stack = json.object("stack").map { stack in
            (URL(string: stack.string("url")), stack.string("base"), stack.objects("layers").map { layer in
                StackLayer(
                    number: layer.int("number"), title: layer.string("title"), state: Self.state(layer.string("state")),
                    current: layer.bool("current"))
            })
        }
        stackedOn = json.object("stacked_on").map { ($0.int("number"), $0.string("title"), Self.state($0.string("state"))) }
        watchable = json.bool("watchable")
    }

    private static func state(_ name: String) -> PullRequest.State {
        switch name {
        case "merged": .merged
        case "closed": .closed
        case "draft": .draft
        default: .open
        }
    }

    /// A label's colour, from GitHub's hex.
    static func color(hex: String) -> Color {
        let value = UInt32(hex, radix: 16) ?? 0x888888
        return Color(platform: Theme.hex(value))
    }

    /// The conversations on lines of the file, by the line of the diff they are on.
    func threads(on path: String) -> [Thread] {
        threads.filter { $0.path == path }
    }
}

extension PullRequestPage.Tone {
    init(json: JSON) {
        self = Self(rawValue: json.string("tone")) ?? .neutral
    }
}

/// A pull request in the repository's list.
struct PullRequestRow: Identifiable {
    let number: Int
    let title: String
    let url: URL?
    let state: PullRequest.State
    let author: String
    let head: String
    let base: String
    let updatedAt: Double
    let checks: PullRequestPage.Tone?
    let checksLabel: String?
    let review: (tone: PullRequestPage.Tone, label: String)?
    let additions: Int
    let deletions: Int

    var id: Int { number }

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
        author = json.string("author")
        head = json.string("head")
        base = json.string("base")
        updatedAt = json.double("updated_at")
        checks = json.optionalString("checks").flatMap(PullRequestPage.Tone.init(rawValue:))
        checksLabel = json.optionalString("checks_label")
        review = (json["review"] as? [String]).flatMap { pair in
            guard pair.count == 2, let tone = PullRequestPage.Tone(rawValue: pair[0]) else { return nil }
            return (tone, pair[1])
        }
        additions = json.int("additions")
        deletions = json.int("deletions")
    }
}

/// Something the tab offers: an action on the pull request, or a prompt for the thread's agent.
struct PullRequestButton: Identifiable {
    let label: String
    let pendingLabel: String?
    let action: String?
    let method: String?
    let prompt: String?
    let style: String
    let confirm: (title: String, message: String, button: String)?
    /// It was chosen from the menu, so what it does is said in the bar rather than on it.
    private(set) var fromMenu = false

    var id: String { label }

    /// Names it while its action runs.
    var key: String { fromMenu ? "menu:\(label)" : label }

    /// The same button as the menu offers it.
    var inMenu: PullRequestButton {
        var copy = self
        copy.fromMenu = true
        return copy
    }

    init(json: JSON) {
        label = json.string("label")
        pendingLabel = json.optionalString("pending_label")
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

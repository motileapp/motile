import Foundation
import Testing

@testable import MotileShared

private func item(_ thread: String, _ state: WorkState, since: Double) -> WorkItem {
    WorkItem(threadId: thread, title: thread, project: "motile", state: state, since: since)
}

@Test func mergesServersWithWhatWaitsFirstAndCountsBeyondTheItems() {
    let studio = ServerWork(
        serverId: "studio", serverName: "studio",
        items: [item("a", .working, since: 10), item("b", .finished, since: 50)], working: 7, waiting: 0, at: 100
    )
    let mini = ServerWork(
        serverId: "mini", serverName: "mini",
        items: [item("c", .waiting, since: 5), item("d", .working, since: 20)], working: 1, waiting: 1, at: 100
    )
    let work = AgentsWork([studio, mini])
    #expect(work.items.map(\.threadId) == ["c", "d", "a", "b"])
    #expect(work.items.first?.serverId == "mini")
    #expect(work.summary == "8 working · 1 needs you")
    #expect(AgentsWork([]).summary == "No agents working")
}

@Test func whatWorksTurnsStaleWhenItsServerWasLastHeardLongAgo() {
    let server = ServerWork(
        serverId: "s", serverName: "s", items: [item("a", .working, since: 0), item("b", .finished, since: 0)], working: 1, waiting: 0, at: 1000
    )
    let work = AgentsWork([server])
    let soon = Date(timeIntervalSince1970: 1000 + 60)
    let later = Date(timeIntervalSince1970: 1000 + AgentsWork.staleAfter + 1)
    #expect(work.items.map { $0.isStale(at: soon) } == [false, false])
    #expect(work.items.map { $0.isStale(at: later) } == [true, false])
}

@Test func aThreadLinkReadsBackWhatItSays() {
    let link = ThreadLink(serverId: "abc 123", threadId: "t&1")
    #expect(ThreadLink(url: link.url) == link)
    #expect(ThreadLink(url: URL(string: "motile://auth?code=1")!) == nil)
}

/// They share the App Group's defaults, so they take turns.
@Suite(.serialized) struct SharedStateTests {
    @Test func theBadgeCountsEachAlertedThreadOnce() {
        let defaults = UserDefaults(suiteName: "MotileSharedTests.badge")!
        defaults.removePersistentDomain(forName: "MotileSharedTests.badge")
        SharedState.defaults = defaults
        SharedState.setBadge(2)
        #expect(SharedState.badgeAfterAlert(threadId: "a") == 3)
        #expect(SharedState.badgeAfterAlert(threadId: "a") == 3)
        #expect(SharedState.badgeAfterAlert(threadId: "b") == 4)
        SharedState.setBadge(5)
        #expect(SharedState.badgeAfterAlert(threadId: "a") == 6)
    }

    @Test func workIsKeptForTheWidget() {
        let defaults = UserDefaults(suiteName: "MotileSharedTests.work")!
        defaults.removePersistentDomain(forName: "MotileSharedTests.work")
        SharedState.defaults = defaults
        let work = [ServerWork(serverId: "s", serverName: "studio", items: [item("a", .waiting, since: 1)], working: 0, waiting: 1, at: 2)]
        #expect(SharedState.setWork(work))
        #expect(SharedState.work == work)
        #expect(!SharedState.setWork(work))
    }
}

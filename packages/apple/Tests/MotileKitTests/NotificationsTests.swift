import MotileShared
import XCTest

@testable import MotileKit

/// The core's notification settings and alerts, as the client reads and writes them.
final class NotificationsTests: XCTestCase {
    func testReadsAndWritesTheCoresSettings() {
        let json: JSON = [
            "mode": "notifications_and_sound", "toasts": false, "live": true,
            "choices": ["finished": true, "failed": false, "approval": true, "question": true, "limit": false],
        ]
        let settings = NotificationSettings(json: json)
        XCTAssertEqual(settings.mode, .notificationsAndSound)
        XCTAssertFalse(settings.toasts)
        XCTAssertEqual(settings.choices, AlertChoices(finished: true, failed: false, approval: true, question: true, limit: false))
        XCTAssertEqual(NotificationSettings(json: settings.json), settings)
        XCTAssertEqual(settings.json["mode"] as? String, "notifications_and_sound")
    }

    func testKeepsTheDefaultsWhenTheSettingsCantBeRead() {
        XCTAssertEqual(NotificationSettings(json: ["mode": "loud"]), NotificationSettings())
    }

    func testReadsAnAlertAndHowItShows() throws {
        let event = try XCTUnwrap(
            AlertEvent(json: [
                "type": "alert", "server_name": "studio",
                "alert": [
                    "id": "a1", "server_id": "s1", "thread_id": "t1", "kind": "approval", "thread_title": "Greet by name",
                    "project": "motile", "detail": "Run greet.py", "body": "Needs your approval · Run greet.py", "at": 12.5,
                ],
                "shown": ["system": false, "sound": true, "toast": true],
            ]))
        XCTAssertEqual(event.serverName, "studio")
        XCTAssertEqual(event.alert.threadId, "t1")
        XCTAssertEqual(event.alert.kind, .approval)
        XCTAssertEqual(event.alert.body, "Needs your approval · Run greet.py")
        XCTAssertEqual(event.shown, AlertShown(system: false, sound: true, toast: true))
        XCTAssertNil(AlertEvent(json: ["type": "alert", "shown": [:]]))
    }

    func testFindsTheNotificationSettingsOnTheMac() {
        let found = SettingsEntry.matching("alert").map(\.id)
        XCTAssertTrue(found.contains("notifications"))
        XCTAssertTrue(found.contains("toasts"))
        XCTAssertFalse(found.contains("live-activities"))
        XCTAssertEqual(SettingsSection.shown.prefix(2), [.general, .notifications])
    }
}

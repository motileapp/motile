import MotileShared
import UserNotifications

/// Opens the alert a server sealed to this phone and shows it in place of the auth server's
/// placeholder. What it can't open stays the placeholder.
final class NotificationService: UNNotificationServiceExtension {
    override func didReceive(_ request: UNNotificationRequest, withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void) {
        guard let content = request.content.mutableCopy() as? UNMutableNotificationContent,
              let sealed = content.userInfo["sealed"] as? String,
              case .alert(let alert) = PushOpener.open(sealed: sealed) else { return contentHandler(request.content) }
        content.title = alert.threadTitle
        content.subtitle = alert.project
        content.body = alert.body
        content.threadIdentifier = alert.threadId
        content.userInfo["server"] = alert.serverId
        content.userInfo["thread"] = alert.threadId
        content.badge = NSNumber(value: SharedState.badgeAfterAlert(threadId: alert.threadId))
        contentHandler(content)
    }
}

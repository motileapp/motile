import Foundation

/// An S3 bucket, at AWS or anywhere that speaks its API.
struct BackupBucket: Equatable {
    var endpoint = ""
    var region = ""
    var bucket = ""
    var accessKey = ""
    var secretKey = ""

    init() {}

    init(json: JSON) {
        endpoint = json.string("endpoint")
        region = json.string("region")
        bucket = json.string("bucket")
        accessKey = json.string("access_key")
        secretKey = json.string("secret_key")
    }

    var json: JSON {
        ["endpoint": endpoint, "region": region, "bucket": bucket, "access_key": accessKey, "secret_key": secretKey]
    }

    var trimmed: BackupBucket {
        var trimmed = self
        for field in [\BackupBucket.endpoint, \.region, \.bucket, \.accessKey, \.secretKey] {
            trimmed[keyPath: field] = trimmed[keyPath: field].trimmingCharacters(in: .whitespacesAndNewlines)
        }
        return trimmed
    }

    var isComplete: Bool {
        let trimmed = trimmed
        return !trimmed.bucket.isEmpty && !trimmed.accessKey.isEmpty && !trimmed.secretKey.isEmpty
    }
}

/// The account's S3 bucket, which every server backs up to and syncs through.
struct Backup: Equatable {
    /// None while there is no backup. Its secret only when this device keeps it.
    var bucket: BackupBucket?
    var description = ""
    /// This device keeps the keys and gives them to a server that has none.
    var saved = false

    init() {}

    init(json: JSON) {
        bucket = json.object("bucket").map(BackupBucket.init(json:))
        description = json.string("description")
        saved = json.bool("saved")
    }
}

extension AppStore {
    /// Has the first connected server check the bucket, then backs every server up to it; `nil`
    /// stops. `done` gets the error, if any.
    func setBackup(_ bucket: BackupBucket?, done: @escaping (String?) -> Void = { _ in }) {
        let value: Any = bucket.map { $0.trimmed.json } ?? NSNull()
        core.send("set_backup", ["bucket": value]) { [weak self] result in
            switch result {
            case .success:
                done(nil)
            case .failure(let error):
                if bucket == nil { self?.alert(error) }
                done(error.message)
            }
        }
    }
}

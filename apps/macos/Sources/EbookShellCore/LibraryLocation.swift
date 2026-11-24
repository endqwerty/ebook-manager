import Foundation

/// Resolves the library directory from CLI arguments, environment variables, or Application Support.
public enum LibraryLocation {
    public static let envKey = "EBOOK_MANAGER_LIBRARY_DIR"
    public static let appFolderName = "Ebook Manager"

    public static func resolve(
        arguments: [String] = ProcessInfo.processInfo.arguments,
        environment: [String: String] = ProcessInfo.processInfo.environment,
        defaultApplicationSupport: URL? = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first
    ) -> String {
        if let flagIndex = arguments.firstIndex(of: "--library"),
           arguments.indices.contains(flagIndex + 1)
        {
            let value = arguments[flagIndex + 1].trimmingCharacters(in: .whitespacesAndNewlines)
            if !value.isEmpty {
                return (value as NSString).expandingTildeInPath
            }
        }
        if let envValue = environment[envKey]?.trimmingCharacters(in: .whitespacesAndNewlines),
           !envValue.isEmpty
        {
            return (envValue as NSString).expandingTildeInPath
        }
        let base = defaultApplicationSupport
            ?? URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true)
                .appendingPathComponent("Library/Application Support", isDirectory: true)
        return base.appendingPathComponent(appFolderName, isDirectory: true).path
    }
}

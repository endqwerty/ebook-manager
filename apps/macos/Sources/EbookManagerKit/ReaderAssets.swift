import Foundation

/// Loads the shared `reader.js` and `reader.css` assets without duplicating them.
public enum ReaderAssets {
    public static func loadScript() throws -> String {
        try loadAsset(name: "reader", ext: "js")
    }

    public static func loadStylesheet() throws -> Data {
        let text = try loadAsset(name: "reader", ext: "css")
        return Data(text.utf8)
    }

    private static func loadAsset(name: String, ext: String) throws -> String {
        if let bundled = Bundle.main.url(forResource: name, withExtension: ext),
           let content = try? String(contentsOf: bundled, encoding: .utf8)
        {
            return content
        }
        let sourceFile = URL(fileURLWithPath: #filePath)
        let repoRoot = sourceFile
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let shared = repoRoot
            .appendingPathComponent("apps/desktop/src/reader", isDirectory: true)
            .appendingPathComponent("\(name).\(ext)")
        return try String(contentsOf: shared, encoding: .utf8)
    }
}

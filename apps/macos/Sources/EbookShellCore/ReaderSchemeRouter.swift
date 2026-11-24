import Foundation
import Security

/// URL validation, link parsing, CSP, content rules, and page themes for the reader's `ebook:` scheme.
public enum ReaderRoute: Equatable, Sendable {
    case chapter(index: UInt32)
    case resource(id: UInt32)
    case stylesheet
}

public struct InternalJump: Equatable, Sendable {
    public let chapter: UInt32
    public let anchor: String?

    public init(chapter: UInt32, anchor: String?) {
        self.chapter = chapter
        self.anchor = anchor
    }
}

public enum ReaderSchemeRouter {
    public static let scheme = "ebook"
    public static let maxAddressLength = 2048
    public static let maxAnchorLength = 256

    public static let contentSecurityPolicy =
        "default-src 'none'; img-src ebook:; style-src ebook: 'unsafe-inline'; form-action 'none'; base-uri 'none'"

    public static let contentRuleListJSON =
        #"[{"trigger":{"url-filter":".*"},"action":{"type":"block"}},{"trigger":{"url-filter":"^ebook:"},"action":{"type":"ignore-previous-rules"}}]"#

    public static func makeToken() -> String {
        var bytes = [UInt8](repeating: 0, count: 16)
        let status = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        if status == errSecSuccess {
            return bytes.map { String(format: "%02x", $0) }.joined()
        }
        return UUID().uuidString.replacingOccurrences(of: "-", with: "").lowercased()
    }

    public static func chapterURL(token: String, chapter: UInt32, loadNumber: UInt32) -> URL? {
        URL(string: "\(scheme)://\(token)/c/\(chapter)?l=\(loadNumber)")
    }

    public static func resolve(url: URL, expectedToken: String) -> ReaderRoute? {
        guard url.scheme?.lowercased() == scheme,
              url.host == expectedToken,
              url.user == nil,
              url.password == nil,
              url.port == nil
        else {
            return nil
        }
        let path = url.path
        if path == "/s/reader.css" {
            return .stylesheet
        }
        if let chapter = parseNumberedPath(path, prefix: "/c/") {
            return .chapter(index: chapter)
        }
        if let resource = parseNumberedPath(path, prefix: "/r/") {
            return .resource(id: resource)
        }
        return nil
    }

    public static func wrapXHTMLDocument(
        bodyMarkup: String,
        language: String?,
        rightToLeft: Bool,
        cssVariables: [(name: String, value: String)]
    ) -> String {
        let validStyle = cssVariables
            .filter { isValidPropertyName($0.name) && isValidPropertyValue($0.value) }
            .map { "\($0.name):\($0.value);" }
            .joined()
        let langAttr = language.map { #" lang="\#(escapeXMLAttribute($0))""# } ?? ""
        let dir = rightToLeft ? "rtl" : "ltr"
        return """
        <?xml version="1.0" encoding="utf-8"?>
        <html xmlns="http://www.w3.org/1999/xhtml"\(langAttr) dir="\(dir)" style="\(escapeXMLAttribute(validStyle))"><head><title>Chapter</title><link rel="stylesheet" href="/s/reader.css"/></head><body>\(bodyMarkup)</body></html>
        """
    }

    private static func escapeXMLAttribute(_ value: String) -> String {
        var out = ""
        out.reserveCapacity(value.count)
        for scalar in value.unicodeScalars where !CharacterSet.controlCharacters.contains(scalar) {
            switch scalar {
            case "&": out += "&amp;"
            case "<": out += "&lt;"
            case "\"": out += "&quot;"
            default: out.unicodeScalars.append(scalar)
            }
        }
        return out
    }

    private static func isValidPropertyName(_ name: String) -> Bool {
        guard name.hasPrefix("--") else { return false }
        let rest = name.dropFirst(2)
        return !rest.isEmpty && rest.utf8.allSatisfy { ($0 >= UInt8(ascii: "a") && $0 <= UInt8(ascii: "z")) || $0 == UInt8(ascii: "-") }
    }

    private static func isValidPropertyValue(_ value: String) -> Bool {
        guard !value.isEmpty, value.utf8.count <= 200 else { return false }
        let allowedPunctuation = Set(" #.,%()-'".unicodeScalars)
        return value.unicodeScalars.allSatisfy { scalar in
            scalar.isASCII && (CharacterSet.alphanumerics.contains(scalar) || allowedPunctuation.contains(scalar))
        }
    }

    public static func parseJump(href: String) -> InternalJump? {
        guard href.utf8.count <= maxAddressLength else { return nil }
        let parts = href.split(separator: "#", maxSplits: 1, omittingEmptySubsequences: false)
        let path = String(parts[0])
        guard let chapter = parseNumberedPath(path, prefix: "/c/") else { return nil }
        if parts.count == 1 {
            return InternalJump(chapter: chapter, anchor: nil)
        }
        let rawFragment = String(parts[1])
        guard let decoded = rawFragment.removingPercentEncoding else { return nil }
        if decoded.count > maxAnchorLength || decoded.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) {
            return nil
        }
        return InternalJump(chapter: chapter, anchor: decoded.isEmpty ? nil : decoded)
    }

    public static func validatedExternalAddress(_ raw: String) -> URL? {
        guard raw.utf8.count <= maxAddressLength else { return nil }
        let lower = raw.lowercased()
        let allowed = lower.hasPrefix("http://") || lower.hasPrefix("https://") || lower.hasPrefix("mailto:")
        let hasBadScalar = raw.unicodeScalars.contains {
            CharacterSet.controlCharacters.contains($0) || CharacterSet.whitespacesAndNewlines.contains($0)
        }
        guard allowed, !hasBadScalar else { return nil }
        return URL(string: raw)
    }

    private static func parseNumberedPath(_ path: String, prefix: String) -> UInt32? {
        guard path.hasPrefix(prefix) else { return nil }
        let digits = path.dropFirst(prefix.count)
        guard !digits.isEmpty, digits.count <= 10, digits.allSatisfy(\.isWholeNumber), digits.allSatisfy(\.isASCII) else {
            return nil
        }
        return UInt32(digits)
    }
}

/// Built-in reader page palettes and text scale steps from `docs/design/interface-reader.md`.
public enum ReaderPaletteChoice: String, CaseIterable, Sendable {
    case automatic
    case white
    case paper
    case grey
    case black

    public var label: String {
        switch self {
        case .automatic: return "Follow Window"
        case .white: return "White"
        case .paper: return "Paper"
        case .grey: return "Grey"
        case .black: return "Black"
        }
    }
}

public struct ResolvedReaderPalette: Equatable, Sendable {
    public let pageHex: String
    public let inkHex: String
    public let secondaryHex: String
    public let pageRGB: UInt32
    public let inkRGB: UInt32
    public let secondaryRGB: UInt32
}

public enum ReaderTheme {
    public static let sizeSteps: [Double] = [
        0.8, 0.9, 1.0, 1.1, 1.2, 1.35, 1.5, 1.65, 1.8, 2.0, 2.25, 2.5,
    ]

    public static func resolvePalette(
        choice: ReaderPaletteChoice,
        isDarkWindow: Bool,
        increasedContrast: Bool
    ) -> ResolvedReaderPalette {
        let base: ReaderPaletteChoice
        switch choice {
        case .automatic:
            base = isDarkWindow ? .grey : .paper
        default:
            base = choice
        }
        if increasedContrast {
            switch base {
            case .grey, .black:
                return ResolvedReaderPalette(
                    pageHex: "#000000",
                    inkHex: "#ffffff",
                    secondaryHex: "#d0d0d0",
                    pageRGB: 0x000000,
                    inkRGB: 0xFFFFFF,
                    secondaryRGB: 0xD0D0D0
                )
            default:
                return ResolvedReaderPalette(
                    pageHex: "#ffffff",
                    inkHex: "#000000",
                    secondaryHex: "#404040",
                    pageRGB: 0xFFFFFF,
                    inkRGB: 0x000000,
                    secondaryRGB: 0x404040
                )
            }
        }
        switch base {
        case .white:
            return ResolvedReaderPalette(
                pageHex: "#ffffff",
                inkHex: "#1a1a1a",
                secondaryHex: "#767676",
                pageRGB: 0xFFFFFF,
                inkRGB: 0x1A1A1A,
                secondaryRGB: 0x767676
            )
        case .paper, .automatic:
            return ResolvedReaderPalette(
                pageHex: "#f4ecd8",
                inkHex: "#3b2f21",
                secondaryHex: "#736858",
                pageRGB: 0xF4ECD8,
                inkRGB: 0x3B2F21,
                secondaryRGB: 0x736858
            )
        case .grey:
            return ResolvedReaderPalette(
                pageHex: "#2b2b2e",
                inkHex: "#d6d6d8",
                secondaryHex: "#929294",
                pageRGB: 0x2B2B2E,
                inkRGB: 0xD6D6D8,
                secondaryRGB: 0x929294
            )
        case .black:
            return ResolvedReaderPalette(
                pageHex: "#000000",
                inkHex: "#c9c9c9",
                secondaryHex: "#797979",
                pageRGB: 0x000000,
                inkRGB: 0xC9C9C9,
                secondaryRGB: 0x797979
            )
        }
    }

    public static func footerText(
        chapterIndex: UInt32,
        chapterCount: Int,
        chapterTitle: String?,
        page: UInt32,
        pages: UInt32,
        partial: Bool
    ) -> (leading: String, trailing: String) {
        let chapterNumber = Int(chapterIndex) + 1
        let leading = chapterTitle?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
            ? chapterTitle!
            : "Chapter \(chapterNumber) of \(max(1, chapterCount))"
        var trailing = "Chapter \(chapterNumber) of \(max(1, chapterCount)) · Page \(page + 1) of \(max(1, pages))"
        if partial {
            trailing += " · Part of this chapter could not be displayed"
        }
        return (leading, trailing)
    }
}

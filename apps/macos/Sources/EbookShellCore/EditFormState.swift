import Foundation

/// Editable form state for book metadata and filing fields.
public struct EditFormState: Equatable, Sendable {
    public var title: String
    public var authorsText: String
    public var sortTitle: String
    public var authorSortsText: String
    public var seriesName: String
    public var seriesPosition: String

    public init(
        title: String,
        authors: [String],
        sortTitle: String?,
        authorSorts: [String?],
        primarySeriesName: String?,
        primarySeriesPosition: String?
    ) {
        self.title = title
        self.authorsText = authors.joined(separator: "\n")
        self.sortTitle = sortTitle ?? ""
        let lines = (0 ..< authors.count).map { index in
            index < authorSorts.count ? (authorSorts[index] ?? "") : ""
        }
        var joined = lines.joined(separator: "\n")
        while joined.hasSuffix("\n") {
            joined.removeLast()
        }
        self.authorSortsText = joined
        self.seriesName = primarySeriesName ?? ""
        self.seriesPosition = primarySeriesPosition ?? ""
    }

    public var isValid: Bool {
        !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    public var normalizedTitle: String {
        title.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    public var normalizedAuthors: [String] {
        authorsText
            .components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
    }

    public var normalizedSortTitle: String? {
        let trimmed = sortTitle.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    public var normalizedAuthorSorts: [String?] {
        if authorSortsText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return []
        }
        return authorSortsText
            .components(separatedBy: .newlines)
            .map { line in
                let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
                return trimmed.isEmpty ? nil : trimmed
            }
    }

    public var normalizedSeriesName: String? {
        let trimmed = seriesName.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    public var normalizedSeriesPosition: String? {
        let trimmed = seriesPosition.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }
}

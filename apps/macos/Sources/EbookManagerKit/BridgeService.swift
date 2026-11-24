import EbookBridge
import Foundation

extension BookDetail: Identifiable {}

extension BridgeError {
    public var userMessage: String {
        switch self {
        case let .Locked(_, _, message),
             let .PublicationPending(_, message),
             let .InvalidInput(message),
             let .Failed(message):
            return message
        }
    }
}

/// Dispatches all blocking `ebook-bridge` operations onto dedicated background queues off the main actor.
public final class BridgeService: @unchecked Sendable {
    private let writeQueue = DispatchQueue(label: "io.github.endqwerty.EbookManager.write", qos: .userInitiated)
    private let readQueue = DispatchQueue(label: "io.github.endqwerty.EbookManager.read", qos: .userInitiated, attributes: .concurrent)
    private let lock = NSLock()
    private var libraryInstance: Library?
    public let directory: String

    public init(directory: String) {
        self.directory = directory
    }

    private func ensureLibrary() throws -> Library {
        lock.lock()
        if let existing = libraryInstance {
            lock.unlock()
            return existing
        }
        lock.unlock()

        let opened = try Library.open(directory: directory)
        lock.lock()
        if let raced = libraryInstance {
            lock.unlock()
            return raced
        }
        libraryInstance = opened
        lock.unlock()
        return opened
    }

    private func runWrite<T: Sendable>(_ work: @Sendable @escaping (Library) throws -> T) async throws -> T {
        try await withCheckedThrowingContinuation { continuation in
            writeQueue.async {
                do {
                    let lib = try self.ensureLibrary()
                    let value = try work(lib)
                    continuation.resume(returning: value)
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    private func runRead<T: Sendable>(_ work: @Sendable @escaping (Library) throws -> T) async throws -> T {
        try await withCheckedThrowingContinuation { continuation in
            readQueue.async {
                do {
                    let lib = try self.ensureLibrary()
                    let value = try work(lib)
                    continuation.resume(returning: value)
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    public func browse(query: String, sort: [SortRule]) async throws -> BrowseSnapshot {
        try await runRead { lib in
            try lib.browse(query: query, sort: sort)
        }
    }

    public func browseWindow(viewId: UInt64, start: UInt32, limit: UInt32) async throws -> [BookRow] {
        try await runRead { lib in
            try lib.browseWindow(viewId: viewId, start: start, limit: limit)
        }
    }

    public func browseChanged() async throws -> Bool {
        try await runRead { lib in
            try lib.browseChanged()
        }
    }

    public func importPaths(_ paths: [String]) async throws -> [ImportItemResult] {
        try await runWrite { lib in
            lib.importPaths(paths: paths)
        }
    }

    public func settleReview(token: String, choice: ReviewChoice) async throws -> BookDetail? {
        try await runWrite { lib in
            try lib.settleReview(reviewToken: token, choice: choice)
        }
    }

    public func book(id: String) async throws -> BookDetail {
        try await runRead { lib in
            try lib.book(bookId: id)
        }
    }

    public func editBook(id: String, edit: BookEdit) async throws -> BookDetail {
        try await runWrite { lib in
            try lib.editBook(bookId: id, edit: edit)
        }
    }

    public func coverThumbnail(bookId: String) async throws -> Data? {
        try await runRead { lib in
            try lib.coverThumbnail(bookId: bookId)
        }
    }

    public func surveyUnsurveyed(limit: UInt32) async throws -> UInt32 {
        try await runWrite { lib in
            try lib.surveyUnsurveyed(limit: limit)
        }
    }

    public func exportBook(bookId: String, destinationDir: String) async throws -> ExportStatus {
        try await runWrite { lib in
            try lib.exportBook(bookId: bookId, destinationDir: destinationDir)
        }
    }

    public func refresh() async throws {
        try await runWrite { lib in
            try lib.refresh()
        }
    }

    public func openBook(bookId: String) async throws -> OpenBookHandle {
        let openBook = try await runRead { lib in
            try lib.openBook(bookId: bookId)
        }
        return OpenBookHandle(book: openBook)
    }
}

/// Async wrapper around an `OpenBook` session keeping every helper call off the main actor.
public final class OpenBookHandle: @unchecked Sendable {
    private let book: OpenBook
    private let queue = DispatchQueue(label: "io.github.endqwerty.EbookManager.openbook", qos: .userInitiated, attributes: .concurrent)
    private let saveQueue = DispatchQueue(label: "io.github.endqwerty.EbookManager.openbook.save", qos: .utility)

    public let bookId: String
    public let versionId: String
    public let title: String
    public let chapters: [ChapterSummary]
    public let toc: [TocItem]

    init(book: OpenBook) {
        self.book = book
        self.bookId = book.bookId()
        self.versionId = book.versionId()
        self.title = book.title()
        self.chapters = book.chapters()
        self.toc = book.toc()
    }

    public func readContent(chapter: UInt32) async throws -> ChapterMarkup {
        try await withCheckedThrowingContinuation { continuation in
            queue.async {
                do {
                    continuation.resume(returning: try self.book.readContent(chapter: chapter))
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    public func readResource(id: UInt32) async -> BookResource? {
        await withCheckedContinuation { continuation in
            queue.async {
                continuation.resume(returning: try? self.book.readResource(id: id))
            }
        }
    }

    public func loadPlace() async throws -> ReadingPlace? {
        try await withCheckedThrowingContinuation { continuation in
            saveQueue.async {
                do {
                    continuation.resume(returning: try self.book.loadPlace())
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    public func savePlace(chapter: UInt32, offset: UInt32, page: UInt32, pages: UInt32) async {
        await withCheckedContinuation { continuation in
            saveQueue.async {
                try? self.book.savePlace(chapter: chapter, offset: offset, page: page, pages: pages)
                continuation.resume()
            }
        }
    }

    public func flushSaveSynchronously(chapter: UInt32, offset: UInt32, page: UInt32, pages: UInt32) {
        saveQueue.sync {
            try? self.book.savePlace(chapter: chapter, offset: offset, page: page, pages: pages)
        }
    }
}

import AppKit
import EbookBridge
import EbookShellCore
import Foundation
import UniformTypeIdentifiers

public enum BrowseLayoutMode: String, CaseIterable, Sendable {
    case grid
    case list

    public var label: String {
        switch self {
        case .grid: return "Grid"
        case .list: return "List"
        }
    }

    public var symbolName: String {
        switch self {
        case .grid: return "square.grid.2x2"
        case .list: return "list.bullet"
        }
    }
}

extension SortField {
    public static let menuFields: [SortField] = [
        .title, .author, .series, .added, .published, .lastRead,
    ]

    public var label: String {
        switch self {
        case .title: return "Title"
        case .author: return "Author"
        case .series: return "Series"
        case .added: return "Date Added"
        case .published: return "Date Published"
        case .lastRead: return "Last Read"
        case .progress: return "Progress"
        }
    }
}

public struct PendingReviewPrompt: Identifiable, Equatable, Sendable {
    public var id: String { reviewToken }
    public let sourceName: String
    public let reviewToken: String
    public let title: String
    public let authors: [String]
    public let published: String?
    public let words: UInt32?
    public let candidates: [ReviewCandidate]
}

public struct ImportReportModel: Identifiable, Equatable, Sendable {
    public let id = UUID()
    public var items: [ImportItemResult]
    public var pendingReviews: [PendingReviewPrompt]
}

public struct LockedAlertModel: Identifiable, Equatable {
    public let id = UUID()
    public let heading: String
    public let body: String
}

public struct BrowseScrollRequest: Equatable, Sendable {
    public let rowId: String
    public let serial: UInt64
}

/// Rows to load so a reloaded view still holds the row the reader was looking at.
public enum BrowseRestorePlan {
    public static func rowsToLoad(anchorRank: UInt32?, previouslyLoaded: Int, total: UInt32, pageSize: UInt32) -> UInt32 {
        let wanted: UInt32
        if let anchorRank {
            wanted = anchorRank + 1
        } else {
            wanted = UInt32(clamping: previouslyLoaded)
        }
        let pages = (wanted + pageSize - 1) / pageSize
        return min(total, max(pageSize, pages * pageSize))
    }
}

public struct SimpleNoticeModel: Identifiable, Equatable {
    public let id = UUID()
    public let heading: String
    public let body: String
}

/// Main-actor view model coordinating library browsing, imports, reviews, editing, exports, and reader windows.
@MainActor
public final class LibraryViewModel: ObservableObject {
    public enum Phase: Equatable {
        case loading
        case showing
        case error(String)
    }

    @Published public var phase: Phase = .loading
    @Published public var showLoadingIndicator = false
    @Published public var viewMode: BrowseLayoutMode = .grid
    @Published public var coverSize: CoverSizeChoice = .medium
    @Published public var sortField: SortField = .title
    @Published public var sortDescending: Bool = false
    @Published public var searchText: String = "" {
        didSet {
            if oldValue != searchText {
                scheduleBrowseReload()
            }
        }
    }

    @Published public private(set) var totalCount: UInt32 = 0
    @Published public private(set) var rows: [BookRow] = []
    @Published public var selectedIds: Set<String> = []
    @Published public private(set) var isBusy = false
    @Published public var importReport: ImportReportModel? {
        didSet {
            let kept = Set(importReport?.pendingReviews.map(\.reviewToken) ?? [])
            let dropped = (oldValue?.pendingReviews ?? []).map(\.reviewToken).filter { !kept.contains($0) }
            if importReport == nil {
                activeReview = nil
            }
            if !dropped.isEmpty {
                let service = bridge
                Task { try? await service.discardReviews(tokens: dropped) }
            }
        }
    }
    @Published public var activeReview: PendingReviewPrompt?
    @Published public var inspectingBook: BookDetail?
    @Published public var editingBook: BookDetail?
    @Published public var showQuickJump = false
    @Published public var lockedAlert: LockedAlertModel?
    @Published public var noticeAlert: SimpleNoticeModel?
    @Published public private(set) var scrollRequest: BrowseScrollRequest?

    public let bridge: BridgeService
    public let coverCache: CoverCache

    private var activeViewId: UInt64 = 0
    private var loadedPages: Set<UInt32> = []
    private var loadingPages: Set<UInt32> = []
    private var pendingBrowse: Task<Void, Never>?
    private var browseSerial: UInt64 = 0
    private var searchDebounceTask: Task<Void, Never>?
    private var openingBookIds: Set<String> = []
    private var openReaderControllers: [String: ReaderWindowCoordinator] = [:]
    private var visibleRowIds: Set<String> = []
    private var shownQuery: (text: String, sort: [SortRule])?
    private var scrollSerial: UInt64 = 0
    private var surveyReloadTask: Task<Void, Never>?
    private lazy var surveyRunner: SurveyRunner = {
        let service = bridge
        return SurveyRunner(
            step: { skip in try await service.surveyUnsurveyed(limit: 16, skip: skip) },
            onProgress: { [weak self] in self?.scheduleSurveyReload() }
        )
    }()

    private static let pageSize: UInt32 = 128

    nonisolated static let publicationPendingNote =
        "Saved in your library. Its backup record will be written when storage is available. To try again, choose Refresh Library."

    public init(libraryDirectory: String = LibraryLocation.resolve()) {
        let service = BridgeService(directory: libraryDirectory)
        self.bridge = service
        self.coverCache = CoverCache(bridge: service)
    }

    public func start() {
        reloadBrowse()
        runBackgroundSurvey()
    }

    public func retryAfterError() {
        reloadBrowse()
        runBackgroundSurvey()
    }

    public func rowAppeared(index: Int, id: String) {
        visibleRowIds.insert(id)
        loadMoreIfNeeded(forAppearIndex: index)
    }

    public func rowDisappeared(id: String) {
        visibleRowIds.remove(id)
    }

    private func topVisibleRow() -> (id: String, index: Int)? {
        guard !visibleRowIds.isEmpty else { return nil }
        for (index, row) in rows.enumerated() where visibleRowIds.contains(row.id) {
            return (row.id, index)
        }
        return nil
    }

    public func setSort(field: SortField) {
        if sortField != field {
            sortField = field
            reloadBrowse()
        }
    }

    public func setSortDirection(descending: Bool) {
        if sortDescending != descending {
            sortDescending = descending
            reloadBrowse()
        }
    }

    private func scheduleBrowseReload() {
        searchDebounceTask?.cancel()
        searchDebounceTask = Task { @MainActor [weak self] in
            try? await Task.sleep(nanoseconds: 90_000_000)
            guard !Task.isCancelled else { return }
            self?.reloadBrowse()
        }
    }

    @discardableResult
    public func reloadBrowse() -> Task<Void, Never> {
        browseSerial &+= 1
        let serial = browseSerial
        let query = searchText
        let rules = [SortRule(field: sortField, descending: sortDescending)]
        let sameView = shownQuery.map { $0.text == query && $0.sort == rules } ?? false
        let anchor = sameView ? topVisibleRow() : nil
        let previouslyLoaded = sameView ? rows.count : 0
        let selection = Array(selectedIds)

        if rows.isEmpty {
            phase = .loading
            showLoadingIndicator = false
            Task { @MainActor [weak self] in
                try? await Task.sleep(nanoseconds: 300_000_000)
                if let self, self.browseSerial == serial, self.phase == .loading {
                    self.showLoadingIndicator = true
                }
            }
        }

        let previous = pendingBrowse
        let task = Task {
            await previous?.value
            guard serial == browseSerial else { return }
            do {
                let snapshot = try await bridge.browse(query: query, sort: rules)
                guard serial == browseSerial else { return }
                var loaded = snapshot.rows
                let complete = snapshot.count <= UInt32(snapshot.rows.count)
                var goneFromView: Set<String> = []
                var anchorRank: UInt32?
                if !complete {
                    let wanted = (anchor.map { [$0.id] } ?? []) + selection
                    if !wanted.isEmpty,
                       let ranks = try? await bridge.browseLocate(viewId: snapshot.viewId, ids: wanted)
                    {
                        if anchor != nil {
                            anchorRank = ranks.first ?? nil
                        }
                        let selectionRanks = ranks.dropFirst(anchor == nil ? 0 : 1)
                        for (id, rank) in zip(selection, selectionRanks) where rank == nil {
                            goneFromView.insert(id)
                        }
                    }
                    let target = BrowseRestorePlan.rowsToLoad(
                        anchorRank: anchorRank,
                        previouslyLoaded: previouslyLoaded,
                        total: snapshot.count,
                        pageSize: Self.pageSize
                    )
                    while UInt32(loaded.count) < target {
                        let start = UInt32(loaded.count)
                        guard let more = try? await bridge.browseWindow(
                            viewId: snapshot.viewId,
                            start: start,
                            limit: min(Self.pageSize, target - start)
                        ), !more.isEmpty else {
                            break
                        }
                        loaded.append(contentsOf: more)
                    }
                    guard serial == browseSerial else { return }
                }
                activeViewId = snapshot.viewId
                totalCount = snapshot.count
                let oldIndex = anchor?.index
                rows = loaded
                shownQuery = (query, rules)
                let fullPages = UInt32(loaded.count) / Self.pageSize
                loadedPages = Set(0 ..< max(1, fullPages))
                loadingPages.removeAll()
                if complete {
                    selectedIds.formIntersection(loaded.map(\.id))
                } else {
                    selectedIds.subtract(goneFromView)
                }
                if let anchor, let newIndex = loaded.firstIndex(where: { $0.id == anchor.id }), newIndex != oldIndex {
                    scrollSerial &+= 1
                    scrollRequest = BrowseScrollRequest(rowId: anchor.id, serial: scrollSerial)
                }
                phase = .showing
                showLoadingIndicator = false
            } catch let error as BridgeError {
                guard serial == browseSerial else { return }
                rows = []
                totalCount = 0
                shownQuery = nil
                selectedIds.removeAll()
                phase = .error(error.userMessage)
                showLoadingIndicator = false
            } catch {
                guard serial == browseSerial else { return }
                rows = []
                totalCount = 0
                shownQuery = nil
                selectedIds.removeAll()
                phase = .error(error.localizedDescription)
                showLoadingIndicator = false
            }
        }
        pendingBrowse = task
        return task
    }

    public func loadMoreIfNeeded(forAppearIndex index: Int) {
        guard UInt32(rows.count) < totalCount, index + 24 >= rows.count else { return }
        let nextPage = UInt32(rows.count) / Self.pageSize
        guard !loadedPages.contains(nextPage), !loadingPages.contains(nextPage) else { return }
        let viewId = activeViewId
        let start = nextPage * Self.pageSize
        loadingPages.insert(nextPage)
        Task {
            defer { loadingPages.remove(nextPage) }
            guard let nextRows = try? await bridge.browseWindow(viewId: viewId, start: start, limit: Self.pageSize),
                  viewId == activeViewId
            else {
                return
            }
            loadedPages.insert(nextPage)
            let existing = Set(rows.map(\.id))
            for item in nextRows where !existing.contains(item.id) {
                rows.append(item)
            }
        }
    }

    @discardableResult
    func runBackgroundSurvey() -> Task<Void, Never> {
        surveyRunner.start()
    }

    private func scheduleSurveyReload() {
        guard surveyReloadTask == nil else { return }
        surveyReloadTask = Task { @MainActor [weak self] in
            try? await Task.sleep(nanoseconds: 750_000_000)
            guard let self else { return }
            self.surveyReloadTask = nil
            self.reloadBrowse()
        }
    }

    public func refreshLibrary() {
        Task {
            isBusy = true
            defer { isBusy = false }
            do {
                try await bridge.refresh()
                reloadBrowse()
                runBackgroundSurvey()
            } catch let err as BridgeError {
                noticeAlert = SimpleNoticeModel(heading: "Couldn’t Refresh Library", body: err.userMessage)
            } catch {
                noticeAlert = SimpleNoticeModel(heading: "Couldn’t Refresh Library", body: error.localizedDescription)
            }
        }
    }

    public func presentAddFilesPanel() {
        let panel = NSOpenPanel()
        panel.title = "Add Books"
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = true
        if let epubType = UTType(filenameExtension: "epub") {
            panel.allowedContentTypes = [epubType]
        }
        if panel.runModal() == .OK {
            importURLs(panel.urls)
        }
    }

    public func presentAddFolderPanel() {
        let panel = NSOpenPanel()
        panel.title = "Add Folder"
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = true
        if panel.runModal() == .OK {
            importURLs(panel.urls)
        }
    }

    public func importURLs(_ urls: [URL]) {
        let paths = urls.map(\.path).filter { !$0.isEmpty }
        guard !paths.isEmpty else { return }
        Task {
            isBusy = true
            defer { isBusy = false }
            do {
                let results = try await bridge.importPaths(paths)
                var reviews: [PendingReviewPrompt] = []
                for item in results {
                    if case let .needsReview(reviewToken, title, authors, published, words, candidates) = item.outcome {
                        reviews.append(PendingReviewPrompt(
                            sourceName: item.sourceName,
                            reviewToken: reviewToken,
                            title: title,
                            authors: authors,
                            published: published,
                            words: words,
                            candidates: candidates
                        ))
                    }
                }
                reloadBrowse()
                let report = ImportReportModel(items: results, pendingReviews: reviews)
                importReport = report
                activeReview = reviews.first
            } catch let err as BridgeError {
                noticeAlert = SimpleNoticeModel(heading: "Couldn’t Import Books", body: err.userMessage)
            } catch {
                noticeAlert = SimpleNoticeModel(heading: "Couldn’t Import Books", body: error.localizedDescription)
            }
        }
    }

    public func settle(review: PendingReviewPrompt, choice: ReviewChoice) {
        Task {
            isBusy = true
            defer { isBusy = false }
            do {
                let settled = try await bridge.settleReview(token: review.reviewToken, choice: choice)
                if var report = importReport {
                    report.pendingReviews.removeAll { $0.reviewToken == review.reviewToken }
                    if let idx = report.items.firstIndex(where: {
                        if case let .needsReview(tok, _, _, _, _, _) = $0.outcome {
                            return tok == review.reviewToken
                        }
                        return false
                    }) {
                        report.items[idx] = ImportItemResult(
                            sourceName: report.items[idx].sourceName,
                            outcome: settled.outcome,
                            publicationPending: settled.publicationPending
                        )
                    }
                    importReport = report
                    activeReview = report.pendingReviews.first
                } else {
                    activeReview = nil
                }
                reloadBrowse()
            } catch let err as BridgeError {
                noticeAlert = SimpleNoticeModel(heading: "Couldn’t Settle Import", body: err.userMessage)
            } catch {
                noticeAlert = SimpleNoticeModel(heading: "Couldn’t Settle Import", body: error.localizedDescription)
            }
        }
    }

    public func openSelectedForReading() {
        guard selectedIds.count == 1, let id = selectedIds.first else { return }
        openBookForReading(bookId: id)
    }

    public func openBookForReading(bookId: String) {
        if let existing = openReaderControllers[bookId] {
            existing.bringToFront()
            return
        }
        if let row = rows.first(where: { $0.id == bookId }),
           let protection = row.protection
        {
            let notice = lockedBookSummary(title: row.title, protection: protection)
            lockedAlert = LockedAlertModel(heading: notice.heading, body: notice.body)
            return
        }
        guard openingBookIds.insert(bookId).inserted else { return }
        Task { [weak self] in
            guard let self else { return }
            defer { self.openingBookIds.remove(bookId) }
            do {
                let handle = try await self.bridge.openBook(bookId: bookId)
                let saved = try? await handle.loadPlace()
                guard self.openingBookIds.contains(bookId) else { return }
                let coordinator = ReaderWindowCoordinator(
                    handle: handle,
                    initialPlace: saved,
                    onSaveError: { [weak self] message in
                        self?.noticeAlert = SimpleNoticeModel(heading: "Reading Position", body: message)
                    },
                    onClosed: { [weak self] closedId in
                        self?.openReaderControllers.removeValue(forKey: closedId)
                        self?.reloadBrowse()
                    }
                )
                self.openReaderControllers[bookId] = coordinator
                coordinator.present()
            } catch BridgeError.Locked {
                if let detail = try? await self.bridge.book(id: bookId),
                   let protection = detail.protection
                {
                    let notice = lockedBookSummary(title: detail.title, protection: protection)
                    self.lockedAlert = LockedAlertModel(heading: notice.heading, body: notice.body)
                } else {
                    let notice = lockedBookSummary(title: "this book", protection: .unknown)
                    self.lockedAlert = LockedAlertModel(heading: notice.heading, body: notice.body)
                }
                self.reloadBrowse()
            } catch let err as BridgeError {
                self.noticeAlert = SimpleNoticeModel(heading: "Couldn’t Open Book", body: err.userMessage)
            } catch {
                self.noticeAlert = SimpleNoticeModel(heading: "Couldn’t Open Book", body: error.localizedDescription)
            }
        }
    }

    public func openEditForSelected() {
        guard selectedIds.count == 1, let id = selectedIds.first else { return }
        openEdit(forBookId: id)
    }

    public func toggleDetailsForSelected() {
        if inspectingBook != nil {
            inspectingBook = nil
            return
        }
        guard selectedIds.count == 1, let id = selectedIds.first else { return }
        openDetails(forBookId: id)
    }

    public func openDetails(forBookId bookId: String) {
        Task {
            do {
                inspectingBook = try await bridge.book(id: bookId)
            } catch let err as BridgeError {
                noticeAlert = SimpleNoticeModel(heading: "Couldn’t Load Book Details", body: err.userMessage)
            } catch {
                noticeAlert = SimpleNoticeModel(heading: "Couldn’t Load Book Details", body: error.localizedDescription)
            }
        }
    }

    public func progressForBook(id: String) -> Float? {
        rows.first(where: { $0.id == id })?.progress
    }

    public func filterByQuery(_ query: String) {
        inspectingBook = nil
        showQuickJump = false
        searchText = query
    }

    public func openEdit(forBookId bookId: String) {
        Task {
            do {
                editingBook = try await bridge.book(id: bookId)
            } catch let err as BridgeError {
                noticeAlert = SimpleNoticeModel(heading: "Couldn’t Load Book Details", body: err.userMessage)
            } catch {
                noticeAlert = SimpleNoticeModel(heading: "Couldn’t Load Book Details", body: error.localizedDescription)
            }
        }
    }

    public func saveBookEdit(bookId: String, form: EditFormState) async -> String? {
        let edit = BookEdit(
            title: form.normalizedTitle,
            authors: form.normalizedAuthors,
            sortTitle: form.normalizedSortTitle,
            authorSorts: form.normalizedAuthorSorts,
            seriesName: form.normalizedSeriesName,
            seriesPosition: form.normalizedSeriesPosition
        )
        do {
            let saved = try await bridge.editBook(id: bookId, edit: edit)
            editingBook = nil
            reloadBrowse()
            if saved.publicationPending {
                noticeAlert = SimpleNoticeModel(heading: "Changes Saved Locally", body: Self.publicationPendingNote)
            }
            return nil
        } catch let err as BridgeError {
            return err.userMessage
        } catch {
            return error.localizedDescription
        }
    }

    public func exportSelectedBooks() {
        let ids = Array(selectedIds)
        guard !ids.isEmpty else { return }
        exportBooks(ids: ids)
    }

    public func exportBooks(ids: [String]) {
        guard !ids.isEmpty else { return }
        let panel = NSOpenPanel()
        panel.title = "Send to Device Folder"
        panel.prompt = "Choose Folder"
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let folderURL = panel.url else { return }
        let destination = folderURL.path

        Task {
            isBusy = true
            defer { isBusy = false }
            var copiedCount = 0
            var existingCount = 0
            var lastPath = ""
            var failures: [String] = []

            for id in ids {
                do {
                    let result = try await bridge.exportBook(bookId: id, destinationDir: destination)
                    switch result {
                    case let .copied(path):
                        copiedCount += 1
                        lastPath = path
                    case let .alreadyPresent(path):
                        existingCount += 1
                        lastPath = path
                    }
                } catch let err as BridgeError {
                    failures.append(err.userMessage)
                } catch {
                    failures.append(error.localizedDescription)
                }
            }

            if ids.count == 1 {
                if let failure = failures.first {
                    noticeAlert = SimpleNoticeModel(heading: "Couldn’t Send Book", body: failure)
                } else if copiedCount == 1 {
                    noticeAlert = SimpleNoticeModel(heading: "Book Sent", body: "Written to \(lastPath).")
                } else {
                    noticeAlert = SimpleNoticeModel(heading: "Already on Device", body: "An identical copy is already at \(lastPath).")
                }
            } else {
                var parts: [String] = []
                if copiedCount > 0 { parts.append("\(copiedCount) sent") }
                if existingCount > 0 { parts.append("\(existingCount) already present") }
                if !failures.isEmpty { parts.append("\(failures.count) failed (\(failures[0]))") }
                noticeAlert = SimpleNoticeModel(
                    heading: "Device Export Finished",
                    body: parts.joined(separator: " · ")
                )
            }
        }
    }

    public func flushAndCloseAllReaders() {
        openingBookIds.removeAll()
        let controllers = Array(openReaderControllers.values)
        openReaderControllers.removeAll()
        for controller in controllers {
            controller.flushAndClose()
        }
    }
}

import AppKit
import EbookBridge
import EbookShellCore
import SwiftUI
import UniformTypeIdentifiers

/// Main single-column library window for macOS.
public struct LibraryWindowView: View {
    @ObservedObject var model: LibraryViewModel

    public init(model: LibraryViewModel) {
        self.model = model
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            contentArea
        }
        .navigationTitle("")
        .searchable(text: $model.searchText, placement: .toolbar, prompt: "Search Books")
        .toolbar {
            ToolbarItemGroup(placement: .navigation) {
                Picker("View", selection: $model.viewMode) {
                    ForEach(BrowseLayoutMode.allCases, id: \.self) { mode in
                        Label(mode.label, systemImage: mode.symbolName)
                            .tag(mode)
                    }
                }
                .pickerStyle(.segmented)
                .help("View")

                Menu {
                    Section("Cover Size") {
                        ForEach(CoverSizeChoice.allCases, id: \.self) { choice in
                            Button {
                                model.coverSize = choice
                            } label: {
                                if model.coverSize == choice {
                                    Label(choice.label, systemImage: "checkmark")
                                } else {
                                    Text(choice.label)
                                }
                            }
                        }
                    }
                } label: {
                    Label("Cover Size", systemImage: "textformat.size")
                }
                .help("Cover Size")

                Menu {
                    Section("Sort By") {
                        ForEach(SortField.menuFields, id: \.self) { field in
                            Button {
                                model.setSort(field: field)
                            } label: {
                                if model.sortField == field {
                                    Label(field.label, systemImage: "checkmark")
                                } else {
                                    Text(field.label)
                                }
                            }
                        }
                    }
                    Section("Order") {
                        Button {
                            model.setSortDirection(descending: false)
                        } label: {
                            if !model.sortDescending {
                                Label("Ascending", systemImage: "checkmark")
                            } else {
                                Text("Ascending")
                            }
                        }
                        Button {
                            model.setSortDirection(descending: true)
                        } label: {
                            if model.sortDescending {
                                Label("Descending", systemImage: "checkmark")
                            } else {
                                Text("Descending")
                            }
                        }
                    }
                } label: {
                    Label("Sort", systemImage: "arrow.up.arrow.down")
                }
                .help("Sort")
            }

            ToolbarItemGroup(placement: .primaryAction) {
                if model.isBusy {
                    ProgressView()
                        .controlSize(.small)
                        .help("Working…")
                }

                prominentAddMenu
            }
        }
        .onDrop(of: [.fileURL], isTargeted: nil) { providers in
            handleDroppedProviders(providers)
        }
        .sheet(item: $model.importReport) { report in
            ImportReportSheet(
                report: report,
                onReview: { prompt in
                    model.activeReview = prompt
                },
                onDone: {
                    model.importReport = nil
                }
            )
            .sheet(item: $model.activeReview) { prompt in
                ReviewSheet(prompt: prompt) { decision in
                    model.settle(review: prompt, choice: decision)
                }
            }
        }
        .sheet(item: $model.inspectingBook) { book in
            BookDetailsSheet(
                book: book,
                progress: model.progressForBook(id: book.id),
                cache: model.coverCache,
                onRead: {
                    let id = book.id
                    model.inspectingBook = nil
                    model.openBookForReading(bookId: id)
                },
                onSend: {
                    let id = book.id
                    model.inspectingBook = nil
                    model.exportBooks(ids: [id])
                },
                onEdit: {
                    model.inspectingBook = nil
                    model.editingBook = book
                },
                onFilter: { query in
                    model.filterByQuery(query)
                },
                onClose: {
                    model.inspectingBook = nil
                }
            )
        }
        .sheet(isPresented: $model.showQuickJump) {
            QuickJumpSheet(
                bridge: model.bridge,
                onChooseFilter: { query in
                    model.filterByQuery(query)
                },
                onChooseBookDetails: { id in
                    model.showQuickJump = false
                    model.selectedIds = [id]
                    model.openDetails(forBookId: id)
                },
                onChooseBookRead: { id in
                    model.showQuickJump = false
                    model.selectedIds = [id]
                    model.openBookForReading(bookId: id)
                },
                onCancel: {
                    model.showQuickJump = false
                }
            )
        }
        .sheet(item: $model.editingBook) { book in
            BookEditSheet(
                book: book,
                cache: model.coverCache,
                onSave: { form in
                    await model.saveBookEdit(bookId: book.id, form: form)
                },
                onCancel: {
                    model.editingBook = nil
                }
            )
        }
        .alert(
            model.lockedAlert?.heading ?? "Can’t Open Book",
            isPresented: Binding(
                get: { model.lockedAlert != nil },
                set: { if !$0 { model.lockedAlert = nil } }
            ),
            presenting: model.lockedAlert
        ) { _ in
            Button("Close", role: .cancel) {
                model.lockedAlert = nil
            }
        } message: { alert in
            Text(alert.body)
        }
        .alert(
            model.noticeAlert?.heading ?? "Notice",
            isPresented: Binding(
                get: { model.noticeAlert != nil },
                set: { if !$0 { model.noticeAlert = nil } }
            ),
            presenting: model.noticeAlert
        ) { _ in
            Button("Close", role: .cancel) {
                model.noticeAlert = nil
            }
        } message: { alert in
            Text(alert.body)
        }
    }

    @ViewBuilder
    private var prominentAddMenu: some View {
        Menu {
            Button("Add Files…") {
                model.presentAddFilesPanel()
            }
            Button("Add Folder…") {
                model.presentAddFolderPanel()
            }
        } label: {
            Label("Add", systemImage: "plus")
        } primaryAction: {
            model.presentAddFilesPanel()
        }
        .applyProminentToolbarStyle()
    }

    @ViewBuilder
    private var contentArea: some View {
        switch model.phase {
        case .loading:
            if model.showLoadingIndicator {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                Color.clear
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }

        case let .error(message):
            ContentUnavailableView {
                Text("Couldn’t Open Library")
                    .font(.system(.title, design: .serif).weight(.semibold))
            } description: {
                Text(message)
            } actions: {
                Button("Try Again") {
                    model.retryAfterError()
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)

        case .showing:
            if model.totalCount == 0, model.searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                emptyLibraryState
            } else if model.totalCount == 0 {
                noMatchesState
            } else {
                switch model.viewMode {
                case .grid:
                    gridView
                case .list:
                    listView
                }
            }
        }
    }

    private var screenHeader: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                Text("All Books")
                    .font(.system(.largeTitle, design: .serif).weight(.semibold))
                Text(model.totalCount.formatted())
                    .font(.title3.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
            let trimmedSearch = model.searchText.trimmingCharacters(in: .whitespacesAndNewlines)
            if !trimmedSearch.isEmpty {
                Text("\(model.totalCount.formatted()) \(model.totalCount == 1 ? "book matches" : "books match") \(trimmedSearch)")
                    .font(.callout.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var emptyLibraryState: some View {
        ContentUnavailableView {
            Text("No books yet")
                .font(.system(.title, design: .serif).weight(.semibold))
        } description: {
            Text("Add EPUB files or a folder to start your library.")
        } actions: {
            Menu {
                Button("Add Files…") {
                    model.presentAddFilesPanel()
                }
                Button("Add Folder…") {
                    model.presentAddFolderPanel()
                }
            } label: {
                Text("Add")
            } primaryAction: {
                model.presentAddFilesPanel()
            }
            .buttonStyle(.borderedProminent)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var noMatchesState: some View {
        ContentUnavailableView {
            Text("No books match")
                .font(.system(.title, design: .serif).weight(.semibold))
        } description: {
            Text("Nothing matched “\(model.searchText.trimmingCharacters(in: .whitespacesAndNewlines))”.")
        } actions: {
            Button("Clear Search") {
                model.searchText = ""
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var gridView: some View {
        GeometryReader { proxy in
            let horizontalInset: CGFloat = 24
            let innerWidth = max(96, proxy.size.width - horizontalInset * 2)
            let metrics = CoverStyle.gridMetrics(availableWidth: innerWidth, choice: model.coverSize)
            let columns = Array(
                repeating: GridItem(.fixed(metrics.itemWidth), spacing: metrics.columnSpacing, alignment: .topLeading),
                count: metrics.columns
            )

            ScrollViewReader { scroller in
                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        screenHeader
                            .padding(.top, 16)

                        LazyVGrid(columns: columns, alignment: .leading, spacing: metrics.rowSpacing) {
                            ForEach(Array(model.rows.enumerated()), id: \.element.id) { index, row in
                                gridCell(row: row, width: metrics.itemWidth)
                                    .onAppear {
                                        model.rowAppeared(index: index, id: row.id)
                                    }
                                    .onDisappear {
                                        model.rowDisappeared(id: row.id)
                                    }
                            }
                        }
                    }
                    .padding(.horizontal, horizontalInset)
                    .padding(.bottom, 28)
                }
                .onChange(of: model.scrollRequest) { _, request in
                    if let request {
                        scroller.scrollTo(request.rowId, anchor: .top)
                    }
                }
            }
        }
        .onKeyPress(.return) {
            model.openSelectedForReading()
            return .handled
        }
        .onKeyPress(.space) {
            model.toggleDetailsForSelected()
            return .handled
        }
    }

    private func gridCell(row: BookRow, width: CGFloat) -> some View {
        let isSelected = model.selectedIds.contains(row.id)
        return VStack(alignment: .leading, spacing: 0) {
            BookCoverView(
                bookId: row.id,
                title: row.title,
                authors: row.authors,
                hasCover: row.hasCover,
                coverDigest: row.coverDigest,
                coverRGB: row.coverRgb,
                rightToLeft: row.rightToLeft,
                progress: row.progress,
                width: width,
                kind: .grid,
                cache: model.coverCache
            )

            VStack(alignment: .leading, spacing: 2) {
                Text(row.title)
                    .font(.body.weight(.semibold))
                    .foregroundStyle(.primary)
                    .lineLimit(2)
                    .truncationMode(.tail)
                    .help(row.title)

                HStack(spacing: 4) {
                    Text(row.authors)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    Spacer(minLength: 0)
                    trailingBadge(for: row)
                }
                Spacer(minLength: 0)
            }
            .frame(width: width, height: 54, alignment: .topLeading)
        }
        .padding(.vertical, 6)
        .background(
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .fill(isSelected ? Color.accentColor.opacity(0.18) : Color.clear)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .strokeBorder(isSelected ? Color.accentColor : Color.clear, lineWidth: 1.5)
        )
        .contentShape(Rectangle())
        .simultaneousGesture(
            TapGesture(count: 2).onEnded {
                model.selectedIds = [row.id]
                model.openBookForReading(bookId: row.id)
            }
        )
        .simultaneousGesture(
            TapGesture(count: 1).onEnded {
                if NSEvent.modifierFlags.contains(.command) {
                    if model.selectedIds.contains(row.id) {
                        model.selectedIds.remove(row.id)
                    } else {
                        model.selectedIds.insert(row.id)
                    }
                } else {
                    model.selectedIds = [row.id]
                }
            }
        )
        .contextMenu {
            bookContextMenu(for: row)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(accessibleDescription(for: row))
    }

    private var listView: some View {
        VStack(alignment: .leading, spacing: 0) {
            screenHeader
                .padding(.horizontal, 24)
                .padding(.top, 16)
                .padding(.bottom, 10)

            ScrollViewReader { scroller in
                List(selection: $model.selectedIds) {
                    ForEach(Array(model.rows.enumerated()), id: \.element.id) { index, row in
                        listRow(row: row)
                            .tag(row.id)
                            .onAppear {
                                model.rowAppeared(index: index, id: row.id)
                            }
                            .onDisappear {
                                model.rowDisappeared(id: row.id)
                            }
                            .contextMenu {
                                bookContextMenu(for: row)
                            }
                    }
                }
                .listStyle(.inset)
                .onKeyPress(.space) {
                    guard model.selectedIds.count == 1 else { return .ignored }
                    model.toggleDetailsForSelected()
                    return .handled
                }
                .contextMenu(forSelectionType: String.self) { _ in
                } primaryAction: { ids in
                    if let first = ids.first {
                        model.openBookForReading(bookId: first)
                    }
                }
                .onChange(of: model.scrollRequest) { _, request in
                    if let request {
                        scroller.scrollTo(request.rowId, anchor: .top)
                    }
                }
            }
        }
    }

    private func listRow(row: BookRow) -> some View {
        let thumbWidth = model.coverSize.listWidth
        return HStack(alignment: .center, spacing: 12) {
            BookCoverView(
                bookId: row.id,
                title: row.title,
                authors: row.authors,
                hasCover: row.hasCover,
                coverDigest: row.coverDigest,
                coverRGB: row.coverRgb,
                rightToLeft: row.rightToLeft,
                progress: nil,
                width: thumbWidth,
                kind: .list,
                cache: model.coverCache
            )

            VStack(alignment: .leading, spacing: 2) {
                Text(row.title)
                    .font(.body.weight(.semibold))
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .help(row.title)

                Text(row.authors)
                    .font(.body)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)

                if let series = row.seriesName {
                    let seriesText = row.seriesPosition.map { "\(series) #\($0)" } ?? series
                    Text(seriesText)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                } else {
                    Text(" ")
                        .font(.caption)
                        .hidden()
                }
            }

            Spacer(minLength: 12)

            listTrailingSlot(for: row)
        }
        .padding(.vertical, 6)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(accessibleDescription(for: row))
    }

    @ViewBuilder
    private func trailingBadge(for row: BookRow) -> some View {
        if let protection = row.protection {
            Image(systemName: "lock.fill")
                .font(.caption2)
                .foregroundStyle(.secondary)
                .help(lockedBookSummary(title: row.title, protection: protection).body)
        } else if let progress = row.progress, progress >= 1.0 {
            Image(systemName: "checkmark")
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private func listTrailingSlot(for row: BookRow) -> some View {
        if let protection = row.protection {
            Image(systemName: "lock.fill")
                .font(.caption)
                .foregroundStyle(.secondary)
                .help(lockedBookSummary(title: row.title, protection: protection).body)
        } else if model.sortField == .added, let added = row.added {
            Text(Date(timeIntervalSince1970: TimeInterval(added)).formatted(date: .abbreviated, time: .omitted))
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
        } else if model.sortField == .published, let published = row.published {
            Text(published)
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
        } else if model.sortField == .lastRead, let lastRead = row.lastRead {
            Text(Date(timeIntervalSince1970: TimeInterval(lastRead)).formatted(date: .abbreviated, time: .omitted))
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
        } else if let progress = row.progress {
            if progress >= 1.0 {
                Image(systemName: "checkmark")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
            } else if progress > 0 {
                Text("\(Int((progress * 100).rounded()))%")
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder
    private func bookContextMenu(for row: BookRow) -> some View {
        let targetIds = model.selectedIds.contains(row.id) ? Array(model.selectedIds) : [row.id]
        if targetIds.count == 1 {
            Button("Read") {
                model.openBookForReading(bookId: row.id)
            }
            Divider()
            Button("Show Details") {
                model.openDetails(forBookId: row.id)
            }
            Button("Edit Book…") {
                model.openEdit(forBookId: row.id)
            }
        }
        Button("Send to Device Folder…") {
            model.exportBooks(ids: targetIds)
        }
    }

    private func accessibleDescription(for row: BookRow) -> String {
        var parts = [row.title]
        if !row.authors.isEmpty {
            parts.append(row.authors)
        }
        if let protection = row.protection {
            switch protection {
            case .lcp: parts.append("Locked by Readium LCP")
            case .adobe: parts.append("Locked by Adobe")
            case .apple: parts.append("Locked by Apple")
            case .password: parts.append("Locked by a password")
            case .unknown: parts.append("Locked by an unrecognised scheme")
            }
        } else if let progress = row.progress {
            if progress >= 1.0 {
                parts.append("Finished")
            } else if progress > 0 {
                parts.append("\(Int((progress * 100).rounded())) percent read")
            }
        }
        return parts.joined(separator: ", ")
    }

    private func handleDroppedProviders(_ providers: [NSItemProvider]) -> Bool {
        guard !providers.isEmpty else { return false }
        let box = SendableURLAccumulator(total: providers.count) { urls in
            Task { @MainActor in
                model.importURLs(urls)
            }
        }
        for provider in providers {
            _ = provider.loadObject(ofClass: URL.self) { url, _ in
                box.record(url)
            }
        }
        return true
    }
}

private final class SendableURLAccumulator: @unchecked Sendable {
    private let lock = NSLock()
    private var urls: [URL] = []
    private var remaining: Int
    private let completion: @Sendable ([URL]) -> Void

    init(total: Int, completion: @escaping @Sendable ([URL]) -> Void) {
        self.remaining = total
        self.completion = completion
    }

    func record(_ url: URL?) {
        lock.lock()
        if let url {
            urls.append(url)
        }
        remaining -= 1
        let done = remaining == 0
        let snapshot = urls
        lock.unlock()
        if done, !snapshot.isEmpty {
            completion(snapshot)
        }
    }
}

private extension View {
    @ViewBuilder
    func applyProminentToolbarStyle() -> some View {
        if #available(macOS 26.0, *) {
            self.buttonStyle(.glassProminent)
        } else {
            self.buttonStyle(.borderedProminent)
        }
    }
}

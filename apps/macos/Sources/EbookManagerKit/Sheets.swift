import EbookBridge
import EbookShellCore
import SwiftUI

/// Per-file import report and review choice sheets, plus the book details and filing edit sheet.
struct ImportReportSheet: View {
    let report: ImportReportModel
    let onReview: (PendingReviewPrompt) -> Void
    let onDone: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Import Report")
                .font(.title2.weight(.semibold))

            if report.items.isEmpty {
                Text("No EPUB files were found in the selected folder.")
                    .foregroundStyle(.secondary)
            } else {
                List(Array(report.items.enumerated()), id: \.offset) { _, item in
                    HStack(alignment: .firstTextBaseline, spacing: 12) {
                        VStack(alignment: .leading, spacing: 3) {
                            Text(item.sourceName)
                                .font(.body.weight(.medium))
                            if let detail = failureDetail(for: item.outcome) {
                                Text(detail)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                        Spacer()
                        statusControl(for: item)
                    }
                    .padding(.vertical, 2)
                }
                .listStyle(.inset)
            }

            HStack {
                if !report.pendingReviews.isEmpty {
                    Text("\(report.pendingReviews.count) \(report.pendingReviews.count == 1 ? "file needs" : "files need") a decision.")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button("Done") {
                    onDone()
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(minWidth: 520, minHeight: 360)
    }

    @ViewBuilder
    private func statusControl(for item: ImportItemResult) -> some View {
        switch item.outcome {
        case .added:
            Text("Added")
                .foregroundStyle(.secondary)
        case .duplicate:
            Text("Skipped as a duplicate")
                .foregroundStyle(.secondary)
        case .replaced:
            Text("Replaced")
                .foregroundStyle(.secondary)
        case let .needsReview(token, _, _, _, _, _):
            if let prompt = report.pendingReviews.first(where: { $0.reviewToken == token }) {
                Button("Review…") {
                    onReview(prompt)
                }
            } else {
                Text("Needs review")
                    .foregroundStyle(.secondary)
            }
        case .failed:
            Text("Failed")
                .foregroundStyle(.secondary)
        }
    }

    private func failureDetail(for outcome: ImportOutcomeKind) -> String? {
        if case let .failed(message) = outcome {
            return message
        }
        return nil
    }
}

struct ReviewSheet: View {
    let prompt: PendingReviewPrompt
    let onDecision: (ReviewChoice) -> Void

    @State private var selectedCandidateId: String

    init(prompt: PendingReviewPrompt, onDecision: @escaping (ReviewChoice) -> Void) {
        self.prompt = prompt
        self.onDecision = onDecision
        _selectedCandidateId = State(initialValue: prompt.candidates.first?.id ?? "")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Review Revised Copy")
                .font(.title2.weight(.semibold))

            Text("\(prompt.sourceName) looks like another version of a book already in your library.")
                .foregroundStyle(.secondary)

            HStack(alignment: .top, spacing: 20) {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Incoming File")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(prompt.title)
                        .font(.body.weight(.semibold))
                    if !prompt.authors.isEmpty {
                        Text(prompt.authors.joined(separator: ", "))
                            .foregroundStyle(.secondary)
                    }
                    if let published = prompt.published {
                        Text("Published \(published)")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    if let words = prompt.words {
                        Text("\(words.formatted()) words")
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(.secondary)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)

                Divider()

                VStack(alignment: .leading, spacing: 6) {
                    Text("In Library")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    if prompt.candidates.count > 1 {
                        Picker("Existing Book", selection: $selectedCandidateId) {
                            ForEach(prompt.candidates, id: \.id) { candidate in
                                Text("\(candidate.title) · \(candidate.authors.joined(separator: ", "))")
                                    .tag(candidate.id)
                            }
                        }
                        .labelsHidden()
                    }
                    if let chosen = prompt.candidates.first(where: { $0.id == selectedCandidateId }) ?? prompt.candidates.first {
                        Text(chosen.title)
                            .font(.body.weight(.semibold))
                        if !chosen.authors.isEmpty {
                            Text(chosen.authors.joined(separator: ", "))
                                .foregroundStyle(.secondary)
                        }
                        if let published = chosen.published {
                            Text("Published \(published)")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        if let words = chosen.words {
                            Text("\(words.formatted()) words")
                                .font(.caption.monospacedDigit())
                                .foregroundStyle(.secondary)
                        }
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(.vertical, 8)

            HStack(spacing: 10) {
                Button("Skip") {
                    onDecision(.skip)
                }
                .keyboardShortcut(.cancelAction)

                Spacer()

                Button("Keep as a Separate Book") {
                    onDecision(.keepSeparate)
                }

                Button("Replace the File") {
                    let target = selectedCandidateId.isEmpty ? (prompt.candidates.first?.id ?? "") : selectedCandidateId
                    onDecision(.replace(bookId: target))
                }
                .disabled(prompt.candidates.isEmpty)
            }
        }
        .padding(20)
        .frame(minWidth: 560, minHeight: 280)
    }
}

struct BookEditSheet: View {
    let book: BookDetail
    @ObservedObject var cache: CoverCache
    let onSave: (EditFormState) async -> String?
    let onCancel: () -> Void

    @State private var form: EditFormState
    @State private var errorMessage: String?
    @State private var isSaving = false

    init(
        book: BookDetail,
        cache: CoverCache,
        onSave: @escaping (EditFormState) async -> String?,
        onCancel: @escaping () -> Void
    ) {
        self.book = book
        self.cache = cache
        self.onSave = onSave
        self.onCancel = onCancel
        _form = State(initialValue: EditFormState(
            title: book.title,
            authors: book.authors,
            sortTitle: book.sortTitle,
            authorSorts: book.authorSorts,
            primarySeriesName: book.series.first?.name,
            primarySeriesPosition: book.series.first?.position
        ))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Edit Book")
                .font(.title2.weight(.semibold))

            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    HStack(alignment: .top, spacing: 18) {
                        BookCoverView(
                            bookId: book.id,
                            title: book.title,
                            authors: book.authors.joined(separator: ", "),
                            hasCover: book.hasCover,
                            coverDigest: book.coverDigest,
                            coverRGB: book.coverRgb,
                            rightToLeft: book.rightToLeft,
                            progress: nil,
                            width: 128,
                            kind: .detail,
                            cache: cache
                        )

                        Grid(alignment: .leadingFirstTextBaseline, horizontalSpacing: 14, verticalSpacing: 6) {
                            if let published = book.published {
                                GridRow {
                                    Text("Published").foregroundStyle(.secondary)
                                    Text(published)
                                }
                            }
                            if let language = book.language {
                                GridRow {
                                    Text("Language").foregroundStyle(.secondary)
                                    Text(language)
                                }
                            }
                            if let words = book.words {
                                GridRow {
                                    Text("Length").foregroundStyle(.secondary)
                                    Text("\(words.formatted()) words").monospacedDigit()
                                }
                            }
                            if let added = book.added {
                                GridRow {
                                    Text("Added").foregroundStyle(.secondary)
                                    Text(Date(timeIntervalSince1970: TimeInterval(added)).formatted(date: .abbreviated, time: .omitted))
                                        .monospacedDigit()
                                }
                            }
                        }
                        .font(.body)
                    }

                    Form {
                        Section("Title and Authors") {
                            TextField("Title", text: $form.title)
                            LabeledContent("Authors") {
                                TextEditor(text: $form.authorsText)
                                    .font(.body)
                                    .frame(height: 60)
                            }
                            Text("One author on each line.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            TextField("Sort Title", text: $form.sortTitle)
                            Text("Leave empty to file by the title without a leading article.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            LabeledContent("Author Sort Names") {
                                TextEditor(text: $form.authorSortsText)
                                    .font(.body)
                                    .frame(height: 60)
                            }
                            Text("One on each line, matching the authors order.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }

                        Section("Series") {
                            TextField("Series", text: $form.seriesName)
                            TextField("Position in Series", text: $form.seriesPosition)
                            Text("A number or text, such as 2, 1.5 or TPB1.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                    .formStyle(.grouped)
                }
            }

            if let errorMessage {
                Text(errorMessage)
                    .font(.callout)
                    .foregroundStyle(.red)
            }

            HStack {
                Spacer()
                Button("Cancel") {
                    onCancel()
                }
                .keyboardShortcut(.cancelAction)

                Button("Done") {
                    guard form.isValid else {
                        errorMessage = "Enter a title for this book."
                        return
                    }
                    isSaving = true
                    Task {
                        errorMessage = await onSave(form)
                        isSaving = false
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(isSaving)
            }
        }
        .padding(20)
        .frame(minWidth: 560, minHeight: 540)
    }
}

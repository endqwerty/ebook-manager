import AppKit
import EbookBridge
import EbookShellCore
import SwiftUI
import WebKit

private struct ReaderWebViewRepresentable: NSViewRepresentable {
    let webView: WKWebView

    func makeNSView(context: Context) -> WKWebView {
        webView
    }

    func updateNSView(_ nsView: WKWebView, context: Context) {}
}

/// View model for an open reader window, driving `ReaderWebController` and `PlaceKeeper`.
@MainActor
final class ReaderWindowViewModel: ObservableObject {
    let handle: OpenBookHandle
    @Published var webController: ReaderWebController?
    @Published var currentChapterIndex: UInt32 = 0
    @Published var currentChapterTitle: String?
    @Published var currentPage: UInt32 = 0
    @Published var totalPages: UInt32 = 1
    @Published var isRightToLeft = false
    @Published var isPartial = false
    @Published var statusError: String?
    @Published var paletteChoice: ReaderPaletteChoice = .automatic
    @Published var sizeStepIndex: Int = 2
    @Published var showContents = false
    @Published var pendingExternalURL: URL?

    private var keeper = PlaceKeeper()
    private var saveTimerTask: Task<Void, Never>?
    private var turnInFlight = false
    private var isDarkAppearance = false
    private var isHighContrast = false

    init(handle: OpenBookHandle, initialPlace: ReadingPlace?) {
        self.handle = handle
        let landing: ChapterLanding
        if let initialPlace {
            self.currentChapterIndex = initialPlace.chapter
            landing = .offset(initialPlace.offset)
        } else {
            self.currentChapterIndex = 0
            landing = .start
        }
        Task { @MainActor [weak self] in
            await self?.setupWebController(initialChapter: self?.currentChapterIndex ?? 0, landing: landing)
        }
    }

    var resolvedPalette: ResolvedReaderPalette {
        ReaderTheme.resolvePalette(
            choice: paletteChoice,
            isDarkWindow: isDarkAppearance,
            increasedContrast: isHighContrast
        )
    }

    private var cssVariables: [(name: String, value: String)] {
        let palette = resolvedPalette
        let scale = ReaderTheme.sizeSteps[sizeStepIndex]
        let motion = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion ? "none" : "full"
        return [
            ("--page", palette.pageHex),
            ("--ink", palette.inkHex),
            ("--secondary", palette.secondaryHex),
            ("--font", "New York, Georgia, serif"),
            ("--size", String(format: "%.2f", scale)),
            ("--turn", "slide"),
            ("--motion", motion),
        ]
    }

    private func setupWebController(initialChapter: UInt32, landing: ChapterLanding) async {
        let source = OpenBookChapterSource(handle: handle)
        do {
            let controller = try await ReaderWebController.create(
                source: source,
                initialStyles: cssVariables
            ) { [weak self] event in
                self?.handleWebEvent(event)
            }
            guard !keeper.isGone else {
                controller.shutDown()
                return
            }
            webController = controller
            openChapter(index: initialChapter, landing: landing)
        } catch {
            statusError = error.localizedDescription
        }
    }

    func updateEnvironment(isDark: Bool, highContrast: Bool) {
        if isDarkAppearance != isDark || isHighContrast != highContrast {
            isDarkAppearance = isDark
            isHighContrast = highContrast
            webController?.setStyles(cssVariables)
        }
    }

    func selectPalette(_ choice: ReaderPaletteChoice) {
        paletteChoice = choice
        webController?.setStyles(cssVariables)
    }

    func stepFontSize(by delta: Int) {
        let next = max(0, min(ReaderTheme.sizeSteps.count - 1, sizeStepIndex + delta))
        if next != sizeStepIndex {
            sizeStepIndex = next
            webController?.setStyles(cssVariables)
        }
    }

    func openChapter(index: UInt32, landing: ChapterLanding) {
        guard index < UInt32(handle.chapters.count), let controller = webController else { return }
        statusError = nil
        currentChapterIndex = index
        currentChapterTitle = handle.chapters[Int(index)].title
        let loadNumber = controller.load(chapter: index, landing: landing)
        performEffects(keeper.step(.asked(number: loadNumber)))
    }

    func goPreviousChapter() {
        guard currentChapterIndex > 0 else { return }
        openChapter(index: currentChapterIndex - 1, landing: .start)
    }

    func goNextChapter() {
        guard Int(currentChapterIndex) + 1 < handle.chapters.count else { return }
        openChapter(index: currentChapterIndex + 1, landing: .start)
    }

    func turnForward() {
        turnPages(by: 1)
    }

    func turnBackward() {
        turnPages(by: -1)
    }

    private func turnPages(by delta: Int) {
        guard !turnInFlight, let controller = webController else { return }
        turnInFlight = true
        let animate = !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        Task { @MainActor [weak self] in
            defer { self?.turnInFlight = false }
            let moved = await controller.turn(by: delta, animate: animate)
            guard let self, !moved else { return }
            if delta > 0, Int(self.currentChapterIndex) + 1 < self.handle.chapters.count {
                self.openChapter(index: self.currentChapterIndex + 1, landing: .start)
            } else if delta < 0, self.currentChapterIndex > 0 {
                self.openChapter(index: self.currentChapterIndex - 1, landing: .end)
            }
        }
    }

    private func handleWebEvent(_ event: ReaderWebEvent) {
        switch event {
        case let .ready(load, rtl, partial, title):
            isRightToLeft = rtl
            isPartial = partial
            if let title, !title.isEmpty {
                currentChapterTitle = title
            }
            performEffects(keeper.step(.ready(load: load)))
        case let .place(report):
            performEffects(keeper.step(.report(report)))
        case let .jump(jump):
            if let anchor = jump.anchor {
                openChapter(index: jump.chapter, landing: .anchor(anchor))
            } else {
                openChapter(index: jump.chapter, landing: .start)
            }
        case let .external(url):
            pendingExternalURL = url
        case let .tap(zone):
            switch zone {
            case .forward: turnForward()
            case .back: turnBackward()
            case .middle: break
            }
        case let .failed(reason):
            statusError = reason
            performEffects(keeper.step(.failed))
        }
    }

    private func performEffects(_ effects: [PlaceEffect]) {
        for effect in effects {
            switch effect {
            case let .show(place):
                currentChapterIndex = place.chapter
                currentPage = place.page
                totalPages = place.pages
            case .blank:
                break
            case .startTimer:
                saveTimerTask?.cancel()
                saveTimerTask = Task { @MainActor [weak self] in
                    try? await Task.sleep(nanoseconds: 333_000_000)
                    guard !Task.isCancelled, let self else { return }
                    self.performEffects(self.keeper.step(.timer))
                }
            case .stopTimer:
                saveTimerTask?.cancel()
                saveTimerTask = nil
            case let .submit(place):
                let bookHandle = handle
                Task {
                    await bookHandle.savePlace(
                        chapter: place.chapter,
                        offset: place.offset,
                        page: place.page,
                        pages: place.pages
                    )
                }
            case .awaitLibrary:
                break
            case .endPage:
                webController?.shutDown()
                webController = nil
            }
        }
    }

    func leaveSynchronously() {
        let effects = keeper.step(.leave)
        saveTimerTask?.cancel()
        saveTimerTask = nil
        for effect in effects {
            if case let .submit(place) = effect {
                handle.flushSaveSynchronously(
                    chapter: place.chapter,
                    offset: place.offset,
                    page: place.page,
                    pages: place.pages
                )
            }
        }
        webController?.shutDown()
        webController = nil
    }
}

struct ReaderWindowView: View {
    @ObservedObject var model: ReaderWindowViewModel
    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.colorSchemeContrast) private var contrast
    @State private var showTextPopover = false

    private var externalLinkAlertPresented: Binding<Bool> {
        Binding(
            get: { model.pendingExternalURL != nil },
            set: { presented in
                if !presented {
                    model.pendingExternalURL = nil
                }
            }
        )
    }

    var body: some View {
        readerContent
            .alert(
                "Open External Link?",
                isPresented: externalLinkAlertPresented,
                presenting: model.pendingExternalURL
            ) { url in
                Button("Open") {
                    NSWorkspace.shared.open(url)
                    model.pendingExternalURL = nil
                }
                Button("Cancel", role: .cancel) {
                    model.pendingExternalURL = nil
                }
            } message: { url in
                Text(url.absoluteString)
            }
    }

    private var readerContent: some View {
        let palette = model.resolvedPalette
        let footer = ReaderTheme.footerText(
            chapterIndex: model.currentChapterIndex,
            chapterCount: model.handle.chapters.count,
            chapterTitle: model.currentChapterTitle,
            page: model.currentPage,
            pages: model.totalPages,
            partial: model.isPartial
        )

        return HStack(spacing: 0) {
            if model.showContents {
                contentsSidebar
                    .frame(width: 240)
                Divider()
            }

            ZStack {
                Color(rgb24: palette.pageRGB)
                    .ignoresSafeArea()

                VStack(spacing: 0) {
                    if let error = model.statusError {
                        VStack(spacing: 12) {
                            Text(error)
                                .foregroundStyle(Color(rgb24: palette.inkRGB))
                                .multilineTextAlignment(.center)
                        }
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .padding(32)
                    } else if let controller = model.webController {
                        ReaderWebViewRepresentable(webView: controller.webView)
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                    } else {
                        ProgressView()
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                    }

                    HStack {
                        Text(footer.leading)
                            .lineLimit(1)
                            .truncationMode(.tail)
                        Spacer(minLength: 16)
                        Text(footer.trailing)
                            .monospacedDigit()
                            .lineLimit(1)
                    }
                    .font(.caption)
                    .foregroundStyle(Color(rgb24: palette.secondaryRGB))
                    .padding(.horizontal, 24)
                    .padding(.bottom, 12)
                    .padding(.top, 4)
                }
            }
        }
        .toolbar {
            ToolbarItemGroup(placement: .navigation) {
                Button {
                    model.showContents.toggle()
                } label: {
                    Label("Contents", systemImage: "sidebar.left")
                }
                .help("Contents")
            }

            ToolbarItemGroup(placement: .primaryAction) {
                Button {
                    model.goPreviousChapter()
                } label: {
                    Label("Previous Chapter", systemImage: "chevron.left")
                }
                .disabled(model.currentChapterIndex == 0)
                .help("Previous Chapter")

                Button {
                    model.goNextChapter()
                } label: {
                    Label("Next Chapter", systemImage: "chevron.right")
                }
                .disabled(Int(model.currentChapterIndex) + 1 >= model.handle.chapters.count)
                .help("Next Chapter")

                Button {
                    showTextPopover.toggle()
                } label: {
                    Text("Aa")
                }
                .help("Text Settings")
                .popover(isPresented: $showTextPopover) {
                    textSettingsPopover
                }
            }
        }
        .onAppear {
            model.updateEnvironment(isDark: colorScheme == .dark, highContrast: contrast == .increased)
        }
        .onChange(of: colorScheme) { _, newScheme in
            model.updateEnvironment(isDark: newScheme == .dark, highContrast: contrast == .increased)
        }
        .onChange(of: contrast) { _, newContrast in
            model.updateEnvironment(isDark: colorScheme == .dark, highContrast: newContrast == .increased)
        }
    }

    private var contentsSidebar: some View {
        List {
            if !model.handle.toc.isEmpty {
                ForEach(Array(model.handle.toc.enumerated()), id: \.offset) { _, entry in
                    Button {
                        if let chapter = entry.chapter {
                            if let anchor = entry.anchor {
                                model.openChapter(index: chapter, landing: .anchor(anchor))
                            } else {
                                model.openChapter(index: chapter, landing: .start)
                            }
                        }
                    } label: {
                        Text(entry.title)
                            .font(entry.chapter == model.currentChapterIndex ? .body.weight(.semibold) : .body)
                            .padding(.leading, CGFloat(min(entry.depth, 6)) * 16)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .buttonStyle(.plain)
                    .disabled(entry.chapter == nil)
                }
            } else {
                ForEach(model.handle.chapters, id: \.index) { chapter in
                    Button {
                        model.openChapter(index: chapter.index, landing: .start)
                    } label: {
                        Text(chapter.title ?? "Chapter \(chapter.index + 1)")
                            .font(chapter.index == model.currentChapterIndex ? .body.weight(.semibold) : .body)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .listStyle(.sidebar)
    }

    private var textSettingsPopover: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("For every book")
                .font(.caption)
                .foregroundStyle(.secondary)

            HStack(spacing: 10) {
                ForEach([ReaderPaletteChoice.white, .paper, .grey, .black], id: \.self) { swatch in
                    let resolved = ReaderTheme.resolvePalette(choice: swatch, isDarkWindow: false, increasedContrast: false)
                    Button {
                        model.selectPalette(swatch)
                    } label: {
                        ZStack {
                            Circle()
                                .fill(Color(rgb24: resolved.pageRGB))
                                .frame(width: 36, height: 36)
                            Circle()
                                .strokeBorder(
                                    model.paletteChoice == swatch ? Color.accentColor : Color.secondary.opacity(0.4),
                                    lineWidth: model.paletteChoice == swatch ? 2 : 1
                                )
                                .frame(width: 36, height: 36)
                            Text("Aa")
                                .font(.callout.weight(.medium))
                                .foregroundStyle(Color(rgb24: resolved.inkRGB))
                        }
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(swatch.label)
                }
            }

            Toggle(
                "Follow Window Appearance",
                isOn: Binding(
                    get: { model.paletteChoice == .automatic },
                    set: { model.selectPalette($0 ? .automatic : (colorScheme == .dark ? .grey : .paper)) }
                )
            )

            HStack {
                Text("Size")
                Spacer()
                Button {
                    model.stepFontSize(by: -1)
                } label: {
                    Text("A").font(.footnote)
                }
                .disabled(model.sizeStepIndex == 0)
                .accessibilityLabel("Smaller Text")

                Button {
                    model.stepFontSize(by: 1)
                } label: {
                    Text("A").font(.title3)
                }
                .disabled(model.sizeStepIndex >= ReaderTheme.sizeSteps.count - 1)
                .accessibilityLabel("Larger Text")
            }
        }
        .padding(16)
        .frame(width: 240)
    }
}

/// Owns an AppKit `NSWindow` for one open book so key events and close-time place flushes are deterministic.
@MainActor
final class ReaderWindowCoordinator: NSObject, NSWindowDelegate {
    private let bookId: String
    private let viewModel: ReaderWindowViewModel
    private var window: NSWindow?
    private var keyMonitor: Any?
    private let onClosed: (String) -> Void
    private var didClose = false

    init(
        handle: OpenBookHandle,
        initialPlace: ReadingPlace?,
        onClosed: @escaping (String) -> Void
    ) {
        self.bookId = handle.bookId
        self.viewModel = ReaderWindowViewModel(handle: handle, initialPlace: initialPlace)
        self.onClosed = onClosed
        super.init()
    }

    func present() {
        let hosting = NSHostingController(rootView: ReaderWindowView(model: viewModel))
        let win = NSWindow(contentViewController: hosting)
        win.isReleasedWhenClosed = false
        win.title = viewModel.handle.title
        win.setContentSize(NSSize(width: 820, height: 680))
        win.minSize = NSSize(width: 480, height: 380)
        win.styleMask = [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView]
        win.delegate = self
        win.center()
        win.makeKeyAndOrderFront(nil)
        self.window = win
        installKeyMonitor()
    }

    func bringToFront() {
        window?.makeKeyAndOrderFront(nil)
    }

    func flushAndClose() {
        guard !didClose else { return }
        didClose = true
        removeKeyMonitor()
        viewModel.leaveSynchronously()
        let win = window
        window = nil
        win?.delegate = nil
        win?.close()
    }

    func windowWillClose(_ notification: Notification) {
        guard !didClose else { return }
        didClose = true
        removeKeyMonitor()
        viewModel.leaveSynchronously()
        window = nil
        onClosed(bookId)
    }

    private func installKeyMonitor() {
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, let win = self.window, event.window === win else {
                return event
            }
            if self.handleKeyDown(event) {
                return nil
            }
            return event
        }
    }

    private func removeKeyMonitor() {
        if let keyMonitor {
            NSEvent.removeMonitor(keyMonitor)
            self.keyMonitor = nil
        }
    }

    private func handleKeyDown(_ event: NSEvent) -> Bool {
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        if flags.contains(.command) || flags.contains(.control) || flags.contains(.option) {
            return false
        }
        switch event.keyCode {
        case 53:
            if viewModel.showContents {
                viewModel.showContents = false
                return true
            }
            return false
        case 124:
            if viewModel.isRightToLeft { viewModel.turnBackward() } else { viewModel.turnForward() }
            return true
        case 123:
            if viewModel.isRightToLeft { viewModel.turnForward() } else { viewModel.turnBackward() }
            return true
        case 121:
            viewModel.turnForward()
            return true
        case 116:
            viewModel.turnBackward()
            return true
        case 49:
            if flags.contains(.shift) {
                viewModel.turnBackward()
            } else {
                viewModel.turnForward()
            }
            return true
        default:
            return false
        }
    }
}

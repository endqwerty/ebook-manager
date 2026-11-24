import EbookBridge
import EbookShellCore
import Foundation
import WebKit

public struct ServedChapter: Sendable {
    public let markup: String
    public let length: UInt32
    public let language: String?
    public let rightToLeft: Bool
    public let title: String?
    public let partial: Bool

    public init(
        markup: String,
        length: UInt32,
        language: String?,
        rightToLeft: Bool,
        title: String?,
        partial: Bool
    ) {
        self.markup = markup
        self.length = length
        self.language = language
        self.rightToLeft = rightToLeft
        self.title = title
        self.partial = partial
    }
}

public struct ServedResource: Sendable {
    public let mediaType: String
    public let bytes: Data

    public init(mediaType: String, bytes: Data) {
        self.mediaType = mediaType
        self.bytes = bytes
    }
}

public protocol ReaderChapterSource: AnyObject, Sendable {
    func loadChapter(index: UInt32) async throws -> ServedChapter
    func loadResource(id: UInt32) async -> ServedResource?
}

public final class OpenBookChapterSource: ReaderChapterSource {
    private let handle: OpenBookHandle

    public init(handle: OpenBookHandle) {
        self.handle = handle
    }

    public func loadChapter(index: UInt32) async throws -> ServedChapter {
        let content = try await handle.readContent(chapter: index)
        return ServedChapter(
            markup: content.markup,
            length: content.length,
            language: content.language,
            rightToLeft: content.rightToLeft,
            title: content.title,
            partial: content.truncated || !content.warnings.isEmpty
        )
    }

    public func loadResource(id: UInt32) async -> ServedResource? {
        guard let res = await handle.readResource(id: id) else { return nil }
        return ServedResource(mediaType: res.mediaType, bytes: res.bytes)
    }
}

public enum ChapterLanding: Equatable, Sendable {
    case start
    case offset(UInt32)
    case anchor(String)
    case end
}

public enum ReaderTapZone: String, Equatable, Sendable {
    case back
    case middle
    case forward
}

public enum ReaderWebEvent: Equatable, Sendable {
    case ready(load: ChapterLoadRecord, rightToLeft: Bool, partial: Bool, title: String?)
    case place(PagePlaceReport)
    case jump(InternalJump)
    case external(URL)
    case tap(ReaderTapZone)
    case failed(String)
}

@MainActor
private final class WeakScriptHandler: NSObject, WKScriptMessageHandler {
    weak var owner: ReaderWebController?

    init(owner: ReaderWebController? = nil) {
        self.owner = owner
    }

    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        owner?.handleScriptMessage(message.body)
    }
}

@MainActor
private final class EbookSchemeHandler: NSObject, WKURLSchemeHandler {
    weak var owner: ReaderWebController?
    private var activeTasks: [ObjectIdentifier: Task<Void, Never>] = [:]

    init(owner: ReaderWebController? = nil) {
        self.owner = owner
    }

    func webView(_ webView: WKWebView, start urlSchemeTask: any WKURLSchemeTask) {
        let id = ObjectIdentifier(urlSchemeTask as AnyObject)
        guard let owner else {
            failNotFound(urlSchemeTask)
            return
        }
        guard let url = urlSchemeTask.request.url,
              let route = ReaderSchemeRouter.resolve(url: url, expectedToken: owner.token)
        else {
            failNotFound(urlSchemeTask)
            return
        }
        let cssProperties = owner.currentCSSVariables
        let source = owner.source
        let stylesheetData = owner.stylesheetData

        let job = Task { @MainActor [weak self, weak owner] in
            switch route {
            case .stylesheet:
                guard self?.activeTasks.removeValue(forKey: id) != nil else { return }
                Self.respond(
                    task: urlSchemeTask,
                    url: url,
                    statusCode: 200,
                    contentType: "text/css; charset=utf-8",
                    data: stylesheetData,
                    includeCSP: false
                )
            case let .chapter(index):
                do {
                    let chapter = try await source.loadChapter(index: index)
                    guard self?.activeTasks.removeValue(forKey: id) != nil else { return }
                    owner?.recordChapterFacts(index: index, chapter: chapter)
                    let xhtml = ReaderSchemeRouter.wrapXHTMLDocument(
                        bodyMarkup: chapter.markup,
                        language: chapter.language,
                        rightToLeft: chapter.rightToLeft,
                        cssVariables: cssProperties
                    )
                    Self.respond(
                        task: urlSchemeTask,
                        url: url,
                        statusCode: 200,
                        contentType: "application/xhtml+xml; charset=utf-8",
                        data: Data(xhtml.utf8),
                        includeCSP: true
                    )
                } catch {
                    guard self?.activeTasks.removeValue(forKey: id) != nil else { return }
                    let message = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
                    owner?.reportImmediateFailure(message)
                    self?.failNotFound(urlSchemeTask)
                }
            case let .resource(resId):
                let resource = await source.loadResource(id: resId)
                guard self?.activeTasks.removeValue(forKey: id) != nil else { return }
                if let resource {
                    Self.respond(
                        task: urlSchemeTask,
                        url: url,
                        statusCode: 200,
                        contentType: resource.mediaType,
                        data: resource.bytes,
                        includeCSP: false
                    )
                } else {
                    self?.failNotFound(urlSchemeTask)
                }
            }
        }
        activeTasks[id] = job
    }

    func webView(_ webView: WKWebView, stop urlSchemeTask: any WKURLSchemeTask) {
        let id = ObjectIdentifier(urlSchemeTask as AnyObject)
        activeTasks.removeValue(forKey: id)?.cancel()
    }

    func cancelAll() {
        for (_, task) in activeTasks {
            task.cancel()
        }
        activeTasks.removeAll()
    }

    private func failNotFound(_ task: any WKURLSchemeTask) {
        if let url = task.request.url,
           let response = HTTPURLResponse(
               url: url,
               statusCode: 404,
               httpVersion: "HTTP/1.1",
               headerFields: ["Cache-Control": "no-store", "X-Content-Type-Options": "nosniff"]
           )
        {
            task.didReceive(response)
            task.didFinish()
        } else {
            task.didFailWithError(URLError(.fileDoesNotExist))
        }
    }

    private static func respond(
        task: any WKURLSchemeTask,
        url: URL,
        statusCode: Int,
        contentType: String,
        data: Data,
        includeCSP: Bool
    ) {
        var headers = [
            "Content-Type": contentType,
            "Content-Length": "\(data.count)",
            "Cache-Control": "no-store",
            "X-Content-Type-Options": "nosniff",
        ]
        if includeCSP {
            headers["Content-Security-Policy"] = ReaderSchemeRouter.contentSecurityPolicy
        }
        if let response = HTTPURLResponse(
            url: url,
            statusCode: statusCode,
            httpVersion: "HTTP/1.1",
            headerFields: headers
        ) {
            task.didReceive(response)
        } else {
            let fallback = URLResponse(
                url: url,
                mimeType: contentType.components(separatedBy: ";").first,
                expectedContentLength: data.count,
                textEncodingName: "utf-8"
            )
            task.didReceive(fallback)
        }
        task.didReceive(data)
        task.didFinish()
    }
}

/// Isolated `WKWebView` controller for EPUB chapters with book scripts disabled and non-`ebook:` URLs blocked.
@MainActor
public final class ReaderWebController: NSObject, WKNavigationDelegate, WKUIDelegate {
    private static let handlerName = "reader"
    private static let worldName = "ebook-reader"
    private static let ruleListId = "ebook-only-v1"
    private static var cachedRuleList: WKContentRuleList?

    // Headless macOS test runners and sleeping displays suspend WebKit's display link; fall back after 32 ms so `reader.js` never stalls.
    private static let rafFallbackShim = """
    (() => {
        "use strict";
        const nativeRAF = window.requestAnimationFrame.bind(window);
        const nativeCancel = window.cancelAnimationFrame.bind(window);
        const active = new Map();
        let nextId = 1;
        window.requestAnimationFrame = (callback) => {
            const id = nextId++;
            let finished = false;
            const rafId = nativeRAF((timestamp) => {
                if (finished) return;
                finished = true;
                const entry = active.get(id);
                if (entry) clearTimeout(entry[1]);
                active.delete(id);
                callback(timestamp);
            });
            const timerId = setTimeout(() => {
                if (finished) return;
                finished = true;
                nativeCancel(rafId);
                active.delete(id);
                callback(performance.now());
            }, 32);
            active.set(id, [rafId, timerId]);
            return id;
        };
        window.cancelAnimationFrame = (id) => {
            const entry = active.get(id);
            if (!entry) return;
            active.delete(id);
            nativeCancel(entry[0]);
            clearTimeout(entry[1]);
        };
    })();
    """

    public let webView: WKWebView
    let token: String
    let source: any ReaderChapterSource
    let stylesheetData: Data
    private let readerWorld = WKContentWorld.world(name: ReaderWebController.worldName)
    private let onEvent: (ReaderWebEvent) -> Void
    private var schemeHandler: EbookSchemeHandler?

    private struct AskedLoad {
        let number: UInt32
        let chapter: UInt32
        let landing: ChapterLanding
        var readyRecord: ChapterLoadRecord?
    }

    private var counter: UInt32 = 0
    private var currentLoad: AskedLoad?
    private var readyRecords: [UInt32: ChapterLoadRecord] = [:]
    private var chapterFacts: [UInt32: (rightToLeft: Bool, partial: Bool, title: String?)] = [:]
    private var allowedNavigationURL: URL?
    private var watchdogTask: Task<Void, Never>?
    private let watchdogSeconds: Double
    private(set) var currentCSSVariables: [(name: String, value: String)] = []

    public static func create(
        source: any ReaderChapterSource,
        initialStyles: [(name: String, value: String)] = [],
        watchdogSeconds: Double = 15.0,
        onEvent: @escaping (ReaderWebEvent) -> Void
    ) async throws -> ReaderWebController {
        let scriptText = try ReaderAssets.loadScript()
        let stylesheetData = try ReaderAssets.loadStylesheet()
        let ruleList = try await compileRuleList()
        return ReaderWebController(
            source: source,
            scriptText: scriptText,
            stylesheetData: stylesheetData,
            ruleList: ruleList,
            initialStyles: initialStyles,
            watchdogSeconds: watchdogSeconds,
            onEvent: onEvent
        )
    }

    private static func compileRuleList() async throws -> WKContentRuleList {
        if let existing = cachedRuleList {
            return existing
        }
        let store = WKContentRuleListStore.default()!
        let compiled: WKContentRuleList = try await withCheckedThrowingContinuation { continuation in
            store.compileContentRuleList(
                forIdentifier: ruleListId,
                encodedContentRuleList: ReaderSchemeRouter.contentRuleListJSON
            ) { list, error in
                if let list {
                    continuation.resume(returning: list)
                } else {
                    continuation.resume(throwing: error ?? URLError(.cannotCreateFile))
                }
            }
        }
        cachedRuleList = compiled
        return compiled
    }

    private init(
        source: any ReaderChapterSource,
        scriptText: String,
        stylesheetData: Data,
        ruleList: WKContentRuleList,
        initialStyles: [(name: String, value: String)],
        watchdogSeconds: Double,
        onEvent: @escaping (ReaderWebEvent) -> Void
    ) {
        self.source = source
        self.stylesheetData = stylesheetData
        self.token = ReaderSchemeRouter.makeToken()
        self.currentCSSVariables = initialStyles
        self.watchdogSeconds = watchdogSeconds
        self.onEvent = onEvent

        let config = WKWebViewConfiguration()
        config.websiteDataStore = .nonPersistent()
        config.suppressesIncrementalRendering = false

        // WKWebpagePreferences.allowsContentJavaScript also disables WKUserScript in isolated content worlds; CSP default-src 'none' blocks all markup scripts instead.
        let pagePrefs = WKWebpagePreferences()
        pagePrefs.allowsContentJavaScript = true
        config.defaultWebpagePreferences = pagePrefs
        config.preferences.javaScriptCanOpenWindowsAutomatically = false

        let contentController = WKUserContentController()
        contentController.add(ruleList)
        let shimScript = WKUserScript(
            source: Self.rafFallbackShim,
            injectionTime: .atDocumentStart,
            forMainFrameOnly: true,
            in: readerWorld
        )
        contentController.addUserScript(shimScript)
        let userScript = WKUserScript(
            source: scriptText,
            injectionTime: .atDocumentEnd,
            forMainFrameOnly: true,
            in: readerWorld
        )
        contentController.addUserScript(userScript)
        config.userContentController = contentController

        let scheme = EbookSchemeHandler(owner: nil)
        config.setURLSchemeHandler(scheme, forURLScheme: ReaderSchemeRouter.scheme)

        self.webView = WKWebView(frame: CGRect(x: 0, y: 0, width: 760, height: 640), configuration: config)
        self.schemeHandler = scheme
        super.init()

        scheme.owner = self
        contentController.add(WeakScriptHandler(owner: self), contentWorld: readerWorld, name: Self.handlerName)
        webView.navigationDelegate = self
        webView.uiDelegate = self
        webView.setValue(false, forKey: "drawsBackground")
    }

    public func shutDown() {
        watchdogTask?.cancel()
        watchdogTask = nil
        currentLoad = nil
        allowedNavigationURL = nil
        schemeHandler?.cancelAll()
        webView.stopLoading()
        webView.configuration.userContentController.removeScriptMessageHandler(
            forName: Self.handlerName,
            contentWorld: readerWorld
        )
        webView.navigationDelegate = nil
        webView.uiDelegate = nil
    }

    @discardableResult
    public func load(chapter: UInt32, landing: ChapterLanding) -> UInt32 {
        counter &+= 1
        let number = counter
        currentLoad = AskedLoad(number: number, chapter: chapter, landing: landing, readyRecord: nil)
        guard let url = ReaderSchemeRouter.chapterURL(token: token, chapter: chapter, loadNumber: number) else {
            reportImmediateFailure("Could not construct chapter address.")
            return number
        }
        allowedNavigationURL = url
        armWatchdog(forLoad: number)
        webView.load(URLRequest(url: url))
        return number
    }

    public func turn(by delta: Int, animate: Bool) async -> Bool {
        guard let ready = currentLoad?.readyRecord else { return false }
        let result = try? await webView.callAsyncJavaScript(
            "return __reader.turn(by, smooth === 1)",
            arguments: ["by": delta, "smooth": animate ? 1 : 0],
            in: nil,
            contentWorld: readerWorld
        )
        guard let text = result as? String, !text.isEmpty else {
            return false
        }
        let parts = text.split(separator: ",").compactMap { UInt32($0) }
        guard parts.count == 4 else { return false }
        return emitPlace(
            load: ready,
            offset: parts[0],
            page: parts[1],
            pages: parts[2],
            count: parts[3],
            moved: true
        )
    }

    public func setStyles(_ properties: [(name: String, value: String)]) {
        for prop in properties {
            currentCSSVariables.removeAll { $0.name == prop.name }
            currentCSSVariables.append(prop)
        }
        guard currentLoad?.readyRecord != nil else { return }
        var dict: [String: String] = [:]
        for prop in currentCSSVariables {
            dict[prop.name] = prop.value
        }
        Task { @MainActor [weak self] in
            guard let self else { return }
            _ = try? await self.webView.callAsyncJavaScript(
                "return __reader.style(properties)",
                arguments: ["properties": dict],
                in: nil,
                contentWorld: self.readerWorld
            )
        }
    }

    public func evaluateInPageWorld(_ expression: String) async throws -> Any? {
        try await webView.callAsyncJavaScript(
            expression,
            arguments: [:],
            in: nil,
            contentWorld: .page
        )
    }

    func recordChapterFacts(index: UInt32, chapter: ServedChapter) {
        chapterFacts[index] = (chapter.rightToLeft, chapter.partial, chapter.title)
    }

    func reportImmediateFailure(_ reason: String) {
        watchdogTask?.cancel()
        watchdogTask = nil
        currentLoad = nil
        onEvent(.failed(reason))
    }

    private func armWatchdog(forLoad number: UInt32) {
        watchdogTask?.cancel()
        let limit = watchdogSeconds
        watchdogTask = Task { @MainActor [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(limit * 1_000_000_000))
            guard !Task.isCancelled, let self, self.currentLoad?.number == number, self.currentLoad?.readyRecord == nil else {
                return
            }
            self.webView.stopLoading()
            self.currentLoad = nil
            self.onEvent(.failed("This chapter took too long to display."))
        }
    }

    fileprivate func handleScriptMessage(_ raw: Any) {
        guard let dict = raw as? [String: Any],
              let kind = dict["kind"] as? String
        else {
            return
        }
        let numberField: (String) -> UInt32? = { key in
            if let u = dict[key] as? UInt32 { return u }
            if let i = dict[key] as? Int, i >= 0, i <= Int(UInt32.max) { return UInt32(i) }
            if let d = dict[key] as? Double, d.isFinite, d >= 0, d <= Double(UInt32.max), d.rounded() == d {
                return UInt32(d)
            }
            return nil
        }

        if kind == "place" {
            let moved = (dict["moved"] as? Bool) ?? false
            guard let loadNum = numberField("load"),
                  let loadRecord = readyRecords[loadNum],
                  let offset = numberField("offset"),
                  let page = numberField("page"),
                  let pages = numberField("pages"),
                  let moveCount = numberField("move")
            else {
                return
            }
            _ = emitPlace(load: loadRecord, offset: offset, page: page, pages: pages, count: moveCount, moved: moved)
            return
        }

        guard let active = currentLoad,
              numberField("load") == active.number
        else {
            return
        }

        switch kind {
        case "ready" where active.readyRecord == nil:
            watchdogTask?.cancel()
            watchdogTask = nil
            let hasBody = (dict["body"] as? Bool) ?? false
            guard hasBody, let length = numberField("length") else {
                currentLoad = nil
                onEvent(.failed("This chapter could not be displayed."))
                return
            }
            let record = ChapterLoadRecord(number: active.number, chapter: active.chapter, length: length)
            currentLoad?.readyRecord = record
            readyRecords[active.number] = record
            let facts = chapterFacts[active.chapter] ?? (false, false, nil)
            onEvent(.ready(load: record, rightToLeft: facts.rightToLeft, partial: facts.partial, title: facts.title))
            applyLanding(active.landing, chapterLength: length)

        case "jump" where active.readyRecord != nil:
            if let href = dict["href"] as? String,
               let jump = ReaderSchemeRouter.parseJump(href: href)
            {
                onEvent(.jump(jump))
            }

        case "external" where active.readyRecord != nil:
            if let address = dict["address"] as? String,
               let url = ReaderSchemeRouter.validatedExternalAddress(address)
            {
                onEvent(.external(url))
            }

        case "tap" where active.readyRecord != nil:
            if let rawZone = dict["zone"] as? String,
               let zone = ReaderTapZone(rawValue: rawZone)
            {
                onEvent(.tap(zone))
            }

        default:
            break
        }
    }

    private func applyLanding(_ landing: ChapterLanding, chapterLength: UInt32) {
        var props: [String: String] = [:]
        for item in currentCSSVariables {
            props[item.name] = item.value
        }
        let body: String
        var args: [String: Any] = ["properties": props]
        switch landing {
        case .start:
            body = "__reader.assign(properties); return __reader.go({page: 0})"
        case let .offset(offset):
            args["offset"] = min(offset, chapterLength)
            body = "__reader.assign(properties); return __reader.go({offset})"
        case let .anchor(anchor):
            args["anchor"] = anchor
            body = "__reader.assign(properties); return __reader.go({anchor})"
        case .end:
            body = "__reader.assign(properties); return __reader.go({end: true})"
        }
        Task { @MainActor [weak self] in
            guard let self else { return }
            _ = try? await self.webView.callAsyncJavaScript(
                body,
                arguments: args,
                in: nil,
                contentWorld: self.readerWorld
            )
        }
    }

    private func emitPlace(
        load: ChapterLoadRecord,
        offset: UInt32,
        page: UInt32,
        pages: UInt32,
        count: UInt32,
        moved: Bool
    ) -> Bool {
        guard pages > 0, page < pages, offset <= load.length else {
            return false
        }
        onEvent(.place(PagePlaceReport(
            load: load,
            offset: offset,
            page: page,
            pages: pages,
            count: count,
            moved: moved
        )))
        return true
    }

    public func webView(
        _ webView: WKWebView,
        decidePolicyFor navigationAction: WKNavigationAction,
        preferences: WKWebpagePreferences,
        decisionHandler: @escaping (WKNavigationActionPolicy, WKWebpagePreferences) -> Void
    ) {
        if let url = navigationAction.request.url,
           url == allowedNavigationURL
        {
            allowedNavigationURL = nil
            decisionHandler(.allow, preferences)
        } else {
            decisionHandler(.cancel, preferences)
        }
    }

    public func webView(
        _ webView: WKWebView,
        decidePolicyFor navigationResponse: WKNavigationResponse,
        decisionHandler: @escaping (WKNavigationResponsePolicy) -> Void
    ) {
        if let url = navigationResponse.response.url,
           url.scheme?.lowercased() == ReaderSchemeRouter.scheme
        {
            decisionHandler(.allow)
        } else {
            decisionHandler(.cancel)
        }
    }

    public func webView(
        _ webView: WKWebView,
        createWebViewWith configuration: WKWebViewConfiguration,
        for navigationAction: WKNavigationAction,
        windowFeatures: WKWindowFeatures
    ) -> WKWebView? {
        nil
    }

    public func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        watchdogTask?.cancel()
        watchdogTask = nil
        currentLoad = nil
        onEvent(.failed("The part of the app that draws the page stopped."))
    }

    public func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: any Error) {
        if (error as NSError).code == NSURLErrorCancelled { return }
        reportImmediateFailure(error.localizedDescription)
    }

    public func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: any Error) {
        if (error as NSError).code == NSURLErrorCancelled { return }
        reportImmediateFailure(error.localizedDescription)
    }
}

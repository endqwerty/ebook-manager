import AppKit
import Compression
import Darwin
import EbookBridge
@testable import EbookManagerKit
import EbookShellCore
import Foundation
import XCTest

private final class LocalProbeServer: @unchecked Sendable {
    let port: UInt16
    private let listenFD: Int32
    private let lock = NSLock()
    private var connectionCount = 0

    init() throws {
        let fd = socket(AF_INET, SOCK_STREAM, 0)
        guard fd >= 0 else { throw POSIXError(.EIO) }
        var reuse: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &reuse, socklen_t(MemoryLayout<Int32>.size))

        var addr = sockaddr_in()
        addr.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = 0
        addr.sin_addr = in_addr(s_addr: inet_addr("127.0.0.1"))

        let bound = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                bind(fd, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard bound == 0, listen(fd, 16) == 0 else {
            close(fd)
            throw POSIXError(.EADDRINUSE)
        }

        var assigned = sockaddr_in()
        var len = socklen_t(MemoryLayout<sockaddr_in>.size)
        _ = withUnsafeMutablePointer(to: &assigned) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                getsockname(fd, $0, &len)
            }
        }
        self.listenFD = fd
        self.port = UInt16(bigEndian: assigned.sin_port)

        DispatchQueue.global(qos: .utility).async { [weak self] in
            while true {
                let client = accept(fd, nil, nil)
                if client < 0 { break }
                close(client)
                self?.recordHit()
            }
        }
    }

    private func recordHit() {
        lock.lock()
        connectionCount += 1
        lock.unlock()
    }

    var hits: Int {
        lock.lock()
        defer { lock.unlock() }
        return connectionCount
    }

    func stop() {
        close(listenFD)
    }

    deinit {
        close(listenFD)
    }
}

private final class HostileChapterSource: ReaderChapterSource {
    let port: UInt16

    init(port: UInt16) {
        self.port = port
    }

    func loadChapter(index: UInt32) async throws -> ServedChapter {
        let repeatedParagraphs = (1 ... 30)
            .map { "<p>Paragraph \($0) with enough words to span multiple pages inside the reader column.</p>" }
            .joined()
        let markup = """
        <h1>Chapter One</h1>
        <script type="text/javascript">document.title = "PWNED"; fetch("http://127.0.0.1:\(port)/script");</script>
        <p id="target" onclick="document.title='CLICKED'">Safe paragraph text inside isolated WKWebView.</p>
        <img src="http://127.0.0.1:\(port)/remote.png" alt="Blocked remote image"/>
        \(repeatedParagraphs)
        """
        return ServedChapter(
            markup: markup,
            length: 4000,
            language: "en",
            rightToLeft: false,
            title: "Chapter One",
            partial: false
        )
    }

    func loadResource(id: UInt32) async -> ServedResource? {
        nil
    }
}

private enum StoredZipBuilder {
    static func build(entries: [(name: String, data: Data)]) -> Data {
        var out = Data()
        var centralDirectory = Data()
        var offsets: [UInt32] = []

        for entry in entries {
            offsets.append(UInt32(out.count))
            let nameBytes = Data(entry.name.utf8)
            let crc = crc32(entry.data)
            let size = UInt32(entry.data.count)

            out.appendUInt32LE(0x0403_4B50)
            out.appendUInt16LE(20)
            out.appendUInt16LE(0)
            out.appendUInt16LE(0)
            out.appendUInt16LE(0)
            out.appendUInt16LE(0)
            out.appendUInt32LE(crc)
            out.appendUInt32LE(size)
            out.appendUInt32LE(size)
            out.appendUInt16LE(UInt16(nameBytes.count))
            out.appendUInt16LE(0)
            out.append(nameBytes)
            out.append(entry.data)
        }

        let cdOffset = UInt32(out.count)
        for (index, entry) in entries.enumerated() {
            let nameBytes = Data(entry.name.utf8)
            let crc = crc32(entry.data)
            let size = UInt32(entry.data.count)

            centralDirectory.appendUInt32LE(0x0201_4B50)
            centralDirectory.appendUInt16LE(20)
            centralDirectory.appendUInt16LE(20)
            centralDirectory.appendUInt16LE(0)
            centralDirectory.appendUInt16LE(0)
            centralDirectory.appendUInt16LE(0)
            centralDirectory.appendUInt16LE(0)
            centralDirectory.appendUInt32LE(crc)
            centralDirectory.appendUInt32LE(size)
            centralDirectory.appendUInt32LE(size)
            centralDirectory.appendUInt16LE(UInt16(nameBytes.count))
            centralDirectory.appendUInt16LE(0)
            centralDirectory.appendUInt16LE(0)
            centralDirectory.appendUInt16LE(0)
            centralDirectory.appendUInt16LE(0)
            centralDirectory.appendUInt32LE(0)
            centralDirectory.appendUInt32LE(offsets[index])
            centralDirectory.append(nameBytes)
        }

        out.append(centralDirectory)
        out.appendUInt32LE(0x0605_4B50)
        out.appendUInt16LE(0)
        out.appendUInt16LE(0)
        out.appendUInt16LE(UInt16(entries.count))
        out.appendUInt16LE(UInt16(entries.count))
        out.appendUInt32LE(UInt32(centralDirectory.count))
        out.appendUInt32LE(cdOffset)
        out.appendUInt16LE(0)
        return out
    }

    private static func crc32(_ data: Data) -> UInt32 {
        var crc: UInt32 = 0xFFFF_FFFF
        for byte in data {
            crc ^= UInt32(byte)
            for _ in 0 ..< 8 {
                if (crc & 1) != 0 {
                    crc = (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >>= 1
                }
            }
        }
        return ~crc
    }

    static func syntheticEPUB(title: String, extraText: String) -> Data {
        let container = """
        <?xml version="1.0"?>
        <container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
          <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
        </container>
        """
        let opf = """
        <?xml version="1.0" encoding="utf-8"?>
        <package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="pub-id">
          <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
            <dc:identifier id="pub-id">urn:uuid:aaaaaaaa-1111-4222-8333-bbbbbbbbbbbb</dc:identifier>
            <dc:title>\(title)</dc:title>
            <dc:creator>Mira Field</dc:creator>
            <dc:language>en</dc:language>
            <meta property="dcterms:modified">2026-01-01T00:00:00Z</meta>
          </metadata>
          <manifest>
            <item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>
            <item id="c2" href="c2.xhtml" media-type="application/xhtml+xml"/>
          </manifest>
          <spine>
            <itemref idref="c1"/>
            <itemref idref="c2"/>
          </spine>
        </package>
        """
        let c1 = """
        <?xml version="1.0" encoding="utf-8"?>
        <html xmlns="http://www.w3.org/1999/xhtml"><head><title>One</title></head><body><h1>One</h1><p>Opening chapter.</p></body></html>
        """
        let c2 = """
        <?xml version="1.0" encoding="utf-8"?>
        <html xmlns="http://www.w3.org/1999/xhtml"><head><title>Two</title></head><body><h1>Two</h1><p>Second chapter \(extraText).</p></body></html>
        """
        return build(entries: [
            ("mimetype", Data("application/epub+zip".utf8)),
            ("META-INF/container.xml", Data(container.utf8)),
            ("OEBPS/content.opf", Data(opf.utf8)),
            ("OEBPS/c1.xhtml", Data(c1.utf8)),
            ("OEBPS/c2.xhtml", Data(c2.utf8)),
        ])
    }
}

private extension Data {
    mutating func appendUInt16LE(_ value: UInt16) {
        var le = value.littleEndian
        Swift.withUnsafeBytes(of: &le) { append(contentsOf: $0) }
    }

    mutating func appendUInt32LE(_ value: UInt32) {
        var le = value.littleEndian
        Swift.withUnsafeBytes(of: &le) { append(contentsOf: $0) }
    }
}

final class EbookManagerKitTests: XCTestCase {
    @MainActor
    func testWKWebViewBlocksBookScriptsAndRemoteRequestsWhileRunningReaderScript() async throws {
        _ = NSApplication.shared
        let server = try LocalProbeServer()
        defer { server.stop() }

        let source = HostileChapterSource(port: server.port)
        let settled = expectation(description: "reader script landed and reported place")
        var places: [PagePlaceReport] = []
        var failureMessage: String?

        let controller = try await ReaderWebController.create(source: source) { event in
            switch event {
            case let .place(report):
                places.append(report)
                if places.count == 1 {
                    settled.fulfill()
                }
            case let .failed(reason) where places.isEmpty && failureMessage == nil:
                failureMessage = reason
                settled.fulfill()
            default:
                break
            }
        }
        let hostWindow = NSWindow(
            contentRect: CGRect(x: 80, y: 80, width: 520, height: 420),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        hostWindow.isReleasedWhenClosed = false
        hostWindow.alphaValue = 0.0
        hostWindow.contentView = controller.webView
        hostWindow.orderBack(nil)

        _ = controller.load(chapter: 0, landing: .start)
        await fulfillment(of: [settled], timeout: 10.0)

        XCTAssertNil(failureMessage, "Unexpected WebKit load failure: \(failureMessage ?? "")")
        XCTAssertEqual(places.first?.page, 0)
        XCTAssertGreaterThan(places.first?.pages ?? 0, 1)

        let moved = await controller.turn(by: 1, animate: false)
        XCTAssertTrue(moved)
        XCTAssertEqual(places.last?.page, 1)

        try await Task.sleep(nanoseconds: 200_000_000)
        XCTAssertEqual(server.hits, 0, "No remote image or script request may leave the reader WKWebView")
        let titleInPage = try await controller.evaluateInPageWorld("document.getElementById('target').click(); return document.title") as? String
        XCTAssertEqual(titleInPage, "Chapter", "Inline book script and onclick handler must not execute under CSP")

        controller.shutDown()
        hostWindow.contentView = nil
        hostWindow.orderOut(nil)
    }

    func testBridgeServiceEndToEndOnMacOS() async throws {
        let scratch = FileManager.default.temporaryDirectory
            .appendingPathComponent("ebook-macos-kit-test-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: scratch, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: scratch) }

        let libDir = scratch.appendingPathComponent("library", isDirectory: true).path
        let exportDir = scratch.appendingPathComponent("device", isDirectory: true)
        try FileManager.default.createDirectory(at: exportDir, withIntermediateDirectories: true)

        let bookURL = scratch.appendingPathComponent("harbor.epub")
        try StoredZipBuilder.syntheticEPUB(title: "Harbor Watch", extraText: "first edition").write(to: bookURL)

        let bookId: String
        do {
            let service = BridgeService(directory: libDir)
            async let initialBrowse = service.browse(query: "", sort: [])
            async let initialSurvey = service.surveyUnsurveyed(limit: 8)
            let (emptySnapshot, surveyedCount) = try await (initialBrowse, initialSurvey)
            XCTAssertEqual(emptySnapshot.count, 0)
            XCTAssertEqual(surveyedCount, 0)

            let imported = try await service.importPaths([bookURL.path])
            XCTAssertEqual(imported.count, 1)
            guard case let .added(book) = imported[0].outcome else {
                return XCTFail("Expected added book")
            }
            bookId = book.id
            let handle = try await service.openBook(bookId: book.id)
            handle.enqueueSavePlace(chapter: 0, offset: 0, page: 0, pages: 1) { XCTFail($0) }
            try handle.flushSaveSynchronously(chapter: 1, offset: 7, page: 0, pages: 2)
        }

        let reopenedService = BridgeService(directory: libDir)
        let reopenedHandle = try await reopenedService.openBook(bookId: bookId)
        let restored = try await reopenedHandle.loadPlace()
        XCTAssertEqual(restored?.chapter, 1)
        XCTAssertEqual(restored?.offset, 7)

        let firstExport = try await reopenedService.exportBook(bookId: bookId, destinationDir: exportDir.path)
        if case .copied = firstExport {} else { XCTFail("Expected copied on first export") }
        let secondExport = try await reopenedService.exportBook(bookId: bookId, destinationDir: exportDir.path)
        if case .alreadyPresent = secondExport {} else { XCTFail("Expected alreadyPresent on second export") }
    }

    @MainActor
    func testDesktopWalkthroughFolderImportReviewEditReadAndExportOnMacOS() async throws {
        _ = NSApplication.shared
        let scratch = FileManager.default.temporaryDirectory
            .appendingPathComponent("ebook-macos-walkthrough-\(UUID().uuidString)", isDirectory: true)
        let incoming = scratch.appendingPathComponent("incoming", isDirectory: true)
        let exportDir = scratch.appendingPathComponent("device", isDirectory: true)
        let libDir = scratch.appendingPathComponent("library", isDirectory: true).path
        try FileManager.default.createDirectory(at: incoming, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: exportDir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: scratch) }

        let originalBytes = StoredZipBuilder.syntheticEPUB(title: "The Lantern Atlas", extraText: "original text")
        let revisedBytes = StoredZipBuilder.syntheticEPUB(title: "The Lantern Atlas", extraText: "revised edition paragraph " + String(repeating: "harbor ", count: 250))
        try originalBytes.write(to: incoming.appendingPathComponent("01-book.epub"))
        try originalBytes.write(to: incoming.appendingPathComponent("02-duplicate.epub"))
        try revisedBytes.write(to: incoming.appendingPathComponent("03-revised.epub"))
        try Data("broken archive".utf8).write(to: incoming.appendingPathComponent("04-broken.epub"))

        let bookId: String
        let savedOffset: UInt32
        do {
            let service = BridgeService(directory: libDir)
            let results = try await service.importPaths([incoming.path])
            XCTAssertEqual(results.count, 4)
            guard case let .added(addedBook) = results[0].outcome else {
                return XCTFail("Expected first file Added")
            }
            bookId = addedBook.id
            if case .duplicate = results[1].outcome {} else { XCTFail("Expected second file Duplicate") }
            guard case let .needsReview(reviewToken, _, _, _, _, candidates) = results[2].outcome else {
                return XCTFail("Expected third file NeedsReview")
            }
            XCTAssertEqual(candidates.map(\.id), [bookId])
            if case .failed = results[3].outcome {} else { XCTFail("Expected fourth file Failed") }

            let beforeSettle = try await service.browse(query: "", sort: [])
            XCTAssertEqual(beforeSettle.count, 1)

            let replaced = try await service.settleReview(token: reviewToken, choice: .replace(bookId: bookId))
            guard case let .replaced(replacedBook) = replaced.outcome else {
                return XCTFail("Expected settled review Replaced")
            }
            XCTAssertEqual(replacedBook.id, bookId)
            XCTAssertFalse(replaced.publicationPending)
            do {
                _ = try await service.settleReview(token: reviewToken, choice: .keepSeparate)
                XCTFail("A settled review token must not import again")
            } catch BridgeError.InvalidInput {}
            try await service.discardReviews(tokens: [reviewToken])
            let afterSettle = try await service.browse(query: "", sort: [])
            XCTAssertEqual(afterSettle.count, 1)

            let updated = try await service.editBook(
                id: bookId,
                edit: BookEdit(
                    title: "The Coastal Lantern",
                    authors: ["Clara Vance"],
                    sortTitle: "Coastal Lantern",
                    authorSorts: ["Vance, Clara"],
                    seriesName: "Tide Guides",
                    seriesPosition: "2"
                )
            )
            XCTAssertFalse(updated.publicationPending)
            XCTAssertEqual(updated.book.title, "The Coastal Lantern")
            XCTAssertEqual(updated.book.sortTitle, "Coastal Lantern")
            XCTAssertEqual(updated.book.series.first?.name, "Tide Guides")
            let jump = try await service.quickJump(query: "vance", limit: 5)
            XCTAssertEqual(jump.books.map(\.id), [bookId])
            XCTAssertEqual(jump.authors.map(\.name), ["Clara Vance"])

            let handle = try await service.openBook(bookId: bookId)
            let chapterSource = OpenBookChapterSource(handle: handle)
            let landed = expectation(description: "walkthrough chapter 2 landed")
            var latestReport: PagePlaceReport?
            let web = try await ReaderWebController.create(source: chapterSource) { event in
                if case let .place(report) = event {
                    latestReport = report
                    if report.page == 0 {
                        landed.fulfill()
                    }
                }
            }
            let win = NSWindow(
                contentRect: CGRect(x: 80, y: 80, width: 480, height: 360),
                styleMask: [.borderless],
                backing: .buffered,
                defer: false
            )
            win.isReleasedWhenClosed = false
            win.alphaValue = 0.0
            win.contentView = web.webView
            win.orderBack(nil)

            _ = web.load(chapter: 1, landing: .start)
            await fulfillment(of: [landed], timeout: 10.0)
            _ = await web.turn(by: 1, animate: false)
            let finalPlace = try XCTUnwrap(latestReport)
            savedOffset = finalPlace.offset
            try handle.flushSaveSynchronously(
                chapter: 1,
                offset: finalPlace.offset,
                page: finalPlace.page,
                pages: finalPlace.pages
            )

            web.shutDown()
            win.contentView = nil
            win.orderOut(nil)
        }

        let reopened = BridgeService(directory: libDir)
        let found = try await reopened.browse(query: "tide guides", sort: [SortRule(field: .author, descending: false)])
        XCTAssertEqual(found.count, 1)
        XCTAssertEqual(found.rows.first?.title, "The Coastal Lantern")

        let reopenedBook = try await reopened.openBook(bookId: bookId)
        let restoredPlace = try await reopenedBook.loadPlace()
        XCTAssertEqual(restoredPlace?.chapter, 1)
        XCTAssertEqual(restoredPlace?.offset, savedOffset)

        let export1 = try await reopened.exportBook(bookId: bookId, destinationDir: exportDir.path)
        guard case let .copied(writtenPath) = export1 else {
            return XCTFail("Expected copied on first export")
        }
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: writtenPath)), revisedBytes)

        let export2 = try await reopened.exportBook(bookId: bookId, destinationDir: exportDir.path)
        XCTAssertEqual(export2, .alreadyPresent(destination: writtenPath))

        let occupied = URL(fileURLWithPath: writtenPath)
        try originalBytes.write(to: occupied)
        let collision = try await reopened.exportBook(bookId: bookId, destinationDir: exportDir.path)
        guard case let .copied(alternate) = collision else {
            return XCTFail("Expected a separate destination for the revised book")
        }
        XCTAssertNotEqual(alternate, writtenPath)
        XCTAssertEqual(try Data(contentsOf: occupied), originalBytes)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: alternate)), revisedBytes)

        let restoredRoot = scratch.appendingPathComponent("restored", isDirectory: true)
        try FileManager.default.createDirectory(at: restoredRoot, withIntermediateDirectories: true)
        try FileManager.default.copyItem(
            at: URL(fileURLWithPath: libDir).appendingPathComponent("portable"),
            to: restoredRoot.appendingPathComponent("portable")
        )
        let recovered = BridgeService(directory: restoredRoot.path)
        try await recovered.refresh()
        let recoveredRows = try await recovered.browse(query: "Clara Vance", sort: [])
        XCTAssertEqual(recoveredRows.rows.map(\.id), [bookId])
        XCTAssertEqual(recoveredRows.rows.first?.title, "The Coastal Lantern")
        let recoveredBook = try await recovered.openBook(bookId: bookId)
        let recoveredPlace = try await recoveredBook.loadPlace()
        XCTAssertEqual(recoveredPlace?.offset, savedOffset)
        XCTAssertEqual(recoveredPlace?.chapter, 1)
    }
}

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
        let markup = """
        <h1>Chapter One</h1>
        <script type="text/javascript">document.title = "PWNED"; fetch("http://127.0.0.1:\(port)/script");</script>
        <p onclick="document.title='CLICKED'">Safe paragraph text inside isolated WKWebView.</p>
        <img src="http://127.0.0.1:\(port)/remote.png" alt="Blocked remote image"/>
        """
        return ServedChapter(
            markup: markup,
            length: 62,
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

final class EbookManagerKitTests: XCTestCase {
    @MainActor
    func testWKWebViewBlocksBookScriptsAndRemoteRequestsWhileRunningReaderScript() async throws {
        let server = try LocalProbeServer()
        defer { server.stop() }

        let source = HostileChapterSource(port: server.port)
        let landedPlace = expectation(description: "reader script landed and reported place")
        var receivedPlace: PagePlaceReport?

        let controller = try await ReaderWebController.create(source: source) { event in
            if case let .place(report) = event, receivedPlace == nil {
                receivedPlace = report
                landedPlace.fulfill()
            }
        }
        defer { controller.shutDown() }

        _ = controller.load(chapter: 0, landing: .start)
        await fulfillment(of: [landedPlace], timeout: 10.0)
        try await Task.sleep(nanoseconds: 250_000_000)

        XCTAssertNotNil(receivedPlace)
        XCTAssertEqual(server.hits, 0, "No remote image or script request may leave the reader WKWebView")
        XCTAssertNotEqual(controller.webView.title, "PWNED", "Inline book script must not execute")
    }
}

import AppKit
import Foundation
import ImageIO

/// Off-main-thread loader and bounded in-memory cache for verified cover thumbnails.
///
/// At most `maxConcurrentLoads` covers are fetched, remade and decoded at once. Waiting requests are served
/// newest first, so the covers on screen now come before ones scrolled past, and the oldest are dropped
/// once more than `maxWaiting` are queued; a cover asked for again later is queued again.
@MainActor
public final class CoverCache: ObservableObject {
    public typealias Loader = @Sendable (_ bookId: String) async -> Data?

    private struct Request {
        let digest: String
        let bookId: String
    }

    @Published private(set) var revision: UInt64 = 0
    private var images: [String: NSImage] = [:]
    private var order: [String] = []
    private var waiting: [Request] = []
    private var inFlight: Set<String> = []
    private var unrecoverable: Set<String> = []
    private let maxCached = 192
    let maxConcurrentLoads: Int
    let maxWaiting: Int
    private let load: Loader

    public convenience init(bridge: BridgeService) {
        self.init(loader: { bookId in
            try? await bridge.coverThumbnail(bookId: bookId)
        })
    }

    init(maxConcurrentLoads: Int = 3, maxWaiting: Int = 48, loader: @escaping Loader) {
        self.maxConcurrentLoads = maxConcurrentLoads
        self.maxWaiting = maxWaiting
        self.load = loader
    }

    var loadingCount: Int { inFlight.count }
    var waitingCount: Int { waiting.count }

    public func image(forDigest digest: String?) -> NSImage? {
        guard let digest else { return nil }
        return images[digest]
    }

    public func request(digest: String?, bookId: String) {
        guard let digest,
              images[digest] == nil,
              !inFlight.contains(digest),
              !unrecoverable.contains("\(digest):\(bookId)")
        else {
            return
        }
        waiting.removeAll { $0.digest == digest }
        waiting.append(Request(digest: digest, bookId: bookId))
        if waiting.count > maxWaiting {
            waiting.removeFirst(waiting.count - maxWaiting)
        }
        pump()
    }

    private func pump() {
        while inFlight.count < maxConcurrentLoads, let next = waiting.popLast() {
            inFlight.insert(next.digest)
            let load = self.load
            Task {
                let decoded: NSImage? = await Task.detached(priority: .utility) {
                    guard let data = await load(next.bookId) else { return nil }
                    return Self.decode(data)
                }.value
                finish(next, decoded: decoded)
            }
        }
    }

    private func finish(_ request: Request, decoded: NSImage?) {
        inFlight.remove(request.digest)
        if let decoded {
            images[request.digest] = decoded
            order.append(request.digest)
            while order.count > maxCached {
                let oldest = order.removeFirst()
                images.removeValue(forKey: oldest)
            }
            revision &+= 1
        } else {
            unrecoverable.insert("\(request.digest):\(request.bookId)")
        }
        pump()
    }

    nonisolated static func decode(_ data: Data) -> NSImage? {
        let options = [kCGImageSourceShouldCache: false] as CFDictionary
        guard let source = CGImageSourceCreateWithData(data as CFData, options),
              let image = CGImageSourceCreateImageAtIndex(
                  source,
                  0,
                  [kCGImageSourceShouldCacheImmediately: true] as CFDictionary
              )
        else {
            return nil
        }
        return NSImage(cgImage: image, size: NSSize(width: image.width, height: image.height))
    }
}

import AppKit
import Foundation

/// Off-main-thread loader and bounded in-memory cache for verified cover thumbnails.
@MainActor
public final class CoverCache: ObservableObject {
    @Published private(set) var revision: UInt64 = 0
    private var images: [String: NSImage] = [:]
    private var order: [String] = []
    private var inFlight: Set<String> = []
    private var unrecoverable: Set<String> = []
    private let maxCached = 192
    private let bridge: BridgeService

    public init(bridge: BridgeService) {
        self.bridge = bridge
    }

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
        inFlight.insert(digest)
        let service = bridge
        Task {
            let decoded: NSImage? = await Task.detached(priority: .utility) {
                guard let data = try? await service.coverThumbnail(bookId: bookId),
                      let image = NSImage(data: data)
                else {
                    return nil
                }
                return image
            }.value
            inFlight.remove(digest)
            if let decoded {
                images[digest] = decoded
                order.append(digest)
                while order.count > maxCached {
                    let oldest = order.removeFirst()
                    images.removeValue(forKey: oldest)
                }
                revision &+= 1
            } else {
                unrecoverable.insert("\(digest):\(bookId)")
            }
        }
    }
}

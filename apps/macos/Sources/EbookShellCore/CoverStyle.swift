import CoreGraphics
import CryptoKit
import Foundation

/// Cover sizing, image placement, and deterministic generated binding colours from `docs/design/interface.md`.
public enum CoverSizeChoice: String, CaseIterable, Sendable {
    case small
    case medium
    case large

    public var label: String {
        switch self {
        case .small: return "Small"
        case .medium: return "Medium"
        case .large: return "Large"
        }
    }

    public var gridWidth: CGFloat {
        switch self {
        case .small: return 96
        case .medium: return 128
        case .large: return 160
        }
    }

    public var listWidth: CGFloat {
        switch self {
        case .small: return 40
        case .medium: return 48
        case .large: return 60
        }
    }
}

public struct BindingSwatch: Equatable, Sendable {
    public let name: String
    public let backgroundRGB: UInt32
    public let textRGB: UInt32
}

public struct CoverPlacement: Equatable, Sendable {
    public let rect: CGRect
    public let fillsFrame: Bool
}

public struct GridMetrics: Equatable, Sendable {
    public let columns: Int
    public let itemWidth: CGFloat
    public let columnSpacing: CGFloat
    public let rowSpacing: CGFloat
    public let captionSpacing: CGFloat
}

public enum CoverStyle {
    public static let columnGap: CGFloat = 20
    public static let captionGap: CGFloat = 12
    public static let rowGap: CGFloat = 28

    public static let bindings: [BindingSwatch] = [
        BindingSwatch(name: "Navy", backgroundRGB: 0x24405C, textRGB: 0xF1E6CC),
        BindingSwatch(name: "Oxblood", backgroundRGB: 0x6F2A2A, textRGB: 0xF1E6CC),
        BindingSwatch(name: "Forest", backgroundRGB: 0x315047, textRGB: 0xF1E6CC),
        BindingSwatch(name: "Umber", backgroundRGB: 0x7A3B22, textRGB: 0xF1E6CC),
        BindingSwatch(name: "Plum", backgroundRGB: 0x51446B, textRGB: 0xF1E6CC),
        BindingSwatch(name: "Ochre", backgroundRGB: 0xB98A3C, textRGB: 0x23190A),
        BindingSwatch(name: "Slate", backgroundRGB: 0x465366, textRGB: 0xF1E6CC),
        BindingSwatch(name: "Moss", backgroundRGB: 0x2F3A2B, textRGB: 0xF1E6CC),
    ]

    public static func bindingIndex(forBookID bookID: String) -> Int {
        let digest = Array(SHA256.hash(data: Data(bookID.utf8)))
        let head = UInt32(digest[0]) << 24
            | UInt32(digest[1]) << 16
            | UInt32(digest[2]) << 8
            | UInt32(digest[3])
        return Int(head % 8)
    }

    public static func binding(forBookID bookID: String) -> BindingSwatch {
        bindings[bindingIndex(forBookID: bookID)]
    }

    public static func cornerRadius(forWidth width: CGFloat) -> CGFloat {
        width >= 200 ? 3 : 2
    }

    public static func frameHeight(forWidth width: CGFloat) -> CGFloat {
        (width * 1.5).rounded()
    }

    public static func showsBindingAuthor(authors: String, coverWidth: CGFloat) -> Bool {
        !authors.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && coverWidth >= 110
    }

    public static func placeImage(imageSize: CGSize, in frameSize: CGSize) -> CoverPlacement {
        guard imageSize.width > 0, imageSize.height > 0, frameSize.width > 0, frameSize.height > 0 else {
            return CoverPlacement(rect: CGRect(origin: .zero, size: frameSize), fillsFrame: false)
        }
        let aspect = imageSize.width / imageSize.height
        let fills = (0.56 ... 0.80).contains(aspect)
        let scaleX = frameSize.width / imageSize.width
        let scaleY = frameSize.height / imageSize.height
        let scale = fills ? max(scaleX, scaleY) : min(scaleX, scaleY)
        let drawnWidth = imageSize.width * scale
        let drawnHeight = imageSize.height * scale
        let rect = CGRect(
            x: (frameSize.width - drawnWidth) / 2.0,
            y: (frameSize.height - drawnHeight) / 2.0,
            width: drawnWidth,
            height: drawnHeight
        )
        return CoverPlacement(rect: rect, fillsFrame: fills)
    }

    public static func gridMetrics(availableWidth: CGFloat, choice: CoverSizeChoice) -> GridMetrics {
        let base = choice.gridWidth
        let usable = max(1, availableWidth)
        let rawColumns = Int(floor((usable + columnGap) / (base + columnGap)))
        let columns = max(1, rawColumns)
        let shared = (usable - CGFloat(columns - 1) * columnGap) / CGFloat(columns)
        let capped = min(usable, min(shared, base * 1.25))
        return GridMetrics(
            columns: columns,
            itemWidth: max(1, floor(capped)),
            columnSpacing: columnGap,
            rowSpacing: rowGap,
            captionSpacing: captionGap
        )
    }
}

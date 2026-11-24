import AppKit
import EbookShellCore
import SwiftUI

public enum BookCoverDisplayKind: Sendable {
    case grid
    case list
    case detail
}

extension Color {
    init(rgb24: UInt32, opacity: Double = 1.0) {
        let r = Double((rgb24 >> 16) & 0xFF) / 255.0
        let g = Double((rgb24 >> 8) & 0xFF) / 255.0
        let b = Double(rgb24 & 0xFF) / 255.0
        self.init(.sRGB, red: r, green: g, blue: b, opacity: opacity)
    }
}

/// Draws a book cover or generated type binding according to `docs/design/interface.md`.
public struct BookCoverView: View {
    public let bookId: String
    public let title: String
    public let authors: String
    public let hasCover: Bool
    public let coverDigest: String?
    public let coverRGB: UInt32?
    public let rightToLeft: Bool
    public let progress: Float?
    public let width: CGFloat
    public let kind: BookCoverDisplayKind
    @ObservedObject var cache: CoverCache

    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.colorSchemeContrast) private var contrast
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var isHovered = false
    @State private var imageShown = false

    public init(
        bookId: String,
        title: String,
        authors: String,
        hasCover: Bool,
        coverDigest: String?,
        coverRGB: UInt32?,
        rightToLeft: Bool,
        progress: Float?,
        width: CGFloat,
        kind: BookCoverDisplayKind,
        cache: CoverCache
    ) {
        self.bookId = bookId
        self.title = title
        self.authors = authors
        self.hasCover = hasCover
        self.coverDigest = coverDigest
        self.coverRGB = coverRGB
        self.rightToLeft = rightToLeft
        self.progress = progress
        self.width = width
        self.kind = kind
        self.cache = cache
    }

    private var height: CGFloat {
        CoverStyle.frameHeight(forWidth: width)
    }

    private var cornerRadius: CGFloat {
        CoverStyle.cornerRadius(forWidth: width)
    }

    private var highContrast: Bool {
        contrast == .increased
    }

    private var shadowOpacity: Double {
        colorScheme == .dark ? 0.28 : 0.16
    }

    private var riseOffset: CGFloat {
        (kind == .grid && isHovered && !reduceMotion) ? -2 : 0
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            coverFrame
                .frame(width: width, height: height)
                .background(shadowLayer)
                .overlay(contrastOutline)
                .offset(y: riseOffset)
                .onHover { hovering in
                    guard kind == .grid else { return }
                    if reduceMotion {
                        isHovered = false
                    } else {
                        withAnimation(.smooth(duration: 0.15)) {
                            isHovered = hovering
                        }
                    }
                }
                .onAppear {
                    requestCoverIfNeeded()
                }
                .onChange(of: coverDigest) { _, _ in
                    imageShown = cache.image(forDigest: coverDigest) != nil
                    requestCoverIfNeeded()
                }
                .onChange(of: cache.revision) { _, _ in
                    if cache.image(forDigest: coverDigest) != nil, !imageShown {
                        if reduceMotion {
                            imageShown = true
                        } else {
                            withAnimation(.smooth(duration: 0.18)) {
                                imageShown = true
                            }
                        }
                    }
                }

            if kind == .grid {
                progressGap
            }
        }
        .accessibilityHidden(true)
    }

    private func requestCoverIfNeeded() {
        guard hasCover, let coverDigest else { return }
        if cache.image(forDigest: coverDigest) != nil {
            imageShown = true
        } else {
            cache.request(digest: coverDigest, bookId: bookId)
        }
    }

    @ViewBuilder
    private var coverFrame: some View {
        let shape = RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
        ZStack {
            if !hasCover {
                generatedBinding
                if !highContrast, kind != .list, width >= 64 {
                    spineShade
                }
            } else {
                artworkLayer
            }
        }
        .clipShape(shape)
        .overlay {
            if !highContrast {
                shape.strokeBorder(
                    colorScheme == .dark ? Color.white.opacity(0.18) : Color.black.opacity(0.12),
                    lineWidth: 1
                )
            }
        }
    }

    @ViewBuilder
    private var artworkLayer: some View {
        let frameSize = CGSize(width: width, height: height)
        let loaded = cache.image(forDigest: coverDigest)
        let placement = loaded.map { CoverStyle.placeImage(imageSize: $0.size, in: frameSize) }
        let fills = placement?.fillsFrame ?? true

        ZStack {
            if !fills {
                Color(nsColor: .quaternaryLabelColor)
            }
            if fills || !imageShown || loaded == nil {
                if let rgb = coverRGB {
                    Color(rgb24: rgb)
                } else {
                    Color(nsColor: .quaternaryLabelColor)
                }
            }
            if let loaded, let placement {
                Image(nsImage: loaded)
                    .resizable()
                    .interpolation(.high)
                    .frame(width: placement.rect.width, height: placement.rect.height)
                    .position(x: placement.rect.midX, y: placement.rect.midY)
                    .opacity(imageShown ? 1 : 0)
            }
            if !highContrast, fills, kind != .list, width >= 64 {
                spineShade
            }
        }
    }

    private var generatedBinding: some View {
        let swatch = CoverStyle.binding(forBookID: bookId)
        let pad = (width * 0.12).rounded()
        let ink = Color(rgb24: swatch.textRGB)
        return ZStack(alignment: .topLeading) {
            Color(rgb24: swatch.backgroundRGB)
            if width >= 64 {
                VStack(alignment: .leading, spacing: max(4, (width * 0.05).rounded())) {
                    Text(title)
                        .font(.system(size: max(6, width * 0.11), weight: .semibold, design: .serif))
                        .foregroundStyle(ink)
                        .lineLimit(4)
                        .truncationMode(.tail)
                        .multilineTextAlignment(.leading)

                    Rectangle()
                        .fill(ink)
                        .frame(height: 1)

                    Spacer(minLength: 0)

                    if CoverStyle.showsBindingAuthor(authors: authors, coverWidth: width) {
                        Text(authors)
                            .font(.system(size: width * 0.09, weight: .regular))
                            .foregroundStyle(ink)
                            .lineLimit(2)
                            .truncationMode(.tail)
                            .multilineTextAlignment(.leading)
                    }
                }
                .padding(pad)
            }
        }
    }

    private var spineShade: some View {
        let shadeWidth = max(1, width * 0.05)
        let start: UnitPoint = rightToLeft ? .trailing : .leading
        let end: UnitPoint = rightToLeft ? .leading : .trailing
        return HStack(spacing: 0) {
            if rightToLeft {
                Spacer(minLength: 0)
            }
            LinearGradient(
                colors: [Color.black.opacity(0.14), Color.black.opacity(0)],
                startPoint: start,
                endPoint: end
            )
            .frame(width: shadeWidth)
            if !rightToLeft {
                Spacer(minLength: 0)
            }
        }
    }

    @ViewBuilder
    private var shadowLayer: some View {
        if !highContrast {
            let shape = RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
            ZStack {
                shape
                    .fill(Color.black)
                    .shadow(
                        color: Color.black.opacity(shadowOpacity),
                        radius: isHovered ? 3 : 2,
                        x: 0,
                        y: isHovered ? 2 : 1
                    )
                if kind == .grid || kind == .detail {
                    shape
                        .inset(by: 4)
                        .fill(Color.black)
                        .shadow(
                            color: Color.black.opacity(shadowOpacity),
                            radius: isHovered ? 18 : 14,
                            x: 0,
                            y: isHovered ? 9 : 6
                        )
                }
            }
        }
    }

    @ViewBuilder
    private var contrastOutline: some View {
        if highContrast {
            RoundedRectangle(cornerRadius: cornerRadius + 1, style: .continuous)
                .stroke(Color.primary, lineWidth: 1)
                .padding(-1)
        }
    }

    private var progressGap: some View {
        ZStack(alignment: .leading) {
            Color.clear
                .frame(width: width, height: CoverStyle.captionGap)
            if let progress, progress > 0, progress < 1 {
                ZStack(alignment: .leading) {
                    Rectangle()
                        .fill(Color(nsColor: .separatorColor))
                        .frame(width: width, height: 2)
                    Rectangle()
                        .fill(Color(nsColor: .secondaryLabelColor))
                        .frame(width: max(1, (width * CGFloat(progress)).rounded()), height: 2)
                }
            }
        }
    }
}

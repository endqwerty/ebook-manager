import Foundation

/// Explicit state machine for reader place-keeping, matching `apps/desktop/src/reader/place.rs`.
public struct ChapterLoadRecord: Equatable, Sendable {
    public let number: UInt32
    public let chapter: UInt32
    public let length: UInt32

    public init(number: UInt32, chapter: UInt32, length: UInt32) {
        self.number = number
        self.chapter = chapter
        self.length = length
    }
}

public struct DisplayedPlace: Equatable, Sendable {
    public let chapter: UInt32
    public let offset: UInt32
    public let page: UInt32
    public let pages: UInt32

    public init(chapter: UInt32, offset: UInt32, page: UInt32, pages: UInt32) {
        self.chapter = chapter
        self.offset = offset
        self.page = page
        self.pages = pages
    }
}

public struct PagePlaceReport: Equatable, Sendable {
    public let load: ChapterLoadRecord
    public let offset: UInt32
    public let page: UInt32
    public let pages: UInt32
    public let count: UInt32
    public let moved: Bool

    public init(
        load: ChapterLoadRecord,
        offset: UInt32,
        page: UInt32,
        pages: UInt32,
        count: UInt32,
        moved: Bool
    ) {
        self.load = load
        self.offset = offset
        self.page = page
        self.pages = pages
        self.count = count
        self.moved = moved
    }
}

public enum PlaceInput: Equatable, Sendable {
    case asked(number: UInt32)
    case ready(load: ChapterLoadRecord)
    case report(PagePlaceReport)
    case failed
    case timer
    case leave
}

public enum PlaceEffect: Equatable, Sendable {
    case show(DisplayedPlace)
    case blank
    case startTimer
    case stopTimer
    case submit(DisplayedPlace)
    case awaitLibrary
    case endPage
}

private struct MoveOrder: Comparable, Equatable {
    let loadNumber: UInt32
    let moveCount: UInt32

    static func < (lhs: MoveOrder, rhs: MoveOrder) -> Bool {
        if lhs.loadNumber != rhs.loadNumber {
            return lhs.loadNumber < rhs.loadNumber
        }
        return lhs.moveCount < rhs.moveCount
    }
}

private struct ReportRank: Comparable, Equatable {
    let order: MoveOrder
    let isReflow: Bool

    static func < (lhs: ReportRank, rhs: ReportRank) -> Bool {
        if lhs.order != rhs.order {
            return lhs.order < rhs.order
        }
        return !lhs.isReflow && rhs.isReflow
    }
}

public struct PlaceKeeper: Sendable {
    private var heldEntry: (place: DisplayedPlace, order: MoveOrder)?
    private var submittedOrder: MoveOrder?
    private var showingLoad: (number: UInt32, ready: Bool)?
    private var displayedEntry: (place: DisplayedPlace, rank: ReportRank)?
    private var timerRunning = false
    public private(set) var isGone = false

    public init() {}

    public var held: DisplayedPlace? {
        heldEntry?.place
    }

    public var displayed: (place: DisplayedPlace, loadNumber: UInt32)? {
        displayedEntry.map { ($0.place, $0.rank.order.loadNumber) }
    }

    public mutating func step(_ input: PlaceInput) -> [PlaceEffect] {
        guard !isGone else { return [] }
        var effects: [PlaceEffect] = []
        switch input {
        case let .asked(number):
            showingLoad = (number, false)
            displayedEntry = nil
            effects.append(.blank)
        case let .ready(load):
            if let current = showingLoad, current.number == load.number {
                showingLoad = (current.number, true)
            }
        case let .report(report):
            handleReport(report, effects: &effects)
        case .failed:
            if let current = showingLoad {
                showingLoad = (current.number, false)
            }
            displayedEntry = nil
        case .timer:
            timerRunning = false
            if let held = heldEntry, submittedOrder != held.order {
                submittedOrder = held.order
                effects.append(.submit(held.place))
            }
        case .leave:
            isGone = true
            if timerRunning {
                timerRunning = false
                effects.append(.stopTimer)
            }
            if let held = heldEntry {
                submittedOrder = held.order
                effects.append(.submit(held.place))
            }
            effects.append(.awaitLibrary)
            effects.append(.endPage)
        }
        return effects
    }

    private mutating func handleReport(_ report: PagePlaceReport, effects: inout [PlaceEffect]) {
        guard report.pages > 0, report.page < report.pages, report.offset <= report.load.length else {
            return
        }
        let place = DisplayedPlace(
            chapter: report.load.chapter,
            offset: report.offset,
            page: report.page,
            pages: report.pages
        )
        let order = MoveOrder(loadNumber: report.load.number, moveCount: report.count)
        if report.moved, heldEntry.map({ order > $0.order }) ?? true {
            heldEntry = (place, order)
            timerRunning = true
            effects.append(.startTimer)
        }
        let isShowingReady = showingLoad.map { $0.ready && $0.number == report.load.number } ?? false
        let rank = ReportRank(order: order, isReflow: !report.moved)
        if isShowingReady, displayedEntry.map({ rank >= $0.rank }) ?? true {
            displayedEntry = (place, rank)
            effects.append(.show(place))
        }
    }
}

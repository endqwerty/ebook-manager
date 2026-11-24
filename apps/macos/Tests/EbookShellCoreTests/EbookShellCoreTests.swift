import CoreGraphics
import EbookShellCore
import Foundation
import XCTest

final class EbookShellCoreTests: XCTestCase {
    func testLibraryLocationPrecedence() {
        let defaultAppSupport = URL(fileURLWithPath: "/Users/test/Library/Application Support", isDirectory: true)
        XCTAssertEqual(
            LibraryLocation.resolve(
                arguments: ["EbookManager", "--library", "/tmp/cli-lib"],
                environment: [LibraryLocation.envKey: "/tmp/env-lib"],
                defaultApplicationSupport: defaultAppSupport
            ),
            "/tmp/cli-lib"
        )
        XCTAssertEqual(
            LibraryLocation.resolve(
                arguments: ["EbookManager"],
                environment: [LibraryLocation.envKey: "/tmp/env-lib"],
                defaultApplicationSupport: defaultAppSupport
            ),
            "/tmp/env-lib"
        )
        XCTAssertEqual(
            LibraryLocation.resolve(
                arguments: ["EbookManager"],
                environment: [:],
                defaultApplicationSupport: defaultAppSupport
            ),
            "/Users/test/Library/Application Support/Ebook Manager"
        )
    }

    func testDeterministicBindingColoursAndImagePlacement() {
        XCTAssertEqual(CoverStyle.binding(forBookID: "abc").backgroundRGB, 0x2F3A2B)
        XCTAssertEqual(CoverStyle.binding(forBookID: "").backgroundRGB, 0x315047)

        let cropped = CoverStyle.placeImage(
            imageSize: CGSize(width: 240, height: 300),
            in: CGSize(width: 96, height: 150)
        )
        XCTAssertTrue(cropped.fillsFrame)
        XCTAssertEqual(cropped.rect, CGRect(x: -12, y: 0, width: 120, height: 150))

        let fitted = CoverStyle.placeImage(
            imageSize: CGSize(width: 300, height: 300),
            in: CGSize(width: 100, height: 150)
        )
        XCTAssertFalse(fitted.fillsFrame)
        XCTAssertEqual(fitted.rect, CGRect(x: 0, y: 25, width: 100, height: 100))

        let metrics = CoverStyle.gridMetrics(availableWidth: 900, choice: .medium)
        XCTAssertEqual(metrics.columns, 6)
        XCTAssertLessThanOrEqual(metrics.itemWidth, 160)
        XCTAssertFalse(CoverStyle.showsBindingAuthor(authors: "Mira Field", coverWidth: 96))
        XCTAssertTrue(CoverStyle.showsBindingAuthor(authors: "Mira Field", coverWidth: 128))
    }

    func testEditFormNormalization() {
        let form = EditFormState(
            title: "  Moby-Dick  ",
            authors: ["Herman Melville", "A. Editor"],
            sortTitle: "  Whale  ",
            authorSorts: ["Melville, Herman", nil],
            primarySeriesName: "Sea Classics",
            primarySeriesPosition: " 2 "
        )
        XCTAssertTrue(form.isValid)
        XCTAssertEqual(form.normalizedTitle, "Moby-Dick")
        XCTAssertEqual(form.normalizedAuthors, ["Herman Melville", "A. Editor"])
        XCTAssertEqual(form.normalizedSortTitle, "Whale")
        XCTAssertEqual(form.normalizedAuthorSorts, ["Melville, Herman"])
        XCTAssertEqual(form.normalizedSeriesName, "Sea Classics")
        XCTAssertEqual(form.normalizedSeriesPosition, "2")
    }

    func testPlaceKeeperPreservesMoveOrderAndIgnoresReflowForSaves() {
        var keeper = PlaceKeeper()
        let loadA = ChapterLoadRecord(number: 1, chapter: 0, length: 10_000)
        let loadB = ChapterLoadRecord(number: 2, chapter: 1, length: 10_000)

        XCTAssertEqual(keeper.step(.asked(number: 1)), [.blank])
        XCTAssertEqual(keeper.step(.ready(load: loadA)), [])
        _ = keeper.step(.report(PagePlaceReport(load: loadA, offset: 0, page: 0, pages: 10, count: 1, moved: true)))

        _ = keeper.step(.asked(number: 2))
        let lateTurn = keeper.step(.report(PagePlaceReport(load: loadA, offset: 900, page: 1, pages: 10, count: 2, moved: true)))
        XCTAssertEqual(lateTurn, [.startTimer])
        XCTAssertEqual(keeper.held, DisplayedPlace(chapter: 0, offset: 900, page: 1, pages: 10))
        XCTAssertNil(keeper.displayed)

        _ = keeper.step(.ready(load: loadB))
        _ = keeper.step(.report(PagePlaceReport(load: loadB, offset: 200, page: 0, pages: 5, count: 1, moved: true)))
        let timed = keeper.step(.timer)
        XCTAssertEqual(timed, [.submit(DisplayedPlace(chapter: 1, offset: 200, page: 0, pages: 5))])

        let reflow = keeper.step(.report(PagePlaceReport(load: loadB, offset: 200, page: 1, pages: 12, count: 1, moved: false)))
        XCTAssertEqual(reflow, [.show(DisplayedPlace(chapter: 1, offset: 200, page: 1, pages: 12))])
        XCTAssertEqual(keeper.step(.timer), [])

        let leaveEffects = keeper.step(.leave)
        XCTAssertEqual(leaveEffects, [
            .submit(DisplayedPlace(chapter: 1, offset: 200, page: 0, pages: 5)),
            .awaitLibrary,
            .endPage,
        ])
    }

    func testReaderSchemeRouterAndPalettes() {
        let token = "0123456789abcdef0123456789abcdef"
        XCTAssertEqual(
            ReaderSchemeRouter.resolve(url: URL(string: "ebook://\(token)/c/12?l=1")!, expectedToken: token),
            .chapter(index: 12)
        )
        XCTAssertEqual(
            ReaderSchemeRouter.resolve(url: URL(string: "ebook://\(token)/r/7")!, expectedToken: token),
            .resource(id: 7)
        )
        XCTAssertEqual(
            ReaderSchemeRouter.resolve(url: URL(string: "ebook://\(token)/s/reader.css")!, expectedToken: token),
            .stylesheet
        )
        XCTAssertNil(
            ReaderSchemeRouter.resolve(url: URL(string: "ebook://wrongtoken/c/0")!, expectedToken: token)
        )
        XCTAssertNil(
            ReaderSchemeRouter.resolve(url: URL(string: "https://example.com/c/0")!, expectedToken: token)
        )
        XCTAssertEqual(
            ReaderSchemeRouter.parseJump(href: "/c/3#sec%201"),
            InternalJump(chapter: 3, anchor: "sec 1")
        )
        XCTAssertNotNil(ReaderSchemeRouter.validatedExternalAddress("https://example.com/book"))
        XCTAssertNil(ReaderSchemeRouter.validatedExternalAddress("javascript:alert(1)"))

        let lightDefault = ReaderTheme.resolvePalette(choice: .automatic, isDarkWindow: false, increasedContrast: false)
        XCTAssertEqual(lightDefault.pageHex, "#f4ecd8")
        let darkDefault = ReaderTheme.resolvePalette(choice: .automatic, isDarkWindow: true, increasedContrast: false)
        XCTAssertEqual(darkDefault.pageHex, "#2b2b2e")
        let contrastDark = ReaderTheme.resolvePalette(choice: .automatic, isDarkWindow: true, increasedContrast: true)
        XCTAssertEqual(contrastDark.pageHex, "#000000")
        XCTAssertEqual(contrastDark.inkHex, "#ffffff")
    }
}

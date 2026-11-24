import AppKit
import EbookManagerKit
import EbookShellCore
import SwiftUI

@MainActor
final class AppLifecycleDelegate: NSObject, NSApplicationDelegate {
    private var libraryModel: LibraryViewModel?
    private var pendingFiles: [URL] = []

    func attach(_ model: LibraryViewModel) {
        libraryModel = model
        if !pendingFiles.isEmpty {
            model.importURLs(pendingFiles)
            pendingFiles.removeAll()
        }
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApplication.shared.setActivationPolicy(.regular)
    }

    func applicationWillTerminate(_ notification: Notification) {
        libraryModel?.flushAndCloseAllReaders()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    func application(_ application: NSApplication, open urls: [URL]) {
        if let libraryModel {
            libraryModel.importURLs(urls)
        } else {
            pendingFiles.append(contentsOf: urls)
        }
    }
}

@main
struct EbookManagerApp: App {
    @NSApplicationDelegateAdaptor(AppLifecycleDelegate.self) private var appDelegate
    @StateObject private var model = LibraryViewModel(libraryDirectory: LibraryLocation.resolve())

    var body: some Scene {
        WindowGroup {
            LibraryWindowView(model: model)
                .frame(minWidth: 640, minHeight: 460)
                .onAppear {
                    appDelegate.attach(model)
                    model.start()
                }
        }
        .defaultSize(width: 980, height: 680)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("Add Files…") {
                    model.presentAddFilesPanel()
                }
                .keyboardShortcut("o", modifiers: [.command])

                Button("Add Folder…") {
                    model.presentAddFolderPanel()
                }
                .keyboardShortcut("o", modifiers: [.command, .shift])
            }

            CommandMenu("Library") {
                Button("Go to…") {
                    model.showQuickJump = true
                }
                .keyboardShortcut("k", modifiers: [.command])

                Divider()

                Button("Read Book") {
                    model.openSelectedForReading()
                }
                .disabled(model.selectedIds.count != 1)

                Button("Show Details") {
                    model.toggleDetailsForSelected()
                }
                .disabled(model.selectedIds.count != 1)

                Button("Edit Book…") {
                    model.openEditForSelected()
                }
                .keyboardShortcut("i", modifiers: [.command])
                .disabled(model.selectedIds.count != 1)

                Button("Send to Device Folder…") {
                    model.exportSelectedBooks()
                }
                .keyboardShortcut("d", modifiers: [.command])
                .disabled(model.selectedIds.isEmpty)

                Divider()

                Button("Grid View") {
                    model.viewMode = .grid
                }
                .keyboardShortcut("1", modifiers: [.command])

                Button("List View") {
                    model.viewMode = .list
                }
                .keyboardShortcut("2", modifiers: [.command])

                Divider()

                Button("Refresh Library") {
                    model.refreshLibrary()
                }
                .keyboardShortcut("r", modifiers: [.command])
            }
        }
    }

}

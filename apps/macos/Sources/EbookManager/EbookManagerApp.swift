import AppKit
import EbookManagerKit
import EbookShellCore
import SwiftUI

@MainActor
final class AppLifecycleDelegate: NSObject, NSApplicationDelegate {
    var libraryModel: LibraryViewModel?

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
        libraryModel?.importURLs(urls)
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
                    appDelegate.libraryModel = model
                    model.start()
                    runSnapshotAutomationIfRequested(model: model)
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
                .keyboardShortcut(.return, modifiers: [])
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

    @MainActor
    private func runSnapshotAutomationIfRequested(model: LibraryViewModel) {
        let args = ProcessInfo.processInfo.arguments
        guard let snapIdx = args.firstIndex(of: "--snapshot-window"),
              args.indices.contains(snapIdx + 1)
        else {
            return
        }
        let outputPath = args[snapIdx + 1]
        var importURLs: [URL] = []
        if let impIdx = args.firstIndex(of: "--import-silent"),
           args.indices.contains(impIdx + 1)
        {
            importURLs.append(URL(fileURLWithPath: args[impIdx + 1]))
        }
        Task { @MainActor in
            try? await Task.sleep(nanoseconds: 250_000_000)
            if let win = NSApplication.shared.windows.first {
                win.setContentSize(NSSize(width: 980, height: 680))
                win.displayIfNeeded()
            }
            if !importURLs.isEmpty {
                await model.importURLsForSnapshot(importURLs)
            }
            try? await Task.sleep(nanoseconds: 1_200_000_000)
            if let win = NSApplication.shared.windows.first {
                win.setContentSize(NSSize(width: 980, height: 680))
                win.layoutIfNeeded()
                win.displayIfNeeded()
                if let targetView = win.contentView?.superview ?? win.contentView {
                    targetView.layoutSubtreeIfNeeded()
                    let bounds = targetView.bounds
                    if let rep = targetView.bitmapImageRepForCachingDisplay(in: bounds) {
                        targetView.cacheDisplay(in: bounds, to: rep)
                        if let png = rep.representation(using: .png, properties: [:]) {
                            try? png.write(to: URL(fileURLWithPath: outputPath))
                        }
                    }
                }
            }
            NSApplication.shared.terminate(nil)
        }
    }
}

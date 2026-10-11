// Annotator.app: Finder "Open With..." launcher for annotator.
// Receives open-document events and hands each file to `annotator open --no-wait`,
// which registers a review session and opens (or reuses) the browser tab.
import AppKit

final class Launcher: NSObject, NSApplicationDelegate {
    private var handledFiles = false

    func application(_ app: NSApplication, open urls: [URL]) {
        handledFiles = true
        for url in urls where url.isFileURL { review(url) }
        NSApp.terminate(nil)
    }

    func applicationDidFinishLaunching(_ note: Notification) {
        // Opened from Finder with files: application(_:open:) arrives right after launch.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [self] in
            if handledFiles { return }
            // Launched on its own (double-click, Spotlight): let the user pick files.
            NSApp.activate(ignoringOtherApps: true)
            let panel = NSOpenPanel()
            panel.title = "Review with annotator"
            panel.allowsMultipleSelection = true
            panel.canChooseDirectories = false
            if panel.runModal() == .OK { panel.urls.forEach(review) }
            NSApp.terminate(nil)
        }
    }

    private func review(_ url: URL) {
        guard let bin = annotatorPath() else {
            alert("annotator not found", "Install it with `make install` (expected in ~/bin).")
            return
        }
        let p = Process()
        p.executableURL = URL(fileURLWithPath: bin)
        p.arguments = ["open", "--no-wait", url.path]
        let err = Pipe()
        p.standardError = err
        p.standardOutput = FileHandle.nullDevice
        do {
            try p.run()
            p.waitUntilExit()
        } catch {
            alert("Could not run annotator", error.localizedDescription)
            return
        }
        if p.terminationStatus != 0 {
            let msg = String(data: err.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
            alert("annotator failed on \(url.lastPathComponent)", msg)
        }
    }

    /// Finder-launched apps get a minimal PATH, so look in the usual install locations.
    private func annotatorPath() -> String? {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let candidates = [
            ProcessInfo.processInfo.environment["ANNOTATOR_BIN"],
            "\(home)/bin/annotator",
            "\(home)/.cargo/bin/annotator",
            "/opt/homebrew/bin/annotator",
            "/usr/local/bin/annotator",
        ].compactMap { $0 }
        return candidates.first { FileManager.default.isExecutableFile(atPath: $0) }
    }

    private func alert(_ title: String, _ text: String) {
        NSApp.activate(ignoringOtherApps: true)
        let a = NSAlert()
        a.messageText = title
        a.informativeText = text
        a.runModal()
    }
}

let app = NSApplication.shared
let delegate = Launcher()
app.delegate = delegate
app.setActivationPolicy(.accessory)
app.run()

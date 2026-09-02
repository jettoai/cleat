import Foundation

/// Watches `~/.config/cleat/config.json` and calls back, debounced, when it changes.
///
/// Two things make this more than one DispatchSource. Editors save atomically - write a temp file,
/// rename it over the original - so the descriptor being watched belongs to a file that no longer
/// has that name, and the watch has to be rebuilt after every event rather than left in place. And
/// the file may not exist yet, in which case the parent directory is watched until it appears.
final class ConfigWatcher: @unchecked Sendable {

    private static let debounce: DispatchTimeInterval = .milliseconds(300)
    /// How long to wait before looking again when neither the file nor its directory exists.
    private static let rediscoverDelay: DispatchTimeInterval = .seconds(2)

    private let url: URL
    private let queue: DispatchQueue
    private let onChange: () -> Void

    private var source: DispatchSourceFileSystemObject?
    private var descriptor: CInt = -1
    private var pending: DispatchWorkItem?

    init(url: URL = Paths.configURL, queue: DispatchQueue, onChange: @escaping () -> Void) {
        self.url = url
        self.queue = queue
        self.onChange = onChange
    }

    func start() {
        queue.async { [weak self] in self?.install() }
    }

    // MARK: - Private (engine queue only)

    private func install() {
        teardown()

        let manager = FileManager.default
        let watched: URL
        if manager.fileExists(atPath: url.path) {
            watched = url
        } else if manager.fileExists(atPath: url.deletingLastPathComponent().path) {
            watched = url.deletingLastPathComponent()
        } else {
            retryInstall()
            return
        }

        descriptor = open(watched.path, O_EVTONLY)
        guard descriptor >= 0 else {
            retryInstall()
            return
        }

        let source = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: descriptor,
            eventMask: [.write, .rename, .delete, .attrib, .extend],
            queue: queue
        )
        source.setEventHandler { [self] in handleEvent() }
        source.setCancelHandler { [descriptor] in
            if descriptor >= 0 { close(descriptor) }
        }
        self.source = source
        source.resume()
    }

    private func handleEvent() {
        // Rebuild the watch on the next turn of the queue: after an atomic save this descriptor
        // points at the replaced file, and after a directory event the file may now exist.
        queue.async { [weak self] in self?.install() }

        pending?.cancel()
        let work = DispatchWorkItem { [self] in
            pending = nil
            onChange()
        }
        pending = work
        queue.asyncAfter(deadline: .now() + Self.debounce, execute: work)
    }

    private func retryInstall() {
        queue.asyncAfter(deadline: .now() + Self.rediscoverDelay) { [weak self] in self?.install() }
    }

    private func teardown() {
        source?.cancel()
        source = nil
        // The cancel handler owns the close; clearing the field here only stops it being closed
        // twice if install() runs again before the handler fires.
        descriptor = -1
    }
}

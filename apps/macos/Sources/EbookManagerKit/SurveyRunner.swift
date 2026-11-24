import EbookBridge
import Foundation

/// Runs at most one background survey loop, passing failed books forward so later ones are still reached.
@MainActor
public final class SurveyRunner {
    public typealias Step = @Sendable (_ skip: [String]) async throws -> SurveyBatch

    private let step: Step
    private let onProgress: @MainActor () -> Void
    private var task: Task<Void, Never>?
    private var restartRequested = false

    public var isRunning: Bool { task != nil }

    public init(step: @escaping Step, onProgress: @escaping @MainActor () -> Void) {
        self.step = step
        self.onProgress = onProgress
    }

    /// Starts the loop, or asks a running loop to retry every book after its current batch.
    @discardableResult
    public func start() -> Task<Void, Never> {
        if let task {
            restartRequested = true
            return task
        }
        let started = Task<Void, Never> { @MainActor [weak self] in
            await self?.run()
        }
        task = started
        return started
    }

    private func run() async {
        var failed = Set<String>()
        while !Task.isCancelled {
            if restartRequested {
                restartRequested = false
                failed.removeAll()
            }
            let batch: SurveyBatch
            do {
                batch = try await step(Array(failed))
            } catch {
                if restartRequested {
                    continue
                }
                break
            }
            failed.formUnion(batch.failed)
            if batch.surveyed > 0 {
                onProgress()
            }
            if batch.surveyed == 0, batch.failed.isEmpty, !restartRequested {
                break
            }
        }
        task = nil
    }
}

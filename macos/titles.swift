// Names a new issue from its description with Apple's on-device model
// (Foundation Models). Exported over the C ABI for `src/titles.rs`: Foundation
// Models has no Objective-C interface, so Rust can't reach it otherwise.
//
// Everything is gated at runtime: the framework is weak-linked, and on a Mac
// without it (before macOS 26, Intel, Apple Intelligence off) the calls answer
// "unavailable" instead of failing to load.

import Foundation
#if canImport(FoundationModels)
import FoundationModels
#endif

/// 0 ready, 1 device not eligible, 2 Apple Intelligence off, 3 model not
/// ready, 4 macOS too old, 5 unknown.
@_cdecl("iboard_titles_availability")
public func iboardTitlesAvailability() -> Int32 {
    #if canImport(FoundationModels)
    if #available(macOS 26.0, *) {
        switch SystemLanguageModel.default.availability {
        case .available:
            return 0
        case .unavailable(.deviceNotEligible):
            return 1
        case .unavailable(.appleIntelligenceNotEnabled):
            return 2
        case .unavailable(.modelNotReady):
            return 3
        default:
            return 5
        }
    }
    #endif
    return 4
}

/// The languages the model supports, one per line by its own name
/// ("English", "Français"), written into `out` (NUL-terminated). Returns the
/// length, or -1.
@_cdecl("iboard_titles_languages")
public func iboardTitlesLanguages(_ out: UnsafeMutablePointer<CChar>, _ capacity: Int) -> Int {
    #if canImport(FoundationModels)
    if #available(macOS 26.0, *) {
        var seen = Set<String>()
        var names: [String] = []
        for language in SystemLanguageModel.default.supportedLanguages {
            guard let code = language.languageCode?.identifier, seen.insert(code).inserted else {
                continue
            }
            names.append(nativeName(code))
        }
        return copy(names.sorted().joined(separator: "\n"), into: out, capacity: capacity)
    }
    #endif
    return -1
}

/// The Mac's own language by its own name, then a tab and 1 when the model
/// supports it (0 otherwise), written into `out`. Returns the length, or -1.
@_cdecl("iboard_titles_user_language")
public func iboardTitlesUserLanguage(_ out: UnsafeMutablePointer<CChar>, _ capacity: Int) -> Int {
    let code = Locale.current.languageCode ?? "en"
    var supported = false
    #if canImport(FoundationModels)
    if #available(macOS 26.0, *) {
        supported = SystemLanguageModel.default.supportedLanguages
            .contains { $0.languageCode?.identifier == code }
    }
    #endif
    return copy("\(nativeName(code))\t\(supported ? 1 : 0)", into: out, capacity: capacity)
}

private func nativeName(_ code: String) -> String {
    let name = Locale(identifier: code).localizedString(forLanguageCode: code) ?? code
    return name.prefix(1).uppercased() + name.dropFirst()
}

/// Loads the model ahead of the first request, so naming the first issue
/// does not pay for it. Returns at once; does nothing when unavailable.
@_cdecl("iboard_titles_prewarm")
public func iboardTitlesPrewarm() {
    #if canImport(FoundationModels)
    if #available(macOS 26.0, *), case .available = SystemLanguageModel.default.availability {
        LanguageModelSession(instructions: instructions).prewarm()
    }
    #endif
}

/// Writes a title for `description` into `out` (NUL-terminated). Blocks the
/// calling thread until the model answers; never call it from the UI thread.
/// Returns the title's length in bytes, or -1 when the model is unavailable
/// or fails (the message, if any, is written instead).
@_cdecl("iboard_titles_infer")
public func iboardTitlesInfer(
    _ description: UnsafePointer<CChar>,
    _ out: UnsafeMutablePointer<CChar>,
    _ capacity: Int
) -> Int {
    let text = String(cString: description)
    #if canImport(FoundationModels)
    if #available(macOS 26.0, *) {
        let done = DispatchSemaphore(value: 0)
        var answer: Result<String, Error> = .failure(CancellationError())
        Task.detached {
            do {
                let session = LanguageModelSession(instructions: instructions)
                let response = try await session.respond(
                    to: "Description: \(text)\nTitle:",
                    options: GenerationOptions(temperature: 0)
                )
                answer = .success(clean(response.content))
            } catch {
                answer = .failure(error)
            }
            done.signal()
        }
        done.wait()
        switch answer {
        case .success(let title):
            return copy(title, into: out, capacity: capacity)
        case .failure(let error):
            _ = copy(String(describing: error), into: out, capacity: capacity)
            return -1
        }
    }
    #endif
    return -1
}

private let instructions = """
    You write titles for issues in a developer's issue tracker. Read the \
    description and write the title a careful developer would give it: what \
    should be done or what is wrong, in at most 10 words. Write it in the same \
    language as the description. Keep the meaning; do not add facts. No \
    quotes, no ticket numbers, no final period.

    Examples:
    Description: When I export the board I want one markdown file per column, \
    with each issue as a checklist item, so I can paste it into a PR.
    Title: Export the board as markdown checklists

    Description: bon alors euh quand je clique sur sauvegarder et que le \
    fichier a changé sur le disque ça écrase sans rien dire
    Title: Avertir avant d'écraser un fichier modifié sur le disque
    """

/// One line, no wrapping quotes or final period, at most 80 characters.
private func clean(_ raw: String) -> String {
    var title = raw.split(whereSeparator: \.isNewline).first.map(String.init) ?? ""
    title = title.trimmingCharacters(in: .whitespaces)
    if title.lowercased().hasPrefix("title:") { title = String(title.dropFirst(6)) }
    title = title.trimmingCharacters(in: CharacterSet(charactersIn: " \"'«»“”‘’`"))
    while title.hasSuffix(".") { title.removeLast() }
    if title.count > 80 { title = String(title.prefix(80)) }
    return title.trimmingCharacters(in: .whitespaces)
}

private func copy(_ text: String, into out: UnsafeMutablePointer<CChar>, capacity: Int) -> Int {
    guard capacity > 0 else { return -1 }
    var bytes = Array(text.utf8)
    if bytes.count >= capacity {
        bytes = Array(bytes.prefix(capacity - 1))
        // Never cut a UTF-8 sequence in half.
        while let last = bytes.last, last & 0xC0 == 0x80 { bytes.removeLast() }
        if let last = bytes.last, last & 0x80 != 0 { bytes.removeLast() }
    }
    bytes.withUnsafeBufferPointer { buffer in
        buffer.baseAddress.map { base in
            base.withMemoryRebound(to: CChar.self, capacity: bytes.count) { src in
                out.update(from: src, count: bytes.count)
            }
        }
    }
    out[bytes.count] = 0
    return bytes.count
}

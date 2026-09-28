// walkie-ai: the bridge to Apple's on-device language model
// (FoundationModels, macOS 26 with Apple Intelligence on). Its own process,
// so a crash inside the framework can't take walkie down and the polish
// timeout can kill a stuck call.
//
//   walkie-ai status                  one word: available, off, not-ready,
//                                     not-eligible, unavailable, unsupported
//   walkie-ai respond <instructions>  the text on stdin → the reply on stdout
//   walkie-ai prewarm                 loads the model, so the next respond
//                                     is fast (~0.4s, not 4–8s after idle)
//
// Built by walkie-app's build.rs. Against an SDK older than macOS 26 it
// compiles without the model and always reports "unsupported".

import Foundation

#if canImport(FoundationModels)
    import FoundationModels
#endif

@main
struct WalkieAI {
    static func main() async {
        let args = Array(CommandLine.arguments.dropFirst())
        switch (args.first, args.count) {
        case ("status", 1):
            print(status())
        case ("respond", 2):
            let input = String(
                decoding: FileHandle.standardInput.readDataToEndOfFile(), as: UTF8.self)
            let state = status()
            guard state == "available" else { fail("apple model unavailable: \(state)") }
            #if canImport(FoundationModels)
                if #available(macOS 26.0, *) {
                    do {
                        print(try await respond(instructions: args[1], to: input))
                    } catch {
                        fail("apple model: \(error)")
                    }
                }
            #endif
        case ("prewarm", 1):
            #if canImport(FoundationModels)
                if #available(macOS 26.0, *), status() == "available" {
                    // A one-token reply: unlike prewarm(), it waits until
                    // the model is actually loaded before this process exits.
                    let options = GenerationOptions(maximumResponseTokens: 1)
                    _ = try? await LanguageModelSession().respond(to: "hi", options: options)
                }
            #endif
        default:
            fail("usage: walkie-ai status | respond <instructions> | prewarm", code: 2)
        }
    }

    static func status() -> String {
        #if canImport(FoundationModels)
            if #available(macOS 26.0, *) {
                switch SystemLanguageModel.default.availability {
                case .available:
                    return "available"
                case .unavailable(let reason):
                    switch reason {
                    case .appleIntelligenceNotEnabled: return "off"
                    case .modelNotReady: return "not-ready"
                    case .deviceNotEligible: return "not-eligible"
                    @unknown default: return "unavailable"
                    }
                }
            }
        #endif
        return "unsupported"
    }

    #if canImport(FoundationModels)
        /// The input goes in <transcript> tags, which keeps the small model
        /// from treating a dictated question or request as one for it. The
        /// reply is capped near the input's length, so if it starts
        /// answering anyway it stops within a second instead of writing an
        /// essay.
        @available(macOS 26.0, *)
        static func respond(instructions: String, to input: String) async throws -> String {
            let session = LanguageModelSession(instructions: instructions)
            let options = GenerationOptions(
                // Greedy: the same dictation polishes the same way every time.
                sampling: .greedy,
                maximumResponseTokens: input.utf8.count / 2 + 64)
            let reply = try await session.respond(
                to: "<transcript>\n\(input)\n</transcript>", options: options)
            return reply.content
                .replacingOccurrences(of: "<transcript>", with: "")
                .replacingOccurrences(of: "</transcript>", with: "")
                .trimmingCharacters(in: .whitespacesAndNewlines)
        }
    #endif

    static func fail(_ message: String, code: Int32 = 1) -> Never {
        FileHandle.standardError.write(Data((message + "\n").utf8))
        exit(code)
    }
}

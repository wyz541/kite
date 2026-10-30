import AppKit
import Combine
import SwiftUI

@MainActor
final class TextEditorDocumentModel: ObservableObject {
    private let core: KiteCore
    private(set) var document: TextDocumentDto
    weak var textView: NSTextView?

    @Published private(set) var isDirty = false
    @Published private(set) var message = ""
    @Published private(set) var line = 1
    @Published private(set) var column = 1
    @Published private(set) var lineCount = 1

    init(path: String, core: KiteCore = KiteCore()) throws {
        self.core = core
        self.document = try core.openTextDocument(path: path)
    }

    var fileName: String {
        URL(fileURLWithPath: document.path).lastPathComponent
    }

    var languageLabel: String {
        switch document.language {
        case "rust": return "Rust"
        case "javascript": return "JavaScript"
        case "typescript": return "TypeScript"
        case "tsx": return "TSX"
        case "python": return "Python"
        case "json": return "JSON"
        case "toml": return "TOML"
        case "yaml": return "YAML"
        case "markdown": return "Markdown"
        case "html": return "HTML"
        case "css": return "CSS"
        case "c": return "C"
        case "cpp": return "C++"
        case "csharp": return "C#"
        case "java": return "Java"
        case "go": return "Go"
        case "kotlin": return "Kotlin"
        case "bash": return "Shell"
        case "sql": return "SQL"
        case "swift": return "Swift"
        case "lua": return "Lua"
        case "ruby": return "Ruby"
        case "php": return "PHP"
        case "proto": return "Protocol Buffers"
        case "cmake": return "CMake"
        case "make": return "Make"
        case "diff": return "Diff"
        default: return "纯文本"
        }
    }

    var metadataLabel: String {
        let ending = document.lineEnding == "crlf" ? "CRLF" : "LF"
        let bom = document.hasBom ? " · BOM" : ""
        return "\(languageLabel) · UTF-8\(bom) · \(ending)"
    }

    func attach(_ textView: NSTextView) {
        self.textView = textView
        updateStatistics()
    }

    func markChanged() {
        isDirty = true
        message = "未保存"
        updateStatistics()
        textView?.enclosingScrollView?.verticalRulerView?.needsDisplay = true
    }

    func updateCursor() {
        updateStatistics()
    }

    func save() -> Bool {
        guard let textView else { return false }
        do {
            document = try core.saveTextDocument(
                path: document.path,
                text: textView.string,
                lineEnding: document.lineEnding,
                hasBom: document.hasBom,
                expectedSha256: document.contentSha256
            )
            isDirty = false
            message = "已保存"
            return true
        } catch {
            message = error.localizedDescription
            return false
        }
    }

    func undo() {
        textView?.undoManager?.undo()
    }

    func redo() {
        textView?.undoManager?.redo()
    }

    func showFindPanel() {
        showFindAction(.showFindPanel)
    }

    func showReplacePanel() {
        showFindAction(.showReplacePanel)
    }

    private func showFindAction(_ action: NSFindPanelAction) {
        guard let textView else { return }
        let sender = NSMenuItem()
        sender.tag = Int(action.rawValue)
        textView.performFindPanelAction(sender)
    }

    private func updateStatistics() {
        guard let textView else { return }
        let value = textView.string as NSString
        let cursor = min(textView.selectedRange().location, value.length)
        let beforeCursor = value.substring(to: cursor)
        let currentLine = beforeCursor.reduce(into: 1) { count, character in
            if character == "\n" { count += 1 }
        }
        let currentColumn = beforeCursor
            .split(separator: "\n", omittingEmptySubsequences: false)
            .last?
            .count ?? 0
        line = currentLine
        column = currentColumn + 1
        lineCount = value.reduce(into: 1) { count, character in
            if character == "\n" { count += 1 }
        }
    }
}

@MainActor
private struct KiteTextEditorSurface: NSViewRepresentable {
    @ObservedObject var model: TextEditorDocumentModel

    func makeCoordinator() -> Coordinator {
        Coordinator(model: model)
    }

    func makeNSView(context: Context) -> NSScrollView {
        let scrollView = NSScrollView()
        scrollView.drawsBackground = false
        scrollView.borderType = .noBorder
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.scrollerStyle = .overlay

        let textView = NSTextView(frame: .zero)
        textView.isRichText = false
        textView.importsGraphics = false
        textView.allowsUndo = true
        textView.usesFindBar = true
        textView.isIncrementalSearchingEnabled = true
        textView.isContinuousSpellCheckingEnabled = false
        textView.isGrammarCheckingEnabled = false
        textView.isAutomaticSpellingCorrectionEnabled = false
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.isAutomaticTextReplacementEnabled = false
        textView.smartInsertDeleteEnabled = false
        textView.drawsBackground = false
        textView.textColor = .labelColor
        textView.insertionPointColor = .controlAccentColor
        textView.font = .monospacedSystemFont(ofSize: 13.5, weight: .regular)
        textView.textContainerInset = NSSize(width: 14, height: 14)
        textView.minSize = NSSize(width: 0, height: 0)
        textView.maxSize = NSSize(width: .greatestFiniteMagnitude, height: .greatestFiniteMagnitude)
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = true
        textView.autoresizingMask = [.width]
        textView.textContainer?.containerSize = NSSize(
            width: .greatestFiniteMagnitude,
            height: .greatestFiniteMagnitude
        )
        textView.textContainer?.widthTracksTextView = false
        textView.textContainer?.lineFragmentPadding = 8
        textView.string = model.document.text
        textView.delegate = context.coordinator

        scrollView.documentView = textView
        scrollView.verticalRulerView = LineNumberRulerView(textView: textView)
        scrollView.hasVerticalRuler = true
        scrollView.rulersVisible = true

        model.attach(textView)
        DispatchQueue.main.async {
            textView.window?.makeFirstResponder(textView)
            textView.setSelectedRange(NSRange(location: 0, length: 0))
            textView.scrollRangeToVisible(NSRange(location: 0, length: 0))
        }
        return scrollView
    }

    func updateNSView(_ nsView: NSScrollView, context: Context) {}

    @MainActor
    final class Coordinator: NSObject, NSTextViewDelegate {
        private let model: TextEditorDocumentModel

        init(model: TextEditorDocumentModel) {
            self.model = model
        }

        func textDidChange(_ notification: Notification) {
            model.markChanged()
        }

        func textViewDidChangeSelection(_ notification: Notification) {
            model.updateCursor()
        }
    }
}

@MainActor
private struct TextEditorWindowView: View {
    @ObservedObject var model: TextEditorDocumentModel

    var body: some View {
        ZStack {
            KiteGlassBackdrop()

            VStack(spacing: 0) {
                toolbar
                KiteTextEditorSurface(model: model)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                statusBar
            }
            .padding(10)
        }
        .frame(minWidth: 720, minHeight: 520)
    }

    private var toolbar: some View {
        HStack(spacing: 8) {
            Button {
                _ = model.save()
            } label: {
                Label("保存", systemImage: "square.and.arrow.down")
            }
            .disabled(!model.isDirty)
            .help("保存 ⌘S")

            Button {
                model.undo()
            } label: {
                Image(systemName: "arrow.uturn.backward")
            }
            .help("撤销 ⌘Z")

            Button {
                model.redo()
            } label: {
                Image(systemName: "arrow.uturn.forward")
            }
            .help("重做 ⇧⌘Z")

            Divider()
                .frame(height: 18)

            Button {
                model.showFindPanel()
            } label: {
                Label("查找", systemImage: "magnifyingglass")
            }
            .help("查找 ⌘F")

            Button {
                model.showReplacePanel()
            } label: {
                Label("替换", systemImage: "arrow.triangle.2.circlepath")
            }
            .help("查找并替换 ⌥⌘F")

            Spacer()

            Text(model.metadataLabel)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .buttonStyle(.borderless)
        .padding(.horizontal, 12)
        .frame(height: 38)
        .kiteGlassGroup(cornerRadius: 12)
    }

    private var statusBar: some View {
        HStack(spacing: 12) {
            Text(model.isDirty ? "● 未保存" : "已保存")
            Text("第 \(model.line) 行，第 \(model.column) 列")
            Text("\(model.lineCount) 行")
            Spacer()
            if !model.message.isEmpty {
                Text(model.message)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
        }
        .font(.caption)
        .foregroundStyle(.secondary)
        .padding(.horizontal, 12)
        .frame(height: 28)
        .kiteGlassGroup(cornerRadius: 10)
    }
}

@MainActor
private final class LineNumberRulerView: NSRulerView {
    private weak var textView: NSTextView?

    init(textView: NSTextView) {
        self.textView = textView
        super.init(scrollView: textView.enclosingScrollView, orientation: .verticalRuler)
        clientView = textView
        ruleThickness = 48
    }

    required init(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }


    override func drawHashMarksAndLabels(in rect: NSRect) {
        NSColor.textBackgroundColor.withAlphaComponent(0.14).setFill()
        NSBezierPath(rect: rect).fill()

        guard let textView,
              let layoutManager = textView.layoutManager,
              let textContainer = textView.textContainer,
              let scrollView else {
            return
        }

        let visibleRect = scrollView.contentView.bounds
        let glyphRange = layoutManager.glyphRange(
            forBoundingRect: visibleRect,
            in: textContainer
        )
        let characterRange = layoutManager.characterRange(
            forGlyphRange: glyphRange,
            actualGlyphRange: nil
        )
        let value = textView.string as NSString
        let font = NSFont.monospacedDigitSystemFont(ofSize: 11, weight: .regular)
        let attributes: [NSAttributedString.Key: Any] = [
            .font: font,
            .foregroundColor: NSColor.secondaryLabelColor
        ]

        var lineNumber = 1
        var index = 0
        while index < min(characterRange.location, value.length) {
            if value.character(at: index) == 10 {
                lineNumber += 1
            }
            index += 1
        }

        if glyphRange.length == 0 {
            drawLineNumber(1, atY: textView.textContainerOrigin.y, attributes: attributes)
            return
        }

        var glyphIndex = glyphRange.location
        while glyphIndex < NSMaxRange(glyphRange) {
            var lineGlyphRange = NSRange()
            let lineRect = layoutManager.lineFragmentRect(
                forGlyphAt: glyphIndex,
                effectiveRange: &lineGlyphRange
            )
            let rectInTextView = lineRect.offsetBy(dx: 0, dy: textView.textContainerOrigin.y)
            let rectInRuler = textView.convert(rectInTextView, to: self)
            drawLineNumber(lineNumber, atY: rectInRuler.minY, attributes: attributes)
            lineNumber += 1
            glyphIndex = NSMaxRange(lineGlyphRange)
        }
    }

    private func drawLineNumber(
        _ lineNumber: Int,
        atY y: CGFloat,
        attributes: [NSAttributedString.Key: Any]
    ) {
        let value = "\(lineNumber)" as NSString
        let size = value.size(withAttributes: attributes)
        value.draw(
            at: NSPoint(x: ruleThickness - size.width - 9, y: y + 1),
            withAttributes: attributes
        )
    }
}

@MainActor
final class TextEditorWindowController: NSWindowController, NSWindowDelegate {
    private static var controllers: [String: TextEditorWindowController] = [:]
    private let documentModel: TextEditorDocumentModel
    private let path: String

    init(path: String) throws {
        self.path = URL(fileURLWithPath: path).standardizedFileURL.path
        self.documentModel = try TextEditorDocumentModel(path: path)

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 980, height: 720),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.title = documentModel.fileName
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.isMovableByWindowBackground = true
        window.minSize = NSSize(width: 720, height: 520)
        window.contentViewController = NSHostingController(
            rootView: TextEditorWindowView(model: documentModel)
        )
        super.init(window: window)
        window.delegate = self
        updateTitle()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    static func open(path: String) throws {
        let normalized = URL(fileURLWithPath: path).standardizedFileURL.path
        if let controller = controllers[normalized] {
            controller.showWindow(nil)
            controller.window?.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
            return
        }

        let controller = try TextEditorWindowController(path: normalized)
        controllers[normalized] = controller
        controller.showWindow(nil)
        controller.window?.center()
        controller.window?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    static func saveKeyWindow() {
        controller(for: NSApp.keyWindow)?.save()
    }

    static func showFindInKeyWindow(replace: Bool) {
        guard let model = controller(for: NSApp.keyWindow)?.documentModel else { return }
        if replace {
            model.showReplacePanel()
        } else {
            model.showFindPanel()
        }
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        guard documentModel.isDirty else { return true }
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = "有未保存的更改"
        alert.informativeText = "关闭窗口会丢失当前修改。"
        alert.addButton(withTitle: "保存并关闭")
        alert.addButton(withTitle: "取消")
        alert.addButton(withTitle: "放弃更改")

        switch alert.runModal() {
        case .alertFirstButtonReturn:
            if documentModel.save() {
                return true
            }
            let error = NSAlert()
            error.alertStyle = .critical
            error.messageText = "无法保存文件"
            error.informativeText = documentModel.message
            error.runModal()
            return false
        case .alertThirdButtonReturn:
            return true
        default:
            return false
        }
    }

    func windowWillClose(_ notification: Notification) {
        TextEditorWindowController.controllers.removeValue(forKey: path)
    }

    private func save() {
        if documentModel.save() {
            updateTitle()
        }
    }

    private func updateTitle() {
        window?.title = documentModel.fileName
        window?.representedURL = URL(fileURLWithPath: documentModel.document.path)
    }

    private static func controller(for window: NSWindow?) -> TextEditorWindowController? {
        controllers.values.first { $0.window === window }
    }
}
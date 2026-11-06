import AppKit
import SwiftUI

@MainActor
final class KiteGlobalSearchWindowController: NSWindowController, NSWindowDelegate {
    static let shared = KiteGlobalSearchWindowController()

    private weak var model: KiteModel?

    var isVisible: Bool {
        window?.isVisible == true
    }

    func show(model: KiteModel) {
        self.model = model

        if window == nil {
            let rootView = KiteGlobalSearchWindowView(model: model)
            let hostingController = NSHostingController(rootView: rootView)
            let searchWindow = NSWindow(
                contentRect: NSRect(x: 0, y: 0, width: 860, height: 640),
                styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
                backing: .buffered,
                defer: false
            )
            searchWindow.title = "全局搜索"
            searchWindow.titlebarAppearsTransparent = true
            searchWindow.minSize = NSSize(width: 720, height: 520)
            searchWindow.isReleasedWhenClosed = false
            searchWindow.contentViewController = hostingController
            searchWindow.delegate = self
            searchWindow.center()
            searchWindow.setFrameAutosaveName("KiteGlobalSearchWindow")
            window = searchWindow
        }

        if window?.isMiniaturized == true {
            window?.deminiaturize(nil)
        }
        showWindow(nil)
        window?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        model.focusGlobalSearch()
    }

    func windowWillClose(_ notification: Notification) {
        model?.clearGlobalSearch()
        model = nil
    }
}

private struct KiteGlobalSearchWindowView: View {
    @ObservedObject var model: KiteModel
    @FocusState private var searchFocused: Bool

    var body: some View {
        ZStack {
            KiteGlassBackdrop()

            VStack(spacing: 0) {
                searchHeader
                Rectangle()
                    .fill(Color.primary.opacity(0.07))
                    .frame(height: 0.5)
                    .padding(.horizontal, 14)
                resultContent
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .padding(12)
            }
        }
        .frame(minWidth: 720, minHeight: 520)
        .onAppear {
            DispatchQueue.main.async {
                searchFocused = true
            }
        }
        .onChange(of: model.focusedGlobalSearchRequest) { _ in
            DispatchQueue.main.async {
                searchFocused = true
            }
        }
        .onChange(of: model.globalSearchMode) { _ in
            model.globalSearchModeDidChange()
        }
        .onChange(of: model.globalSearchScope) { _ in
            model.globalSearchScopeDidChange()
        }
        .onExitCommand {
            NSApp.keyWindow?.performClose(nil)
        }
    }

    private var searchHeader: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(.secondary)

            Picker("搜索模式", selection: $model.globalSearchMode) {
                ForEach(GlobalSearchMode.allCases) { mode in
                    Text(mode.title).tag(mode)
                }
            }
            .labelsHidden()
            .pickerStyle(.segmented)
            .controlSize(.small)
            .frame(width: 110)

            HStack(spacing: 4) {
                Image(systemName: "scope")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(.secondary)
                KiteSearchScopeControl(model: model)
                    .frame(width: 190, height: 22)
            }

            TextField(
                model.globalSearchMode == .content ? "搜索文件内容" : "搜索文件名",
                text: $model.globalSearchText
            )
            .textFieldStyle(.plain)
            .font(.system(size: 12))
            .focused($searchFocused)
            .onSubmit {
                model.submitGlobalSearch()
            }
            .onChange(of: model.globalSearchText) { _ in
                model.globalSearchTextDidChange()
            }

            if model.globalSearchLoading || model.contentSearchLoading {
                ProgressView()
                    .controlSize(.small)
            }

            if !model.globalSearchText.isEmpty {
                Button {
                    model.clearGlobalSearch()
                    searchFocused = true
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
                .help("清除搜索")
            }

            Button("搜索") {
                model.submitGlobalSearch()
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.small)
            .disabled(model.globalSearchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
        .padding(.horizontal, 14)
        .frame(height: 52)
        .kiteGlass(.panel, cornerRadius: KiteTheme.controlRadius)
        .padding(12)
    }

    @ViewBuilder
    private var resultContent: some View {
        if model.globalSearchMode == .content,
           model.contentSearch != nil || model.contentSearchLoading {
            KiteContentSearchResults(model: model)
        } else if model.globalSearchMode == .name,
                  model.globalSearch != nil || model.globalSearchLoading {
            KiteGlobalSearchResults(model: model)
        } else {
            VStack(spacing: 12) {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 34, weight: .light))
                    .foregroundStyle(.tertiary)
                Text("输入关键词开始全局搜索")
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(.secondary)
                Text(model.globalSearchScope.allowsLiveSearch
                    ? "输入至少两个字符后自动搜索。"
                    : "当前范围较大，请输入关键词后按 Enter 或点击搜索。")
                    .font(.system(size: 10.5))
                    .foregroundStyle(.tertiary)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(
                .ultraThinMaterial,
                in: RoundedRectangle(cornerRadius: 18, style: .continuous)
            )
        }
    }
}

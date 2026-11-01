import AppKit
import SwiftUI

struct ContentView: View {
    @ObservedObject var model: KiteModel

    @AppStorage("kite.macos.sidebar-visible") private var showsSidebar = true
    @AppStorage("kite.macos.preview-visible") private var showsPreview = true
    @State private var isEditingAddress = false
    @FocusState private var addressFocused: Bool

    var body: some View {
        ZStack {
            KiteGlassBackdrop()

            VStack(spacing: 0) {
                Color.clear
                    .frame(height: 28)
                    .contentShape(Rectangle())

                HStack(spacing: 10) {
                    if showsSidebar {
                        KiteSidebar(model: model)
                            .transition(.move(edge: .leading).combined(with: .opacity))
                    }

                    VStack(spacing: 10) {
                        tabBar
                        toolbar
                        KiteColumnBrowser(model: model, showsPreview: $showsPreview)
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                        statusBar
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
                .padding(.horizontal, 12)
                .padding(.bottom, 12)
            }
        }
        .frame(minWidth: 1000, minHeight: 660)
        .background(KiteWindowConfigurator())
        .onChange(of: model.focusedAddressRequest) { _ in
            beginAddressEditing()
        }
        .animation(.snappy(duration: 0.22), value: showsSidebar)
    }

    private var tabBar: some View {
        KiteGlassEffectContainer(spacing: 7) {
            HStack(spacing: 7) {
                ScrollView(.horizontal) {
                    HStack(spacing: 5) {
                        ForEach(model.tabs) { tab in
                            KiteTabItem(
                                model: model,
                                tab: tab,
                                isActive: model.activeTabID == tab.id
                            )
                        }
                    }
                    .padding(.vertical, 3)
                }
                .scrollIndicators(.hidden)

                KiteToolbarIconButton(
                    systemName: "plus",
                    title: "新建页签"
                ) {
                    model.newTab()
                }

                Rectangle()
                    .fill(Color.primary.opacity(0.08))
                    .frame(width: 0.5, height: 18)
                    .padding(.horizontal, 1)

                KiteToolbarIconButton(
                    systemName: "sidebar.left",
                    title: showsSidebar ? "隐藏侧边栏" : "显示侧边栏",
                    isActive: showsSidebar
                ) {
                    withAnimation(.snappy(duration: 0.22)) {
                        showsSidebar.toggle()
                    }
                }

                KiteToolbarIconButton(
                    systemName: "rectangle.righthalf.inset.filled",
                    title: showsPreview ? "隐藏预览" : "显示预览",
                    isActive: showsPreview
                ) {
                    withAnimation(.snappy(duration: 0.22)) {
                        showsPreview.toggle()
                    }
                }
            }
            .padding(.horizontal, 6)
            .frame(height: 38)
            .kiteGlass(.panel, cornerRadius: 14)
        }
    }

    private var toolbar: some View {
        KiteGlassEffectContainer(spacing: 9) {
            HStack(spacing: 9) {
                navigationControls
                addressBar
                actionControls
            }
        }
    }

    private var navigationControls: some View {
        HStack(spacing: 2) {
            KiteToolbarIconButton(
                systemName: "arrow.up",
                title: "上层文件夹 (Command+↑)",
                isEnabled: canNavigateUp
            ) {
                model.navigateUp()
            }

            KiteToolbarIconButton(
                systemName: "arrow.clockwise",
                title: "刷新"
            ) {
                model.refresh()
            }

            if let selectedEntry = model.selectedColumnEntry {
                KiteToolbarIconButton(
                    systemName: selectedEntry.isDirectory
                        ? "arrow.down.right"
                        : "arrow.up.forward.app",
                    title: selectedEntry.isDirectory ? "进入文件夹" : "打开文件"
                ) {
                    if selectedEntry.isDirectory {
                        model.loadRoot(selectedEntry.path)
                    } else {
                        KitePlatform.open(selectedEntry.path)
                    }
                }
            }
        }
        .padding(.horizontal, 4)
        .frame(height: 40)
        .kiteGlass(.panel, cornerRadius: 14)
    }

    private var addressBar: some View {
        HStack(spacing: 5) {
            Image(systemName: "point.topleft.down.to.point.bottomright.curvepath")
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.secondary)
                .padding(.leading, 7)

            if isEditingAddress {
                TextField("输入路径", text: $model.address)
                    .textFieldStyle(.plain)
                    .font(.system(size: 11.5, design: .monospaced))
                    .focused($addressFocused)
                    .onSubmit {
                        commitAddress()
                    }
                    .onExitCommand {
                        cancelAddressEditing()
                    }

                Button("前往") {
                    commitAddress()
                }
                .buttonStyle(.plain)
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(Color.accentColor)
                .padding(.horizontal, 8)
                .frame(height: 26)
                .background(Color.accentColor.opacity(0.12), in: Capsule())
            } else {
                breadcrumbs
                    .frame(maxWidth: .infinity, alignment: .leading)

                KiteToolbarIconButton(
                    systemName: "pencil",
                    title: "编辑路径 (Command+L)"
                ) {
                    beginAddressEditing()
                }
                .scaleEffect(0.88)
            }
        }
        .padding(.horizontal, 5)
        .frame(maxWidth: .infinity)
        .frame(height: 40)
        .kiteGlass(.panel, cornerRadius: 14)
        .contextMenu {
            Button("编辑路径") { beginAddressEditing() }
            Button("拷贝当前路径") { KitePlatform.copyPath(model.currentPath) }
            Button("在 Finder 中显示") { KitePlatform.revealInFinder(model.currentPath) }
        }
    }

    private var breadcrumbs: some View {
        ScrollView(.horizontal) {
            HStack(spacing: 2) {
                ForEach(breadcrumbParts) { part in
                    Button {
                        model.loadRoot(part.path)
                    } label: {
                        Text(part.title)
                            .font(.system(
                                size: 11.5,
                                weight: part.isCurrent ? .semibold : .regular
                            ))
                            .foregroundStyle(part.isCurrent ? Color.primary : Color.secondary)
                            .lineLimit(1)
                            .padding(.horizontal, 5)
                            .frame(height: 24)
                            .contentShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
                    }
                    .buttonStyle(.plain)
                    .background {
                        if part.isCurrent {
                            RoundedRectangle(cornerRadius: 6, style: .continuous)
                                .fill(Color.accentColor.opacity(0.10))
                        }
                    }
                    .help(part.path)
                    .accessibilityLabel("打开 \(part.title)")

                    if !part.isCurrent {
                        Image(systemName: "chevron.right")
                            .font(.system(size: 7.5, weight: .bold))
                            .foregroundStyle(.quaternary)
                    }
                }
            }
            .padding(.horizontal, 1)
        }
        .scrollIndicators(.hidden)
    }

    private var actionControls: some View {
        HStack(spacing: 2) {
            KiteToolbarIconButton(
                systemName: "doc.on.doc",
                title: "拷贝选中项目",
                isEnabled: model.selectedColumnEntry != nil
            ) {
                if let selectedEntry = model.selectedColumnEntry {
                    model.copyFile(selectedEntry.path)
                }
            }

            KiteToolbarIconButton(
                systemName: "scissors",
                title: "剪切选中项目",
                isEnabled: model.selectedColumnEntry != nil
            ) {
                if let selectedEntry = model.selectedColumnEntry {
                    model.cutFile(selectedEntry.path)
                }
            }

            KiteToolbarIconButton(
                systemName: "doc.on.clipboard",
                title: "粘贴文件"
            ) {
                model.pasteFiles(into: model.currentPath)
            }

            KiteToolbarIconButton(
                systemName: "folder.badge.plus",
                title: "新建文件夹",
                isEnabled: KitePlatform.canModify(model.currentPath)
            ) {
                model.createFolder(in: model.currentPath)
            }

            Menu {
                Button {
                    model.createFolder(in: model.currentPath)
                } label: {
                    Label("新建文件夹…", systemImage: "folder.badge.plus")
                }

                Button {
                    model.createFile(in: model.currentPath)
                } label: {
                    Label("新建文件…", systemImage: "doc.badge.plus")
                }

                Divider()

                Button {
                    if let selectedEntry = model.selectedColumnEntry {
                        model.copyFile(selectedEntry.path)
                    }
                } label: {
                    Label("拷贝选中项目", systemImage: "doc.on.doc")
                }
                .disabled(model.selectedColumnEntry == nil)

                Button {
                    if let selectedEntry = model.selectedColumnEntry {
                        model.cutFile(selectedEntry.path)
                    }
                } label: {
                    Label("剪切选中项目", systemImage: "scissors")
                }
                .disabled(model.selectedColumnEntry == nil)

                Button {
                    model.pasteFiles(into: model.currentPath)
                } label: {
                    Label("粘贴文件", systemImage: "doc.on.clipboard")
                }

                Divider()

                Button {
                    withAnimation(.snappy(duration: 0.22)) {
                        showsSidebar.toggle()
                    }
                } label: {
                    Label(
                        showsSidebar ? "隐藏侧边栏" : "显示侧边栏",
                        systemImage: "sidebar.left"
                    )
                }
                .keyboardShortcut("s", modifiers: [.command, .option])

                Button {
                    withAnimation(.snappy(duration: 0.22)) {
                        showsPreview.toggle()
                    }
                } label: {
                    Label(
                        showsPreview ? "隐藏预览" : "显示预览",
                        systemImage: "rectangle.righthalf.inset.filled"
                    )
                }
                .keyboardShortcut("p", modifiers: [.command, .option])

                Divider()

                Button {
                    model.checkForUpdates()
                } label: {
                    Label("检查更新…", systemImage: "arrow.triangle.2.circlepath")
                }
                .disabled(model.updateInProgress)

                Button {
                    model.setAsDefaultFileViewer()
                } label: {
                    Label(
                        KitePlatform.canSetDefaultFolderHandler
                            ? "设为默认文件夹打开方式…"
                            : "设为默认文件查看器…",
                        systemImage: "folder.badge.gearshape"
                    )
                }

                Button {
                    model.restoreFinderAsDefaultFileViewer()
                } label: {
                    Label("恢复 Finder 为默认…", systemImage: "arrow.uturn.backward.circle")
                }
            } label: {
                Image(systemName: "ellipsis")
                    .font(.system(size: 13, weight: .semibold))
                    .frame(width: 30, height: 30)
                    .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .help("更多操作")
            .accessibilityLabel("更多操作")
        }
        .padding(.horizontal, 4)
        .frame(height: 40)
        .fixedSize(horizontal: true, vertical: false)
        .kiteGlass(.panel, cornerRadius: 14)
    }

    private var statusBar: some View {
        HStack(spacing: 8) {
            Image(systemName: statusSystemImage)
                .font(.system(size: 10.5, weight: .semibold))
                .foregroundStyle(statusAccent)

            Text(statusText)
                .font(.system(size: 10.5))
                .foregroundStyle(statusAccent)
                .lineLimit(1)
                .truncationMode(.middle)

            Spacer(minLength: 8)

            if let selectedEntry = model.selectedColumnEntry {
                Text(selectedEntry.name)
                    .font(.system(size: 10.5, weight: .medium))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }

            Text("\(model.columns.last?.entries.count ?? 0) 个项目")
                .font(.system(size: 10.5, design: .rounded))
                .foregroundStyle(.secondary)

            if let skipped = model.columns.last?.skipped, skipped > 0 {
                Text("已跳过 \(skipped)")
                    .font(.system(size: 10.5, design: .rounded))
                    .foregroundStyle(.tertiary)
            }

            if model.columns.last?.gitAvailable == true {
                KiteStatusBadge(title: "Git", systemImage: "arrow.triangle.branch")
            }

            if model.columns.last?.svnAvailable == true {
                KiteStatusBadge(title: "SVN", systemImage: "arrow.triangle.branch")
            }
        }
        .padding(.horizontal, 12)
        .frame(height: 30)
        .kiteGlass(.panel, cornerRadius: 12)
    }

    private var canNavigateUp: Bool {
        URL(fileURLWithPath: model.currentPath).deletingLastPathComponent().path
            != model.currentPath
    }

    private var statusText: String {
        if let error = model.errorMessage {
            return error
        }
        if let update = model.updateStatusMessage {
            return update
        }
        if model.updateInProgress {
            return "正在检查更新…"
        }
        let name = FileManager.default.displayName(atPath: model.currentPath)
        return name.isEmpty ? model.currentPath : name
    }

    private var statusAccent: Color {
        model.errorMessage == nil ? Color.secondary : Color.red
    }

    private var statusSystemImage: String {
        if model.errorMessage != nil {
            return "exclamationmark.triangle.fill"
        }
        if model.updateInProgress || model.updateStatusMessage != nil {
            return "arrow.triangle.2.circlepath"
        }
        return "wind"
    }

    private var breadcrumbParts: [KiteBreadcrumbPart] {
        let url = URL(fileURLWithPath: model.currentPath).standardizedFileURL
        let components = url.pathComponents
        guard !components.isEmpty else { return [] }

        var current = URL(fileURLWithPath: "/", isDirectory: true)
        var parts: [KiteBreadcrumbPart] = []

        for component in components {
            if component == "/" {
                let title = FileManager.default.displayName(atPath: "/")
                parts.append(KiteBreadcrumbPart(title: title, path: "/"))
                continue
            }

            current.appendPathComponent(component, isDirectory: true)
            parts.append(
                KiteBreadcrumbPart(
                    title: FileManager.default.displayName(atPath: current.path),
                    path: current.path
                )
            )
        }

        guard let last = parts.indices.last else { return parts }
        for index in parts.indices {
            parts[index].isCurrent = index == last
        }
        return parts
    }

    private func beginAddressEditing() {
        model.address = model.currentPath
        isEditingAddress = true
        DispatchQueue.main.async {
            addressFocused = true
        }
    }

    private func commitAddress() {
        model.loadRoot(model.address)
        isEditingAddress = false
        addressFocused = false
    }

    private func cancelAddressEditing() {
        model.address = model.currentPath
        isEditingAddress = false
        addressFocused = false
    }
}

private struct KiteTabItem: View {
    @ObservedObject var model: KiteModel
    let tab: BrowserTab
    let isActive: Bool

    @State private var isHovering = false

    var body: some View {
        HStack(spacing: 4) {
            Button {
                model.activateTab(id: tab.id)
            } label: {
                HStack(spacing: 7) {
                    Image(systemName: "folder.fill")
                        .font(.system(size: 11, weight: .semibold))
                        .symbolRenderingMode(.hierarchical)
                        .foregroundStyle(isActive ? Color.accentColor : Color.secondary)

                    Text(tab.title)
                        .font(.system(size: 11.5, weight: isActive ? .semibold : .regular))
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("切换到 \(tab.title)")

            if model.canCloseTab {
                Button {
                    model.closeTab(id: tab.id)
                } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 8, weight: .bold))
                        .frame(width: 18, height: 18)
                        .contentShape(RoundedRectangle(cornerRadius: 5, style: .continuous))
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .help("关闭页签")
                .accessibilityLabel("关闭页签")
            }
        }
        .padding(.leading, 10)
        .padding(.trailing, 5)
        .frame(width: 174, height: 28)
        .background {
            if isActive {
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .kiteGlass(.selection, cornerRadius: 9)
            } else if isHovering {
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .fill(Color.primary.opacity(0.055))
            }
        }
        .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
        .onHover { isHovering = $0 }
        .contextMenu {
            Button("关闭页签") { model.closeTab(id: tab.id) }
                .disabled(!model.canCloseTab)
            Button("新建页签") { model.newTab() }
        }
    }
}

private struct KiteBreadcrumbPart: Identifiable {
    let title: String
    let path: String
    var isCurrent = false

    var id: String { path }
}

private struct KiteStatusBadge: View {
    let title: String
    let systemImage: String

    var body: some View {
        Label(title, systemImage: systemImage)
            .font(.system(size: 9, weight: .semibold))
            .foregroundStyle(.tertiary)
            .labelStyle(.titleAndIcon)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(Color.primary.opacity(0.045), in: Capsule())
    }
}

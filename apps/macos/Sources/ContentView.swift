import AppKit
import QuickLookUI
import SwiftUI

struct ContentView: View {
    @ObservedObject var model: KiteModel
    @FocusState private var addressFocused: Bool

    var body: some View {
        ZStack {
            KiteGlassBackdrop()

            VStack(spacing: 0) {
                tabBar
                toolbar
                columnBrowser
                statusBar
            }
        }
        .frame(minWidth: 900, minHeight: 620)
        .onChange(of: model.focusedAddressRequest) { _ in
            addressFocused = true
        }
    }

    private var tabBar: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(model.tabs) { tab in
                    tabItem(tab)
                }

                Button {
                    model.newTab()
                } label: {
                    Image(systemName: "plus")
                        .frame(width: 28, height: 28)
                }
                .buttonStyle(.borderless)
                .help("新建页签")
                .accessibilityLabel("新建页签")
            }
            .padding(.horizontal, 12)
        }
        .frame(height: 36)
        .background(.ultraThinMaterial)
        .overlay(alignment: .bottom) {
            Divider()
        }
    }

    private func tabItem(_ tab: BrowserTab) -> some View {
        let active = model.activeTabID == tab.id
        return HStack(spacing: 3) {
            Button {
                model.activateTab(id: tab.id)
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "folder.fill")
                        .foregroundStyle(active ? Color.accentColor : Color.orange)
                    Text(tab.title)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("切换到 \(tab.title)")

            if model.canCloseTab {
                Button {
                    model.closeTab(id: tab.id)
                } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 9, weight: .semibold))
                        .frame(width: 20, height: 20)
                }
                .buttonStyle(.plain)
                .help("关闭页签")
                .accessibilityLabel("关闭页签")
            }
        }
        .padding(.leading, 9)
        .padding(.trailing, 5)
        .frame(width: 176, height: 28)
        .background(
            active ? Color.accentColor.opacity(0.18) : Color.clear,
            in: RoundedRectangle(cornerRadius: 8, style: .continuous)
        )
        .overlay {
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .strokeBorder(
                    active ? Color.accentColor.opacity(0.30) : Color.clear,
                    lineWidth: 0.5
                )
        }
    }

    private var toolbar: some View {
        KiteGlassEffectContainer(spacing: 8) {
            toolbarContent
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
    }

    private var toolbarContent: some View {
        HStack(spacing: 10) {
            navigationControls
                .kiteGlassGroup()

            pathControls
                .frame(maxWidth: .infinity)
                .kiteGlassGroup()
        }
    }

    private var navigationControls: some View {
        HStack(spacing: 4) {
            Button {
                model.navigateUp()
            } label: {
                Image(systemName: "arrow.up")
                    .frame(width: 28, height: 28)
            }
            .buttonStyle(.borderless)
            .keyboardShortcut(.upArrow, modifiers: [.command])
            .help("上层文件夹 (Command+↑)")
            .accessibilityLabel("上层文件夹")

            Button {
                model.refresh()
            } label: {
                Image(systemName: "arrow.clockwise")
                    .frame(width: 28, height: 28)
            }
            .buttonStyle(.borderless)
            .help("刷新")
            .accessibilityLabel("刷新")
        }
        .padding(.horizontal, 4)
        .frame(height: 34)
    }

    private var pathControls: some View {
        HStack(spacing: 8) {
            TextField("路径", text: $model.address)
                .textFieldStyle(.plain)
                .focused($addressFocused)
                .onSubmit {
                    model.loadRoot(model.address)
                }
                .padding(.horizontal, 8)
                .frame(minHeight: 28)

            Button("打开") {
                model.loadRoot(model.address)
            }
            .buttonStyle(.borderless)
            .controlSize(.small)
            .help("打开输入的路径")
            .accessibilityLabel("打开输入的路径")
            .frame(minHeight: 28)
        }
        .padding(.horizontal, 6)
        .frame(height: 34)
    }

    private var columnBrowser: some View {
        ScrollView(.horizontal) {
            HStack(alignment: .top, spacing: 1) {
                ForEach(Array(model.columns.enumerated()), id: \.element.id) { index, column in
                    browserColumn(column, index: index)
                }
                if let selectedFile = model.selectedColumnFile {
                    previewColumn(selectedFile)
                }
            }
            .frame(maxHeight: .infinity, alignment: .top)
        }
        .background(.clear)
    }

    private func browserColumn(_ column: BrowserColumn, index: Int) -> some View {
        VStack(spacing: 0) {
            Text(column.title)
                .font(.caption.weight(.medium))
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 10)
                .frame(height: 30)
                .background(Color(nsColor: .controlBackgroundColor).opacity(0.90))

            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(column.entries) { entry in
                        entryRow(entry, columnIndex: index)
                    }
                }
            }
        }
        .frame(width: 260)
        .frame(maxHeight: .infinity)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.84))
        .overlay(alignment: .trailing) {
            Rectangle()
                .fill(Color(nsColor: .separatorColor).opacity(0.75))
                .frame(width: 1)
        }
        .contextMenu {
            directoryContextMenu(column)
        }
    }

    @ViewBuilder
    private func directoryContextMenu(_ column: BrowserColumn) -> some View {
        Button("打开") { KitePlatform.open(column.path) }
        Button("在 Finder 中显示") { KitePlatform.revealInFinder(column.path) }
        Button("快速查看") { KitePlatform.quickLook(column.path) }
        Button("拷贝路径") { KitePlatform.copyPath(column.path) }
        Divider()
        Button("新建文件夹…") { model.createFolder(in: column.path) }
            .disabled(!KitePlatform.canModify(column.path))
        Button("新建文件…") { model.createFile(in: column.path) }
            .disabled(!KitePlatform.canModify(column.path))
        Button("重命名…") { model.rename(column.path) }
            .disabled(!KitePlatform.canModify(column.path))
        Divider()
        Button("分享…") { KitePlatform.share(column.path) }
        Divider()
        Button("移到废纸篓") { model.moveToTrash(path: column.path) }
            .disabled(!KitePlatform.canModify(column.path))
    }

    private func previewColumn(_ entry: BrowserEntry) -> some View {
        VStack(spacing: 0) {
            HStack {
                Text("预览")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.secondary)
                Spacer()
                Button {
                    KitePlatform.copyText(previewInfoText(entry))
                } label: {
                    Image(systemName: "doc.on.doc")
                        .font(.caption)
                        .frame(width: 20, height: 20)
                }
                .buttonStyle(.borderless)
                .help("复制预览信息")
                .accessibilityLabel("复制预览信息")
            }
                .padding(.horizontal, 10)
                .frame(height: 30)
                .background(Color(nsColor: .controlBackgroundColor).opacity(0.90))

            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    QuickLookPreviewView(url: URL(fileURLWithPath: entry.path))
                        .frame(height: 220)
                        .background(Color(nsColor: .controlBackgroundColor))
                        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))

                    Text(entry.name)
                        .font(.headline)
                        .lineLimit(2)
                        .truncationMode(.middle)

                    VStack(alignment: .leading, spacing: 10) {
                        previewProperty("类型", entry.kindLabel)
                        previewProperty("大小", formattedSize(entry.size))
                        previewProperty("修改日期", formattedDate(entry.modifiedUnixMs))
                        previewProperty("位置", entry.path)
                        if let git = entry.gitStatus {
                            previewProperty("Git", git)
                        }
                        if let svn = entry.svnStatus {
                            previewProperty("SVN", svn)
                        }
                    }
                }
                .padding(12)
                .textSelection(.enabled)
            }
        }
        .frame(width: 300)
        .frame(maxHeight: .infinity)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.84))
        .overlay(alignment: .trailing) {
            Rectangle()
                .fill(Color(nsColor: .separatorColor).opacity(0.75))
                .frame(width: 1)
        }
    }

    private func previewProperty(_ label: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
            Text(value)
                .font(.caption)
                .textSelection(.enabled)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func previewInfoText(_ entry: BrowserEntry) -> String {
        var lines = [
            "名称：\(entry.name)",
            "类型：\(entry.kindLabel)",
            "大小：\(formattedSize(entry.size))",
            "修改日期：\(formattedDate(entry.modifiedUnixMs))",
            "位置：\(entry.path)",
        ]
        if let git = entry.gitStatus {
            lines.append("Git：\(git)")
        }
        if let svn = entry.svnStatus {
            lines.append("SVN：\(svn)")
        }
        return lines.joined(separator: "\n")
    }

    private func formattedSize(_ bytes: UInt64) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file)
    }

    private func formattedDate(_ unixMs: Int64?) -> String {
        guard let unixMs else { return "未知" }
        let date = Date(timeIntervalSince1970: TimeInterval(unixMs) / 1_000)
        return date.formatted(date: .abbreviated, time: .shortened)
    }
    private func entryRow(_ entry: BrowserEntry, columnIndex: Int) -> some View {
        let selected = model.columns[columnIndex].selectedPath == entry.path
        return Button {
            model.select(column: columnIndex, entry: entry)
        } label: {
            HStack(spacing: 7) {
                entryIcon(entry)
                Text(entry.name)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 4)
                if entry.isDirectory {
                    Image(systemName: "chevron.right")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
            }            .padding(.horizontal, 8)
            .frame(height: 30)
            .background(selected ? Color.accentColor.opacity(0.35) : Color.clear)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(entryAccessibilityLabel(entry))
        .contextMenu {
            Button("打开") { KitePlatform.open(entry.path) }
            Button("在 Finder 中显示") { KitePlatform.revealInFinder(entry.path) }
            Button("快速查看") { KitePlatform.quickLook(entry.path) }
            Button("拷贝路径") { KitePlatform.copyPath(entry.path) }
            Divider()
            Button("新建文件夹…") {
                model.createFolder(in: URL(fileURLWithPath: entry.path).deletingLastPathComponent().path)
            }
            Button("新建文件…") {
                model.createFile(in: URL(fileURLWithPath: entry.path).deletingLastPathComponent().path)
            }
            Divider()
            Button("分享…") { KitePlatform.share(entry.path) }
            Divider()
            Button("重命名…") { model.rename(entry.path) }
            Button("移到废纸篓") { model.moveToTrash(entry) }
        }
    }

    private func entryAccessibilityLabel(_ entry: BrowserEntry) -> String {
        var parts = [entry.name]
        if let git = entry.gitStatus {
            parts.append("Git \(git)")
        }
        if let svn = entry.svnStatus {
            parts.append("SVN \(svn)")
        }
        return parts.joined(separator: "，")
    }
    private func entryIcon(_ entry: BrowserEntry) -> some View {
        ZStack(alignment: .bottomTrailing) {
            Image(systemName: entry.isDirectory ? "folder.fill" : "doc")
                .foregroundStyle(entry.isDirectory ? Color.orange : Color.secondary)
                .frame(width: 18, height: 18)

            if let badge = vcsBadge(for: entry) {
                let radius: CGFloat = badge.isSvn ? 3 : 6
                Text(badge.code)
                    .font(.system(size: 8, weight: .bold))
                    .foregroundStyle(badge.color)
                    .frame(width: 12, height: 12)
                    .background(
                        Color(nsColor: .windowBackgroundColor),
                        in: RoundedRectangle(cornerRadius: radius, style: .continuous)
                    )
                    .overlay {
                        RoundedRectangle(cornerRadius: radius, style: .continuous)
                            .strokeBorder(badge.color.opacity(0.9), lineWidth: 0.75)
                    }
                    .offset(x: 2, y: 2)
            }
        }
        .frame(width: 20, height: 20)
        .accessibilityHidden(true)
    }

    private struct VcsIconBadge {
        let code: String
        let color: Color
        let isSvn: Bool
        let rank: Int
    }

    private func vcsBadge(for entry: BrowserEntry) -> VcsIconBadge? {
        let git = vcsBadge(from: entry.gitStatus, isSvn: false)
        let svn = vcsBadge(from: entry.svnStatus, isSvn: true)
        if let git, let svn {
            return svn.rank > git.rank ? svn : git
        }
        return git ?? svn
    }

    private func vcsBadge(from value: String?, isSvn: Bool) -> VcsIconBadge? {
        guard let value else { return nil }
        let code: String
        if let open = value.lastIndex(of: "("),
           let close = value.lastIndex(of: ")"),
           open < close {
            code = String(value[value.index(after: open)..<close])
        } else {
            code = String(value.prefix(1))
        }

        let color: Color
        let rank: Int
        switch code {
        case "C":
            color = .red
            rank = 10
        case "!":
            color = .red
            rank = 9
        case "D":
            color = .red
            rank = 8
        case "R":
            color = .green
            rank = 7
        case "A":
            color = .green
            rank = 6
        case "P":
            color = .orange
            rank = 5
        case "M":
            color = .orange
            rank = 4
        case "?":
            color = .cyan
            rank = 3
        case "X":
            color = .gray
            rank = 2
        default:
            color = .gray
            rank = 1
        }
        return VcsIconBadge(code: code, color: color, isSvn: isSvn, rank: rank)
    }
    private var statusBar: some View {
        HStack {
            if let error = model.errorMessage {
                Text(error).foregroundStyle(.red)
            } else if let update = model.updateStatusMessage {
                Text(update).foregroundStyle(.secondary)
            } else {
                Text(model.currentPath).lineLimit(1).truncationMode(.middle)
            }
            Spacer()
            Text("\(model.columns.last?.entries.count ?? 0) 个项目")
                .foregroundStyle(.secondary)
        }
        .font(.caption)
        .padding(.horizontal, 12)
        .frame(height: 28)
        .background(.ultraThinMaterial)
        .overlay(alignment: .top) {
            Divider()
        }
    }
}

private struct QuickLookPreviewView: NSViewRepresentable {
    let url: URL

    func makeNSView(context: Context) -> QLPreviewView {
        let view = QLPreviewView(frame: .zero, style: .normal)!
        view.previewItem = url as NSURL
        return view
    }

    func updateNSView(_ view: QLPreviewView, context: Context) {
        view.previewItem = url as NSURL
    }
}

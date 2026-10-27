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
    }

    private func entryRow(_ entry: BrowserEntry, columnIndex: Int) -> some View {
        let selected = model.columns[columnIndex].selectedPath == entry.path
        return Button {
            model.select(column: columnIndex, entry: entry)
        } label: {
            HStack(spacing: 7) {
                Image(systemName: entry.isDirectory ? "folder.fill" : "doc")
                    .foregroundStyle(entry.isDirectory ? Color.orange : Color.secondary)
                    .frame(width: 18)
                Text(entry.name)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 4)
                if let git = entry.gitStatus {
                    Text(git).font(.caption2).foregroundStyle(.green)
                } else if let svn = entry.svnStatus {
                    Text(svn).font(.caption2).foregroundStyle(.blue)
                }
                if entry.isDirectory {
                    Image(systemName: "chevron.right")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
            }
            .padding(.horizontal, 8)
            .frame(height: 30)
            .background(selected ? Color.accentColor.opacity(0.35) : Color.clear)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .contextMenu {
            Button("打开") { KitePlatform.open(entry.path) }
            Button("在 Finder 中显示") { KitePlatform.revealInFinder(entry.path) }
            Button("快速查看") { KitePlatform.quickLook(entry.path) }
            Button("拷贝路径") { KitePlatform.copyPath(entry.path) }
            Divider()
            Button("分享…") { KitePlatform.share(entry.path) }
            Divider()
            Button("移到废纸篓") { model.moveToTrash(entry) }
        }
    }

    private var statusBar: some View {
        HStack {
            if let error = model.errorMessage {
                Text(error).foregroundStyle(.red)
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

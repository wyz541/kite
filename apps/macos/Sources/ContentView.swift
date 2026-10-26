import SwiftUI

struct ContentView: View {
    @ObservedObject var model: KiteModel
    @FocusState private var addressFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            toolbar
            Divider()
            columnBrowser
            Divider()
            statusBar
        }
        .frame(minWidth: 900, minHeight: 620)
        .onChange(of: model.focusedAddressRequest) { _ in
            addressFocused = true
        }
    }

    private var toolbar: some View {
        HStack(spacing: 8) {
            Button {
                model.navigateUp()
            } label: {
                Image(systemName: "arrow.up")
            }
            .buttonStyle(.borderless)
            .help("上层文件夹")

            TextField("路径", text: $model.address)
                .textFieldStyle(.roundedBorder)
                .focused($addressFocused)
                .onSubmit {
                    model.loadRoot(model.address)
                }

            Button("前往") {
                model.loadRoot(model.address)
            }

            Button {
                model.refresh()
            } label: {
                Image(systemName: "arrow.clockwise")
            }
            .buttonStyle(.borderless)
            .help("刷新")
        }
        .padding(.horizontal, 12)
        .frame(height: 44)
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
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private func browserColumn(_ column: BrowserColumn, index: Int) -> some View {
        VStack(spacing: 0) {
            Text(column.title)
                .font(.caption.weight(.medium))
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 10)
                .frame(height: 30)
                .background(Color(nsColor: .controlBackgroundColor))

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
        .background(Color(nsColor: .controlBackgroundColor))
        .overlay(alignment: .trailing) {
            Rectangle()
                .fill(Color(nsColor: .separatorColor))
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
        .padding(.horizontal, 10)
        .frame(height: 26)
        .background(Color(nsColor: .windowBackgroundColor))
    }
}

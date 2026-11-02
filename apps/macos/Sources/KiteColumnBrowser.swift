import AppKit
import SwiftUI

struct KiteColumnBrowser: View {
    @ObservedObject var model: KiteModel
    @Binding var showsPreview: Bool
    @AppStorage("kite.macos.column-details-visible") private var showsColumnDetails = false

    var body: some View {
        HStack(spacing: showsColumnDetails ? 8 : 10) {
            GeometryReader { viewport in
                ScrollViewReader { proxy in
                    ScrollView(.horizontal) {
                        HStack(alignment: .top, spacing: 8) {
                            ForEach(Array(model.columns.enumerated()), id: \.element.id) { index, column in
                                let isActive = index == model.columns.count - 1
                                KiteBrowserColumn(
                                    model: model,
                                    column: column,
                                    index: index,
                                    isActive: isActive,
                                    showsDetails: showsColumnDetails && isActive,
                                    onToggleDetails: {
                                        withAnimation(.snappy(duration: 0.22)) {
                                            showsColumnDetails.toggle()
                                        }
                                    }
                                )
                            }
                        }
                        .padding(.vertical, 1)
                        // Keep short column sets at the leading edge instead of the trailing anchor.
                        .frame(minWidth: viewport.size.width, alignment: .leading)
                        .background(KiteShiftWheelHorizontalScroller())
                    }
                    .scrollIndicators(.hidden)
                    .kiteDefaultTrailingAnchor()
                    .onAppear {
                        scrollToActiveColumn(proxy, animated: false)
                    }
                    .onChange(of: model.columns.map(\.id)) { _ in
                        scrollToActiveColumn(proxy, animated: true)
                    }
                    .onChange(of: showsPreview) { _ in
                        scrollToActiveColumn(proxy, animated: true)
                    }
                    .onChange(of: model.selectedColumnEntry?.path) { _ in
                        scrollToActiveColumn(proxy, animated: true)
                    }
                    .onChange(of: showsColumnDetails) { _ in
                        scrollToActiveColumn(proxy, animated: true)
                    }
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)

            if showsPreview, let selectedEntry = model.selectedColumnEntry {
                KitePreviewPane(model: model, entry: selectedEntry) {
                    withAnimation(.snappy(duration: 0.22)) {
                        showsPreview = false
                    }
                }
                .transition(.move(edge: .trailing).combined(with: .opacity))
            }
        }
        .animation(.snappy(duration: 0.22), value: showsPreview)
        .animation(.snappy(duration: 0.22), value: showsColumnDetails)
    }

    private func scrollToActiveColumn(_ proxy: ScrollViewProxy, animated: Bool) {
        guard let activeColumnID = model.columns.last?.id else { return }
        DispatchQueue.main.async {
            if animated {
                withAnimation(.snappy(duration: 0.22)) {
                    proxy.scrollTo(activeColumnID, anchor: .trailing)
                }
            } else {
                proxy.scrollTo(activeColumnID, anchor: .trailing)
            }
        }
    }
}

private struct KiteShiftWheelHorizontalScroller: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        KiteShiftWheelHorizontalScrollerView()
    }

    func updateNSView(_ nsView: NSView, context: Context) {}
}

private final class KiteShiftWheelHorizontalScrollerView: NSView {
    private weak var scrollView: NSScrollView?
    private var eventMonitor: Any?

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        removeEventMonitor()
        scrollView = nil

        guard window != nil else { return }
        DispatchQueue.main.async { [weak self] in
            self?.installEventMonitorIfNeeded()
        }
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }

    deinit {
        removeEventMonitor()
    }

    private func installEventMonitorIfNeeded() {
        guard eventMonitor == nil,
              let enclosingScrollView = enclosingScrollView else { return }
        scrollView = enclosingScrollView
        eventMonitor = NSEvent.addLocalMonitorForEvents(matching: .scrollWheel) {
            [weak self] event in
            self?.handleShiftWheel(event) ?? event
        }
    }

    private func removeEventMonitor() {
        guard let eventMonitor else { return }
        NSEvent.removeMonitor(eventMonitor)
        self.eventMonitor = nil
    }

    private func handleShiftWheel(_ event: NSEvent) -> NSEvent? {
        guard event.modifierFlags.contains(.shift),
              let scrollView,
              event.window === scrollView.window else { return event }

        let location = scrollView.convert(event.locationInWindow, from: nil)
        guard scrollView.bounds.contains(location),
              abs(event.scrollingDeltaY) > abs(event.scrollingDeltaX),
              event.scrollingDeltaY != 0 else { return event }

        let clipView = scrollView.contentView
        let documentWidth = scrollView.documentView?.frame.width ?? 0
        let maxX = max(0, documentWidth - clipView.bounds.width)
        let scale: CGFloat = event.hasPreciseScrollingDeltas ? 1 : 24
        let targetX = (clipView.bounds.origin.x - event.scrollingDeltaY * scale)
            .clamped(to: 0...maxX)
        guard targetX != clipView.bounds.origin.x else { return nil }

        clipView.scroll(to: NSPoint(x: targetX, y: clipView.bounds.origin.y))
        scrollView.reflectScrolledClipView(clipView)
        return nil
    }
}

private extension Comparable {
    func clamped(to range: ClosedRange<Self>) -> Self {
        min(max(self, range.lowerBound), range.upperBound)
    }
}

private extension View {
    @ViewBuilder
    func kiteDefaultTrailingAnchor() -> some View {
        if #available(macOS 15.0, *) {
            defaultScrollAnchor(.trailing)
                .defaultScrollAnchor(.trailing, for: .sizeChanges)
        } else if #available(macOS 14.0, *) {
            defaultScrollAnchor(.trailing)
        } else {
            self
        }
    }
}

private enum KiteColumnDetailsLayout {
    static let spacing: CGFloat = 8
    static let dateWidth: CGFloat = 84
    static let kindWidth: CGFloat = 72
    static let sizeWidth: CGFloat = 54
}

private struct KiteBrowserColumn: View {
    @ObservedObject var model: KiteModel
    let column: BrowserColumn
    let index: Int
    let isActive: Bool
    let showsDetails: Bool
    let onToggleDetails: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            header
            Rectangle()
                .fill(Color.primary.opacity(0.06))
                .frame(height: 0.5)
                .padding(.horizontal, 10)

            if showsDetails {
                detailsHeader
                    .transition(.move(edge: .top).combined(with: .opacity))
            }

            if column.entries.isEmpty {
                emptyState
            } else {
                ScrollView {
                    LazyVStack(spacing: 2) {
                        ForEach(column.entries) { entry in
                            KiteBrowserRow(
                                model: model,
                                entry: entry,
                                columnIndex: index,
                                isSelected: column.selectedPath == entry.path,
                                showsDetails: showsDetails
                            )
                        }
                    }
                    .padding(.horizontal, 5)
                    .padding(.vertical, 5)
                }
                .scrollIndicators(.hidden)
            }
        }
        .frame(width: showsDetails ? KiteTheme.detailedColumnWidth : KiteTheme.columnWidth)
        .frame(maxHeight: .infinity)
        .background(
            Color(nsColor: .controlBackgroundColor).opacity(0.42),
            in: RoundedRectangle(cornerRadius: 20, style: .continuous)
        )
        .background(
            .ultraThinMaterial,
            in: RoundedRectangle(cornerRadius: 20, style: .continuous)
        )
        .overlay {
            RoundedRectangle(cornerRadius: 20, style: .continuous)
                .strokeBorder(
                    isActive ? Color.accentColor.opacity(0.22) : Color.primary.opacity(0.07),
                    lineWidth: 0.7
                )
        }
        .shadow(color: .black.opacity(0.08), radius: 12, y: 3)
        .animation(.snappy(duration: 0.22), value: showsDetails)
        .contextMenu {
            KiteDirectoryMenu(model: model, column: column)
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("\(column.title)，\(column.entries.count) 个项目")
    }

    private var header: some View {
        HStack(spacing: 8) {
            Image(systemName: isActive ? "folder.fill" : "folder")
                .font(.system(size: 12, weight: .semibold))
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(isActive ? Color.accentColor : Color.secondary)

            Text(column.title)
                .font(.system(size: 11.5, weight: .semibold))
                .lineLimit(1)
                .truncationMode(.middle)

            Spacer(minLength: 4)

            Text("\(column.entries.count)")
                .font(.system(size: 9.5, weight: .semibold, design: .rounded))
                .foregroundStyle(.tertiary)
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(Color.primary.opacity(0.055), in: Capsule())

            if isActive {
                Button(action: onToggleDetails) {
                    Image(systemName: showsDetails ? "rectangle.grid.2x2.fill" : "rectangle.grid.2x2")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(showsDetails ? Color.accentColor : Color.secondary)
                        .frame(width: 22, height: 22)
                        .contentShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
                }
                .buttonStyle(.plain)
                .help(showsDetails ? "隐藏详细信息" : "显示详细信息")
                .accessibilityLabel(showsDetails ? "隐藏详细信息" : "显示详细信息")
            }

            Menu {
                KiteDirectoryMenu(model: model, column: column)
            } label: {
                Image(systemName: "ellipsis")
                    .font(.system(size: 11, weight: .semibold))
                    .frame(width: 22, height: 22)
                    .contentShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .help("文件夹操作")
            .accessibilityLabel("文件夹操作")
        }
        .padding(.leading, 12)
        .padding(.trailing, 8)
        .frame(height: 39)
        .overlay(alignment: .bottomLeading) {
            if isActive {
                Capsule()
                    .fill(
                        LinearGradient(
                            colors: [KiteTheme.brandOrange, Color.accentColor],
                            startPoint: .leading,
                            endPoint: .trailing
                        )
                    )
                    .frame(width: 28, height: 2)
                    .padding(.leading, 13)
                    .padding(.bottom, -1)
            }
        }
    }

    private var detailsHeader: some View {
        HStack(spacing: KiteColumnDetailsLayout.spacing) {
            Text("名称")
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.leading, 29)

            Text("日期")
                .frame(width: KiteColumnDetailsLayout.dateWidth, alignment: .leading)

            Text("类型")
                .frame(width: KiteColumnDetailsLayout.kindWidth, alignment: .leading)

            Text("大小")
                .frame(width: KiteColumnDetailsLayout.sizeWidth, alignment: .trailing)
        }
        .font(.system(size: 9.5, weight: .semibold))
        .foregroundStyle(.tertiary)
        .padding(.horizontal, 8)
        .frame(height: 24)
        .background(Color.primary.opacity(0.025))
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(Color.primary.opacity(0.045))
                .frame(height: 0.5)
        }
    }

    private var emptyState: some View {
        VStack(spacing: 10) {
            ZStack {
                Circle()
                    .fill(Color.primary.opacity(0.045))
                    .frame(width: 54, height: 54)

                Image(systemName: "wind")
                    .font(.system(size: 22, weight: .light))
                    .foregroundStyle(.tertiary)
            }

            Text("文件夹为空")
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(.secondary)

            Button("新建文件夹…") {
                model.createFolder(in: column.path)
            }
            .buttonStyle(.plain)
            .font(.system(size: 11, weight: .medium))
            .foregroundStyle(Color.accentColor)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(24)
    }
}

private struct KiteBrowserRow: View {
    @ObservedObject var model: KiteModel
    let entry: BrowserEntry
    let columnIndex: Int
    let isSelected: Bool
    let showsDetails: Bool

    @State private var isHovering = false

    var body: some View {
        Button {
            model.select(column: columnIndex, entry: entry)
        } label: {
            HStack(spacing: showsDetails ? KiteColumnDetailsLayout.spacing : 9) {
                nameCell

                if showsDetails {
                    Text(metadataDate)
                        .font(.system(size: 10.5, design: .rounded))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .frame(width: KiteColumnDetailsLayout.dateWidth, alignment: .leading)

                    Text(metadataKind)
                        .font(.system(size: 10.5))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .frame(width: KiteColumnDetailsLayout.kindWidth, alignment: .leading)

                    Text(metadataSize)
                        .font(.system(size: 10.5, design: .rounded))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .monospacedDigit()
                        .frame(width: KiteColumnDetailsLayout.sizeWidth, alignment: .trailing)
                }
            }
            .padding(.horizontal, 8)
            .frame(height: 32)
            .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
            .background {
                if isSelected {
                    RoundedRectangle(cornerRadius: 9, style: .continuous)
                        .kiteGlass(.selection, cornerRadius: 9)
                } else if isHovering {
                    RoundedRectangle(cornerRadius: 9, style: .continuous)
                        .fill(Color.primary.opacity(0.055))
                }
            }
        }
        .buttonStyle(.plain)
        .onHover { isHovering = $0 }
        .simultaneousGesture(
            TapGesture(count: 2).onEnded {
                if !entry.isDirectory {
                    KitePlatform.open(entry.path)
                }
            }
        )
        .contextMenu {
            KiteEntryMenu(model: model, entry: entry)
        }
        .help(entry.path)
        .accessibilityLabel(accessibilityLabel)
    }

    private var nameCell: some View {
        HStack(spacing: 9) {
            ZStack(alignment: .bottomTrailing) {
                KiteFileIcon(path: entry.path, isDirectory: entry.isDirectory, size: 20)

                if let badge = vcsBadge {
                    Text(badge.code)
                        .font(.system(size: 7.5, weight: .bold, design: .rounded))
                        .foregroundStyle(badge.color)
                        .frame(width: 11, height: 11)
                        .background(
                            Color(nsColor: .windowBackgroundColor),
                            in: RoundedRectangle(
                                cornerRadius: badge.isSvn ? 3 : 5,
                                style: .continuous
                            )
                        )
                        .overlay {
                            RoundedRectangle(
                                cornerRadius: badge.isSvn ? 3 : 5,
                                style: .continuous
                            )
                            .strokeBorder(badge.color.opacity(0.85), lineWidth: 0.7)
                        }
                        .offset(x: 2, y: 2)
                }
            }
            .frame(width: 22, height: 22)
            .accessibilityHidden(true)

            Text(entry.name)
                .font(.system(size: 12.5, weight: isSelected ? .medium : .regular))
                .lineLimit(1)
                .truncationMode(.middle)
                .layoutPriority(1)

            Spacer(minLength: 4)

            if entry.isDirectory {
                Image(systemName: "chevron.right")
                    .font(.system(size: 8.5, weight: .semibold))
                    .foregroundStyle(.tertiary)
                    .padding(.trailing, 3)
            } else if isHovering {
                Image(systemName: "arrow.up.forward")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(.tertiary)
                    .padding(.trailing, 3)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var metadataDate: String {
        guard let modifiedUnixMs = entry.modifiedUnixMs else { return "—" }
        let date = Date(timeIntervalSince1970: TimeInterval(modifiedUnixMs) / 1_000)
        return date.formatted(date: .numeric, time: .omitted)
    }

    private var metadataKind: String {
        entry.isDirectory ? "文件夹" : entry.kindLabel
    }

    private var metadataSize: String {
        guard !entry.isDirectory else { return "—" }
        let megabytes = Double(entry.size) / 1_000_000
        return String(format: "%.2f MB", megabytes)
    }

    private var accessibilityLabel: String {
        var parts = [entry.name, entry.isDirectory ? "文件夹" : entry.kindLabel]
        if let git = entry.gitStatus {
            parts.append("Git \(git)")
        }
        if let svn = entry.svnStatus {
            parts.append("SVN \(svn)")
        }
        return parts.joined(separator: "，")
    }

    private var vcsBadge: KiteVcsBadge? {
        let git = KiteVcsBadge(value: entry.gitStatus, isSvn: false)
        let svn = KiteVcsBadge(value: entry.svnStatus, isSvn: true)
        if let git, let svn {
            return svn.rank > git.rank ? svn : git
        }
        return git ?? svn
    }
}

private struct KiteVcsBadge {
    let code: String
    let color: Color
    let isSvn: Bool
    let rank: Int

    init?(value: String?, isSvn: Bool) {
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
        case "C", "!", "D":
            color = .red
            rank = code == "C" ? 10 : (code == "!" ? 9 : 8)
        case "R", "A":
            color = .green
            rank = code == "R" ? 7 : 6
        case "P", "M":
            color = .orange
            rank = code == "P" ? 5 : 4
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

        self.code = code
        self.color = color
        self.isSvn = isSvn
        self.rank = rank
    }
}

private struct KiteEntryMenu: View {
    @ObservedObject var model: KiteModel
    let entry: BrowserEntry

    var body: some View {
        Button {
            KitePlatform.open(entry.path)
        } label: {
            Label("打开", systemImage: "arrow.up.forward.app")
        }

        if KitePlatform.isEditableTextFile(entry.path) {
            Button {
                KitePlatform.openTextEditor(entry.path)
            } label: {
                Label("在 Kite 编辑器中打开", systemImage: "square.and.pencil")
            }
        }

        Button {
            KitePlatform.revealInFinder(entry.path)
        } label: {
            Label("在 Finder 中显示", systemImage: "folder")
        }

        Button {
            KitePlatform.quickLook(entry.path)
        } label: {
            Label("快速查看", systemImage: "eye")
        }

        Button {
            KitePlatform.copyPath(entry.path)
        } label: {
            Label("拷贝路径", systemImage: "point.topleft.down.to.point.bottomright.curvepath")
        }

        Divider()

        Button {
            model.copyFile(entry.path)
        } label: {
            Label("拷贝", systemImage: "doc.on.doc")
        }

        Button {
            model.cutFile(entry.path)
        } label: {
            Label("剪切", systemImage: "scissors")
        }

        Button {
            model.pasteFiles(into: pasteDirectory)
        } label: {
            Label("粘贴", systemImage: "doc.on.clipboard")
        }

        Button {
            model.copyTo(path: entry.path)
        } label: {
            Label("拷贝到…", systemImage: "folder.badge.plus")
        }

        Divider()

        Button {
            model.createFolder(in: parentDirectory)
        } label: {
            Label("新建文件夹…", systemImage: "folder.badge.plus")
        }

        Button {
            model.createFile(in: parentDirectory)
        } label: {
            Label("新建文件…", systemImage: "doc.badge.plus")
        }

        Button {
            model.toggleFavorite(
                path: entry.path,
                name: entry.name,
                isDirectory: entry.isDirectory
            )
        } label: {
            Label(
                isFavorite ? "从收藏移除" : "添加到收藏",
                systemImage: isFavorite ? "star.slash" : "star.badge.plus"
            )
        }

        Divider()

        Button {
            KitePlatform.share(entry.path)
        } label: {
            Label("分享…", systemImage: "square.and.arrow.up")
        }

        Divider()

        Button {
            model.rename(entry.path)
        } label: {
            Label("重命名…", systemImage: "pencil")
        }
        .disabled(!KitePlatform.canModify(entry.path))

        Button(role: .destructive) {
            model.moveToTrash(entry)
        } label: {
            Label("移到废纸篓", systemImage: "trash")
        }
        .disabled(!KitePlatform.canModify(entry.path))
    }

    private var parentDirectory: String {
        URL(fileURLWithPath: entry.path).deletingLastPathComponent().path
    }

    private var pasteDirectory: String {
        entry.isDirectory ? entry.path : parentDirectory
    }

    private var isFavorite: Bool {
        model.isFavorite(path: entry.path)
    }
}

private struct KiteDirectoryMenu: View {
    @ObservedObject var model: KiteModel
    let column: BrowserColumn

    var body: some View {
        Button {
            KitePlatform.open(column.path)
        } label: {
            Label("打开", systemImage: "arrow.up.forward.app")
        }

        Button {
            KitePlatform.revealInFinder(column.path)
        } label: {
            Label("在 Finder 中显示", systemImage: "folder")
        }

        Button {
            KitePlatform.quickLook(column.path)
        } label: {
            Label("快速查看", systemImage: "eye")
        }

        Button {
            KitePlatform.copyPath(column.path)
        } label: {
            Label("拷贝路径", systemImage: "point.topleft.down.to.point.bottomright.curvepath")
        }

        Divider()

        Button {
            model.copyFile(column.path)
        } label: {
            Label("拷贝文件夹", systemImage: "doc.on.doc")
        }

        Button {
            model.pasteFiles(into: column.path)
        } label: {
            Label("粘贴", systemImage: "doc.on.clipboard")
        }

        Button {
            model.copyTo(path: column.path)
        } label: {
            Label("拷贝到…", systemImage: "folder.badge.plus")
        }

        Divider()

        Button {
            model.createFolder(in: column.path)
        } label: {
            Label("新建文件夹…", systemImage: "folder.badge.plus")
        }
        .disabled(!KitePlatform.canModify(column.path))

        Button {
            model.createFile(in: column.path)
        } label: {
            Label("新建文件…", systemImage: "doc.badge.plus")
        }
        .disabled(!KitePlatform.canModify(column.path))

        Button {
            model.toggleFavorite(
                path: column.path,
                name: column.title,
                isDirectory: true
            )
        } label: {
            Label(
                isFavorite ? "从收藏移除" : "添加到收藏",
                systemImage: isFavorite ? "star.slash" : "star.badge.plus"
            )
        }

        Divider()

        Button {
            KitePlatform.share(column.path)
        } label: {
            Label("分享…", systemImage: "square.and.arrow.up")
        }

        Divider()

        Button {
            model.rename(column.path)
        } label: {
            Label("重命名…", systemImage: "pencil")
        }
        .disabled(!KitePlatform.canModify(column.path))

        Button(role: .destructive) {
            model.moveToTrash(path: column.path)
        } label: {
            Label("移到废纸篓", systemImage: "trash")
        }
        .disabled(!KitePlatform.canModify(column.path))
    }

    private var isFavorite: Bool {
        model.isFavorite(path: column.path)
    }
}

import AppKit
import SwiftUI

struct KiteColumnBrowser: View {
    @ObservedObject var model: KiteModel
    @Binding var showsPreview: Bool
    @AppStorage("kite.macos.column-details-visible") private var showsColumnDetails = false
    @AppStorage("kite.macos.column-date-visible") private var showsDateColumn = true
    @AppStorage("kite.macos.column-kind-visible") private var showsKindColumn = true
    @AppStorage("kite.macos.column-size-visible") private var showsSizeColumn = true
    @AppStorage("kite.macos.detailed-column-width") private var preferredColumnWidth = 416.0
    @State private var detailedColumnWidth = KiteTheme.detailedColumnWidth

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
                                    showsDateColumn: $showsDateColumn,
                                    showsKindColumn: $showsKindColumn,
                                    showsSizeColumn: $showsSizeColumn,
                                    preferredColumnWidth: $preferredColumnWidth,
                                    columnWidth: showsColumnDetails && isActive
                                        ? detailedColumnWidth
                                        : KiteTheme.columnWidth,
                                    onToggleDetails: {
                                        withAnimation(.snappy(duration: 0.22)) {
                                            showsColumnDetails.toggle()
                                        }
                                    },
                                    onShowDetails: {
                                        withAnimation(.snappy(duration: 0.22)) {
                                            showsColumnDetails = true
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
                        scrollToActiveColumn(
                            proxy,
                            viewportWidth: viewport.size.width,
                            animated: false
                        )
                    }
                    .onChange(of: model.columns.map(\.id)) { _ in
                        scrollToActiveColumn(
                            proxy,
                            viewportWidth: viewport.size.width,
                            animated: true
                        )
                    }
                    .onChange(of: showsPreview) { _ in
                        scrollToActiveColumn(
                            proxy,
                            viewportWidth: viewport.size.width,
                            animated: false
                        )
                    }
                    .onChange(of: model.selectedColumnEntry?.path) { _ in
                        scrollToActiveColumn(
                            proxy,
                            viewportWidth: viewport.size.width,
                            animated: true
                        )
                    }
                    .onChange(of: showsColumnDetails) { _ in
                        scrollToActiveColumn(
                            proxy,
                            viewportWidth: viewport.size.width,
                            animated: true
                        )
                    }
                    .onChange(of: detailedColumnWidth) { _ in
                        scrollToActiveColumn(
                            proxy,
                            viewportWidth: viewport.size.width,
                            animated: true
                        )
                    }
                    .onChange(of: showsDateColumn) { _ in
                        scrollToActiveColumn(proxy, viewportWidth: viewport.size.width, animated: true)
                    }
                    .onChange(of: showsKindColumn) { _ in
                        scrollToActiveColumn(proxy, viewportWidth: viewport.size.width, animated: true)
                    }
                    .onChange(of: showsSizeColumn) { _ in
                        scrollToActiveColumn(proxy, viewportWidth: viewport.size.width, animated: true)
                    }
                    .onChange(of: viewport.size.width) { width in
                        scrollToActiveColumn(
                            proxy,
                            viewportWidth: width,
                            animated: false
                        )
                    }
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .clipped()

            if showsPreview {
                previewPane
                    .fixedSize(horizontal: true, vertical: false)
                    .layoutPriority(1)
                    .clipped()
            }
        }
        .animation(.snappy(duration: 0.22), value: showsColumnDetails)
        .animation(.snappy(duration: 0.22), value: detailedColumnWidth)
        .task(id: detailsMeasurementKey) {
            await measureDetailedColumnWidth()
        }
    }

    @ViewBuilder
    private var previewPane: some View {
        if let selectedEntry = model.selectedColumnEntry {
            KitePreviewPane(model: model, entry: selectedEntry) {
                showsPreview = false
            }
        } else {
            KitePreviewEmptyPane {
                showsPreview = false
            }
        }
    }

    private var detailsMeasurementKey: ColumnDetailsMeasurementKey {
        ColumnDetailsMeasurementKey(
            showsDetails: showsColumnDetails,
            showsDateColumn: showsDateColumn,
            showsKindColumn: showsKindColumn,
            showsSizeColumn: showsSizeColumn,
            preferredColumnWidth: preferredColumnWidth,
            columnID: model.columns.last?.id,
            entries: model.columns.last?.entries ?? []
        )
    }

    private func measureDetailedColumnWidth() async {
        guard showsColumnDetails,
              let entries = model.columns.last?.entries else {
            detailedColumnWidth = KiteTheme.detailedColumnWidth
            return
        }

        let names = entries.map(\.name)
        let dateVisible = showsDateColumn
        let kindVisible = showsKindColumn
        let sizeVisible = showsSizeColumn
        let preferredWidth = CGFloat(preferredColumnWidth)
        let measuredWidth = await Task.detached(priority: .userInitiated) {
            KiteColumnDetailsLayout.columnWidth(
                for: names,
                showsDateColumn: dateVisible,
                showsKindColumn: kindVisible,
                showsSizeColumn: sizeVisible,
                preferredWidth: preferredWidth
            )
        }.value
        guard !Task.isCancelled else { return }
        detailedColumnWidth = measuredWidth
    }

    private func scrollToActiveColumn(
        _ proxy: ScrollViewProxy,
        viewportWidth: CGFloat,
        animated: Bool
    ) {
        guard let activeColumnID = model.columns.last?.id else { return }
        let activeColumnWidth = showsColumnDetails
            ? detailedColumnWidth
            : KiteTheme.columnWidth
        let anchor: UnitPoint = activeColumnWidth > viewportWidth ? .leading : .trailing
        DispatchQueue.main.async {
            if animated {
                withAnimation(.snappy(duration: 0.22)) {
                    proxy.scrollTo(activeColumnID, anchor: anchor)
                }
            } else {
                proxy.scrollTo(activeColumnID, anchor: anchor)
            }
        }
    }
}

private struct ColumnDetailsMeasurementKey: Equatable {
    let showsDetails: Bool
    let showsDateColumn: Bool
    let showsKindColumn: Bool
    let showsSizeColumn: Bool
    let preferredColumnWidth: Double
    let columnID: String?
    let entries: [BrowserEntry]
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
            self?.handleWheel(event) ?? event
        }
    }

    private func removeEventMonitor() {
        guard let eventMonitor else { return }
        NSEvent.removeMonitor(eventMonitor)
        self.eventMonitor = nil
    }

    private func handleWheel(_ event: NSEvent) -> NSEvent? {
        guard let scrollView,
              event.window === scrollView.window else { return event }

        let location = scrollView.convert(event.locationInWindow, from: nil)
        guard scrollView.bounds.contains(location),
              abs(event.scrollingDeltaY) > abs(event.scrollingDeltaX),
              event.scrollingDeltaY != 0 else { return event }

        if event.modifierFlags.contains(.shift) {
            let clipView = scrollView.contentView
            let documentWidth = scrollView.documentView?.frame.width ?? 0
            let maxX = max(0, documentWidth - clipView.bounds.width)
            let scale: CGFloat = event.hasPreciseScrollingDeltas ? 1 : 24
            let targetX = (clipView.bounds.origin.x - event.scrollingDeltaY * scale)
                .clamped(to: 0...maxX)

            clipView.scroll(to: NSPoint(x: targetX, y: clipView.bounds.origin.y))
            scrollView.reflectScrolledClipView(clipView)
            return nil
        }

        if let innerScrollView = verticalScrollView(
            at: event.locationInWindow,
            inside: scrollView
        ), innerScrollView.canScrollVertically(for: event.scrollingDeltaY) {
            return event
        }

        // Do not let the horizontal container map remaining vertical wheel
        // input onto horizontal scrolling after the inner list reaches an edge.
        return nil
    }

    private func verticalScrollView(
        at windowLocation: NSPoint,
        inside horizontalScrollView: NSScrollView
    ) -> NSScrollView? {
        guard let contentView = horizontalScrollView.window?.contentView else {
            return nil
        }
        let location = contentView.convert(windowLocation, from: nil)
        guard let hitView = contentView.hitTest(location) else { return nil }

        var current: NSView? = hitView
        while let view = current, view !== horizontalScrollView {
            if let scrollView = view as? NSScrollView,
               scrollView !== horizontalScrollView {
                return scrollView
            }
            current = view.superview
        }
        return nil
    }
}

private extension NSScrollView {
    func canScrollVertically(for deltaY: CGFloat) -> Bool {
        guard let documentView else { return false }
        let visibleHeight = contentView.bounds.height
        let documentHeight = documentView.bounds.height
        let maxY = max(0, documentHeight - visibleHeight)
        guard maxY > 0.5 else { return false }

        let y = contentView.bounds.origin.y
        let atTop = documentView.isFlipped ? y <= 0.5 : y >= maxY - 0.5
        let atBottom = documentView.isFlipped ? y >= maxY - 0.5 : y <= 0.5
        return deltaY > 0 ? !atTop : !atBottom
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
    static let kindWidth: CGFloat = 70
    static let sizeWidth: CGFloat = 64
    private static let nameFontSize: CGFloat = 12.5
    private static let rowFixedWidth: CGFloat = 150
    private static let minimumColumnWidth: CGFloat = 250
    private static let maximumColumnWidth: CGFloat = 640

    static func columnWidth(
        for names: [String],
        showsDateColumn: Bool,
        showsKindColumn: Bool,
        showsSizeColumn: Bool,
        preferredWidth: CGFloat
    ) -> CGFloat {
        let font = NSFont.systemFont(ofSize: nameFontSize, weight: .medium)
        let textWidth = names.reduce(CGFloat.zero) { width, name in
            let nameWidth = (name as NSString).size(withAttributes: [.font: font]).width
            return max(width, ceil(nameWidth))
        }
        let details = [
            (showsDateColumn, dateWidth),
            (showsKindColumn, kindWidth),
            (showsSizeColumn, sizeWidth)
        ]
        let detailsWidth = details
            .filter { $0.0 }
            .reduce(CGFloat.zero) { $0 + $1.1 }
        let gapWidth = CGFloat(max(details.filter { $0.0 }.count - 1, 0)) * spacing
        let desiredWidth = max(
            preferredWidth,
            ceil(rowFixedWidth + detailsWidth + gapWidth + textWidth)
        )
        return min(max(desiredWidth, minimumColumnWidth), maximumColumnWidth)
    }
}

private struct KiteBrowserColumn: View {
    @ObservedObject var model: KiteModel
    let column: BrowserColumn
    let index: Int
    let isActive: Bool
    let showsDetails: Bool
    @Binding var showsDateColumn: Bool
    @Binding var showsKindColumn: Bool
    @Binding var showsSizeColumn: Bool
    @Binding var preferredColumnWidth: Double
    let columnWidth: CGFloat
    let onToggleDetails: () -> Void
    let onShowDetails: () -> Void

    @FocusState private var searchFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            header
            Rectangle()
                .fill(Color.primary.opacity(0.06))
                .frame(height: 0.5)
                .padding(.horizontal, 10)

            if isSearchOpen {
                searchBar
                    .transition(.move(edge: .top).combined(with: .opacity))
            }

            if showsDetails {
                detailsHeader
                    .transition(.move(edge: .top).combined(with: .opacity))
            }

            if column.entries.isEmpty {
                emptyState
            } else {
                ScrollView {
                    LazyVStack(spacing: 2) {
                        ForEach(column.displayRows) { row in
                            columnRow(row)
                        }
                    }
                    .padding(.horizontal, 5)
                    .padding(.vertical, 5)
                }
                .scrollIndicators(.hidden)
            }
        }
        .frame(width: columnWidth)
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
        .animation(.snappy(duration: 0.18), value: model.activeColumnSearchPath)
        .onChange(of: model.activeColumnSearchPath) { activePath in
            searchFocused = activePath == column.path
        }
        .onAppear {
            searchFocused = model.activeColumnSearchPath == column.path
        }
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

            if column.searchTruncated {
                Text("1000+")
                    .font(.system(size: 9.5, weight: .semibold, design: .rounded))
                    .foregroundStyle(.tertiary)
            }

            Button {
                model.toggleColumnSearch(path: column.path)
            } label: {
                Image(systemName: isSearchOpen ? "xmark" : "magnifyingglass")
                    .font(.system(size: 10.5, weight: .semibold))
                    .foregroundStyle(isSearchOpen ? Color.accentColor : Color.secondary)
                    .frame(width: 22, height: 22)
                    .contentShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
            }
            .buttonStyle(.plain)
            .help(isSearchOpen ? "关闭分栏搜索" : "搜索此分栏")
            .accessibilityLabel(isSearchOpen ? "关闭分栏搜索" : "搜索此分栏")

            Button {
                model.toggleColumnSort(index: index)
            } label: {
                Image(systemName: column.sortAscending ? "arrow.up" : "arrow.down")
                    .font(.system(size: 10.5, weight: .semibold))
                    .foregroundStyle(Color.secondary)
                    .frame(width: 22, height: 22)
                    .contentShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
            }
            .buttonStyle(.plain)
            .help(column.sortAscending ? "当前升序，点击改为降序" : "当前降序，点击改为升序")
            .accessibilityLabel(column.sortAscending ? "切换为降序排序" : "切换为升序排序")

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

            columnRulesMenu
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

    private var isSearchOpen: Bool {
        model.activeColumnSearchPath == column.path
    }

    private var searchBar: some View {
        HStack(spacing: 6) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 10.5, weight: .semibold))
                .foregroundStyle(.secondary)

            TextField(
                "搜索此分栏",
                text: Binding(
                    get: { model.columnSearchText },
                    set: { model.setColumnSearchText($0) }
                )
            )
            .textFieldStyle(.plain)
            .focused($searchFocused)
            .onSubmit {
                model.submitColumnSearch(path: column.path)
            }

            if model.columnSearchLoading {
                ProgressView()
                    .controlSize(.small)
            }

            Button {
                model.clearColumnSearch(path: column.path)
            } label: {
                Image(systemName: "xmark.circle.fill")
                    .font(.system(size: 11))
                    .foregroundStyle(.tertiary)
            }
            .buttonStyle(.plain)
            .help("关闭分栏搜索")
            .accessibilityLabel("关闭分栏搜索")
        }
        .padding(.horizontal, 8)
        .frame(height: 34)
        .background(Color(nsColor: .textBackgroundColor).opacity(0.28))
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(Color.primary.opacity(0.045))
                .frame(height: 0.5)
        }
        .onExitCommand {
            model.clearColumnSearch(path: column.path)
        }
    }

    @ViewBuilder
    private func columnRow(_ row: BrowserColumnRow) -> some View {
        switch row {
        case .group(let title):
            HStack {
                Text(title)
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(.secondary)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 8)
            .frame(height: 24)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.primary.opacity(0.045), in: RoundedRectangle(cornerRadius: 7))
        case .entry(let entry):
            KiteBrowserRow(
                model: model,
                entry: entry,
                columnIndex: index,
                isSelected: column.selectedPaths.contains(entry.path),
                showsDetails: showsDetails,
                showsDateColumn: showsDateColumn,
                showsKindColumn: showsKindColumn,
                showsSizeColumn: showsSizeColumn
            )
        }
    }

    private var columnRulesMenu: some View {
        Menu {
            Menu {
                ForEach(BrowserColumnSortKey.allCases) { key in
                    Button {
                        model.setColumnSortKey(index: index, key: key)
                    } label: {
                        ruleLabel(key.title, selected: column.sortKey == key)
                    }
                }
            } label: {
                Label("排序", systemImage: "arrow.up.arrow.down")
            }

            Menu {
                ForEach(BrowserColumnFilter.allCases) { filter in
                    Button {
                        model.setColumnFilter(index: index, filter: filter)
                    } label: {
                        ruleLabel(filter.title, selected: column.filter == filter)
                    }
                }
            } label: {
                Label("筛选", systemImage: "line.3.horizontal.decrease.circle")
            }

            Menu {
                ForEach(BrowserColumnGroup.allCases) { group in
                    Button {
                        model.setColumnGroup(index: index, group: group)
                    } label: {
                        ruleLabel(group.title, selected: column.group == group)
                    }
                }
            } label: {
                Label("分组", systemImage: "rectangle.3.group")
            }

            Menu {
                Toggle("显示修改日期列", isOn: columnVisibilityBinding($showsDateColumn))
                Toggle("显示类型列", isOn: columnVisibilityBinding($showsKindColumn))
                Toggle("显示大小列", isOn: columnVisibilityBinding($showsSizeColumn))

                Divider()

                Button {
                    preferredColumnWidth = 260
                } label: {
                    ruleLabel("紧凑宽度", selected: preferredColumnWidth == 260)
                }
                Button {
                    preferredColumnWidth = 416
                } label: {
                    ruleLabel("标准宽度", selected: preferredColumnWidth == 416)
                }
                Button {
                    preferredColumnWidth = 520
                } label: {
                    ruleLabel("宽列", selected: preferredColumnWidth == 520)
                }

                Divider()

                Button("重置列与布局") {
                    showsDateColumn = true
                    showsKindColumn = true
                    showsSizeColumn = true
                    preferredColumnWidth = 416
                    onShowDetails()
                }
            } label: {
                Label("列与布局", systemImage: "tablecells")
            }

            Divider()

            Button {
                model.resetColumnView(index: index)
            } label: {
                Label("重置分栏显示规则", systemImage: "arrow.counterclockwise")
            }
            .disabled(column.isDefaultView)
        } label: {
            Image(systemName: "slider.horizontal.3")
                .font(.system(size: 11, weight: .semibold))
                .frame(width: 22, height: 22)
                .contentShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .help("分栏显示规则")
        .accessibilityLabel("分栏显示规则")
    }

    private func ruleLabel(_ title: String, selected: Bool) -> some View {
        HStack {
            Text(title)
            Spacer()
            if selected {
                Image(systemName: "checkmark")
            }
        }
    }

    private func columnVisibilityBinding(_ binding: Binding<Bool>) -> Binding<Bool> {
        Binding(
            get: { binding.wrappedValue },
            set: { visible in
                binding.wrappedValue = visible
                if visible, !showsDetails {
                    onShowDetails()
                }
            }
        )
    }

    private var detailsHeader: some View {
        HStack(spacing: KiteColumnDetailsLayout.spacing) {
            Text("名称")
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.leading, 29)

            if showsDateColumn {
                Text("日期")
                    .frame(width: KiteColumnDetailsLayout.dateWidth, alignment: .leading)
            }

            if showsKindColumn {
                Text("类型")
                    .frame(width: KiteColumnDetailsLayout.kindWidth, alignment: .leading)
            }

            if showsSizeColumn {
                Text("大小")
                    .frame(width: KiteColumnDetailsLayout.sizeWidth, alignment: .trailing)
            }
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

                Image(
                    systemName: column.searchQuery != nil
                        ? "magnifyingglass"
                        : (column.filter == .all ? "wind" : "line.3.horizontal.decrease")
                )
                .font(.system(size: 22, weight: .light))
                .foregroundStyle(.tertiary)
            }

            Text(column.emptyLabel)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(.secondary)

            if column.searchQuery == nil && column.filter == .all {
                Button("新建文件夹…") {
                    model.createFolder(in: column.path)
                }
                .buttonStyle(.plain)
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(Color.accentColor)
            }
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
    let showsDateColumn: Bool
    let showsKindColumn: Bool
    let showsSizeColumn: Bool

    @State private var isHovering = false

    var body: some View {
        Button {
            model.select(
                column: columnIndex,
                entry: entry,
                modifiers: NSEvent.modifierFlags
            )
        } label: {
            HStack(spacing: showsDetails ? KiteColumnDetailsLayout.spacing : 9) {
                nameCell

                if showsDetails && showsDateColumn {
                    Text(metadataDate)
                        .font(.system(size: 10.5, design: .rounded))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .frame(width: KiteColumnDetailsLayout.dateWidth, alignment: .leading)

                }

                if showsDetails && showsKindColumn {
                    Text(metadataKind)
                        .font(.system(size: 10.5))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .frame(width: KiteColumnDetailsLayout.kindWidth, alignment: .leading)

                }

                if showsDetails && showsSizeColumn {
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
        return ByteCountFormatter.string(
            fromByteCount: Int64(clamping: entry.size),
            countStyle: .file
        )
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

struct KiteGlobalSearchResults: View {
    @ObservedObject var model: KiteModel
    @State private var selectedPath: String?

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(Color.accentColor)

                if let search = model.globalSearch {
                    Text("“\(search.query)”的搜索结果")
                        .font(.system(size: 12, weight: .semibold))
                    Text("\(search.entries.count) 项")
                        .font(.system(size: 9.5, weight: .semibold, design: .rounded))
                        .foregroundStyle(.tertiary)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Color.primary.opacity(0.055), in: Capsule())
                    if search.truncated {
                        Text("1000+")
                            .font(.system(size: 9.5, weight: .semibold, design: .rounded))
                            .foregroundStyle(.tertiary)
                    }
                } else {
                    Text("正在搜索…")
                        .font(.system(size: 12, weight: .semibold))
                    ProgressView()
                        .controlSize(.small)
                }

                Spacer(minLength: 8)

                Text(model.globalSearch?.rootPath ?? model.currentPath)
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .padding(.horizontal, 14)
            .frame(height: 40)

            Rectangle()
                .fill(Color.primary.opacity(0.06))
                .frame(height: 0.5)
                .padding(.horizontal, 10)

            if let search = model.globalSearch, search.entries.isEmpty {
                VStack(spacing: 10) {
                    Image(systemName: "doc.text.magnifyingglass")
                        .font(.system(size: 30, weight: .light))
                        .foregroundStyle(.tertiary)
                    Text("无搜索结果")
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let entries = model.globalSearch?.entries {
                ScrollView {
                    LazyVStack(spacing: 2) {
                        ForEach(entries) { entry in
                            searchRow(entry)
                        }
                    }
                    .padding(.horizontal, 6)
                    .padding(.vertical, 6)
                }
                .scrollIndicators(.hidden)
            } else {
                ProgressView()
                    .controlSize(.small)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
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
                .strokeBorder(Color.accentColor.opacity(0.2), lineWidth: 0.7)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .onChange(of: model.globalSearch?.query) { _ in
            selectedPath = nil
        }
    }

    private func searchRow(_ entry: BrowserEntry) -> some View {
        Button {
            selectedPath = entry.path
        } label: {
            HStack(spacing: 9) {
                KiteFileIcon(path: entry.path, isDirectory: entry.isDirectory, size: 20)
                    .frame(width: 22, height: 22)

                Text(entry.name)
                    .font(.system(
                        size: 12.5,
                        weight: selectedPath == entry.path ? .medium : .regular
                    ))
                    .lineLimit(1)
                    .truncationMode(.middle)

                Spacer(minLength: 8)

                Text(relativeLocation(for: entry))
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .frame(maxWidth: 320, alignment: .trailing)
            }
            .padding(.horizontal, 8)
            .frame(height: 32)
            .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
            .background {
                if selectedPath == entry.path {
                    RoundedRectangle(cornerRadius: 9, style: .continuous)
                        .kiteGlass(.selection, cornerRadius: 9)
                }
            }
        }
        .buttonStyle(.plain)
        .simultaneousGesture(
            TapGesture(count: 2).onEnded {
                if entry.isDirectory {
                    model.loadRoot(entry.path)
                } else {
                    KitePlatform.open(entry.path)
                }
            }
        )
        .contextMenu {
            Button("打开") {
                if entry.isDirectory {
                    model.loadRoot(entry.path)
                } else {
                    KitePlatform.open(entry.path)
                }
            }
            Button("在 Finder 中显示") {
                KitePlatform.revealInFinder(entry.path)
            }
            Button("拷贝路径") {
                KitePlatform.copyPath(entry.path)
            }
            Divider()
            Button("拷贝") {
                model.copyFile(entry.path)
            }
            Button("剪切") {
                model.cutFile(entry.path)
            }
            Button("拷贝到…") {
                model.copyTo(path: entry.path)
            }
            Button("分享…") {
                KitePlatform.share(entry.path)
            }
            Button(role: .destructive) {
                model.moveToTrash(entry)
            } label: {
                Label("移到废纸篓", systemImage: "trash")
            }
            .disabled(!KitePlatform.canModify(entry.path))
        }
        .help(entry.path)
        .accessibilityLabel("\(entry.name)，\(entry.isDirectory ? "文件夹" : entry.kindLabel)")
    }

    private func relativeLocation(for entry: BrowserEntry) -> String {
        let root = model.globalSearch?.rootPath ?? model.currentPath
        let rootURL = URL(fileURLWithPath: root, isDirectory: true).standardizedFileURL
        let parentURL = URL(fileURLWithPath: entry.path)
            .deletingLastPathComponent()
            .standardizedFileURL
        let parent = parentURL.path
        let rootPath = rootURL.path
        let rootPrefix = rootPath.hasSuffix("/") ? rootPath : rootPath + "/"
        guard parent == rootPath || parent.hasPrefix(rootPrefix) else {
            return parent
        }
        let suffix = String(parent.dropFirst(rootPath.count))
        if suffix.isEmpty {
            return "."
        }
        return suffix.hasPrefix("/") ? "." + suffix : "./" + suffix
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
            model.copyFiles(operationPaths)
        } label: {
            Label(operationEntries.count > 1 ? "拷贝所选项目" : "拷贝", systemImage: "doc.on.doc")
        }

        Button {
            model.cutFiles(operationPaths)
        } label: {
            Label(operationEntries.count > 1 ? "剪切所选项目" : "剪切", systemImage: "scissors")
        }

        Button {
            model.pasteFiles(into: pasteDirectory)
        } label: {
            Label("粘贴", systemImage: "doc.on.clipboard")
        }

        Button {
            model.copyTo(paths: operationPaths)
        } label: {
            Label(
                operationEntries.count > 1 ? "将所选项目拷贝到…" : "拷贝到…",
                systemImage: "folder.badge.plus"
            )
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
            KitePlatform.share(operationPaths)
        } label: {
            Label(operationEntries.count > 1 ? "分享所选项目…" : "分享…", systemImage: "square.and.arrow.up")
        }

        Divider()

        Button {
            model.rename(entry.path)
        } label: {
            Label("重命名…", systemImage: "pencil")
        }
        .disabled(!KitePlatform.canModify(entry.path) || operationEntries.count > 1)

        Button(role: .destructive) {
            model.moveToTrash(entries: operationEntries)
        } label: {
            Label(
                operationEntries.count > 1 ? "将所选项目移到废纸篓" : "移到废纸篓",
                systemImage: "trash"
            )
        }
        .disabled(operationEntries.contains { !KitePlatform.canModify($0.path) })
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

    private var operationEntries: [BrowserEntry] {
        let selected = model.selectedColumnEntries
        if selected.count > 1, selected.contains(where: { $0.path == entry.path }) {
            return selected
        }
        return [entry]
    }

    private var operationPaths: [String] {
        operationEntries.map(\.path)
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

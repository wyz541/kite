import AppKit
import SwiftUI

struct KiteSidebar: View {
    @ObservedObject var model: KiteModel

    private var locations: [KiteSidebarLocation] {
        let fileManager = FileManager.default
        let home = fileManager.homeDirectoryForCurrentUser.path

        return [
            KiteSidebarLocation(
                id: "home",
                title: "个人",
                path: home,
                systemImage: "house.fill"
            ),
            KiteSidebarLocation(
                id: "desktop",
                title: "桌面",
                path: fileManager.urls(for: .desktopDirectory, in: .userDomainMask).first?.path,
                systemImage: "rectangle.on.rectangle"
            ),
            KiteSidebarLocation(
                id: "documents",
                title: "文稿",
                path: fileManager.urls(for: .documentDirectory, in: .userDomainMask).first?.path,
                systemImage: "doc.text.fill"
            ),
            KiteSidebarLocation(
                id: "downloads",
                title: "下载",
                path: fileManager.urls(for: .downloadsDirectory, in: .userDomainMask).first?.path,
                systemImage: "arrow.down.circle.fill"
            ),
        ].filter { $0.path != nil }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    locationSection
                    favoriteSection
                    volumeSection
                }
                .padding(.bottom, 8)
            }
            .scrollIndicators(.hidden)

            Spacer(minLength: 0)

            Divider()
                .opacity(0.35)
                .padding(.horizontal, 4)

            settingsBar
        }
        .padding(.horizontal, 10)
        .padding(.top, 10)
        .padding(.bottom, 10)
        .frame(width: KiteTheme.sidebarWidth)
        .frame(maxHeight: .infinity)
        .kiteGlass(.panel, cornerRadius: KiteTheme.panelRadius)
        .kiteBackgroundExtension()
    }

    private var locationSection: some View {
        VStack(alignment: .leading, spacing: 4) {
            KiteSidebarSectionTitle("位置")

            ForEach(locations) { location in
                KiteSidebarRow(
                    title: location.title,
                    subtitle: nil,
                    systemImage: location.systemImage,
                    isSelected: isCurrent(location.path)
                ) {
                    if let path = location.path {
                        model.loadRoot(path)
                    }
                }
                .contextMenu {
                    if let path = location.path {
                        Button("打开") {
                            model.loadRoot(path)
                        }
                        Button("在 Finder 中显示") {
                            KitePlatform.revealInFinder(path)
                        }
                        Button("显示简介") {
                            KitePlatform.showInfo(path)
                        }
                    }
                }
            }
        }
    }

    @ViewBuilder
    private var favoriteSection: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                KiteSidebarSectionTitle("收藏")
                Spacer()
                Text("\(model.favorites.count)")
                    .font(.system(size: 10, weight: .semibold, design: .rounded))
                    .foregroundStyle(.tertiary)
                    .padding(.trailing, 6)
            }

            if model.favorites.isEmpty {
                Text("点击工具栏星标或右键菜单添加收藏。")
                    .font(.system(size: 11))
                    .foregroundStyle(.tertiary)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 6)
                    .fixedSize(horizontal: false, vertical: true)
            } else {
                ForEach(model.favorites, id: \.path) { favorite in
                    KiteSidebarRow(
                        title: favorite.name,
                        subtitle: nil,
                        systemImage: favorite.isDirectory ? "folder.fill" : "doc.fill",
                        isSelected: isCurrent(favorite.path)
                    ) {
                        model.openFavorite(favorite)
                    }
                    .contextMenu {
                        Button("打开") {
                            model.openFavorite(favorite)
                        }
                        Button("在 Finder 中显示") {
                            KitePlatform.revealInFinder(favorite.path)
                        }
                        Button("显示简介") {
                            KitePlatform.showInfo(favorite.path)
                        }
                        Button("拷贝路径") {
                            KitePlatform.copyPath(favorite.path)
                        }
                        Divider()
                        Button("从收藏移除") {
                            model.removeFavorite(favorite)
                        }
                    }
                }
            }
        }
    }

    private var settingsBar: some View {
        Menu {
            Button {
                model.checkForUpdates()
            } label: {
                Label("检查更新…", systemImage: "arrow.triangle.2.circlepath")
            }
            .disabled(model.updateInProgress)

            Divider()

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
            HStack(spacing: 9) {
                Image(systemName: "gearshape.fill")
                    .font(.system(size: 13, weight: .semibold))
                    .symbolRenderingMode(.hierarchical)
                    .frame(width: 18)

                Text("设置")
                    .font(.system(size: 12, weight: .medium))

                Spacer(minLength: 0)

                if let download = model.updateDownload,
                   let fraction = download.fraction {
                    ProgressView(value: fraction)
                        .progressViewStyle(.linear)
                        .frame(width: 54)
                } else if model.updateInProgress {
                    ProgressView()
                        .controlSize(.mini)
                }
            }
            .foregroundStyle(.secondary)
            .padding(.horizontal, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .frame(height: 34)
            .contentShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            .background {
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(Color.primary.opacity(0.045))
            }
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .help("Kite 设置")
        .accessibilityLabel("Kite 设置")
    }

    @ViewBuilder
    private var volumeSection: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                KiteSidebarSectionTitle("磁盘")
                Spacer()
                KiteToolbarIconButton(
                    systemName: "arrow.clockwise",
                    title: "重新扫描磁盘"
                ) {
                    model.reloadVolumes()
                }
                .scaleEffect(0.88)
                .padding(.trailing, 2)
            }

            if visibleVolumes.isEmpty {
                Text("未发现可用磁盘。")
                    .font(.system(size: 11))
                    .foregroundStyle(.tertiary)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 6)
            } else {
                ForEach(visibleVolumes, id: \.path) { volume in
                    KiteVolumeRow(volume: volume, isSelected: isCurrent(volume.path)) {
                        model.loadRoot(volume.path)
                    }
                }
            }
        }
    }

    private func isCurrent(_ path: String?) -> Bool {
        guard let path else { return false }
        return URL(fileURLWithPath: path).standardizedFileURL.path
            == URL(fileURLWithPath: model.currentPath).standardizedFileURL.path
    }

    private var visibleVolumes: [VolumeDto] {
        model.volumes.filter { volume in
            !volume.path.hasPrefix("/System/Volumes/")
        }
    }
}

private struct KiteSidebarLocation: Identifiable {
    let id: String
    let title: String
    let path: String?
    let systemImage: String
}

private struct KiteSidebarSectionTitle: View {
    let title: String

    init(_ title: String) {
        self.title = title
    }

    var body: some View {
        Text(title)
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(.tertiary)
            .textCase(.uppercase)
            .tracking(0.7)
            .padding(.horizontal, 8)
            .padding(.bottom, 2)
    }
}

private struct KiteSidebarRow: View {
    let title: String
    let subtitle: String?
    let systemImage: String
    let isSelected: Bool
    let action: () -> Void

    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 9) {
                Image(systemName: systemImage)
                    .font(.system(size: 13, weight: .medium))
                    .symbolRenderingMode(.hierarchical)
                    .foregroundStyle(isSelected ? Color.accentColor : Color.secondary)
                    .frame(width: 18)

                VStack(alignment: .leading, spacing: 1) {
                    Text(title)
                        .font(.system(size: 12.5, weight: isSelected ? .semibold : .regular))
                        .lineLimit(1)
                        .truncationMode(.middle)

                    if let subtitle {
                        Text(subtitle)
                            .font(.system(size: 10))
                            .foregroundStyle(.tertiary)
                            .lineLimit(1)
                    }
                }

                Spacer(minLength: 0)
            }
            .padding(.horizontal, 8)
            .frame(height: 30)
            .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
            .background {
                if isSelected {
                    RoundedRectangle(cornerRadius: 9, style: .continuous)
                        .kiteGlass(.selection, cornerRadius: 9)
                } else if isHovering {
                    RoundedRectangle(cornerRadius: 9, style: .continuous)
                        .fill(Color.primary.opacity(0.052))
                }
            }
        }
        .buttonStyle(.plain)
        .onHover { isHovering = $0 }
        .accessibilityLabel(title)
    }
}

private struct KiteVolumeRow: View {
    let volume: VolumeDto
    let isSelected: Bool
    let action: () -> Void

    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 5) {
                HStack(spacing: 9) {
                    Image(systemName: systemImage)
                        .font(.system(size: 14, weight: .medium))
                        .symbolRenderingMode(.hierarchical)
                        .foregroundStyle(isSelected ? Color.accentColor : Color.secondary)
                        .frame(width: 18)

                    VStack(alignment: .leading, spacing: 1) {
                        Text(title)
                            .font(.system(size: 12.5, weight: isSelected ? .semibold : .regular))
                            .lineLimit(1)
                            .truncationMode(.middle)

                        if let storageText {
                            Text(storageText)
                                .font(.system(size: 10))
                                .foregroundStyle(.tertiary)
                                .lineLimit(1)
                        }
                    }

                    Spacer(minLength: 0)
                }

                if let usedFraction {
                    ProgressView(value: usedFraction)
                        .progressViewStyle(.linear)
                        .tint(isSelected ? Color.accentColor : Color.secondary.opacity(0.65))
                        .controlSize(.mini)
                        .padding(.leading, 27)
                        .padding(.trailing, 2)
                }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .contentShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            .background {
                if isSelected {
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .kiteGlass(.selection, cornerRadius: 10)
                } else if isHovering {
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .fill(Color.primary.opacity(0.052))
                }
            }
        }
        .buttonStyle(.plain)
        .onHover { isHovering = $0 }
        .help(volume.path)
        .accessibilityLabel(title)
        .contextMenu {
            Button("打开") { action() }
            Button("在 Finder 中显示") { KitePlatform.revealInFinder(volume.path) }
            Button("显示简介") { KitePlatform.showInfo(volume.path) }
        }
    }

    private var title: String {
        if let name = volume.name, !name.isEmpty {
            return name
        }
        if let deviceName = volume.deviceName, !deviceName.isEmpty {
            return deviceName
        }
        let fallback = URL(fileURLWithPath: volume.path).lastPathComponent
        return fallback.isEmpty ? volume.path : fallback
    }

    private var storageText: String? {
        if let free = volume.freeBytes, let total = volume.totalBytes {
            let formatter = ByteCountFormatter()
            formatter.countStyle = .file
            let freeText = formatter.string(fromByteCount: Int64(free))
            let totalText = formatter.string(fromByteCount: Int64(total))
            return "\(freeText) 可用 / \(totalText)"
        }
        return volume.deviceName
    }

    private var usedFraction: Double? {
        guard let free = volume.freeBytes,
              let total = volume.totalBytes,
              total > 0 else {
            return nil
        }
        return Double(total - min(free, total)) / Double(total)
    }

    private var systemImage: String {
        switch volume.kind {
        case "usb", "removable":
            return "externaldrive.fill"
        case "network":
            return "network"
        case "optical":
            return "opticaldiscdrive.fill"
        default:
            return "internaldrive.fill"
        }
    }
}

import AppKit
import QuickLookUI
import SwiftUI

struct KitePreviewPane: View {
    @ObservedObject var model: KiteModel
    let entry: BrowserEntry
    let onClose: () -> Void

    var body: some View {
        VStack(spacing: 12) {
            header

            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    previewSurface
                    identity
                    actionGrid
                    metadata
                }
                .padding(.bottom, 4)
            }
            .scrollIndicators(.hidden)
        }
        .padding(12)
        .frame(width: KiteTheme.previewWidth)
        .frame(maxHeight: .infinity)
        .kiteGlass(.panel, cornerRadius: KiteTheme.panelRadius)
    }

    private var header: some View {
        HStack(spacing: 7) {
            Image(systemName: "rectangle.righthalf.inset.filled")
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.secondary)

            Text("预览")
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.secondary)

            Spacer()

            KiteToolbarIconButton(
                systemName: "doc.on.doc",
                title: "复制预览信息"
            ) {
                KitePlatform.copyText(previewInfoText)
            }

            KiteToolbarIconButton(
                systemName: "xmark",
                title: "关闭预览",
                action: onClose
            )
            .scaleEffect(0.9)
        }
        .padding(.leading, 2)
    }

    private var previewSurface: some View {
        KiteQuickLookPreview(url: URL(fileURLWithPath: entry.path))
            .frame(height: 224)
            .background(Color(nsColor: .controlBackgroundColor).opacity(0.55))
            .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .strokeBorder(Color.white.opacity(0.10), lineWidth: 0.5)
            }
    }

    private var identity: some View {
        HStack(alignment: .top, spacing: 10) {
            KiteFileIcon(path: entry.path, isDirectory: entry.isDirectory, size: 36)

            VStack(alignment: .leading, spacing: 3) {
                Text(entry.name)
                    .font(.system(size: 15, weight: .semibold))
                    .lineLimit(2)
                    .truncationMode(.middle)
                    .textSelection(.enabled)

                Text(entry.kindLabel)
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)

                if let git = entry.gitStatus {
                    KiteVcsLabel(text: git, systemImage: "arrow.triangle.branch")
                }
                if let svn = entry.svnStatus {
                    KiteVcsLabel(text: svn, systemImage: "arrow.triangle.branch")
                }
            }

            Spacer(minLength: 0)
        }
    }

    private var actionGrid: some View {
        LazyVGrid(
            columns: [GridItem(.flexible()), GridItem(.flexible())],
            spacing: 8
        ) {
            KitePreviewAction(title: "打开", systemImage: "arrow.up.forward.app", prominent: true) {
                KitePlatform.open(entry.path)
            }

            KitePreviewAction(title: "快速查看", systemImage: "eye") {
                KitePlatform.quickLook(entry.path)
            }

            if KitePlatform.isEditableTextFile(entry.path) {
                KitePreviewAction(title: "编辑", systemImage: "square.and.pencil") {
                    KitePlatform.openTextEditor(entry.path)
                }
            } else {
                KitePreviewAction(title: "分享", systemImage: "square.and.arrow.up") {
                    KitePlatform.share(entry.path)
                }
            }

            KitePreviewAction(title: "Finder", systemImage: "folder") {
                KitePlatform.revealInFinder(entry.path)
            }
        }
    }

    private var metadata: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("信息")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(.tertiary)
                    .textCase(.uppercase)
                    .tracking(0.7)

                Spacer()
            }
            .padding(.horizontal, 2)

            VStack(spacing: 0) {
                if !entry.isDirectory {
                    KiteMetadataRow(label: "大小", value: formattedSize(entry.size))
                    KiteMetadataDivider()
                }
                KiteMetadataRow(label: "修改日期", value: formattedDate(entry.modifiedUnixMs))
                KiteMetadataDivider()
                KiteMetadataRow(label: "位置", value: entry.path, selectable: true, wraps: true)

                if let git = entry.gitStatus {
                    KiteMetadataDivider()
                    KiteMetadataRow(label: "Git", value: git)
                }
                if let svn = entry.svnStatus {
                    KiteMetadataDivider()
                    KiteMetadataRow(label: "SVN", value: svn)
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 4)
            .background(
                Color(nsColor: .controlBackgroundColor).opacity(0.28),
                in: RoundedRectangle(cornerRadius: 12, style: .continuous)
            )
            .overlay {
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .strokeBorder(Color.primary.opacity(0.06), lineWidth: 0.5)
            }
        }
    }

    private var previewInfoText: String {
        var lines = [
            "名称：\(entry.name)",
            "类型：\(entry.kindLabel)",
            "修改日期：\(formattedDate(entry.modifiedUnixMs))",
            "位置：\(entry.path)",
        ]
        if !entry.isDirectory {
            lines.insert("大小：\(formattedSize(entry.size))", at: 2)
        }
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
}

private struct KitePreviewAction: View {
    let title: String
    let systemImage: String
    var prominent = false
    let action: () -> Void

    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            Label(title, systemImage: systemImage)
                .font(.system(size: 11.5, weight: .medium))
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 10)
                .frame(height: 32)
                .contentShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        }
        .buttonStyle(.plain)
        .foregroundStyle(prominent ? Color.white : Color.primary)
        .background {
            if prominent {
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .kiteGlass(.prominent, cornerRadius: 10)
            } else {
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(Color.primary.opacity(isHovering ? 0.075 : 0.045))
            }
        }
        .onHover { isHovering = $0 }
        .help(title)
        .accessibilityLabel(title)
    }
}

private struct KiteVcsLabel: View {
    let text: String
    let systemImage: String

    var body: some View {
        Label(text, systemImage: systemImage)
            .font(.system(size: 9.5, weight: .medium))
            .foregroundStyle(.secondary)
            .lineLimit(1)
    }
}

private struct KiteMetadataRow: View {
    let label: String
    let value: String
    var selectable = false
    var wraps = false

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Text(label)
                .font(.system(size: 10.5))
                .foregroundStyle(.tertiary)
                .frame(width: 52, alignment: .leading)

            Group {
                if selectable {
                    Text(value).textSelection(.enabled)
                } else {
                    Text(value)
                }
            }
            .font(.system(size: 11.5))
            .foregroundStyle(.secondary)
            .lineLimit(wraps ? nil : 2)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.vertical, 7)
    }
}

private struct KiteMetadataDivider: View {
    var body: some View {
        Rectangle()
            .fill(Color.primary.opacity(0.055))
            .frame(height: 0.5)
            .padding(.leading, 62)
    }
}

struct KiteQuickLookPreview: NSViewRepresentable {
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

import AppKit
import SwiftUI

enum KiteGlassLevel {
    case panel
    case control
    case selection
    case clear
    case prominent

    @available(macOS 26.0, *)
    var glass: Glass {
        switch self {
        case .panel:
            return .regular
        case .control:
            return .regular.interactive()
        case .selection:
            return .regular
                .tint(Color.accentColor.opacity(0.28))
                .interactive()
        case .clear:
            return .clear.interactive()
        case .prominent:
            return .regular
                .tint(Color.accentColor.opacity(0.70))
                .interactive()
        }
    }

    var fallbackMaterial: Material {
        switch self {
        case .panel, .selection:
            return .regularMaterial
        case .control, .clear:
            return .thinMaterial
        case .prominent:
            return .thickMaterial
        }
    }

    var fallbackTint: Color {
        switch self {
        case .panel:
            return Color(nsColor: .windowBackgroundColor).opacity(0.24)
        case .control, .clear:
            return Color(nsColor: .controlBackgroundColor).opacity(0.16)
        case .selection:
            return Color.accentColor.opacity(0.22)
        case .prominent:
            return Color.accentColor.opacity(0.68)
        }
    }

    var fallbackShadowOpacity: Double {
        switch self {
        case .panel:
            return 0.10
        case .control, .clear:
            return 0.06
        case .selection, .prominent:
            return 0.08
        }
    }
}

struct KiteGlassEffectContainer<Content: View>: View {
    let spacing: CGFloat
    private let content: Content

    init(spacing: CGFloat = 10, @ViewBuilder content: () -> Content) {
        self.spacing = spacing
        self.content = content()
    }

    @ViewBuilder
    var body: some View {
        if #available(macOS 26.0, *) {
            GlassEffectContainer(spacing: spacing) {
                content
            }
        } else {
            content
        }
    }
}

private struct KiteGlassSurfaceModifier: ViewModifier {
    let level: KiteGlassLevel
    let cornerRadius: CGFloat

    @ViewBuilder
    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
        if #available(macOS 26.0, *) {
            content.glassEffect(level.glass, in: shape)
        } else {
            content
                .background(level.fallbackMaterial, in: shape)
                .background(level.fallbackTint, in: shape)
                .overlay {
                    shape.strokeBorder(
                        Color(nsColor: .separatorColor).opacity(0.48),
                        lineWidth: 0.5
                    )
                }
                .shadow(
                    color: .black.opacity(level.fallbackShadowOpacity),
                    radius: 14,
                    y: 4
                )
        }
    }
}

private struct KiteGlassGroupModifier: ViewModifier {
    let cornerRadius: CGFloat

    func body(content: Content) -> some View {
        content.kiteGlass(.control, cornerRadius: cornerRadius)
    }
}

private struct KiteBackgroundExtensionModifier: ViewModifier {
    @ViewBuilder
    func body(content: Content) -> some View {
        if #available(macOS 26.0, *) {
            content.backgroundExtensionEffect()
        } else {
            content
        }
    }
}

extension View {
    func kiteGlass(
        _ level: KiteGlassLevel = .panel,
        cornerRadius: CGFloat = 18
    ) -> some View {
        modifier(KiteGlassSurfaceModifier(level: level, cornerRadius: cornerRadius))
    }

    func kiteGlassGroup(cornerRadius: CGFloat = 15) -> some View {
        modifier(KiteGlassGroupModifier(cornerRadius: cornerRadius))
    }

    func kiteBackgroundExtension() -> some View {
        modifier(KiteBackgroundExtensionModifier())
    }
}

struct KiteToolbarIconButton: View {
    let systemName: String
    let title: String
    var isEnabled = true
    var isActive = false
    var prominent = false
    let action: () -> Void

    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            Image(systemName: systemName)
                .font(.system(size: 13, weight: .semibold))
                .symbolRenderingMode(.hierarchical)
                .frame(width: 30, height: 30)
                .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
        }
        .buttonStyle(.plain)
        .foregroundStyle(isEnabled ? AnyShapeStyle(.primary) : AnyShapeStyle(.tertiary))
        .background {
            if prominent {
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .fill(Color.accentColor.opacity(0.16))
            } else if isHovering || isActive {
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .fill(Color.primary.opacity(isActive ? 0.10 : 0.06))
            }
        }
        .disabled(!isEnabled)
        .onHover { isHovering = $0 }
        .help(title)
        .accessibilityLabel(title)
    }
}

struct KiteFileIcon: View {
    let path: String
    let isDirectory: Bool
    var size: CGFloat = 19

    var body: some View {
        Image(nsImage: NSWorkspace.shared.icon(forFile: path))
            .resizable()
            .interpolation(.high)
            .aspectRatio(contentMode: .fit)
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }
}

private final class KiteWindowConfigurationView: NSView {
    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard let window else { return }

        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.titlebarSeparatorStyle = .none
        window.styleMask.insert(.fullSizeContentView)
        window.isOpaque = false
        window.backgroundColor = .clear
        window.isMovableByWindowBackground = true
        window.minSize = NSSize(width: 980, height: 640)
    }
}

struct KiteWindowConfigurator: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        KiteWindowConfigurationView(frame: .zero)
    }

    func updateNSView(_ nsView: NSView, context: Context) {}
}

struct KiteGlassBackdrop: View {
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        GeometryReader { proxy in
            ZStack {
                Color(nsColor: .windowBackgroundColor)

                LinearGradient(
                    stops: [
                        .init(color: KiteTheme.brandOrange.opacity(colorScheme == .dark ? 0.18 : 0.13), location: 0),
                        .init(color: Color.clear, location: 0.38),
                        .init(color: KiteTheme.brandNavy.opacity(colorScheme == .dark ? 0.28 : 0.12), location: 1),
                    ],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )

                Circle()
                    .fill(KiteTheme.brandOrange.opacity(colorScheme == .dark ? 0.22 : 0.16))
                    .frame(width: min(proxy.size.width, 680), height: min(proxy.size.width, 680))
                    .blur(radius: 120)
                    .offset(x: -proxy.size.width * 0.38, y: -proxy.size.height * 0.42)

                Circle()
                    .fill(KiteTheme.brandNavy.opacity(colorScheme == .dark ? 0.38 : 0.13))
                    .frame(width: min(proxy.size.width, 780), height: min(proxy.size.width, 780))
                    .blur(radius: 150)
                    .offset(x: proxy.size.width * 0.38, y: proxy.size.height * 0.34)

                KiteWindTrail()
                    .stroke(
                        LinearGradient(
                            colors: [
                                Color.clear,
                                KiteTheme.brandOrange.opacity(colorScheme == .dark ? 0.40 : 0.30),
                                Color.white.opacity(colorScheme == .dark ? 0.16 : 0.42),
                                KiteTheme.brandNavy.opacity(colorScheme == .dark ? 0.34 : 0.18),
                                Color.clear,
                            ],
                            startPoint: .leading,
                            endPoint: .trailing
                        ),
                        style: StrokeStyle(lineWidth: 1.1, lineCap: .round)
                    )
                    .blur(radius: 0.3)
                    .padding(.horizontal, -40)
                    .offset(y: -proxy.size.height * 0.12)

                KiteWindTrail()
                    .stroke(
                        KiteTheme.brandOrange.opacity(colorScheme == .dark ? 0.20 : 0.15),
                        style: StrokeStyle(lineWidth: 0.7, lineCap: .round)
                    )
                    .offset(y: proxy.size.height * 0.16)
                    .blur(radius: 0.7)
            }
        }
        .ignoresSafeArea()
    }
}

private struct KiteWindTrail: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.move(to: CGPoint(x: -rect.width * 0.06, y: rect.height * 0.58))
        path.addCurve(
            to: CGPoint(x: rect.width * 0.42, y: rect.height * 0.30),
            control1: CGPoint(x: rect.width * 0.12, y: rect.height * 0.20),
            control2: CGPoint(x: rect.width * 0.30, y: rect.height * 0.58)
        )
        path.addCurve(
            to: CGPoint(x: rect.width * 1.06, y: rect.height * 0.18),
            control1: CGPoint(x: rect.width * 0.58, y: rect.height * 0.02),
            control2: CGPoint(x: rect.width * 0.76, y: rect.height * 0.43)
        )
        return path
    }
}

enum KiteTheme {
    static let brandOrange = Color(red: 0.94, green: 0.36, blue: 0.17)
    static let brandNavy = Color(red: 0.05, green: 0.19, blue: 0.34)
    static let sidebarWidth: CGFloat = 218
    static let columnWidth: CGFloat = 274
    static let previewWidth: CGFloat = 316
    static let panelRadius: CGFloat = 24
    static let controlRadius: CGFloat = 14
}

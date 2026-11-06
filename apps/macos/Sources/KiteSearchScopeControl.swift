import AppKit
import SwiftUI

final class KiteSearchScopePopUpButton: NSPopUpButton {
    override var intrinsicContentSize: NSSize {
        NSSize(width: 190, height: 22)
    }
}

final class KiteSearchScopeContainer: NSView {
    let popup: KiteSearchScopePopUpButton

    override var intrinsicContentSize: NSSize {
        NSSize(width: 190, height: 22)
    }

    init(popup: KiteSearchScopePopUpButton) {
        self.popup = popup
        super.init(frame: NSRect(x: 0, y: 0, width: 190, height: 22))
        popup.autoresizingMask = [.width, .height]
        popup.frame = bounds
        addSubview(popup)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override func layout() {
        super.layout()
        popup.frame = bounds
    }
}

@MainActor
struct KiteSearchScopeControl: NSViewRepresentable {
    @ObservedObject var model: KiteModel

    func makeCoordinator() -> Coordinator {
        Coordinator(model: model)
    }

    func makeNSView(context: Context) -> KiteSearchScopeContainer {
        let popup = KiteSearchScopePopUpButton(frame: .zero, pullsDown: false)
        popup.controlSize = .small
        popup.bezelStyle = .inline
        popup.isBordered = false
        popup.font = .systemFont(ofSize: 10.5, weight: .medium)
        popup.focusRingType = .exterior
        popup.cell?.lineBreakMode = .byTruncatingTail
        popup.setContentHuggingPriority(.required, for: .horizontal)
        popup.setContentCompressionResistancePriority(.required, for: .horizontal)
        popup.menu?.autoenablesItems = false
        popup.menu?.title = "搜索范围"
        popup.target = context.coordinator
        popup.action = #selector(Coordinator.selectionChanged(_:))
        popup.setAccessibilityLabel("搜索范围")
        popup.setAccessibilityHelp("选择全局搜索范围")
        context.coordinator.update(popup, model: model)
        return KiteSearchScopeContainer(popup: popup)
    }

    func updateNSView(_ container: KiteSearchScopeContainer, context: Context) {
        context.coordinator.update(container.popup, model: model)
    }

    @MainActor
    final class Coordinator: NSObject {
        private var model: KiteModel
        private var signature = ""

        init(model: KiteModel) {
            self.model = model
        }

        func update(_ popup: NSPopUpButton, model: KiteModel) {
            self.model = model
            let scopes = GlobalSearchScope.allCases
            let nextSignature = scopes.map { scope in
                let enabled = model.globalSearchScopeHasRoots(scope)
                return "\(scope.rawValue):\(scope == model.globalSearchScope):\(enabled):\(model.globalSearchScopeMenuTitle(scope))"
            }.joined(separator: "|")
            guard nextSignature != signature else { return }
            signature = nextSignature

            popup.menu?.removeAllItems()
            popup.menu?.autoenablesItems = false
            for (index, scope) in scopes.enumerated() {
                let item = NSMenuItem(
                    title: model.globalSearchScopeMenuTitle(scope),
                    action: nil,
                    keyEquivalent: ""
                )
                item.tag = index
                item.representedObject = scope.rawValue
                item.state = scope == model.globalSearchScope ? .on : .off
                item.isEnabled = model.globalSearchScopeHasRoots(scope)
                item.toolTip = model.globalSearchScopeDisabledReason(scope)
                    ?? model.globalSearchScopeDetail(scope)
                if let image = NSImage(
                    systemSymbolName: scope.systemImage,
                    accessibilityDescription: scope.title
                ) {
                    image.size = NSSize(width: 14, height: 14)
                    item.image = image
                }
                popup.menu?.addItem(item)
            }

            if let index = scopes.firstIndex(of: model.globalSearchScope) {
                popup.selectItem(withTag: index)
            }
            popup.title = model.currentSearchScopeSummary
            popup.toolTip = "当前搜索范围：\(model.currentSearchScopeSummary)"
            popup.setAccessibilityValue(model.currentSearchScopeSummary)
        }

        @objc
        func selectionChanged(_ sender: NSPopUpButton) {
            let scopes = GlobalSearchScope.allCases
            let index = sender.selectedTag()
            guard scopes.indices.contains(index) else { return }
            let scope = scopes[index]
            guard model.globalSearchScopeHasRoots(scope) else { return }
            model.globalSearchScope = scope
        }
    }
}

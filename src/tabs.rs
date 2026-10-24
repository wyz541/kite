use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::actions::{CloseTab, NewTab, NextTab, PreviousTab};
use crate::app::{KiteApp, SharedClipboard, TabRequest};
use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

struct TabEntry {
    view: Entity<KiteApp>,
    _subscription: Subscription,
    _tab_subscription: Subscription,
}

pub struct KiteWindow {
    tabs: Vec<TabEntry>,
    active_index: usize,
    clipboard: SharedClipboard,
    tab_scroll: ScrollHandle,
    focus_handle: FocusHandle,
}

impl KiteWindow {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let clipboard = Rc::new(RefCell::new(None));
        let initial_path = KiteApp::default_directory();
        let view = Self::create_tab(initial_path, clipboard.clone(), window, cx);
        let mut this = Self {
            tabs: Vec::new(),
            active_index: 0,
            clipboard,
            tab_scroll: ScrollHandle::new(),
            focus_handle: cx.focus_handle(),
        };
        let subscription = this.subscribe_tab(&view, window, cx);
        let tab_subscription = this.subscribe_tab_requests(&view, window, cx);
        this.tabs.push(TabEntry {
            view,
            _subscription: subscription,
            _tab_subscription: tab_subscription,
        });
        this.update_window_title(window, cx);
        cx.defer_in(window, |this, window, cx| {
            this.focus_active(window, cx);
        });
        this
    }

    fn create_tab(
        path: PathBuf,
        clipboard: SharedClipboard,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<KiteApp> {
        cx.new(|cx| KiteApp::new_at(path, clipboard, window, cx))
    }

    fn subscribe_tab(
        &mut self,
        view: &Entity<KiteApp>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.observe_in(view, window, |this, observed, window, cx| {
            if this.active_view_id() == Some(observed.entity_id()) {
                this.update_window_title(window, cx);
            }
            cx.notify();
        })
    }

    fn subscribe_tab_requests(
        &mut self,
        view: &Entity<KiteApp>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.subscribe_in(
            view,
            window,
            |this, source, request: &TabRequest, window, cx| {
                if this.active_view_id() != Some(source.entity_id()) {
                    return;
                }
                match request {
                    TabRequest::New => this.on_new_tab(&NewTab, window, cx),
                    TabRequest::Close => this.on_close_tab(&CloseTab, window, cx),
                    TabRequest::Next => this.on_next_tab(&NextTab, window, cx),
                    TabRequest::Previous => this.on_previous_tab(&PreviousTab, window, cx),
                }
            },
        )
    }

    fn active_view_id(&self) -> Option<EntityId> {
        self.tabs
            .get(self.active_index)
            .map(|entry| entry.view.entity_id())
    }

    fn active_view(&self) -> Option<Entity<KiteApp>> {
        self.tabs
            .get(self.active_index)
            .map(|entry| entry.view.clone())
    }

    fn update_window_title(&self, window: &mut Window, cx: &App) {
        let Some(entry) = self.tabs.get(self.active_index) else {
            return;
        };
        let path = entry.view.read(cx).current_dir().display().to_string();
        window.set_window_title(&format!("Kite — {path}"));
    }

    fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.tabs.get(self.active_index) else {
            return;
        };
        let handle = entry.view.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    }

    fn sync_active_clipboard(&self, cx: &mut Context<Self>) {
        let Some(entry) = self.tabs.get(self.active_index) else {
            return;
        };
        entry
            .view
            .update(cx, |view, cx| view.sync_clipboard_state(cx));
    }

    fn activate_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }
        self.active_index = index;
        self.tab_scroll.scroll_to_item(index);
        self.sync_active_clipboard(cx);
        self.update_window_title(window, cx);
        self.focus_active(window, cx);
        cx.notify();
    }

    fn open_tab(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let view = Self::create_tab(path, self.clipboard.clone(), window, cx);
        let subscription = self.subscribe_tab(&view, window, cx);
        let tab_subscription = self.subscribe_tab_requests(&view, window, cx);
        self.tabs.push(TabEntry {
            view,
            _subscription: subscription,
            _tab_subscription: tab_subscription,
        });
        self.activate_tab(self.tabs.len() - 1, window, cx);
    }

    fn close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(next_active) = active_after_close(self.active_index, index, self.tabs.len())
        else {
            return;
        };
        self.tabs.remove(index);
        self.active_index = next_active;
        self.sync_active_clipboard(cx);
        self.update_window_title(window, cx);
        self.focus_active(window, cx);
        cx.notify();
    }

    fn on_new_tab(&mut self, _: &NewTab, window: &mut Window, cx: &mut Context<Self>) {
        let path = self
            .tabs
            .get(self.active_index)
            .map(|entry| entry.view.read(cx).current_dir().to_path_buf())
            .unwrap_or_else(KiteApp::default_directory);
        self.open_tab(path, window, cx);
    }

    fn on_close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
        self.close_tab(self.active_index, window, cx);
    }

    fn on_next_tab(&mut self, _: &NextTab, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = next_tab_index(self.active_index, self.tabs.len()) else {
            return;
        };
        self.activate_tab(index, window, cx);
    }

    fn on_previous_tab(&mut self, _: &PreviousTab, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = previous_tab_index(self.active_index, self.tabs.len()) else {
            return;
        };
        self.activate_tab(index, window, cx);
    }

    fn new_tab_clicked(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.on_new_tab(&NewTab, window, cx);
    }

    fn render_tab_bar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let tab_count = self.tabs.len();
        let active_index = self.active_index;
        let hover_color = cx.theme().muted_foreground.opacity(0.12);
        let active_background = cx.theme().background;
        let inactive_foreground = cx.theme().muted_foreground;
        let active_foreground = cx.theme().foreground;
        let primary = cx.theme().primary;

        let tabs = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let path = entry.view.read(cx).current_dir().to_path_buf();
                let label: SharedString = tab_label(&path).into();
                let active = index == active_index;
                let can_close = tab_count > 1;
                let select = h_flex()
                    .id(format!("kite-tab-select-{index}"))
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .gap_2()
                    .items_center()
                    .pl_3()
                    .pr(if can_close { px(4.) } else { px(12.) })
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover_color))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.activate_tab(index, window, cx);
                    }))
                    .child(
                        Icon::new(IconName::Folder)
                            .size_3p5()
                            .text_color(if active { primary } else { inactive_foreground }),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .text_color(if active {
                                active_foreground
                            } else {
                                inactive_foreground
                            })
                            .child(label),
                    );

                let close = div()
                    .id(format!("kite-tab-close-{index}"))
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(22.))
                    .h(px(22.))
                    .mr_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover_color))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.close_tab(index, window, cx);
                    }))
                    .child(Icon::new(IconName::X).size_3());

                h_flex()
                    .id(format!("kite-tab-{index}"))
                    .relative()
                    .w(px(188.))
                    .h(px(30.))
                    .flex_shrink_0()
                    .items_center()
                    .rounded_md()
                    .bg(if active {
                        active_background
                    } else {
                        cx.theme().title_bar
                    })
                    .border_1()
                    .border_color(if active {
                        cx.theme().border
                    } else {
                        cx.theme().title_bar
                    })
                    .child(select)
                    .when(can_close, |this| this.child(close))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        h_flex()
            .h(px(38.))
            .w_full()
            .flex_shrink_0()
            .px_2()
            .gap_2()
            .items_center()
            .bg(cx.theme().title_bar)
            .border_b_1()
            .border_color(cx.theme().title_bar_border)
            .child(
                h_flex()
                    .id("kite-tab-scroll")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .gap_1()
                    .items_center()
                    .overflow_x_scroll()
                    .track_scroll(&self.tab_scroll)
                    .children(tabs),
            )
            .child(
                div()
                    .id("kite-new-tab")
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(28.))
                    .rounded_md()
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover_color))
                    .on_click(cx.listener(Self::new_tab_clicked))
                    .child(Icon::new(IconName::Plus).size_4()),
            )
            .into_any_element()
    }
}

fn tab_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.display().to_string())
}

fn next_tab_index(active: usize, len: usize) -> Option<usize> {
    (len > 1).then(|| (active + 1) % len)
}

fn previous_tab_index(active: usize, len: usize) -> Option<usize> {
    (len > 1).then(|| (active + len - 1) % len)
}

fn active_after_close(active: usize, closing: usize, len: usize) -> Option<usize> {
    if len <= 1 || closing >= len {
        return None;
    }
    let new_len = len - 1;
    let next = if closing < active { active - 1 } else { active };
    Some(next.min(new_len - 1))
}

impl Focusable for KiteWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for KiteWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab_bar = self.render_tab_bar(cx);
        let active_view = self.active_view();

        v_flex()
            .id("kite-window-root")
            .key_context("Kite")
            .track_focus(&self.focus_handle)
            .relative()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(Self::on_new_tab))
            .on_action(cx.listener(Self::on_close_tab))
            .on_action(cx.listener(Self::on_next_tab))
            .on_action(cx.listener(Self::on_previous_tab))
            .child(tab_bar)
            .when_some(active_view, |this, view| {
                this.child(div().flex_1().min_h_0().w_full().child(view))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{active_after_close, next_tab_index, previous_tab_index};

    #[test]
    fn tab_switching_wraps_in_both_directions() {
        assert_eq!(next_tab_index(0, 3), Some(1));
        assert_eq!(next_tab_index(2, 3), Some(0));
        assert_eq!(previous_tab_index(0, 3), Some(2));
        assert_eq!(previous_tab_index(2, 3), Some(1));
        assert_eq!(next_tab_index(0, 1), None);
        assert_eq!(previous_tab_index(0, 0), None);
    }

    #[test]
    fn closing_tabs_keeps_a_valid_active_index() {
        assert_eq!(active_after_close(1, 0, 3), Some(0));
        assert_eq!(active_after_close(1, 1, 3), Some(1));
        assert_eq!(active_after_close(1, 2, 3), Some(1));
        assert_eq!(active_after_close(2, 2, 3), Some(1));
        assert_eq!(active_after_close(0, 0, 1), None);
        assert_eq!(active_after_close(0, 4, 3), None);
    }
}

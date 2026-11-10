use std::path::PathBuf;

use crate::app::KiteApp;
use crate::ui_preferences::{self, AppearancePreference, UiPreferences};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, StyledExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SettingsPage {
    #[default]
    General,
    Appearance,
    Layout,
    FileHandling,
    Updates,
    About,
}

impl SettingsPage {
    const ALL: [Self; 6] = [
        Self::General,
        Self::Appearance,
        Self::Layout,
        Self::FileHandling,
        Self::Updates,
        Self::About,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::General => "通用",
            Self::Appearance => "外观",
            Self::Layout => "布局",
            Self::FileHandling => "文件处理",
            Self::Updates => "更新",
            Self::About => "关于",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Self::General => "启动和窗口恢复",
            Self::Appearance => "主题和界面外观",
            Self::Layout => "侧边栏、预览和详细信息",
            Self::FileHandling => "默认资源管理器",
            Self::Updates => "版本和更新记录",
            Self::About => "Kite 版本信息",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Self::General => IconName::Settings,
            Self::Appearance => IconName::Palette,
            Self::Layout => IconName::LayoutPanelLeft,
            Self::FileHandling => IconName::Folder,
            Self::Updates => IconName::RefreshCw,
            Self::About => IconName::Info,
        }
    }

    fn tint(self) -> Hsla {
        match self {
            Self::General => rgb(0x8e9aab).into(),
            Self::Appearance => rgb(0xf47a32).into(),
            Self::Layout => rgb(0x5e9cff).into(),
            Self::FileHandling => rgb(0x45b8d8).into(),
            Self::Updates => rgb(0x55b978).into(),
            Self::About => rgb(0xa675df).into(),
        }
    }
}

pub(crate) struct SettingsWindow {
    main_app: Entity<KiteApp>,
    page: SettingsPage,
    preferences: UiPreferences,
    update_in_progress: bool,
    status: Option<SharedString>,
    _main_app_subscription: Subscription,
}

impl SettingsWindow {
    pub(crate) fn new(
        main_app: Entity<KiteApp>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let preferences = ui_preferences::load_ui_preferences();
        let (update_in_progress, status) = main_app.read(cx).settings_update_status();
        let subscription = cx.observe_in(&main_app, window, |this, main_app, _, cx| {
            let (update_in_progress, status) = main_app.read(cx).settings_update_status();
            this.update_in_progress = update_in_progress;
            this.status = status;
            cx.notify();
        });
        Self {
            main_app,
            page: SettingsPage::General,
            preferences,
            update_in_progress,
            status,
            _main_app_subscription: subscription,
        }
    }

    fn save_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match ui_preferences::save_ui_preferences(self.preferences.clone()) {
            Ok(()) => self.status = Some("设置已保存".into()),
            Err(error) => self.status = Some(error.into()),
        }
        self.main_app.update(cx, |app, cx| {
            app.apply_ui_preferences(self.preferences.clone(), cx);
        });
        let mode = match self.preferences.appearance {
            AppearancePreference::System => ThemeMode::from(window.appearance()),
            AppearancePreference::Light => ThemeMode::Light,
            AppearancePreference::Dark => ThemeMode::Dark,
        };
        Theme::change(mode, Some(window), cx);
        cx.notify();
    }

    fn render_page(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match self.page {
            SettingsPage::General => self.render_general(cx),
            SettingsPage::Appearance => self.render_appearance(cx),
            SettingsPage::Layout => self.render_layout(cx),
            SettingsPage::FileHandling => self.render_file_handling(window, cx),
            SettingsPage::Updates => self.render_updates(window, cx),
            SettingsPage::About => self.render_about(cx),
        }
    }

    fn render_sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = v_flex().gap_1();
        for page in SettingsPage::ALL {
            let selected = self.page == page;
            let tint = page.tint();
            rows = rows.child(
                Button::new(SharedString::from(format!(
                    "settings-page-{}",
                    page.title()
                )))
                .ghost()
                .compact()
                .selected(selected)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.page = page;
                    cx.notify();
                }))
                .child(
                    h_flex()
                        .h(px(38.))
                        .w_full()
                        .gap_2p5()
                        .items_center()
                        .px_2()
                        .child(
                            div()
                                .size(px(25.))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(6.))
                                .bg(tint.opacity(if selected { 0.24 } else { 0.13 }))
                                .child(Icon::new(page.icon()).size_4().text_color(tint)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_sm()
                                .font_medium()
                                .child(page.title()),
                        ),
                ),
            );
        }

        v_flex()
            .w(px(220.))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar.opacity(0.76))
            .child(
                h_flex()
                    .w_full()
                    .gap_3()
                    .items_center()
                    .px_5()
                    .pt_5()
                    .pb_4()
                    .child(
                        div()
                            .size(px(38.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(10.))
                            .bg(cx.theme().primary.opacity(0.16))
                            .child(
                                Icon::new(IconName::Wind)
                                    .size_6()
                                    .text_color(cx.theme().primary),
                            ),
                    )
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(div().text_sm().font_semibold().child("Kite"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("设置"),
                            ),
                    ),
            )
            .child(div().px_2p5().child(rows))
            .child(div().flex_1())
            .child(
                div()
                    .px_5()
                    .pb_4()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Kite {}", env!("CARGO_PKG_VERSION"))),
            )
            .into_any_element()
    }

    fn render_header(&self, cx: &App) -> AnyElement {
        v_flex()
            .gap_1()
            .child(div().text_2xl().font_semibold().child(self.page.title()))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.page.subtitle()),
            )
            .into_any_element()
    }

    fn render_general(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.section(
            "启动",
            vec![
                self.row(
                    "general-restore-session",
                    "恢复上次会话",
                    Some("重新打开 Kite 时恢复页签和目录。"),
                    Switch::new("restore-session")
                        .checked(self.preferences.restore_session)
                        .small()
                        .accessibility_label("恢复上次会话")
                        .on_click(cx.listener(|this, checked, window, cx| {
                            this.preferences.restore_session = *checked;
                            this.save_preferences(window, cx);
                        }))
                        .into_any_element(),
                    cx,
                ),
            ],
            Some("关闭此选项后，Kite 下次启动时打开默认文件夹。"),
            cx,
        )
    }

    fn render_appearance(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let buttons = h_flex()
            .gap_1()
            .p_1()
            .rounded_md()
            .bg(cx.theme().muted.opacity(0.22))
            .child(self.appearance_button(
                "appearance-system",
                "跟随系统",
                AppearancePreference::System,
                cx,
            ))
            .child(self.appearance_button(
                "appearance-light",
                "浅色",
                AppearancePreference::Light,
                cx,
            ))
            .child(self.appearance_button(
                "appearance-dark",
                "深色",
                AppearancePreference::Dark,
                cx,
            ))
            .into_any_element();

        self.section(
            "主题",
            vec![self.row(
                "appearance-theme",
                "外观",
                Some("选择 Kite 窗口使用的明暗外观。"),
                buttons,
                cx,
            )],
            Some("Kite 的所有窗口会立即应用相同外观。"),
            cx,
        )
    }

    fn appearance_button(
        &self,
        id: &'static str,
        label: &'static str,
        preference: AppearancePreference,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        Button::new(id)
            .ghost()
            .compact()
            .label(label)
            .selected(self.preferences.appearance == preference)
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.preferences.appearance = preference;
                this.save_preferences(window, cx);
            }))
            .into_any_element()
    }

    fn render_layout(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.section(
            "窗口组件",
            vec![
                self.row(
                    "layout-sidebar",
                    "显示侧边栏",
                    Some("显示快速访问、收藏和磁盘。"),
                    Switch::new("sidebar-visible")
                        .checked(self.preferences.sidebar_visible)
                        .small()
                        .accessibility_label("显示侧边栏")
                        .on_click(cx.listener(|this, checked, window, cx| {
                            this.preferences.sidebar_visible = *checked;
                            this.save_preferences(window, cx);
                        }))
                        .into_any_element(),
                    cx,
                ),
                self.separator(cx),
                self.row(
                    "layout-preview",
                    "显示预览",
                    Some("显示所选文件的内容和属性。"),
                    Switch::new("preview-visible")
                        .checked(self.preferences.preview_visible)
                        .small()
                        .accessibility_label("显示预览")
                        .on_click(cx.listener(|this, checked, window, cx| {
                            this.preferences.preview_visible = *checked;
                            this.save_preferences(window, cx);
                        }))
                        .into_any_element(),
                    cx,
                ),
                self.separator(cx),
                self.row(
                    "layout-details",
                    "显示详细信息列",
                    Some("在活动栏显示日期、类型和大小。"),
                    Switch::new("details-visible")
                        .checked(self.preferences.column_details_visible)
                        .small()
                        .accessibility_label("显示详细信息列")
                        .on_click(cx.listener(|this, checked, window, cx| {
                            this.preferences.column_details_visible = *checked;
                            this.save_preferences(window, cx);
                        }))
                        .into_any_element(),
                    cx,
                ),
            ],
            None,
            cx,
        )
    }

    fn render_file_handling(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let set_default = Button::new("set-default-explorer")
            .small()
            .outline()
            .label("设为默认…")
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.main_app.update(cx, |app, cx| {
                    app.set_default_explorer_from_settings(window, cx);
                });
            }))
            .into_any_element();
        let restore_default = Button::new("restore-default-explorer")
            .small()
            .outline()
            .label("恢复…")
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.main_app.update(cx, |app, cx| {
                    app.restore_default_explorer_from_settings(window, cx);
                });
            }))
            .into_any_element();
        let shortcut = Button::new("create-start-menu-shortcut")
            .small()
            .outline()
            .label("创建快捷方式")
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.main_app.update(cx, |app, cx| {
                    app.create_start_menu_shortcut_from_settings(window, cx);
                });
            }))
            .into_any_element();

        let mut content = v_flex()
            .gap_6()
            .child(self.section(
                "默认资源管理器",
                vec![
                    self.row(
                        "file-default",
                        "使用 Kite 打开文件夹",
                        Some("写入当前用户的文件夹打开命令。"),
                        set_default,
                        cx,
                    ),
                    self.separator(cx),
                    self.row(
                        "file-restore",
                        "恢复之前的打开方式",
                        Some("恢复 Kite 保存的命令。"),
                        restore_default,
                        cx,
                    ),
                ],
                Some("Windows 桌面、任务栏和文件对话框仍由 Windows 管理。"),
                cx,
            ))
            .child(self.section(
                "系统入口",
                vec![self.row(
                    "file-shortcut",
                    "开始菜单快捷方式",
                    Some("创建当前用户的 Kite 入口。"),
                    shortcut,
                    cx,
                )],
                None,
                cx,
            ));
        if let Some(status) = self.status.clone() {
            content = content.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(status),
            );
        }
        content.into_any_element()
    }

    fn render_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let check = Button::new("check-updates")
            .small()
            .outline()
            .label(if self.update_in_progress {
                "正在检查…"
            } else {
                "检查更新…"
            })
            .disabled(self.update_in_progress)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.main_app.update(cx, |app, cx| {
                    app.check_for_updates_from_settings(window, cx);
                });
            }))
            .into_any_element();
        let log = Button::new("show-update-log")
            .small()
            .outline()
            .label("打开更新日志")
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                if let Err(error) = open_update_log() {
                    this.status = Some(error.into());
                    cx.notify();
                }
            }))
            .into_any_element();

        let mut content = v_flex().gap_6().child(self.section(
            "版本",
            vec![
                self.row(
                    "updates-version",
                    "当前版本",
                    Some(env!("CARGO_PKG_VERSION")),
                    check,
                    cx,
                ),
                self.separator(cx),
                self.row(
                    "updates-log",
                    "更新日志",
                    Some("查看下载和安装记录。"),
                    log,
                    cx,
                ),
            ],
            Some("Kite 只在用户选择“检查更新…”后访问更新服务。"),
            cx,
        ));
        if let Some(status) = self.status.clone() {
            content = content.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(status),
            );
        }
        content.into_any_element()
    }

    fn render_about(&self, cx: &App) -> AnyElement {
        v_flex()
            .gap_6()
            .child(
                h_flex()
                    .gap_5()
                    .items_center()
                    .py_1()
                    .child(
                        div()
                            .size(px(74.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(18.))
                            .bg(cx.theme().primary.opacity(0.14))
                            .border_1()
                            .border_color(cx.theme().primary.opacity(0.18))
                            .child(
                                Icon::new(IconName::Wind)
                                    .size(px(48.))
                                    .text_color(cx.theme().primary),
                            ),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(div().text_3xl().font_semibold().child("Kite"))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("版本 {}", env!("CARGO_PKG_VERSION"))),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("原生 Windows 文件导航器"),
                            ),
                    ),
            )
            .child(self.section(
                "产品",
                vec![
                    self.info_row(
                        "about-navigation",
                        "导航先于保管",
                        "Kite 把文件系统显示为可牵引的层级路径。",
                        cx,
                    ),
                    self.separator(cx),
                    self.info_row(
                        "about-native",
                        "原生实现",
                        "Windows 使用 Rust、GPUI 和 Windows Shell。",
                        cx,
                    ),
                    self.separator(cx),
                    self.info_row(
                        "about-local",
                        "本地优先",
                        "Kite 不上传文件内容，也不收集浏览记录。",
                        cx,
                    ),
                ],
                None,
                cx,
            ))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("MIT License · Kite {}", env!("CARGO_PKG_VERSION"))),
            )
            .into_any_element()
    }

    fn info_row(
        &self,
        id: &'static str,
        title: &'static str,
        detail: &'static str,
        cx: &App,
    ) -> AnyElement {
        v_flex()
            .id(id)
            .w_full()
            .gap_1()
            .px_4()
            .py_3()
            .child(div().text_sm().font_medium().child(title))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(detail),
            )
            .into_any_element()
    }

    fn section(
        &self,
        title: &'static str,
        rows: Vec<AnyElement>,
        footer: Option<&'static str>,
        cx: &App,
    ) -> AnyElement {
        v_flex()
            .gap_2p5()
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child(title),
            )
            .child(
                v_flex()
                    .w_full()
                    .overflow_hidden()
                    .rounded(px(12.))
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().title_bar.opacity(0.86))
                    .children(rows),
            )
            .when_some(footer, |this, footer| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(footer),
                )
            })
            .into_any_element()
    }

    fn row(
        &self,
        id: &'static str,
        title: &'static str,
        detail: Option<&'static str>,
        control: AnyElement,
        cx: &App,
    ) -> AnyElement {
        h_flex()
            .id(id)
            .w_full()
            .min_h(px(58.))
            .gap_6()
            .items_center()
            .px_4()
            .py_3()
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(div().text_sm().font_medium().child(title))
                    .when_some(detail, |this, detail| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(detail),
                        )
                    }),
            )
            .child(div().flex_shrink_0().child(control))
            .into_any_element()
    }

    fn separator(&self, cx: &App) -> AnyElement {
        div()
            .ml(px(16.))
            .h(px(1.))
            .bg(cx.theme().border)
            .into_any_element()
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_sidebar(cx))
            .child(
                v_flex()
                    .flex_1()
                    .h_full()
                    .min_w_0()
                    .child(
                        v_flex()
                            .w_full()
                            .gap_6()
                            .px(px(30.))
                            .pt(px(26.))
                            .pb(px(20.))
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .child(self.render_header(cx)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .w_full()
                            .overflow_y_scrollbar()
                            .child(
                                v_flex()
                                    .w_full()
                                    .gap_6()
                                    .px(px(30.))
                                    .pt(px(22.))
                                    .pb(px(30.))
                                    .child(self.render_page(window, cx)),
                            ),
                    ),
            )
    }
}

pub(crate) fn open_settings_window(main_app: Entity<KiteApp>, cx: &mut App) -> Result<(), String> {
    let options = WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some("Kite 设置".into()),
            ..Default::default()
        }),
        window_bounds: Some(WindowBounds::centered(size(px(760.), px(560.)), cx)),
        window_min_size: Some(size(px(680.), px(500.))),
        ..Default::default()
    };
    gpui_kit::open_window(options, cx, move |window, cx| {
        cx.new(|cx| SettingsWindow::new(main_app.clone(), window, cx))
    })
    .map(|_| ())
    .map_err(|error| format!("无法打开设置窗口：{error}"))
}

fn open_update_log() -> Result<(), String> {
    let updates = dirs::data_local_dir()
        .ok_or_else(|| "无法确定更新日志目录".to_string())?
        .join("Kite")
        .join("updates");
    std::fs::create_dir_all(&updates).map_err(|error| format!("无法创建更新日志目录：{error}"))?;
    let log = updates.join("update.log");
    let target: PathBuf = if log.is_file() { log } else { updates };
    open::that(target).map_err(|error| format!("无法打开更新日志：{error}"))
}

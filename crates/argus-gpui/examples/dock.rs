//! Whether gpui-kit's Sidebar, resizable splits and TabBar can carry the desktop's shell.
//!
//! The layout is the Tauri window's: a sidebar down the left, and beside it a
//! column of the stage header, a stage of two panes in a horizontal split, and
//! a bottom dock of two tabbed panels under a vertical split. Pane bodies are
//! placeholders; what this proves is the container behaviour: the drag, the
//! clamp, the tab switch and the sidebar's selection.
//!
//! Run it with `cargo run -p argus-gpui --example dock`. Set
//! `ARGUS_SPIKE_QUIT_MS` to have it print its measured layout and exit, which
//! is how the spike runs it unattended.

// A console window behind the GUI is noise outside a debug build.
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

use argus_gpui::theme::Ledger;
use argus_gpui::window::{self, Header};
use gpui_kit::test::TestSupportExt as _;
use gpui_kit::component::resizable::{h_resizable, resizable_panel, v_resizable, ResizableState};
use gpui_kit::component::sidebar::{Sidebar, SidebarGroup, SidebarMenu, SidebarMenuItem};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{h_flex, v_flex, IconName, Sizable as _};
use gpui_kit::{
    div, prelude::*, px, App, Context, Entity, Pixels, Role, SharedString, Window,
};

/// The smallest a stage pane may be, so a drag cannot hide one.
const PANE_MIN: Pixels = px(160.0);
/// The smallest the stage above the dock may be; the dock takes the rest.
const STAGE_MIN: Pixels = px(180.0);
const DOCK_MIN: Pixels = px(96.0);
const SIDEBAR_WIDTH: Pixels = px(224.0);

/// A top-level view the sidebar's nav picks. Only the title follows it here;
/// the stage's content is the real port's to fill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Nav {
    Workspace,
    Features,
    Git,
    Checkouts,
    Code,
    Console,
}

impl Nav {
    const ALL: [Nav; 6] = [
        Nav::Workspace,
        Nav::Features,
        Nav::Git,
        Nav::Checkouts,
        Nav::Code,
        Nav::Console,
    ];

    fn label(self) -> &'static str {
        match self {
            Nav::Workspace => "Workspace",
            Nav::Features => "Features",
            Nav::Git => "Git",
            Nav::Checkouts => "Checkouts",
            Nav::Code => "Code",
            Nav::Console => "Console",
        }
    }

    /// The default Lucide bundle has no git or code glyph, so those two borrow
    /// the nearest one it does have.
    fn icon(self) -> IconName {
        match self {
            Nav::Workspace => IconName::LayoutDashboard,
            Nav::Features => IconName::Star,
            Nav::Git => IconName::Network,
            Nav::Checkouts => IconName::HardDrive,
            Nav::Code => IconName::FileText,
            Nav::Console => IconName::SquareTerminal,
        }
    }
}

/// The panels the bottom dock's tab bar chooses between.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Panel {
    Console,
    AgentView,
}

impl Panel {
    const ALL: [Panel; 2] = [Panel::Console, Panel::AgentView];

    fn label(self) -> &'static str {
        match self {
            Panel::Console => "Console",
            Panel::AgentView => "Agent view",
        }
    }

    /// Distinct from the sidebar's "Console" item: a test finds both by label.
    fn tab_label(self) -> &'static str {
        match self {
            Panel::Console => "Console tab",
            Panel::AgentView => "Agent view tab",
        }
    }
}

/// The dock: owns what the sidebar and the tab bar select, and the split
/// states so the sizes can be read back.
struct Dock {
    nav: Nav,
    panel: Panel,
    /// The two stage panes side by side.
    panes: Entity<ResizableState>,
    /// The stage above the bottom dock.
    rows: Entity<ResizableState>,
}

impl Dock {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            nav: Nav::Workspace,
            panel: Panel::Console,
            panes: cx.new(|_| ResizableState::default()),
            rows: cx.new(|_| ResizableState::default()),
        }
    }

    fn sidebar(&self, ledger: Ledger, cx: &mut Context<Self>) -> impl IntoElement {
        let nav = SidebarMenu::new().children(Nav::ALL.map(|item| {
            SidebarMenuItem::new(item.label())
                .icon(item.icon())
                .active(self.nav == item)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.nav = item;
                    cx.notify();
                }))
        }));
        let projects = SidebarMenu::new()
            .child(
                SidebarMenuItem::new("orion").default_open(true).children([
                    SidebarMenuItem::new("main"),
                    SidebarMenuItem::new("gpui-migration"),
                    SidebarMenuItem::new("docs-pass"),
                ]),
            )
            .child(SidebarMenuItem::new("nebula"));

        Sidebar::new("sidebar")
            .collapsible(false)
            .w(SIDEBAR_WIDTH)
            .child(SidebarGroup::new("Argus").child(nav))
            .child(SidebarGroup::new("Projects").child(projects))
            .footer(foot(ledger))
    }

    fn stage(&self, ledger: Ledger) -> impl IntoElement {
        let pane = |id: &'static str, title: &'static str| {
            v_flex()
                .id(id)
                .test_support()
                .size_full()
                .gap_1()
                .p_3()
                .bg(ledger.background)
                .child(div().text_xs().text_color(ledger.faint).child(title))
                .child(
                    div()
                        .text_sm()
                        .text_color(ledger.muted)
                        .child("placeholder pane body"),
                )
        };
        h_resizable("stage-split")
            .with_state(&self.panes)
            .child(
                resizable_panel()
                    .size(px(520.0))
                    .size_range(PANE_MIN..Pixels::MAX)
                    .child(pane("pane-a", "pane a")),
            )
            .child(
                resizable_panel()
                    .size_range(PANE_MIN..Pixels::MAX)
                    .child(pane("pane-b", "pane b")),
            )
    }

    fn bottom(&self, ledger: Ledger, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = TabBar::new("dock-tabs")
            .underline()
            .small()
            .selected_index(Panel::ALL.iter().position(|p| *p == self.panel).unwrap_or(0))
            .on_click(cx.listener(|this, ix: &usize, _, cx| {
                if let Some(panel) = Panel::ALL.get(*ix) {
                    this.panel = *panel;
                    cx.notify();
                }
            }))
            .children(
                Panel::ALL.map(|p| Tab::new().label(p.label()).aria_label(p.tab_label())),
            );
        let body = match self.panel {
            Panel::Console => div()
                .id("panel-console")
                .test_support()
                .font_family(cx_mono(cx))
                .text_sm()
                .text_color(ledger.muted)
                .child("$ argus server status"),
            Panel::AgentView => div()
                .id("panel-agent")
                .test_support()
                .text_sm()
                .text_color(ledger.muted)
                .child("3 agents working, 3 need you"),
        };
        v_flex()
            .id("dock")
            .size_full()
            .bg(ledger.background)
            .child(
                h_flex()
                    .flex_shrink_0()
                    .bg(ledger.chrome)
                    .border_b_1()
                    .border_color(ledger.line)
                    .child(tabs),
            )
            .child(div().flex_1().min_h_0().p_3().child(body))
    }
}

/// The mono face the theme resolved at install, for the console body.
fn cx_mono<T>(cx: &mut Context<T>) -> SharedString {
    use gpui_kit::component::ActiveTheme as _;
    cx.theme().mono_font_family.clone()
}

/// The sidebar foot: what needs the user and where the daemon is.
fn foot(ledger: Ledger) -> impl IntoElement {
    let dot = |colour| div().size(px(6.0)).rounded_full().bg(colour).flex_shrink_0();
    v_flex()
        .w_full()
        .gap_1()
        .pt_2()
        .border_t_1()
        .border_color(ledger.line)
        .text_xs()
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .text_color(ledger.muted)
                .child(dot(ledger.warning))
                .child("3 need you · 3 working"),
        )
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .text_color(ledger.faint)
                .child(dot(ledger.success))
                .child("argusd · local"),
        )
}

impl Render for Dock {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ledger = *Ledger::global(cx);
        let sidebar = self.sidebar(ledger, cx);
        let stage = self.stage(ledger);
        let bottom = self.bottom(ledger, cx);
        h_flex()
            .size_full()
            .bg(ledger.background)
            .text_color(ledger.foreground)
            .child(sidebar)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    // The sidebar's own rows hide their selected state from the
                    // accessibility tree, so the stage names what is selected.
                    .child(
                        div()
                            .id("stage-title")
                            .role(Role::Status)
                            .test_support()
                            .aria_label(self.nav.label())
                            .child(Header::new().title(self.nav.label())),
                    )
                    .child(
                        div().flex_1().min_h_0().child(
                            v_resizable("dock-split")
                                .with_state(&self.rows)
                                .child(
                                    resizable_panel()
                                        .size_range(STAGE_MIN..Pixels::MAX)
                                        .child(stage),
                                )
                                .child(
                                    resizable_panel()
                                        .size(px(220.0))
                                        .size_range(DOCK_MIN..Pixels::MAX)
                                        .child(bottom),
                                ),
                        ),
                    ),
            )
    }
}

fn main() {
    let quit_after: Option<u64> = std::env::var("ARGUS_SPIKE_QUIT_MS")
        .ok()
        .and_then(|ms| ms.parse().ok());
    window::application().run(move |cx: &mut App| {
        window::init(cx);
        let options = window::options(cx);
        let (_handle, dock) = gpui_kit::open_window(options, cx, |_, cx| cx.new(Dock::new))
            .expect("the dock window opens");
        cx.activate(true);
        if let Some(ms) = quit_after {
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(ms))
                    .await;
                cx.update(|cx| {
                    let dock = dock.read(cx);
                    println!("panes: {:?}", dock.panes.read(cx).sizes());
                    println!("rows: {:?}", dock.rows.read(cx).sizes());
                    cx.quit();
                })
            })
            .detach();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::test::{TestEventExt as _, TestQueryExt as _, TestWindowExt as _};
    use gpui_kit::{
        point, size, AnyWindowHandle, Bounds, ElementId, MouseButton, Point, TestAppContext,
        WindowBounds, WindowOptions,
    };

    const WIDTH: f32 = 1280.0;
    const HEIGHT: f32 = 800.0;

    fn open(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Dock>) {
        cx.update(window::init);
        let (handle, dock) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(WIDTH), px(HEIGHT)),
                    ))),
                    ..Default::default()
                },
                cx,
                |_, cx| cx.new(Dock::new),
            )
            .expect("open test window")
        });
        let handle: AnyWindowHandle = handle.into();
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
        (handle, dock)
    }

    fn pane_widths(cx: &mut TestAppContext, dock: &Entity<Dock>) -> Vec<f32> {
        cx.update(|cx| {
            let sizes = dock.read(cx).panes.read(cx).sizes().clone();
            sizes.iter().map(|s| s.as_f32()).collect()
        })
    }

    fn row_heights(cx: &mut TestAppContext, dock: &Entity<Dock>) -> Vec<f32> {
        cx.update(|cx| {
            let sizes = dock.read(cx).rows.read(cx).sizes().clone();
            sizes.iter().map(|s| s.as_f32()).collect()
        })
    }

    /// Where the divider between the group's first two panels sits. The stage
    /// split is nested in the dock split, so the outer group's scope also
    /// holds the inner handle and the match is the one outside `stage-split`.
    fn handle_center(cx: &mut TestAppContext, handle: AnyWindowHandle, group: &str) -> Point<Pixels> {
        cx.update_window(handle, |_, window, _| {
            let handles = window
                .within(group.to_owned())
                .find_all(("resizable-handle", 0usize));
            handles
                .iter()
                .find(|h| {
                    let inner = h.path().contains(&ElementId::from("stage-split"));
                    inner == (group == "stage-split")
                })
                .unwrap_or_else(|| panic!("no handle for {group}"))
                .bounds()
                .center()
        })
        .unwrap()
    }

    fn drag_by(
        cx: &mut TestAppContext,
        handle: AnyWindowHandle,
        from: Point<Pixels>,
        by: Point<Pixels>,
    ) {
        cx.update_window(handle, |_, window, cx| {
            window.pointer_down(from, MouseButton::Left, cx);
            window.pointer_move(from + by * 0.5, Some(MouseButton::Left), cx);
            window.pointer_move(from + by, Some(MouseButton::Left), cx);
            window.pointer_up(from + by, MouseButton::Left, cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
    }

    fn click_label(cx: &mut TestAppContext, handle: AnyWindowHandle, label: &str) {
        let at = cx
            .update_window(handle, |_, window, _| window.find_by_label(label).bounds().center())
            .unwrap();
        click_at(cx, handle, at);
    }

    /// The sidebar names its rows `{group}-{menu}-{item}`, and only those
    /// wrappers reach the accessibility tree, so a row is found by that id.
    fn click_id(cx: &mut TestAppContext, handle: AnyWindowHandle, id: &'static str) {
        let at = cx
            .update_window(handle, |_, window, _| window.find(id).bounds().center())
            .unwrap();
        click_at(cx, handle, at);
    }

    fn click_at(cx: &mut TestAppContext, handle: AnyWindowHandle, at: Point<Pixels>) {
        cx.update_window(handle, |_, window, cx| {
            window.pointer_down(at, MouseButton::Left, cx);
            window.pointer_up(at, MouseButton::Left, cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
    }

    #[gpui_kit::test]
    fn dragging_the_stage_split_resizes_both_panes(cx: &mut TestAppContext) {
        let (handle, dock) = open(cx);
        let before = pane_widths(cx, &dock);
        assert_eq!(before.len(), 2);
        eprintln!("panes before drag: {before:?}");

        let at = handle_center(cx, handle, "stage-split");
        drag_by(cx, handle, at, point(px(-120.0), px(0.0)));

        let after = pane_widths(cx, &dock);
        eprintln!("panes after -120px drag: {after:?}");
        assert!((before[0] - after[0] - 120.0).abs() < 2.0, "{before:?} -> {after:?}");
        assert!((after[1] - before[1] - 120.0).abs() < 2.0, "{before:?} -> {after:?}");
        // The panes are what the user sees: the layout followed the state.
        cx.update_window(handle, |_, window, _| {
            let a = window.find("pane-a").bounds().size.width.as_f32();
            assert!((a - after[0]).abs() < 2.0, "pane-a is {a}, state says {}", after[0]);
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn the_stage_split_clamps_at_the_pane_minimum(cx: &mut TestAppContext) {
        let (handle, dock) = open(cx);
        let at = handle_center(cx, handle, "stage-split");
        drag_by(cx, handle, at, point(px(-2000.0), px(0.0)));
        let left = pane_widths(cx, &dock);
        eprintln!("panes after far-left drag: {left:?}");
        assert!((left[0] - PANE_MIN.as_f32()).abs() < 1.0, "{left:?}");

        let at = handle_center(cx, handle, "stage-split");
        drag_by(cx, handle, at, point(px(4000.0), px(0.0)));
        let right = pane_widths(cx, &dock);
        eprintln!("panes after far-right drag: {right:?}");
        assert!((right[1] - PANE_MIN.as_f32()).abs() < 1.0, "{right:?}");
    }

    #[gpui_kit::test]
    fn dragging_the_dock_split_resizes_the_dock(cx: &mut TestAppContext) {
        let (handle, dock) = open(cx);
        let before = row_heights(cx, &dock);
        eprintln!("rows before drag: {before:?}");
        let at = handle_center(cx, handle, "dock-split");
        drag_by(cx, handle, at, point(px(0.0), px(-80.0)));
        let after = row_heights(cx, &dock);
        eprintln!("rows after -80px drag: {after:?}");
        assert!((after[1] - before[1] - 80.0).abs() < 2.0, "{before:?} -> {after:?}");

        let at = handle_center(cx, handle, "dock-split");
        drag_by(cx, handle, at, point(px(0.0), px(2000.0)));
        let low = row_heights(cx, &dock);
        eprintln!("rows after far-down drag: {low:?}");
        assert!((low[1] - DOCK_MIN.as_f32()).abs() < 1.0, "{low:?}");
    }

    #[gpui_kit::test]
    fn choosing_a_tab_switches_the_bottom_panel(cx: &mut TestAppContext) {
        let (handle, dock) = open(cx);
        cx.update_window(handle, |_, window, _| {
            assert!(window.try_find("panel-console").is_some());
            assert!(window.try_find("panel-agent").is_none());
        })
        .unwrap();

        click_label(cx, handle, "Agent view tab");
        cx.update(|cx| assert_eq!(dock.read(cx).panel, Panel::AgentView));
        cx.update_window(handle, |_, window, _| {
            assert!(window.try_find("panel-agent").is_some());
            assert!(window.try_find("panel-console").is_none());
        })
        .unwrap();

        click_label(cx, handle, "Console tab");
        cx.update(|cx| assert_eq!(dock.read(cx).panel, Panel::Console));
        cx.update_window(handle, |_, window, _| {
            assert!(window.try_find("panel-console").is_some());
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn the_sidebar_selection_follows_a_click(cx: &mut TestAppContext) {
        let (handle, dock) = open(cx);
        let title = |cx: &mut TestAppContext| {
            cx.update_window(handle, |_, window, _| {
                window.find("stage-title").label().map(str::to_owned)
            })
            .unwrap()
        };
        assert_eq!(title(cx).as_deref(), Some("Workspace"));

        // Git is the third nav row: group 0, menu 0, item 2.
        click_id(cx, handle, "0-0-2");
        cx.update(|cx| assert_eq!(dock.read(cx).nav, Nav::Git));
        assert_eq!(title(cx).as_deref(), Some("Git"));

        click_id(cx, handle, "0-0-5");
        cx.update(|cx| assert_eq!(dock.read(cx).nav, Nav::Console));
        assert_eq!(title(cx).as_deref(), Some("Console"));
    }

    #[gpui_kit::test]
    fn the_sidebar_holds_its_width_beside_the_stage(cx: &mut TestAppContext) {
        let (handle, _dock) = open(cx);
        cx.update_window(handle, |_, window, _| {
            let item = window.find("0-0-0").bounds();
            let pane = window.find("pane-a").bounds();
            eprintln!("nav row {item:?}, pane a {pane:?}");
            // The sidebar's width is the group's `w`: the stage starts past it.
            assert!(pane.left() >= SIDEBAR_WIDTH - px(2.0));
            assert!(item.right() <= SIDEBAR_WIDTH);
        })
        .unwrap();
    }
}

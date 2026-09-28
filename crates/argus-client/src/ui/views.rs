//! The tab strip naming the four stages, and which tab a click lands on.
//!
//! The strip is the top of the command center, above the rail and every
//! stage. It costs its rows on purpose: a view you cannot see the existence
//! of is a view nobody opens, and the numbers on the tabs are the whole of
//! their documentation.

use super::*;

use crate::app::View;

/// One tab's text, including the padding that makes it a click target
/// rather than a word.
fn tab_text(view: View) -> String {
    format!("  {}  ", view.label().to_ascii_uppercase())
}

/// How wide the WORKSPACE tab is drawn.
///
/// The rail's right edge lines up with the start of the FEATURE tab, so a
/// wider rail grows the WORKSPACE tab and its highlight to that junction.
fn workspace_tab_width(strip_width: u16) -> u16 {
    let rail = crate::ui::command_center::rail_width(strip_width);
    let brand = BRAND.chars().count() as u16;
    // The junction and sidebar border sit on the rail's last column.
    rail.saturating_sub(brand + 1)
        .max(tab_text(View::Workspace).chars().count() as u16)
}

fn drawn_tab_text(view: View, strip_width: u16) -> String {
    let base = tab_text(view);
    if view == View::Workspace {
        let target = workspace_tab_width(strip_width);
        let current = base.chars().count() as u16;
        if target > current {
            return format!("{base}{}", " ".repeat((target - current) as usize));
        }
    }
    base
}

fn drawn_tab_width(view: View, strip_width: u16) -> u16 {
    drawn_tab_text(view, strip_width)
        .chars()
        .count() as u16
}

/// The product mark owns a cell-height divider, as it does in the reference
/// shell. Keeping its width in the same constant used by hit-testing prevents
/// the visible divider and click targets from drifting apart.
const BRAND: &str = " ■  ARGUS  ";

/// Which tab a point falls on. Shared with the renderer rather than
/// re-derived, so a click lands on the tab that was actually drawn.
pub fn tab_at(views: Panel, x: u16, y: u16) -> Option<View> {
    let strip = views.outer;
    // A zero-sized strip is one that was not drawn — on a short terminal,
    // or before the first frame — and its default rect sits on row 0,
    // where a click would otherwise land on a tab that is not there.
    if strip.width == 0 || strip.height == 0 {
        return None;
    }
    if y != strip.y || x < strip.x {
        return None;
    }
    let mut cell = strip.x.saturating_add(brand_cells(views));
    for view in View::ALL {
        let width = drawn_tab_width(view, strip.width);
        if x >= cell && x < cell + width {
            return Some(view);
        }
        cell += width;
    }
    None
}

/// How wide the product mark was drawn: recorded in the strip's panel so a
/// click is resolved against the tabs actually on screen.
fn brand_cells(views: Panel) -> u16 {
    if views.first == 0 {
        BRAND.chars().count() as u16
    } else {
        views.first as u16
    }
}

/// How long a host name may run in the product mark before it is cut.
const HOST_IN_BRAND: usize = 24;

/// The product mark, naming the host on screen when it is not this machine:
/// every tab below it is that host's, so it is the one thing that says whose
/// agents these are.
fn brand_text(host: Option<&str>) -> String {
    match host {
        None => BRAND.to_string(),
        Some(host) => {
            let mut name: String = host.chars().take(HOST_IN_BRAND).collect();
            if host.chars().count() > HOST_IN_BRAND {
                name.pop();
                name.push('…');
            }
            format!(" ■  ARGUS · {name}  ")
        }
    }
}

pub(super) fn render_view_tabs(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let brand = brand_text(app.host.as_deref());
    let brand_width = brand.chars().count() as u16;
    let mut labels = vec![Span::styled(
        brand.clone(),
        Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
    )];
    let mut rule = vec![Span::styled(
        "─".repeat(brand_width as usize),
        Style::default().fg(th.edge),
    )];
    for view in View::ALL {
        let open = view == app.view;
        let text = drawn_tab_text(view, area.width);
        labels.push(Span::styled(
            text.clone(),
            Style::default()
                .fg(if open { th.text } else { th.dim })
                .add_modifier(if open {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ));
        rule.push(Span::styled(
            if open {
                "━".repeat(text.len())
            } else {
                "─".repeat(text.len())
            },
            Style::default().fg(if open { th.accent } else { th.edge }),
        ));
    }
    // The strip's rule runs the full width, as the template's bar does.
    let drawn: usize = rule.iter().map(Span::width).sum();
    rule.push(Span::styled(
        "─".repeat((area.width as usize).saturating_sub(drawn)),
        Style::default().fg(th.edge),
    ));
    f.render_widget(
        Paragraph::new(Line::from(labels)),
        Rect { height: 1, ..area },
    );
    if (brand_width as usize) < area.width as usize {
        f.render_widget(
            Paragraph::new(Span::styled("│", Style::default().fg(th.edge))),
            Rect {
                x: area.x + brand_width.saturating_sub(1),
                y: area.y,
                width: 1,
                height: 1,
            },
        );
    }
    if area.height > 1 {
        f.render_widget(
            Paragraph::new(Line::from(rule)),
            Rect {
                y: area.y + 1,
                height: 1,
                ..area
            },
        );
        if (brand_width as usize) < area.width as usize {
            f.render_widget(
                Paragraph::new(Span::styled("┴", Style::default().fg(th.edge))),
                Rect {
                    x: area.x + brand_width.saturating_sub(1),
                    y: area.y + 1,
                    width: 1,
                    height: 1,
                },
            );
        }
    }
    app.layout.views = Panel {
        outer: area,
        inner: area,
        first: brand_width as usize,
    };
}

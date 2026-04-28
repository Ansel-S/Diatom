
use diatom_bridge::protocol::ConsoleLevel;
use gpui::{Context, Render, Window};
use std::time::{SystemTime, UNIX_EPOCH};
use ui::{prelude::*, Divider, Label};

const MAX_ENTRIES: usize = 2_000;

#[derive(Debug, Clone)]
pub struct ConsoleEntry {
    pub level:       ConsoleLevel,
    pub text:        String,
    pub source_file: Option<String>,
    pub source_line: Option<u32>,
    pub ts_ms:       u64,
}

#[derive(Default)]
pub struct ConsolePanel {
    entries: Vec<ConsoleEntry>,
    filter:  String,
}

impl ConsolePanel {
    /// Append a new log entry (called from BridgeDispatch on the main thread).
    pub fn push(
        &mut self,
        level: ConsoleLevel,
        text: String,
        source_file: Option<String>,
        source_line: Option<u32>,
        cx: &mut Context<Self>,
    ) {
        let ts_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        self.entries.push(ConsoleEntry { level, text, source_file, source_line, ts_ms });

        if self.entries.len() > MAX_ENTRIES {
            let overflow = self.entries.len() - MAX_ENTRIES;
            self.entries.drain(..overflow);
        }
        cx.notify();
    }

    /// Clear entries on page navigation.
    pub fn on_navigate(&mut self, _url: &str, cx: &mut Context<Self>) {
        self.entries.clear();
        cx.notify();
    }

    fn level_color(level: &ConsoleLevel) -> gpui::Hsla {
        match level {
            ConsoleLevel::Error => gpui::red(),
            ConsoleLevel::Warn  => gpui::yellow(),
            ConsoleLevel::Info  => gpui::blue(),
            _                   => gpui::white(),
        }
    }

    fn level_label(level: &ConsoleLevel) -> &'static str {
        match level {
            ConsoleLevel::Log   => "log",
            ConsoleLevel::Info  => "info",
            ConsoleLevel::Warn  => "warn",
            ConsoleLevel::Error => "error",
            ConsoleLevel::Debug => "debug",
        }
    }
}

impl Render for ConsolePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let filter = self.filter.to_lowercase();
        let visible: Vec<_> = self
            .entries
            .iter()
            .filter(|e| filter.is_empty() || e.text.to_lowercase().contains(&filter))
            .collect();

        v_flex()
            .size_full()
            .bg(cx.theme().colors().panel_background)
            .child(
                h_flex()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .child(
                        ui::TextField::new("console-filter", "Filter")
                            .on_change(cx.listener(|this, val: &str, cx| {
                                this.filter = val.to_string();
                                cx.notify();
                            }))
                    )
                    .child(
                        Label::new(format!("{} entries", visible.len()))
                            .color(Color::Muted)
                            .size(LabelSize::Small)
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_y_scroll()
                    .children(visible.iter().map(|entry| {
                        let color = Self::level_color(&entry.level);
                        let badge = Self::level_label(&entry.level);
                        let loc = match (&entry.source_file, entry.source_line) {
                            (Some(f), Some(l)) => format!(" {}:{}", shorten_url(f), l),
                            _ => String::new(),
                        };
                        h_flex()
                            .px_2()
                            .py_px()
                            .gap_2()
                            .border_b_1()
                            .border_color(cx.theme().colors().border.opacity(0.3))
                            .child(
                                Label::new(badge)
                                    .color(Color::Custom(color))
                                    .size(LabelSize::XSmall)
                            )
                            .child(
                                Label::new(entry.text.as_str())
                                    .size(LabelSize::Small)
                            )
                            .child(
                                Label::new(loc.as_str())
                                    .color(Color::Muted)
                                    .size(LabelSize::XSmall)
                            )
                    }))
            )
    }
}

/// Shorten a URL to just `filename:line` for compact display.
fn shorten_url(url: &str) -> &str {
    url.rsplit('/').next().unwrap_or(url)
}


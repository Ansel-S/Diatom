
use diatom_bridge::protocol::NetworkEventPayload;
use gpui::{Context, Render, Window};
use ui::{prelude::*, Label};

const MAX_EVENTS: usize = 1_000;

#[derive(Default)]
pub struct NetworkPanel {
    events: Vec<NetworkEventPayload>,
    filter: String,
}

impl NetworkPanel {
    pub fn push(&mut self, ev: NetworkEventPayload, cx: &mut Context<Self>) {
        self.events.push(ev);
        if self.events.len() > MAX_EVENTS {
            let over = self.events.len() - MAX_EVENTS;
            self.events.drain(..over);
        }
        cx.notify();
    }

    pub fn on_navigate(&mut self, _url: &str, cx: &mut Context<Self>) {
        self.events.clear();
        cx.notify();
    }
}

impl Render for NetworkPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let filter = self.filter.to_lowercase();
        let visible: Vec<_> = self
            .events
            .iter()
            .filter(|e| filter.is_empty() || e.url.to_lowercase().contains(&filter))
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
                        ui::TextField::new("net-filter", "Filter URL")
                            .on_change(cx.listener(|this, val: &str, cx| {
                                this.filter = val.to_string();
                                cx.notify();
                            }))
                    )
                    .child(
                        Label::new(format!("{} requests", visible.len()))
                            .color(Color::Muted)
                            .size(LabelSize::Small)
                    ),
            )
            .child(
                h_flex()
                    .px_2()
                    .py_1()
                    .bg(cx.theme().colors().surface_background)
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .child(header_cell("Method",  60.0))
                    .child(header_cell("Status",  50.0))
                    .child(header_cell("URL",    400.0))
                    .child(header_cell("Latency", 70.0))
                    .child(header_cell("Size",    70.0))
                    .child(header_cell("Blocked", 60.0)),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_y_scroll()
                    .children(visible.iter().map(|ev| {
                        let status_color = match ev.status {
                            Some(s) if s >= 400 => gpui::red(),
                            Some(s) if s >= 300 => gpui::yellow(),
                            _ => gpui::white(),
                        };
                        let status_text = ev.status
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "…".into());

                        h_flex()
                            .px_2()
                            .py_px()
                            .border_b_1()
                            .border_color(cx.theme().colors().border.opacity(0.3))
                            .child(cell(ev.method.as_str(), 60.0))
                            .child(
                                div()
                                    .w(gpui::px(50.0))
                                    .child(
                                        Label::new(status_text.as_str())
                                            .color(Color::Custom(status_color))
                                            .size(LabelSize::Small)
                                    )
                            )
                            .child(
                                div()
                                    .w(gpui::px(400.0))
                                    .overflow_x_hidden()
                                    .child(
                                        Label::new(ev.url.as_str())
                                            .size(LabelSize::Small)
                                    )
                            )
                            .child(cell(&format!("{}ms", ev.latency_ms), 70.0))
                            .child(cell(&format_bytes(ev.response_bytes), 70.0))
                            .child(cell(if ev.blocked { "✗" } else { "" }, 60.0))
                    }))
            )
    }
}

fn header_cell(label: &str, width: f32) -> impl IntoElement {
    div()
        .w(gpui::px(width))
        .child(Label::new(label).color(Color::Muted).size(LabelSize::XSmall))
}

fn cell(text: &str, width: f32) -> impl IntoElement {
    div()
        .w(gpui::px(width))
        .child(Label::new(text.to_string()).size(LabelSize::Small))
}

fn format_bytes(b: u64) -> String {
    if b >= 1024 * 1024 {
        format!("{:.1} MB", b as f64 / (1024.0 * 1024.0))
    } else if b >= 1024 {
        format!("{:.1} KB", b as f64 / 1024.0)
    } else {
        format!("{} B", b)
    }
}


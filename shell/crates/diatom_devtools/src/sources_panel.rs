
use std::collections::HashMap;
use diatom_bridge::protocol::{DomNode, RequestId};
use diatom_bridge::DevPanelMessage;
use gpui::{Context, Entity, Render, Window};
use ui::{prelude::*, Button, ButtonStyle, Label};

/// A fetched source file.
#[derive(Clone)]
struct SourceFile {
    url:     String,
    content: String,
}

/// Accumulates streaming SLM completion deltas.
struct PendingCompletion {
    buffer: String,
}

#[derive(Default)]
pub struct SourcesPanel {
    /// url → fetched source.
    sources:       HashMap<String, SourceFile>,
    /// Requests awaiting a SourceFileContent response.
    pending_fetch: HashMap<RequestId, String>,
    /// Currently selected URL.
    selected:      Option<String>,
    /// Streaming SLM completions keyed by request ID.
    slm_pending:   HashMap<RequestId, PendingCompletion>,
    /// Current page metadata.
    page_title:    String,
    page_url:      String,
    dom_root:      Option<DomNode>,
    /// Channel to send messages back to the Diatom shell.
    outbound:      Option<tokio::sync::mpsc::Sender<DevPanelMessage>>,
}

impl SourcesPanel {
    pub fn with_outbound(
        outbound: tokio::sync::mpsc::Sender<DevPanelMessage>,
    ) -> Self {
        Self { outbound: Some(outbound), ..Default::default() }
    }


    pub fn on_navigate(
        &mut self,
        url: &str,
        title: &str,
        dom_snapshot: Option<DomNode>,
        cx: &mut Context<Self>,
    ) {
        self.sources.clear();
        self.pending_fetch.clear();
        self.slm_pending.clear();
        self.selected   = None;
        self.page_url   = url.to_string();
        self.page_title = title.to_string();
        self.dom_root   = dom_snapshot;
        cx.notify();
    }

    /// Store a source file delivered by the Diatom shell.
    pub fn receive_source(
        &mut self,
        id: RequestId,
        url: String,
        content: String,
        cx: &mut Context<Self>,
    ) {
        self.pending_fetch.remove(&id);
        if self.selected.is_none() {
            self.selected = Some(url.clone());
        }
        self.sources.insert(url.clone(), SourceFile { url, content });
        cx.notify();
    }

    /// Append a streaming SLM completion delta.
    pub fn receive_slm_delta(
        &mut self,
        id: RequestId,
        delta: String,
        done: bool,
        cx: &mut Context<Self>,
    ) {
        let entry = self
            .slm_pending
            .entry(id)
            .or_insert(PendingCompletion { buffer: String::new() });
        entry.buffer.push_str(&delta);
        if done {
            let _completed = self.slm_pending.remove(&id);
        }
        cx.notify();
    }

    /// Set project root — used when the DevPanel is opened with a local project.
    pub fn open_project(&mut self, _project_root: String, cx: &mut Context<Self>) {
        cx.notify();
    }

    /// Send an "Open in Zed" request for the currently selected source.
    fn open_selected_in_zed(&self, line: Option<u32>, cx: &mut Context<Self>) {
        let Some(url) = self.selected.clone() else { return };
        let Some(tx) = self.outbound.clone() else { return };
        cx.spawn(|_, _| async move {
            tx.send(DevPanelMessage::OpenInZedIde { url, line }).await.ok();
        })
        .detach();
    }
}

impl Render for SourcesPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .size_full()
            .bg(cx.theme().colors().panel_background)
            .child(
                v_flex()
                    .w(gpui::px(220.0))
                    .h_full()
                    .border_r_1()
                    .border_color(cx.theme().colors().border)
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .border_b_1()
                            .border_color(cx.theme().colors().border)
                            .child(
                                Label::new(self.page_title.as_str())
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            )
                    )
                    .child(
                        div()
                            .flex_1()
                            .overflow_y_scroll()
                            .children(self.sources.keys().map(|url| {
                                let short = url
                                    .rsplit('/')
                                    .next()
                                    .unwrap_or(url)
                                    .to_string();
                                let is_selected = self.selected.as_deref() == Some(url);
                                let url_clone   = url.clone();
                                div()
                                    .px_2()
                                    .py_1()
                                    .cursor_pointer()
                                    .when(is_selected, |d| {
                                        d.bg(cx.theme().colors().element_selected)
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.selected = Some(url_clone.clone());
                                        cx.notify();
                                    }))
                                    .child(Label::new(short).size(LabelSize::Small))
                            }))
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .h_full()
                    .overflow_hidden()
                    .child(
                        h_flex()
                            .px_2()
                            .py_1()
                            .gap_2()
                            .border_b_1()
                            .border_color(cx.theme().colors().border)
                            .bg(cx.theme().colors().surface_background)
                            .child(
                                Label::new("Read-only")
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted)
                            )
                            .child(div().flex_1())
                            .when(self.selected.is_some(), |row| {
                                row.child(
                                    Button::new("open-in-zed", "Open in Zed")
                                        .style(ButtonStyle::Filled)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.open_selected_in_zed(None, cx);
                                        }))
                                )
                            })
                    )
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .child(self.render_source_body(cx))
                    )
            )
    }
}

impl SourcesPanel {
    fn render_source_body(&self, cx: &Context<Self>) -> impl IntoElement {
        match &self.selected {
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Label::new("Select a source file")
                        .color(Color::Muted)
                )
                .into_any_element(),

            Some(url) => match self.sources.get(url) {
                None => div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Label::new("Loading…").color(Color::Muted))
                    .into_any_element(),

                Some(file) => {
                    div()
                        .size_full()
                        .overflow_y_scroll()
                        .font_family("Zed Mono")
                        .text_size(gpui::rems(0.85))
                        .child(
                            Label::new(file.content.as_str())
                                .size(LabelSize::Small)
                        )
                        .into_any_element()
                }
            },
        }
    }
}


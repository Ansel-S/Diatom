
use diatom_bridge::{BrowserMessage, DevPanelMessage, RequestId};
use gpui::{Context, Entity, Task, Window};
use tokio::sync::mpsc;

use crate::console_panel::ConsolePanel;
use crate::network_panel::NetworkPanel;
use crate::sources_panel::SourcesPanel;

/// Root GPUI model for the DevPanel.
pub struct BridgeDispatch {
    pub console:  Entity<ConsolePanel>,
    pub network:  Entity<NetworkPanel>,
    pub sources:  Entity<SourcesPanel>,
    pub page_url: String,
    outbound:     mpsc::Sender<DevPanelMessage>,
    _pump:        Task<()>,
}

impl BridgeDispatch {
    pub fn new(
        mut inbound: mpsc::Receiver<BrowserMessage>,
        outbound: mpsc::Sender<DevPanelMessage>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let console = cx.new(|_| ConsolePanel::default());
        let network = cx.new(|_| NetworkPanel::default());
        let sources = cx.new(|_| SourcesPanel::with_outbound(outbound.clone()));

        let handle    = cx.entity().downgrade();
        let console_h = console.downgrade();
        let network_h = network.downgrade();
        let sources_h = sources.downgrade();
        let out_tx    = outbound.clone();

        let pump = cx.spawn(|_, mut cx| async move {
            while let Some(msg) = inbound.recv().await {
                match msg {
                    BrowserMessage::ConsoleEntry { level, text, source_file, source_line } => {
                        console_h.update(&mut cx, |p, cx| {
                            p.push(level, text, source_file, source_line, cx);
                        }).ok();
                    }

                    BrowserMessage::NetworkEvent(ev) => {
                        network_h.update(&mut cx, |p, cx| p.push(ev, cx)).ok();
                    }

                    BrowserMessage::SourceFileContent { id, url, content } => {
                        sources_h.update(&mut cx, |p, cx| {
                            p.receive_source(id, url, content, cx);
                        }).ok();
                    }

                    BrowserMessage::PageNavigated { url, title, dom_snapshot } => {
                        handle.update(&mut cx, |this, cx| {
                            this.page_url = url.clone();
                            this.console.update(cx, |p, cx| p.on_navigate(&url, cx));
                            this.network.update(cx, |p, cx| p.on_navigate(&url, cx));
                            this.sources.update(cx, |p, cx| {
                                p.on_navigate(&url, &title, dom_snapshot, cx)
                            });
                        }).ok();
                    }

                    BrowserMessage::SlmCompletion { id, delta, done } => {
                        sources_h.update(&mut cx, |p, cx| {
                            p.receive_slm_delta(id, delta, done, cx);
                        }).ok();
                    }

                    BrowserMessage::Open { id: _, project_root } => {
                        sources_h.update(&mut cx, |p, cx| {
                            p.open_project(project_root, cx);
                        }).ok();
                    }

                    BrowserMessage::Shutdown => {
                        log::info!("[bridge-dispatch] shutdown requested");
                        out_tx.send(DevPanelMessage::Closed).await.ok();
                        break;
                    }
                }
            }
        });

        Self {
            console,
            network,
            sources,
            page_url: String::new(),
            outbound,
            _pump: pump,
        }
    }

    /// Fire-and-forget a message to the Diatom shell from a GPUI sync context.
    pub fn send(&self, msg: DevPanelMessage, cx: &mut Context<Self>) {
        let tx = self.outbound.clone();
        cx.spawn(|_, _| async move { tx.send(msg).await.ok() }).detach();
    }
}


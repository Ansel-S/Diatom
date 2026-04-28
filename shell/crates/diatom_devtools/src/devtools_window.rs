
use anyhow::Result;
use diatom_bridge::{BridgeServer, BrowserMessage, DevPanelMessage};
use gpui::{App, AppContext, AsyncApp, Context, Entity, Task, Window, actions, px, size};

use crate::bridge_dispatch::BridgeDispatch;
use crate::console_panel::ConsolePanel;
use crate::network_panel::NetworkPanel;
use crate::sources_panel::SourcesPanel;

actions!(devpanel, [ToggleConsole, ToggleNetwork, ToggleSources]);

/// Called once from `main.rs` inside `App::run`.
pub fn init(socket_path: String, cx: &mut App) {
    cx.bind_keys([
        gpui::KeyBinding::new("cmd-shift-c", ToggleConsole, None),
        gpui::KeyBinding::new("cmd-shift-n", ToggleNetwork, None),
        gpui::KeyBinding::new("cmd-shift-s", ToggleSources, None),
    ]);

    cx.spawn(|cx| async move {
        run(socket_path, cx).await.unwrap_or_else(|e| {
            log::error!("[devpanel] fatal: {e:?}");
        });
    })
    .detach();
}

async fn run(socket_path: String, mut cx: AsyncApp) -> Result<()> {
    let bridge = BridgeServer::start(&socket_path).await?;
    let outbound = bridge.outbound.clone();

    outbound.send(DevPanelMessage::Ready).await.ok();

    let window_options = gpui::WindowOptions {
        window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds {
            origin: gpui::point(px(0.0), px(0.0)),
            size: size(px(1200.0), px(800.0)),
        })),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some("Diatom DevPanel".into()),
            appears_transparent: true,
            ..Default::default()
        }),
        ..Default::default()
    };

    let dispatch = cx
        .update(|cx| {
            cx.open_window(window_options, |window, cx| {
                cx.new(|cx| {
                    BridgeDispatch::new(
                        bridge.inbound,
                        outbound,
                        window,
                        cx,
                    )
                })
            })
        })?
        .1; // second element is the root entity

    let _ = dispatch;
    Ok(())
}


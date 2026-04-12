
mod devtools_window;
mod network_panel;
mod sources_panel;
mod console_panel;
mod bridge_dispatch;

use anyhow::{bail, Context, Result};
use diatom_bridge::{transport::socket_path, BridgeServer};
use gpui::App;
use std::env;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> Result<()> {
    let diatom_pid = parse_diatom_pid()?;
    let sock = socket_path(diatom_pid);

    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .format_timestamp_millis()
        .init();

    log::info!("[devpanel] starting, pid={}, socket={}", std::process::id(), sock);

    App::new().run(move |cx| {
        devtools_window::init(sock, cx);
    });

    Ok(())
}

fn parse_diatom_pid() -> Result<u32> {
    let mut args = env::args().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--diatom-pid" {
            let val = args
                .next()
                .context("--diatom-pid requires a value")?;
            return val.parse::<u32>().context("--diatom-pid must be a u32");
        }
    }
    bail!("missing --diatom-pid argument; DevPanel must be launched by Diatom");
}


pub mod protocol;
pub mod transport;
pub mod server;
pub mod client;
pub mod slm_adapter;
pub mod zed_link;

pub use protocol::{
    BrowserMessage, DevPanelMessage, RequestId,
    ShellMessage, BackendToShell, ShellTab,    // Shell ↔ Backend direction
    DomNode, ConsoleLevel, NetworkEventPayload,
    SlmMessage, ResonanceContext, ActiveSource,
};
pub use server::BridgeServer;
pub use client::BridgeClient;
pub use zed_link::ZedContextServer;

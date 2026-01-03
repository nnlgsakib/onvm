pub mod content_detector;
mod fetcher;
mod server;
// mod ui;
mod ui_embed;

pub use server::{gateway_router, start_gateway, GatewayConfig};

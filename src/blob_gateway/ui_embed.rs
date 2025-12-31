use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "src/blob_gateway/ui/dist"]
pub struct UiAssets;

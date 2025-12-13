use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    onvm::cli::run().await
}

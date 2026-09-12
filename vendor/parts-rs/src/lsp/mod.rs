pub mod analyzer;
pub mod protocol;
pub mod server;

pub use analyzer::Document;
pub use server::LspServer;

/// Run the LSP language server over stdio.
pub fn run_stdio_server() -> std::io::Result<()> {
    let mut server = LspServer::new();
    server.run_stdio()
}

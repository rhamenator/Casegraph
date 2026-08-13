#![forbid(unsafe_code)]

#[tokio::main]
async fn main() {
    if let Err(error) = casegraph_cli::run(std::env::args().skip(1).collect()).await {
        eprintln!("casegraph: {error}");
        std::process::exit(1);
    }
}

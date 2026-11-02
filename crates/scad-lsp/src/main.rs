mod analysis;
mod documents;
mod positions;
mod protocol;
mod server;

fn main() -> anyhow::Result<()> {
    server::run(std::io::stdin().lock(), std::io::stdout().lock())
}

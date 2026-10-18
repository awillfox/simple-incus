use simple_incus::config::IncusEndpoint;
use simple_incus::incus::IncusClient;

#[tokio::main]
async fn main() {
    let raw = std::env::var("SI_INCUS").unwrap_or_else(|_| simple_incus::config::DEFAULT_INCUS.into());
    let c = IncusClient::new(IncusEndpoint::parse(&raw).expect("bad SI_INCUS"));
    match c.get::<serde_json::Value>("/1.0").await {
        Ok(v) => println!("incus {}", v["environment"]["server_version"]),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

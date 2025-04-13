pub fn build_client(proxy_address: Option<String>) -> reqwest::Client {
    let builder = reqwest::ClientBuilder::new();
    match proxy_address {
        Some(proxy) => builder
            .proxy(reqwest::Proxy::all(proxy).unwrap_or_else(|e| panic!("Invalid proxy URL: {e}")))
            .build()
            .expect("Client builder with proxy failed"),
        None => builder.build().expect("Client builder failed"),
    }
}

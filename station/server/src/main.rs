use station_server::Server;
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let content = args
        .windows(2)
        .find(|a| a[0] == "--content")
        .map_or_else(
            || Ok(station_server::Content::test_station()),
            |a| station_server::Content::load(&a[1]),
        )
        .expect("load station content");
    if let Some(address) = args.windows(2).find(|a| a[0] == "--listen").map(|a| &a[1]) {
        let host = station_server::net::Host::bind_content(address, 7, content.clone())
            .expect("bind station server");
        println!("Station server: {}", host.address);
        loop {
            std::thread::park();
        }
    }
    let ticks = args
        .windows(2)
        .find(|a| a[0] == "--ticks")
        .map(|a| a[1].parse::<u64>().expect("--ticks: integer"))
        .unwrap_or(600);
    let mut server = Server::with_content(7, content);
    for tick in 0..ticks {
        if tick == 240 && args.iter().any(|a| a == "--demo-hole") {
            server.open_floor();
        }
        server.tick([0.0, 0.0]);
    }
    println!("{:?}", server.snapshot());
}

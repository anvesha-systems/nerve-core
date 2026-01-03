use tracing::info;

fn main()-> std::io::Result<()> {
    tracing_subscriber::fmt::init();

    let socket_path = "/tmp/nerve.sock";
    info!("starting NERVE core");

    nerve_core::server::run_server(socket_path)
}
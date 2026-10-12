use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use wiiu_cemu::{write_profile, WiimoteExtension, WiimoteProfile};
use wiiu_connect::ServerConfig;

#[derive(Parser)]
#[command(
    name = "wiiu-connect",
    version,
    about = "Use phones as Wii U GamePad / Wii Remotes in Cemu"
)]
#[command(args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// Options for `serve` when no subcommand is given.
    #[command(flatten)]
    serve: ServeArgs,
}

#[derive(Subcommand)]
enum Command {
    /// Run the controller server (default).
    Serve(ServeArgs),
    /// Write Cemu controller profiles that bind Wii Remotes to Wii U Connect's DSU slots.
    CemuProfile(ProfileArgs),
    /// Print live DSU data, like Cemu would receive it (for testing without Cemu).
    Probe(ProbeArgs),
}

#[derive(clap::Args)]
struct ServeArgs {
    /// HTTP/WebSocket address phones connect to.
    #[arg(long, default_value = "0.0.0.0:8737")]
    bind: SocketAddr,
    /// DSU (Cemuhook) address Cemu connects to.
    #[arg(long, default_value = "127.0.0.1:26760")]
    dsu: SocketAddr,
    /// Fixed pairing token (min. 16 chars). Random per start when omitted.
    #[arg(long, env = "WIIU_CONNECT_TOKEN")]
    token: Option<String>,
    /// Directory with the built phone app.
    #[arg(long, default_value = "apps/phone/dist")]
    web_dir: PathBuf,
    /// Additional allowed WebSocket origin, e.g. http://localhost:5173 for the Vite dev server.
    #[arg(long = "allow-origin")]
    allowed_origins: Vec<String>,
    /// Host name or IP that phones should use (default: detected LAN IP).
    #[arg(long)]
    public_host: Option<String>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Extension {
    None,
    Nunchuk,
    MotionPlus,
    MotionPlusNunchuk,
}

#[derive(clap::Args)]
struct ProfileArgs {
    /// Cemu's controllerProfiles directory. Defaults to the first existing standard location.
    #[arg(long)]
    dir: Option<PathBuf>,
    /// Folder containing Cemu.exe (used to find portable installs).
    #[arg(long)]
    cemu_dir: Option<PathBuf>,
    /// Wii Remote players to write (1-3).
    #[arg(long, value_delimiter = ',', default_value = "1,2,3")]
    players: Vec<u8>,
    #[arg(long, value_enum, default_value = "none")]
    extension: Extension,
    #[arg(long, default_value_t = wiiu_dsu::DEFAULT_PORT)]
    dsu_port: u16,
    /// Overwrite existing profiles with the same name.
    #[arg(long)]
    force: bool,
}

#[derive(clap::Args)]
struct ProbeArgs {
    #[arg(long, default_value = "127.0.0.1:26760")]
    dsu: SocketAddr,
    #[arg(long, default_value_t = 1)]
    slot: u8,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,wiiu_dsu=info".into()),
        )
        .init();
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Serve(cli.serve)) {
        Command::Serve(args) => serve(args).await,
        Command::CemuProfile(args) => cemu_profile(args),
        Command::Probe(args) => probe(args).await,
    }
}

async fn serve(args: ServeArgs) -> anyhow::Result<()> {
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let web_dir = args
        .web_dir
        .join("index.html")
        .is_file()
        .then_some(args.web_dir.clone());
    if web_dir.is_none() {
        tracing::warn!(
            "phone app not found in {} – build apps/phone or pass --web-dir",
            args.web_dir.display()
        );
    }
    let (running, task) = wiiu_connect::start(
        ServerConfig {
            bind: args.bind,
            dsu_bind: args.dsu,
            token: args.token,
            web_dir,
            allowed_origins: args.allowed_origins,
            public_host: args.public_host,
        },
        stop_rx,
    )
    .await?;

    let local = format!("http://localhost:{}", running.http_addr.port());
    println!();
    println!("  Wii U Connect is running");
    println!("  ─────────────────────────");
    println!("  Pair phones:        {local}/pair   (open on this PC)");
    println!("  Phone link:         {}", running.phone_url);
    println!("  Mouse pointer test: {local}/dev/pointer");
    println!("  Cemu DSU server:    {}", running.dsu_addr);
    println!();
    if let Ok(code) = qrcode::QrCode::new(running.phone_url.as_bytes()) {
        let qr = code
            .render::<qrcode::render::unicode::Dense1x2>()
            .quiet_zone(true)
            .build();
        println!("{qr}");
    }

    tokio::signal::ctrl_c().await.context("waiting for Ctrl+C")?;
    tracing::info!("shutting down");
    let _ = stop_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(3), task).await;
    Ok(())
}

fn cemu_profile(args: ProfileArgs) -> anyhow::Result<()> {
    let dir = match args.dir {
        Some(d) => d,
        None => wiiu_cemu::profile_dir_candidates(args.cemu_dir.as_deref())
            .into_iter()
            .find(|d| d.is_dir())
            .context("could not find Cemu's controllerProfiles folder; pass --dir")?,
    };
    for player in args.players {
        anyhow::ensure!((1..=3).contains(&player), "players must be 1, 2 or 3");
        let mut profile = WiimoteProfile::for_player(player);
        profile.dsu_port = args.dsu_port;
        profile.extension = match args.extension {
            Extension::None => WiimoteExtension::None,
            Extension::Nunchuk => WiimoteExtension::Nunchuk,
            Extension::MotionPlus => WiimoteExtension::MotionPlus,
            Extension::MotionPlusNunchuk => WiimoteExtension::MotionPlusNunchuk,
        };
        let path = write_profile(&dir, &profile, args.force)?;
        println!("wrote {}", path.display());
    }
    println!("In Cemu: Options → Input settings → pick Controller 2–4 → load profile WiiUConnect_WiimoteN.");
    Ok(())
}

async fn probe(args: ProbeArgs) -> anyhow::Result<()> {
    let probe = wiiu_dsu::probe::DsuProbe::connect(args.dsu).await?;
    println!("watching DSU slot {} on {} (Ctrl+C to stop)", args.slot, args.dsu);
    let mut last = None;
    loop {
        let pad = probe.wait_for_pad(args.slot, Duration::from_secs(5), |_| true).await?;
        if last != Some(pad) {
            let touch = pad.touch[0]
                .map(|t| format!("({:4}, {:3})", t.x, t.y))
                .unwrap_or("off".into());
            println!(
                "buttons {:016b}  touch {touch}  stick {:3},{:3}  motion ts {}",
                pad.buttons, pad.left_stick[0], pad.left_stick[1], pad.motion.timestamp_us
            );
            last = Some(pad);
        }
    }
}

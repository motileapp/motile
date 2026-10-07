use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use motile_protocol::DEFAULT_AUTH_URL;
use motile_protocol::auth_client::AuthClient;
use motile_protocol::identity::is_public_key;
use motile_server::access::{Access, AccountSource};
use motile_server::agents::environment::Environment;
use motile_server::config::DataDir;
use motile_server::hub::{Hub, LIMITS_CHECK, PULL_REQUEST_FRESH, PULL_REQUEST_WATCH};
use motile_server::serve::{BindOptions, Server, bind};
use motile_server::store::Store;
use motile_server::{pricing, service, setup, update};

/// Runs coding agents on this machine for the Motile clients.
#[derive(Parser)]
#[command(name = "motile", version)]
struct Cli {
    /// Where the device key, the account and the threads are kept.
    #[arg(long, global = true, env = "MOTILE_DATA_DIR")]
    data_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand)]
enum CliCommand {
    /// Checks for agents, links this machine to your account and starts the service.
    Setup {
        /// The code from the install command the client shows. Not needed once linked.
        token: Option<String>,
        #[arg(long, env = "MOTILE_AUTH_URL", default_value = DEFAULT_AUTH_URL)]
        auth_url: String,
        /// How the server appears in the client. Defaults to the hostname.
        #[arg(long)]
        name: Option<String>,
        /// Ask nothing.
        #[arg(long, short = 'y')]
        yes: bool,
        /// Link the machine but don't install the service.
        #[arg(long)]
        no_service: bool,
    },
    /// Serves the account's clients until stopped. The service runs this.
    Run {
        /// Also accept this device key, account or not.
        #[arg(long = "allow-key")]
        allow_keys: Vec<String>,
        /// Skip relays and address lookup; clients must be given this machine's address.
        #[arg(long)]
        local: bool,
        #[arg(long)]
        port: Option<u16>,
    },
    /// Shows this server's key, account, agents and service.
    Status,
    /// Follows the service's log.
    Logs,
    /// Stops and removes the service.
    Uninstall,
    #[command(hide = true)]
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },
}

#[derive(Subcommand)]
enum ServiceCommand {
    /// Run by `setup` through sudo on Linux.
    Install {
        #[arg(long)]
        user: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_ansi(std::io::stdout().is_terminal())
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,iroh=warn".into()),
        )
        .init();

    let cli = Cli::parse();
    if !matches!(cli.command, CliCommand::Run { .. }) {
        // Piped into `head`, a command should end quietly rather than panic on the closed pipe.
        unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    }
    match cli.command {
        CliCommand::Setup { token, auth_url, name, yes, no_service } => {
            let options = setup::Options { token, auth_url, name, assume_defaults: yes, skip_service: no_service };
            setup::setup(&DataDir::new(cli.data_dir)?, options).await
        }
        CliCommand::Run { allow_keys, local, port } => {
            run(&DataDir::new(cli.data_dir)?, allow_keys, BindOptions { local_only: local, port }).await
        }
        CliCommand::Status => status(&DataDir::new(cli.data_dir)?).await,
        CliCommand::Logs => service::logs(),
        CliCommand::Uninstall => service::uninstall(),
        CliCommand::Service { command: ServiceCommand::Install { user } } => {
            let data_dir = cli.data_dir.context("--data-dir is required.")?;
            service::install_unit(&std::env::current_exe()?, &data_dir, &user)
        }
    }
}

async fn run(data_dir: &DataDir, allow_keys: Vec<String>, options: BindOptions) -> anyhow::Result<()> {
    if let Some(key) = allow_keys.iter().find(|key| !is_public_key(key)) {
        bail!("{key} isn't a device key.");
    }
    let key = data_dir.device_key()?;
    let account = match data_dir.account() {
        Some(account) => Some(AccountSource {
            auth_url: account.auth_url,
            key: data_dir.device_key()?,
            cache: data_dir.account_keys(),
        }),
        None => None,
    };
    if account.is_none() && allow_keys.is_empty() {
        tracing::warn!("this server isn't linked to an account, so no client may connect; run `motile setup <code>`");
    }
    let access = Access::new(allow_keys, account);
    access.keep_fresh();

    let environment = Environment::resolve().await;
    for agent in environment.agents() {
        match agent.version {
            Some(version) => tracing::info!("{:?}: {version}", agent.agent),
            None => tracing::warn!("{:?} isn't installed", agent.agent),
        }
    }
    let store = Store::open(&data_dir.database()).context("The thread database can't be opened.")?;
    let hub = Hub::new(
        store,
        data_dir.media(),
        data_dir.attachments(),
        data_dir.worktrees(),
        data_dir.no_project(),
        environment,
    )?;
    hub.keep_uploads_swept();
    hub.keep_pull_requests_current(PULL_REQUEST_FRESH);
    hub.watch_pull_requests(PULL_REQUEST_WATCH);
    hub.keep_limits_continued(LIMITS_CHECK);
    hub.continue_interrupted();
    hub.refresh_limits();
    hub.keep_prices_current(std::env::var("MOTILE_PRICES_URL").unwrap_or_else(|_| pricing::LIST_URL.to_string()));
    let endpoint = bind(&key, &options).await?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), key = key.public(), "serving");
    if options.local_only {
        tracing::info!("local only, at {:?}", endpoint.bound_sockets());
    }

    let server = Server { hub: hub.clone(), access, attachments: data_dir.attachments() };
    tokio::select! {
        _ = server.run(endpoint.clone()) => {}
        _ = tokio::signal::ctrl_c() => hub.close(false).await,
        _ = terminated() => hub.close(false).await,
        program = update::restart_requested() => {
            // Hanging up first tells the clients to dial again, which the new server answers.
            endpoint.close().await;
            update::restart(&program)
        }
    }
    endpoint.close().await;
    Ok(())
}

async fn terminated() {
    let Ok(mut signal) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) else {
        return std::future::pending().await;
    };
    signal.recv().await;
}

async fn status(data_dir: &DataDir) -> anyhow::Result<()> {
    let device_key = data_dir.device_key()?;
    println!("Key:       {}", device_key.public());
    match data_dir.account() {
        Some(account) => {
            println!("Account:   {} at {}", account.email, account.auth_url);
            // The client can remove this server; only the auth server knows.
            match AuthClient::new(&account.auth_url).me(&device_key).await {
                Ok(me) if me.user.is_some() => {}
                Ok(_) => println!("           It no longer lists this server. Link it again from a client."),
                Err(error) => println!("           Couldn't check with it: {error:#}"),
            }
        }
        None => println!("Account:   not linked"),
    }
    for agent in Environment::resolve().await.agents() {
        let version = agent.version.unwrap_or_else(|| "not installed".to_string());
        println!("{:<10} {version}", format!("{:?}:", agent.agent));
    }
    println!("Service:   {}", service::state());
    println!("Data:      {}", data_dir.path().display());
    Ok(())
}

mod install;
mod launch;
mod mcp;
mod native_host;
mod project;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "passvalet",
    version,
    about = "PassValet — API key valet for vibe coders and their agents",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the MCP server over stdio (used by Cursor, Claude Code, Windsurf…)
    Mcp,
    /// Create `.passvalet.json` in the current project, seeded from .env.example if present
    Init {
        /// Comma-separated service ids to include (e.g. supabase,stripe)
        #[arg(long, value_delimiter = ',')]
        services: Vec<String>,
        /// Project name (defaults to the directory name)
        #[arg(long)]
        name: Option<String>,
        /// Overwrite an existing config
        #[arg(long)]
        force: bool,
    },
    /// Ask for permission and write the project's keys into its env file
    Inject {
        /// Env file to write (defaults to `env_file` in .passvalet.json, then `.env.local`)
        #[arg(long)]
        out: Option<String>,
        /// Session lifetime in seconds
        #[arg(long)]
        ttl: Option<u64>,
        /// Print to stdout instead of writing the file
        #[arg(long)]
        stdout: bool,
    },
    /// Show vault status
    Status,
    /// List stored keys (metadata only)
    List,
    /// Chrome native-messaging host (invoked by the browser, not by users)
    #[command(name = "native-host", hide = true)]
    NativeHost,
    /// Register this binary as the native-messaging host for the PassValet extension
    #[command(name = "install-extension-host")]
    InstallExtensionHost {
        /// Chrome extension id (32 lowercase letters). Repeatable.
        #[arg(long = "extension-id", required = true)]
        extension_ids: Vec<String>,
    },
    /// Print MCP client configuration snippets
    #[command(name = "mcp-config")]
    McpConfig,
}

fn main() -> Result<()> {
    init_tracing();
    // Chrome launches native-messaging hosts as `<binary> chrome-extension://<id>/ [parent-window]`.
    if std::env::args()
        .nth(1)
        .map(|a| a.starts_with("chrome-extension://"))
        .unwrap_or(false)
    {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        return rt.block_on(native_host::run());
    }
    let cli = Cli::parse();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        match cli.command {
            Command::Mcp => mcp::run().await,
            Command::Init {
                services,
                name,
                force,
            } => project::init(services, name, force),
            Command::Inject { out, ttl, stdout } => project::inject(out, ttl, stdout).await,
            Command::Status => project::status().await,
            Command::List => project::list().await,
            Command::NativeHost => native_host::run().await,
            Command::InstallExtensionHost { extension_ids } => install::install_host(extension_ids),
            Command::McpConfig => {
                install::print_mcp_config();
                Ok(())
            }
        }
    })
}

fn init_tracing() {
    // MCP and native-host speak on stdout; keep all logging on stderr.
    let filter = tracing_subscriber::EnvFilter::try_from_env("PASSVALET_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init();
}

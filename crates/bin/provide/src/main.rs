//! `provide`: the service an operator runs beside a ducktape node to answer the provider requests
//! the chain elects that node for, with the node's identity key and the operator's secrets.
mod call;
mod config;
mod serve;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use noded::{Client, Workspace};
use tracing::error;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::serve::Node;

#[derive(Parser)]
#[command(about = "answer the provider requests the chain elects this node for")]
struct Cli {
    #[arg(long, help = "the node's workspace; the ducktape home if omitted")]
    workspace: Option<PathBuf>,
    #[arg(long, help = "the services to provide, a TOML file")]
    config: PathBuf,
}

fn node(cli: &Cli) -> Result<Node, String> {
    let workspace = match &cli.workspace {
        Some(dir) => Workspace::at(dir),
        None => Workspace::at(ducktape_home::root()?),
    };
    let key = workspace.identity().map_err(|error| error.to_string())?;
    let address = workspace.http().map_err(|error| error.to_string())?;
    Ok(Node::new(Client::new(format!("http://{address}")), key))
}

fn config(cli: &Cli) -> Result<Config, String> {
    let text = std::fs::read_to_string(&cli.config)
        .map_err(|error| format!("{}: {error}", cli.config.display()))?;
    Config::parse(&text, |variable| std::env::var(variable).ok())
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();
    let served = match (node(&cli), config(&cli)) {
        (Ok(node), Ok(config)) => serve::run(node, config).await,
        (Err(problem), _) | (_, Err(problem)) => Err(problem),
    };
    match served {
        Ok(()) => ExitCode::SUCCESS,
        Err(problem) => {
            error!(target: "ducktape::provide", event = "stopped", %problem);
            ExitCode::FAILURE
        }
    }
}

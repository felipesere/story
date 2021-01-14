use std::os::unix::fs::PermissionsExt;
use std::thread::{sleep, spawn};
use std::time::Duration;

use anyhow::{anyhow, Result};
use async_std::channel::{bounded, Receiver, TryRecvError};
use async_std::fs::{remove_file, File};
use async_std::prelude::*;
use async_trait::async_trait;
use clap::AppSettings::*;
use clap::Clap;
use dialoguer::{theme::ColorfulTheme, Confirm, Select};
use directories_next::UserDirs;
use futures::stream::FuturesUnordered;
use indicatif::ProgressBar;
use serde::{Deserialize, Serialize};
use serde_json;

const HOOK_BASH: &str = include_str!("../hook.bash");

#[derive(Clap)]
#[clap(
setting = ColorAlways,
setting = ColorAuto,
setting = ColoredHelp,
setting = DeriveDisplayOrder,
setting = VersionlessSubcommands,
)]
struct Opts {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Clap)]
enum SubCommand {
    #[clap(about = "Select which story you are working on")]
    Select(SelectCmd),

    #[clap(about = "Install the required prepare-message-hook and add an entry to .gitignore")]
    Install(InstallCmd),

    #[clap(about = "Complete the story and remove it from .story")]
    Complete(CompleteCmd),
}

#[async_trait]
trait Run {
    async fn run(self) -> Result<()>;
}

#[derive(Serialize, Deserialize)]
struct Query {
    #[serde(rename = "query_hash[0][condition]")]
    condition: String,
    #[serde(rename = "query_hash[0][operator]")]
    operator: String,
    #[serde(rename = "query_hash[0][value]")]
    value: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct FreshreleaseResponse {
    issues: Vec<Item>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Item {
    key: String,
    title: String,
    position: i32,
}

impl ToString for Item {
    fn to_string(&self) -> String {
        format!("{} - {}", self.key, self.title)
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Token {
    token: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Freshrelease {
    #[serde(flatten)]
    token: Token,
    in_progress: Vec<String>,
    priority: Vec<String>,
    inbox: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    freshrelease: Freshrelease,
}

#[async_std::main]
async fn main() -> Result<()> {
    let o: Opts = Opts::parse();

    o.subcmd.run().await?;

    Ok(())
}

#[async_trait]
impl Run for SubCommand {
    async fn run(self) -> Result<()> {
        use SubCommand::*;
        match self {
            Select(s) => s.run().await,
            Install(s) => s.run().await,
            Complete(s) => s.run().await,
        }
    }
}

#[derive(Clap)]
#[clap(
setting = ColorAlways,
setting = ColoredHelp,
setting = DeriveDisplayOrder,
)]
struct InstallCmd {}

#[async_trait]
impl Run for InstallCmd {
    async fn run(self) -> Result<()> {
        let executable = std::fs::Permissions::from_mode(0o755);

        let create_hook = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Create hook file?")
            .default(true)
            .interact()?;

        if !create_hook {
            return Ok(());
        }

        let mut hook_file = File::create(".git/hooks/prepare-commit-msg").await?;
        hook_file.write_all(HOOK_BASH.as_bytes()).await?;
        hook_file.set_permissions(executable).await?;

        let add_to_gitignore = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Add .story to the gitignore")
            .default(true)
            .interact()?;

        if !add_to_gitignore {
            return Ok(());
        }

        let mut ignore = async_std::fs::OpenOptions::new()
            .append(true)
            .open(".gitignore")
            .await?;
        ignore.write_all(b".story\n").await?;

        Ok(())
    }
}

#[derive(Clap)]
#[clap(
setting = ColorAlways,
setting = ColoredHelp,
setting = DeriveDisplayOrder,
)]
struct CompleteCmd {}

#[async_trait]
impl Run for CompleteCmd {
    async fn run(self) -> Result<()> {
        remove_file(".story").await.map_err(|e| anyhow!(e))
    }
}

#[derive(Clap)]
#[clap(
setting = ColorAlways,
setting = ColoredHelp,
setting = DeriveDisplayOrder,
)]
struct SelectCmd {
    #[clap(
        about = "Select from the inbox column",
        long = "inbox",
        conflicts_with = "priority"
    )]
    inbox: bool,
    #[clap(
        about = "Select from the priority column",
        long = "priority",
        conflicts_with = "inbox"
    )]
    priority: bool,
}

#[async_trait]
impl Run for SelectCmd {
    async fn run(self) -> Result<()> {
        let Config { freshrelease } = read_config();

        let tasks = FuturesUnordered::new();
        let mut ids = freshrelease.in_progress.clone();
        if self.inbox {
            ids = freshrelease.inbox.clone();
        }
        if self.priority {
            ids = freshrelease.priority.clone();
        }

        ids.into_iter()
            .map(|id| team_tasks(&freshrelease.token, id.clone()))
            .for_each(|t| tasks.push(t));

        let (tx, rx) = bounded(1);
        spinner(rx);
        let fs: Vec<Result<FreshreleaseResponse>> = tasks.collect().await;
        tx.send(()).await?;

        let mut issues: Vec<Item> = fs
            .into_iter()
            .flatten()
            .map(|f| f.issues)
            .flatten()
            .collect();

        issues.sort_by(|a, b| a.position.cmp(&b.position).reverse());

        let selection = Select::with_theme(&ColorfulTheme::default())
            .items(&issues)
            .default(0)
            .interact()?;

        let mut story_file = File::create(".story").await?;
        story_file
            .write_all(format!("story_id={}", issues[selection].key).as_bytes())
            .await?;

        Ok(())
    }
}

async fn team_tasks(token: &Token, id: String) -> Result<FreshreleaseResponse> {
    let query = Query {
        condition: "status_id".into(),
        operator: "is".into(),
        value: id,
    };

    let mut req = surf::get("<YOUR INSTANCE>.freshrelease.com/PT/issues").build();
    req.set_query(&query).expect("setting query");
    req.set_header("authorization", format!("Token {}", token.token));
    req.set_header("accept", "application/json");

    let client = surf::Client::new();

    let mut response = client.send(req).await.map_err(|e| anyhow!(e))?;
    response
        .body_json::<FreshreleaseResponse>()
        .await
        .map_err(|e| anyhow!(e))
}

fn spinner(rx: Receiver<()>) {
    spawn(move || {
        let progress = ProgressBar::new_spinner();
        loop {
            match rx.try_recv() {
                Err(TryRecvError::Empty) => {
                    progress.tick();
                    sleep(Duration::new(0, 50000));
                }
                Ok(()) => break,
                e => panic!(e),
            };
        }
    });
}

fn read_config() -> Config {
    let config_path = UserDirs::new()
        .map(|u| u.home_dir().to_owned())
        .unwrap()
        .join("<YOUR CONFIG>");

    let config_content = std::fs::read_to_string(config_path).unwrap();

    serde_json::from_str(&config_content).expect("parse the configuration")
}

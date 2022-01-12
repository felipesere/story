use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::thread::{sleep, spawn};
use std::time::Duration;
use std::{env::current_dir, fs::read_to_string, path::Path};

use anyhow::{anyhow, Result};
use async_std::channel::{bounded, Receiver, TryRecvError};
use async_std::fs::{remove_file, File};
use async_std::prelude::*;
use async_trait::async_trait;
use clap::AppSettings::*;
use clap::Parser;
use colored_json::prelude::*;
use dialoguer::{theme::ColorfulTheme, Confirm, Select};
use directories_next::UserDirs;
use futures::stream::FuturesUnordered;
use git2::Repository;
use indicatif::ProgressBar;
use serde::__private::Formatter;
use serde::{Deserialize, Serialize};
use serde_json;

const HOOK_BASH: &str = include_str!("../hook.bash");

#[derive(Parser)]
#[clap(
color = clap::ColorChoice::Always,
setting = DeriveDisplayOrder,
version = env!("FANCY_VERSION")
)]
struct Opts {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser)]
enum SubCommand {
    #[clap(about = "Select which story you are working on")]
    Select(SelectCmd),

    #[clap(about = "Install the required prepare-message-hook and add an entry to .gitignore")]
    Install(InstallCmd),

    #[clap(about = "Complete the story and remove it from .story")]
    Complete(CompleteCmd),

    #[clap(about = "Show and edit the story config")]
    Config(ConfigCmd),
}

#[async_trait]
trait Run {
    async fn run(self, root: &Path) -> Result<()>;
}

#[derive(Serialize, Deserialize)]
struct Query {
    #[serde(rename = "query_hash[0][condition]")]
    condition: &'static str,
    #[serde(rename = "query_hash[0][operator]")]
    operator: &'static str,
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

#[derive(Serialize, Deserialize, Debug, Default)]
struct Token {
    token: String,
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.token)
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct TeamConfig {
    short_code: String,
    in_progress: String,
    priority: String,
    inbox: String,
}

#[derive(Clone, Copy)]
enum Column {
    InProgress,
    Priority,
    Inbox,
}

impl TeamConfig {
    async fn get_tasks(&self, fresh: &Freshrelease, column: Column) -> Result<Vec<Item>> {
        use Column::*;
        let id = match column {
            InProgress => self.in_progress.clone(),
            Priority => self.priority.clone(),
            Inbox => self.inbox.clone(),
        };

        let url = format!("{}/{}/issues", fresh.base_url, self.short_code);

        let response = team_tasks(url.clone(), &fresh.token, id).await?;

        Ok(response.issues)
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct Freshrelease {
    base_url: String,
    #[serde(flatten)]
    token: Token,
    teams: Vec<TeamConfig>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct Config {
    freshrelease: Freshrelease,
}

#[async_std::main]
async fn main() -> Result<()> {
    ctrlc::set_handler(move || {
        println!("\x1b[?25h") // reset the terminal
    })?;

    let cwd = current_dir()?;
    let repo = Repository::discover(&cwd).map_err(|_| anyhow!("Not in a git repo!"))?;
    let root = repo
        .path()
        .parent()
        .expect("Unable to step out of the .git folder");

    let o: Opts = Opts::parse();

    o.subcmd.run(root).await?;

    Ok(())
}

#[async_trait]
impl Run for SubCommand {
    async fn run(self, root: &Path) -> Result<()> {
        use SubCommand::*;
        match self {
            Select(s) => s.run(root).await,
            Install(s) => s.run(root).await,
            Complete(s) => s.run(root).await,
            Config(s) => s.run(root).await,
        }
    }
}

#[derive(Parser)]
#[clap(
color = clap::ColorChoice::Always,
setting = DeriveDisplayOrder,
)]
struct InstallCmd {}

#[async_trait]
impl Run for InstallCmd {
    async fn run(self, root: &Path) -> Result<()> {
        let executable = std::fs::Permissions::from_mode(0o755);

        let create_hook = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Create hook file?")
            .default(true)
            .interact()?;

        if !create_hook {
            return Ok(());
        }

        let mut hook_file = File::create(root.join(".git/hooks/prepare-commit-msg")).await?;
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
            .open(root.join(".gitignore"))
            .await?;
        ignore.write_all(b".story\n").await?;

        Ok(())
    }
}

#[derive(Parser)]
#[clap(
color = clap::ColorChoice::Always,
setting = DeriveDisplayOrder,
)]
struct CompleteCmd {}

#[async_trait]
impl Run for CompleteCmd {
    async fn run(self, root: &Path) -> Result<()> {
        remove_file(root.join(".story"))
            .await
            .map_err(|e| anyhow!(e))
    }
}

#[derive(Parser)]
#[clap(
color = clap::ColorChoice::Always,
setting = DeriveDisplayOrder,
)]
struct ConfigCmd {
    /// Edit the configuration
    #[clap(long = "edit")]
    edit: bool,
}

#[async_trait]
impl Run for ConfigCmd {
    async fn run(self, _root: &Path) -> Result<()> {
        let c = config_path();

        if std::fs::metadata(&c).is_err() {
            let create_config = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt("We didn't find a config. Should we create one now?")
                .default(true)
                .interact()?;

            if create_config {
                let d = Config::default();
                let json = serde_json::to_string_pretty(&d)?;
                std::fs::write(&c, json)?;
            } else {
                return Ok(());
            }
        }

        if self.edit {
            let editor = edit::get_editor().expect("Couldn't get an editor");
            let mut h = Command::new(editor)
                .arg(c)
                .spawn()
                .expect("Couldn't run editor");
            h.wait()?;
        } else {
            let f = std::fs::read_to_string(c)?;
            println!("{}", f.to_colored_json_auto()?);
        }

        Ok(())
    }
}

#[derive(Parser)]
#[clap(
setting = DeriveDisplayOrder,
color = clap::ColorChoice::Always,
)]
struct SelectCmd {
    /// Select from the inbox column
    #[clap(
        long = "inbox",
        conflicts_with = "priority"
    )]
    inbox: bool,

    /// Select from the priority column
    #[clap(
        long = "priority",
        conflicts_with = "inbox"
    )]
    priority: bool,
}

#[async_trait]
impl Run for SelectCmd {
    async fn run(self, root: &Path) -> Result<()> {
        let Config { freshrelease } = read_config()?;

        let column = if self.inbox {
            Column::Inbox
        } else if self.priority {
            Column::Priority
        } else {
            Column::InProgress
        };

        let tasks = FuturesUnordered::new();
        for team in freshrelease.teams.iter() {
            tasks.push(team.get_tasks(&freshrelease, column));
        }

        let (tx, rx) = bounded(1);
        spinner(rx);
        let fs: Vec<Result<Vec<Item>>> = tasks.collect().await;
        tx.send(()).await?;

        let mut issues: Vec<Item> = fs.into_iter().flatten().flatten().collect();

        issues.sort_by(|a, b| a.position.cmp(&b.position).reverse());

        let selection = Select::with_theme(&ColorfulTheme::default())
            .items(&issues)
            .default(0)
            .interact()?;

        let mut story_file = File::create(root.join(".story")).await?;
        story_file
            .write_all(format!("story_id={}", issues[selection].key).as_bytes())
            .await?;

        Ok(())
    }
}

async fn team_tasks(url: String, token: &Token, id: String) -> Result<FreshreleaseResponse> {
    let query = Query {
        condition: "status_id",
        operator: "is",
        value: id,
    };

    let mut req = surf::get(url).build();
    req.set_query(&query).expect("setting query");
    req.set_header("authorization", format!("Token {}", token));
    req.set_header("accept", "application/json");

    let mut response = surf::Client::new()
        .send(req)
        .await
        .map_err(|e| anyhow!(e))?;

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

fn read_config() -> Result<Config> {
    let p = config_path();
    read_to_string(p)
        .map_err(|e| anyhow!(e))
        .and_then(|content| serde_json::from_str(&content).map_err(|e| anyhow!(e)))
}

fn config_path() -> PathBuf {
    UserDirs::new()
        .map(|u| u.home_dir().to_owned().join(".story.json"))
        .expect("Could not find path to home")
}

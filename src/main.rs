use std::fmt::Formatter;
use std::fs::{metadata, write, Permissions};
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
use git2::Repository;
use indicatif::ProgressBar;
use jira::JiraConfig;
use serde::{Deserialize, Serialize};

mod jira;

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
pub enum Column {
    Todo,
    InProgress,
    Done,
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct Config {
    jira: JiraConfig,
}

#[async_std::main]
async fn main() -> Result<()> {
    ctrlc::set_handler(move || {
        println!("\x1b[?25h") // reset the terminal
    })?;

    env_logger::init();

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
        let create_hook = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Create hook file?")
            .default(true)
            .interact()?;

        if !create_hook {
            return Ok(());
        }

        let mut hook_file = File::create(root.join(".git/hooks/prepare-commit-msg")).await?;
        hook_file.write_all(HOOK_BASH.as_bytes()).await?;
        hook_file
            .set_permissions(Permissions::from_mode(0o755))
            .await?;

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
        let config_path = config_path();

        if metadata(&config_path).is_err() {
            let create_config = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt("We didn't find a config. Should we create one now?")
                .default(true)
                .interact()?;

            if create_config {
                let d = Config::default();
                let json = serde_json::to_string_pretty(&d)?;
                write(&config_path, json)?;
            } else {
                return Ok(());
            }
        }

        if self.edit {
            let editor = edit::get_editor().expect("Couldn't get an editor");
            let mut h = Command::new(editor)
                .arg(config_path)
                .spawn()
                .expect("Couldn't run editor");
            h.wait()?;
        } else {
            let f = std::fs::read_to_string(config_path)?;
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
    #[clap(long = "todo", conflicts_with = "done")]
    todo: bool,

    /// Select from the priority column
    #[clap(long = "done", conflicts_with = "todo")]
    done: bool,
}

#[async_trait]
impl Run for SelectCmd {
    async fn run(self, root: &Path) -> Result<()> {
        let Config { jira, .. } = read_config()?;

        let column = if self.todo {
            Column::Todo
        } else if self.done {
            Column::Done
        } else {
            Column::InProgress
        };

        let (tx, rx) = bounded(1);
        spinner(rx);
        let tasks = jira.get_matching_tasks(column).await?;
        tx.send(()).await?;

        let selection = Select::with_theme(&ColorfulTheme::default())
            .items(&tasks)
            .default(0)
            .interact()?;

        let mut story_file = File::create(root.join(".story")).await?;
        story_file
            .write_all(format!("story_id={}", tasks[selection].key).as_bytes())
            .await?;

        Ok(())
    }
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
                e => panic!("{:?}", e),
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

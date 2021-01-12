use std::thread::{sleep, spawn};
use std::time::Duration;

use anyhow::{anyhow, Result};
use async_std::channel::{bounded, Receiver, TryRecvError};
use async_std::prelude::*;
use async_trait::async_trait;
use clap::Clap;
use dialoguer::{console::Term, theme::ColorfulTheme, Select};
use directories_next::UserDirs;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use indicatif::ProgressBar;
use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Clap)]
struct Opts {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Clap)]
enum SubCommand {
    Select(SelectCmd),
}

#[async_trait]
impl Run for SubCommand {
    async fn run(self) -> Result<()> {
        use SubCommand::*;
        match self {
            Select(s) => s.run().await,
        }
    }
}

#[derive(Clap)]
struct SelectCmd {}

#[async_trait]
impl Run for SelectCmd {
    async fn run(self) -> Result<()> {
        let config = read_config();

        let ids = vec!["2000000617", "2000002392"];

        let tasks = FuturesUnordered::new();
        ids.iter()
            .map(|id| team_tasks(&config.freshrelease, id))
            .for_each(|t| tasks.push(t));

        let (tx, rx) = bounded(1);
        spinner(rx);
        let fs: Vec<Result<Freshrelease>> = tasks.collect().await;
        tx.send(()).await?;

        let mut issues: Vec<Item> = fs
            .into_iter()
            .flatten()
            .map(|f| f.issues)
            .flatten()
            .collect();
        issues.sort_by(|a, b| a.key.cmp(&b.key));

        let selection = Select::with_theme(&ColorfulTheme::default())
            .items(&issues)
            .default(0)
            .interact_on_opt(&Term::stderr())?;

        match selection {
            Some(index) => println!("selected: {}", issues[index].to_string()),
            None => println!("User did not select anything"),
        };

        Ok(())
    }
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
struct Freshrelease {
    issues: Vec<Item>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Item {
    key: String,
    title: String,
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
struct Config {
    freshrelease: Token,
}

#[async_std::main]
async fn main() -> Result<()> {
    let o: Opts = Opts::parse();

    o.subcmd.run().await?;

    Ok(())
}

async fn team_tasks(token: &Token, id: &str) -> Result<Freshrelease> {
    let query = Query {
        condition: "status_id".into(),
        operator: "is".into(),
        value: id.to_string(),
    };

    let mut req = surf::get("<YOUR INSTANCE>.freshrelease.com/PT/issues").build();
    req.set_query(&query).expect("setting query");
    req.set_header("authorization", format!("Token {}", token.token));
    req.set_header("accept", "application/json");

    let client = surf::Client::new();

    let mut response = client.send(req).await.map_err(|e| anyhow!(e))?;
    response
        .body_json::<Freshrelease>()
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

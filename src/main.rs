use leg::*;
use serde::{Deserialize, Serialize};
use anyhow::{Result, anyhow};
use serde_json;
use dialoguer::{Select, theme::ColorfulTheme, console::Term};
use async_std::prelude::*;
use async_std::future;


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
    issues: Vec<Item>
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
    token: String
}

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    freshrelease: Token
}

#[async_std::main]
async fn main() -> Result<()>  {
    let x = std::fs::read_to_string("<YOUR_CONFIG>").unwrap();


    let config: Config = serde_json::from_str(&x)?;

    let l = tasks(&config.freshrelease, "2000000617");
    let r = tasks(&config.freshrelease, "2000002392");

    let (left, right) = l.join(r).await;

    let mut issues = left?.issues.clone();
    issues.extend(right?.issues);
    issues.sort_by(|a, b| a.key.cmp(&b.key));


    let selection = Select::with_theme(&ColorfulTheme::default())
        .items(&issues)
        .default(0)
        .interact_on_opt(&Term::stderr())?;

    match selection {
        Some(index) => success(&format!("User selected item : {}", issues[index].to_string()), None, None).await,
        None => warn("User did not select anything", None, None).await,
    }

    Ok(())
}

async fn tasks(token: &Token, id: &str) -> Result<Freshrelease> {
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
    response.body_json::<Freshrelease>().await.map_err(|e| anyhow!(e))
}

use leg::*;
use serde::{Deserialize, Serialize};
use anyhow::{Result, anyhow};
use serde_json;


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

#[derive(Serialize, Deserialize, Debug)]
struct Item {
    key: String,
    title: String,
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

    let l = tasks(&config.freshrelease, "2000000617").await;
    let r = tasks(&config.freshrelease, "2000002392").await;

    dbg!(l);
    dbg!(r);

    success("Got the access token", None, None).await;

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

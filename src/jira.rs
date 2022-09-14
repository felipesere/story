use anyhow::Result;
use std::collections::HashMap;
use std::fmt::Display;

use jsonpath::Selector;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Column;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
struct JiraAuth {
    user: String,
    personal_access_token: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(transparent)]
struct Jql(HashMap<String, String>);

impl Jql {
    fn to_query(&self, column: Column) -> String {
        let mut parts: Vec<String> = Vec::new();
        for (k, v) in &self.0 {
            parts.push(format!(r#"{}="{}""#, k, v));
        }
        let status = match column {
            Column::Todo =>  todo!(),
            Column::InProgress => "In Progress",
            Column::Done => "Done",
        };
        parts.push(format!(r#"status="{}""#, status)); 

        parts.join(" and ")
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct JiraConfig {
    base_url: String,
    auth: JiraAuth,
    query: Jql,
}

impl Default for JiraConfig {
    fn default() -> Self {
        JiraConfig {
            base_url: "https://path.to.your.jira.com".into(),
            auth: JiraAuth {
                user: "your user".into(),
                personal_access_token: "your access token".into()
            },
            query: Jql(HashMap::new()),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Task {
    summary: String,
    href: String,
    pub key: String,
}

impl Display for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.key, self.summary)
    }
}

struct Selection {
    summary: Selector,
    href: Selector,
    key: Selector,
}

impl Selection {
    fn extract_from(&self, issue: &Value) -> Option<Task> {
        let summary: String = self.summary.find(issue).next()?.as_str()?.to_string();
        let href: String = self.href.find(issue).next()?.as_str()?.to_string();
        let key: String = self.key.find(issue).next()?.as_str()?.to_string();

        Some(Task { summary, href, key })
    }
}

impl JiraConfig {
    pub async fn get_matching_tasks(&self, column: Column) -> Result<Vec<Task>> {
        #[derive(Serialize)]
        struct Params {
            jql: String,
            #[serde(rename= "maxResults")]
            max_results: usize,
        }

        let params = Params {
            jql: self.query.to_query(column),
            max_results: 50,
        };

        let client = surf::Client::new();

        let credentials = base64::encode(format!("{}:{}", self.auth.user, self.auth.personal_access_token));
        
        let body: Value = client
            .get(&self.base_url)
            .header(
                "Authorization",
                format!("Basic {}", credentials),
            )
            .query(&params)
            .map_err(|e| anyhow::anyhow!(e))?
            .recv_json()
            .await
            .map_err(|e| anyhow::anyhow!(e))?;

        let issues = Selector::new("$.issues")
            .unwrap()
            .find(&body)
            .next()
            .unwrap();

        let selection = Selection {
            summary: Selector::new("$.fields.summary").unwrap(),
            href: Selector::new("$.self").unwrap(),
            key: Selector::new("$.key").unwrap(),
        };

        let mut tasks = Vec::new();

        if let Some(array) = issues.as_array() {
            for issue in array {
                if let Some(task) = selection.extract_from(issue) {
                    tasks.push(task);
                }
            }
        };

        Ok(tasks)
    }
}

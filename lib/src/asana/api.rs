use super::tags::*;
use reqwest::header::{HeaderMap, ACCEPT, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use tokio::sync::RwLock;

pub(super) const BASE_URL: &'static str = "https://app.asana.com/api/1.0";

#[derive(Debug)]
pub struct AsyncClient {
    pub(super) client: reqwest::Client,
    pub(super) workspace: WorkspaceId,
    // Cache for tags
    pub(super) tags: RwLock<HashMap<TagId, Tag>>,
}

pub enum ResourceType {
    Project,
    Task,
    Unknown(String),
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct ProjectId(pub String);

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct TaskId(pub String);

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct WorkspaceId(pub String);

#[derive(Debug, Deserialize, Clone)]
pub struct Project {
    #[serde(rename = "gid")]
    id: ProjectId,
    name: String,
}

// #[derive(Debug, Deserialize, Clone)]
// pub struct CompactTask {
//     #[serde(rename = "gid")]
//     id: TaskId,
//     name: String,
// }

// #[derive(Debug, Deserialize, Clone)]
// struct CompactTasks {
//     data: Vec<CompactTask>,
// }

#[derive(Debug, Deserialize, Clone)]
pub struct Task {
    #[serde(rename = "gid")]
    id: TaskId,
    name: String,
    notes: Option<String>,
    #[serde(rename = "projects")]
    project_ids: Option<Vec<ProjectId>>,
    #[serde(rename = "tags")]
    tag_ids: Option<Vec<TagId>>,
}

#[derive(Debug, Deserialize, Clone)]
struct Tasks {
    data: Vec<Task>,
}

#[derive(Debug, Serialize, Clone)]
pub struct CreateTask {
    name: String,
    notes: Option<String>,
    #[serde(rename = "projects")]
    project_ids: Option<Vec<ProjectId>>,
    #[serde(rename = "tags")]
    tag_ids: Option<Vec<TagId>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ErrorDetail {
    help: Option<String>,
    message: Option<String>,
    phrase: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct ErrorDetails {
    errors: Vec<ErrorDetail>,
}

impl<'de> Deserialize<'de> for ResourceType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "project" => Ok(ResourceType::Project),
            "task" => Ok(ResourceType::Task),
            _ => Ok(ResourceType::Unknown(s)),
        }
    }
}

#[derive(Deserialize)]
pub(super) struct Wrapper<T> {
    pub(super) data: T,
}

#[derive(Debug)]
pub enum Error {
    Api(reqwest::StatusCode, Vec<ErrorDetail>),
    Reqwest(reqwest::Error),
    Serde {
        error: serde_json::Error,
        input: String,
    },
    ResourceNotFound,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Api(code, details) => {
                write!(
                    f,
                    "Asana Client Error (API Error response, HTTP code {}):\n",
                    code
                )?;
                for detail in details {
                    write!(
                        f,
                        "\tAsana error:\n\tMessage: {:?}\n\tHelp: {:?}\n\tPhrase: {:?}\n",
                        detail.message, detail.help, detail.phrase
                    )?;
                }
                Ok(())
            }
            Error::Reqwest(error) => write!(f, "Asana Client Error (Reqwest Error):\n{:?}", error),
            Error::Serde { error, input } => write!(
                f,
                "Asana Client Error (Deserialization Error):\n{:?}\nInput was:\n{}",
                error, input
            ),
            Error::ResourceNotFound => write!(f, "Asana Client Error: Resource not found"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Api(..) => None,
            Error::Reqwest(err) => Some(err),
            Error::Serde { error: err, .. } => Some(err),
            Error::ResourceNotFound => None,
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Error::Reqwest(err)
    }
}

impl AsyncClient {
    pub fn new(access_token: &str, workspace: WorkspaceId) -> AsyncClient {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            format!("Bearer {}", access_token).parse().unwrap(),
        );
        headers.insert(ACCEPT, "application/json".parse().unwrap());
        AsyncClient {
            client: reqwest::Client::builder()
                .default_headers(headers)
                .build()
                .expect("Could not initialize the reqwest Asana client"),
            workspace,
            tags: RwLock::new(HashMap::new()),
        }
    }

    pub async fn get_project_tasks(&self, project_id: &str) -> Result<Vec<Task>, Error> {
        let url = format!("{}/projects/{}/tasks", BASE_URL, project_id);
        let res = self
            .client
            .get(&url)
            .query(&[("opt_fields", "notes,tags")])
            .send()
            .await?;
        let tasks: Tasks = Self::try_deserialize(res).await?;
        Ok(tasks.data)
    }

    pub async fn create_task(
        &self,
        projects: &[ProjectId],
        task: &CreateTask,
    ) -> Result<Task, Error> {
        let url = format!("{}/tasks", BASE_URL);
        let payload = json!({ "data": task });
        let res = self
            .client
            .post(&url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(payload.to_string())
            .send()
            .await?;
        let task: Wrapper<Task> = Self::try_deserialize(res).await?;
        Ok(task.data)
    }

    pub(super) async fn try_deserialize<T: serde::de::DeserializeOwned>(
        response: reqwest::Response,
    ) -> Result<T, Error> {
        let status = response.status();
        if status == reqwest::StatusCode::OK {
            let text = response.text().await?;
            let value: T = serde_json::from_str(&text).map_err(|err| Error::Serde {
                error: err,
                input: text,
            })?;
            Ok(value)
        } else {
            // Status code is not OK (200)
            // Try to get an error object from the response
            let text = response.text().await?;
            let error_details: ErrorDetails =
                serde_json::from_str(&text).map_err(|err| Error::Serde {
                    error: err,
                    input: text,
                })?;
            Err(Error::Api(status, error_details.errors))
        }
    }
}

use super::api::*;
use super::tags::Tag;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Deserialize, Clone)]
pub struct Task {
    #[serde(rename = "gid")]
    pub id: TaskId,
    pub name: String,
    pub notes: Option<String>,
    // #[serde(rename = "projects")]
    // pub project_ids: Option<Vec<ProjectId>>,
    #[serde(rename = "tags")]
    pub tag_ids: Option<Vec<Tag>>,
}

#[derive(Debug, Deserialize, Clone)]
struct Tasks {
    data: Vec<Task>,
}

#[derive(Debug, Serialize, Clone)]
pub struct CreateTask {
    pub name: String,
    pub notes: Option<String>,
    #[serde(rename = "projects")]
    pub project_ids: Option<Vec<ProjectId>>,
    #[serde(rename = "tags")]
    pub tag_ids: Option<Vec<TagId>>,
}

impl AsyncClient {
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

    pub async fn create_task(&self, task: &CreateTask) -> Result<Task, Error> {
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
}

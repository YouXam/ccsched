use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Running,
    Done,
    Failed,
    Waiting,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::Running => write!(f, "running"),
            TaskStatus::Done => write!(f, "done"),
            TaskStatus::Failed => write!(f, "failed"),
            TaskStatus::Waiting => write!(f, "waiting"),
        }
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(TaskStatus::Pending),
            "running" => Ok(TaskStatus::Running),
            "done" => Ok(TaskStatus::Done),
            "failed" => Ok(TaskStatus::Failed),
            "waiting" => Ok(TaskStatus::Waiting),
            _ => Err(format!("Invalid task status: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: i64,
    pub name: String,
    pub prompt: String,
    pub cwd: String,
    pub status: TaskStatus,
    pub session_id: Option<String>,
    pub submitted_at: NaiveDateTime,
    pub finished_at: Option<NaiveDateTime>,
    pub output: Option<String>,
    pub resume_at: Option<NaiveDateTime>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDependency {
    pub task_id: i64,
    pub depends_on_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskRequest {
    pub name: String,
    pub prompt: String,
    pub cwd: String,
    pub depends_on: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskResponse {
    pub task_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskListResponse {
    pub tasks: Vec<TaskInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: i64,
    pub name: String,
    pub status: TaskStatus,
    pub session_id: Option<String>,
    pub submitted_at: NaiveDateTime,
    pub finished_at: Option<NaiveDateTime>,
    pub resume_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfoWithPrompt {
    pub id: i64,
    pub name: String,
    pub prompt: String,
    pub status: TaskStatus,
    pub session_id: Option<String>,
    pub submitted_at: NaiveDateTime,
    pub finished_at: Option<NaiveDateTime>,
    pub resume_at: Option<NaiveDateTime>,
}

impl From<Task> for TaskInfo {
    fn from(task: Task) -> Self {
        Self {
            id: task.id,
            name: task.name,
            status: task.status,
            session_id: task.session_id,
            submitted_at: task.submitted_at,
            finished_at: task.finished_at,
            resume_at: task.resume_at,
        }
    }
}

impl From<Task> for TaskInfoWithPrompt {
    fn from(task: Task) -> Self {
        Self {
            id: task.id,
            name: task.name,
            prompt: task.prompt,
            status: task.status,
            session_id: task.session_id,
            submitted_at: task.submitted_at,
            finished_at: task.finished_at,
            resume_at: task.resume_at,
        }
    }
}
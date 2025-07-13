use crate::models::*;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, post, put},
    Router,
};
use ccsched_core::{
    config::Config,
    db::Database,
    scheduler::Scheduler,
};
use serde_json::Value;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{error, info};

#[derive(Clone)]
pub struct ServerState {
    pub db: Arc<Database>,
}

pub async fn start_server(config: Config) -> anyhow::Result<()> {
    let db = Database::new(&config.database_url).await?;
    let mut scheduler = Scheduler::new(db.clone(), config.clone());
    
    let state = ServerState {
        db: Arc::new(db),
    };

    let app = Router::new()
        .route("/submit", post(submit_task))
        .route("/list", get(list_tasks))
        .route("/task/:id", get(get_task_with_prompt))
        .route("/task/:id", delete(delete_task))
        .route("/task/:id/rename", put(rename_task))
        .route("/task/:id/edit", put(edit_task))
        .route("/task/session/:session_id", get(get_task_by_session))
        .with_state(state);

    let bind_address = config.bind_address();
    info!("Starting server on {}", bind_address);

    tokio::spawn(async move {
        if let Err(e) = scheduler.run().await {
            error!("Scheduler error: {}", e);
        }
    });

    let listener = TcpListener::bind(&bind_address).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn submit_task(
    State(state): State<ServerState>,
    Json(request): Json<CreateTaskRequest>,
) -> Result<Json<CreateTaskResponse>, (StatusCode, String)> {
    let db = state.db;

    if let Err(e) = db.validate_dependencies(&request.depends_on).await {
        error!("Invalid dependencies: {}", e);
        return Err((StatusCode::BAD_REQUEST, format!("Invalid dependencies: {e}")));
    }

    if let Err(e) = db.check_circular_dependency(0, &request.depends_on).await {
        error!("Circular dependency detected: {}", e);
        return Err((StatusCode::BAD_REQUEST, format!("Circular dependency detected: {e}")));
    }

    match db
        .create_task(&request.name, &request.prompt, &request.cwd, &request.depends_on)
        .await
    {
        Ok(task_id) => {
            info!("Created task {} with ID {}", request.name, task_id);
            Ok(Json(CreateTaskResponse { task_id }))
        }
        Err(e) => {
            error!("Failed to create task: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create task: {e}")))
        }
    }
}

async fn list_tasks(
    State(state): State<ServerState>,
) -> Result<Json<TaskListResponse>, (StatusCode, String)> {
    let db = state.db;

    match db.list_tasks().await {
        Ok(tasks) => {
            let task_infos: Vec<TaskInfo> = tasks.into_iter().map(TaskInfo::from).collect();
            Ok(Json(TaskListResponse { tasks: task_infos }))
        }
        Err(e) => {
            error!("Failed to list tasks: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to list tasks: {e}")))
        }
    }
}

async fn get_task_with_prompt(
    State(state): State<ServerState>,
    Path(id): Path<i64>,
) -> Result<Json<TaskInfoWithPrompt>, (StatusCode, String)> {
    let db = state.db;

    match db.get_task(id).await {
        Ok(task) => Ok(Json(TaskInfoWithPrompt::from(task))),
        Err(e) => Err((StatusCode::NOT_FOUND, format!("Task not found: {e}"))),
    }
}

async fn get_task_by_session(
    State(state): State<ServerState>,
    Path(session_id): Path<String>,
) -> Result<Json<TaskInfo>, (StatusCode, String)> {
    let db = state.db;

    match db.get_task_by_session_id(&session_id).await {
        Ok(task) => Ok(Json(TaskInfo::from(task))),
        Err(e) => Err((StatusCode::NOT_FOUND, format!("Task not found: {e}"))),
    }
}

async fn delete_task(
    State(state): State<ServerState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, String)> {
    let db = state.db;

    match db.delete_task(id).await {
        Ok(()) => {
            info!("Deleted task {}", id);
            Ok(StatusCode::NO_CONTENT)
        },
        Err(e) => {
            error!("Failed to delete task {}: {}", id, e);
            Err((StatusCode::NOT_FOUND, format!("Failed to delete task: {e}")))
        }
    }
}

async fn rename_task(
    State(state): State<ServerState>,
    Path(id): Path<i64>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, (StatusCode, String)> {
    let db = state.db;
    
    let name = payload.get("name")
        .and_then(|v| v.as_str())
        .ok_or((StatusCode::BAD_REQUEST, "Missing 'name' field".to_string()))?;

    match db.update_task_name(id, name).await {
        Ok(()) => {
            info!("Renamed task {} to '{}'", id, name);
            Ok(StatusCode::OK)
        },
        Err(e) => {
            error!("Failed to rename task {}: {}", id, e);
            Err((StatusCode::NOT_FOUND, format!("Failed to rename task: {e}")))
        }
    }
}

async fn edit_task(
    State(state): State<ServerState>,
    Path(id): Path<i64>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, (StatusCode, String)> {
    let db = state.db;
    
    let prompt = payload.get("prompt")
        .and_then(|v| v.as_str())
        .ok_or((StatusCode::BAD_REQUEST, "Missing 'prompt' field".to_string()))?;

    // Check if task is completed and needs to be reset
    let task = match db.get_task(id).await {
        Ok(task) => task,
        Err(e) => {
            error!("Failed to get task {}: {}", id, e);
            return Err((StatusCode::NOT_FOUND, format!("Task not found: {e}")));
        }
    };

    let update_result = if matches!(task.status, TaskStatus::Done | TaskStatus::Failed) {
        // Task is completed, reset status to pending
        db.update_task_prompt_and_reset_status(id, prompt).await
    } else {
        // Task is not completed, just update prompt
        db.update_task_prompt(id, prompt).await
    };

    match update_result {
        Ok(()) => {
            info!("Updated prompt for task {}", id);
            Ok(StatusCode::OK)
        },
        Err(e) => {
            error!("Failed to update task {} prompt: {}", id, e);
            Err((StatusCode::NOT_FOUND, format!("Failed to update task prompt: {e}")))
        }
    }
}
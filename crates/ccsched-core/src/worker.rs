use crate::config::Config;
use crate::db::Database;
use crate::error::{CcschedError, Result};
use crate::models::{Task, TaskStatus};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::fs::OpenOptions;
use tokio::process::Command;
use tokio::sync::{mpsc, watch};
use tracing::{error, info, warn};

pub struct Worker {
    db: Database,
    config: Config,
    rate_limit_sender: mpsc::Sender<DateTime<Utc>>,
}

impl Worker {
    pub fn new(db: Database, config: Config, rate_limit_sender: mpsc::Sender<DateTime<Utc>>) -> Self {
        Self { db, config, rate_limit_sender }
    }

    pub async fn run(&self, mut task_receiver: mpsc::Receiver<Task>, mut pause_receiver: watch::Receiver<Option<DateTime<Utc>>>) {
        loop {
            tokio::select! {
                task_opt = task_receiver.recv() => {
                    if let Some(task) = task_opt {
                        // Check if we're paused before starting task
                        let current_pause = *pause_receiver.borrow();
                        if let Some(resume_time) = current_pause {
                            let now = Utc::now();
                            if now < resume_time {
                                // We're paused, put task back to pending
                                warn!("Worker is paused, reverting task {} to pending", task.id);
                                if let Err(e) = self.db.update_task_status(
                                    task.id, 
                                    TaskStatus::Pending, 
                                    task.session_id.as_deref(), 
                                    None
                                ).await {
                                    error!("Failed to revert task {} to pending: {}", task.id, e);
                                }
                                continue;
                            }
                        }
                        
                        let task_id = task.id;
                        info!("Starting execution of task {}: {}", task_id, task.name);

                        if let Err(e) = self.execute_task(task).await {
                            error!("Task {} failed: {}", task_id, e);
                            if let Err(update_err) = self
                                .db
                                .update_task_status(task_id, TaskStatus::Failed, None, Some(Utc::now().naive_utc()))
                                .await
                            {
                                error!("Failed to update task {} status: {}", task_id, update_err);
                            }
                        }
                    } else {
                        // Channel closed, exit
                        break;
                    }
                }
                _ = pause_receiver.changed() => {
                    // Pause state changed, will be handled in next iteration
                    continue;
                }
            }
        }
    }

    async fn execute_task(&self, task: Task) -> Result<()> {
        let task_id = task.id;
        
        // Task is already marked as running by the scheduler
        
        let task_log_path = format!("task_{task_id}.jsonl");
        // Remove logs directory creation since we're writing to current directory

        let initial_result = self.run_claude_initial(&task, &task_log_path, task_id).await?;

        // Check for rate limit in initial result
        if let Some(timestamp) = initial_result.rate_limit_timestamp {
            let resume_at_utc = DateTime::from_timestamp(timestamp, 0)
                .unwrap_or_else(|| Utc::now() + chrono::Duration::hours(1));
            let resume_at = resume_at_utc.naive_utc();
            
            info!("Task {} hit rate limit, will resume at {:?}", task_id, resume_at);
            
            // Send global rate limit signal to scheduler
            if let Err(e) = self.rate_limit_sender.send(resume_at_utc).await {
                error!("Failed to send rate limit signal to scheduler: {}", e);
            }
            
            self.db
                .update_task_status_with_resume_at(
                    task_id,
                    TaskStatus::Waiting,
                    initial_result.session_id.as_deref(),
                    None,
                    Some(resume_at),
                )
                .await?;
            return Ok(());
        }

        if initial_result.session_id.is_none() {
            return Err(CcschedError::ClaudeExecution(
                "No session ID found in initial run".to_string(),
            ));
        }

        let session_id = initial_result.session_id.unwrap();

        // Update with session_id
        self.db
            .update_task_status(task_id, TaskStatus::Running, Some(&session_id), None)
            .await?;

        if !initial_result.success {
            self.db
                .update_task_status(
                    task_id,
                    TaskStatus::Failed,
                    None,
                    Some(Utc::now().naive_utc()),
                )
                .await?;
            return Err(CcschedError::ClaudeExecution(
                "Initial Claude execution failed".to_string(),
            ));
        }

        let verification_prompt = format!(
            "{}\n\n如果你确认任务成功，能够正确完成用户的每一个需求，则回复 CLAUDE_CODE_SCHEDULER_SUCCESS；如果其中有的需求没有完成，再继续进行任务；如果你确认因为某些原因，在没有用户干预的情况下无法完成任务，则回复 CLAUDE_CODE_SCHEDULER_FAILED",
            task.prompt
        );

        let mut max_retries = 3;
        let mut current_session_id = session_id;
        loop {
            let verification_result = self
                .run_claude_verification(&task, &current_session_id, &verification_prompt, &task_log_path, task_id)
                .await?;
            
            // Check for rate limit in verification result
            if let Some(timestamp) = verification_result.rate_limit_timestamp {
                let resume_at_utc = DateTime::from_timestamp(timestamp, 0)
                    .unwrap_or_else(|| Utc::now() + chrono::Duration::hours(1));
                let resume_at = resume_at_utc.naive_utc();
                
                info!("Task {} hit rate limit during verification, will resume at {:?}", task_id, resume_at);
                
                // Send global rate limit signal to scheduler
                if let Err(e) = self.rate_limit_sender.send(resume_at_utc).await {
                    error!("Failed to send rate limit signal to scheduler: {}", e);
                }
                
                self.db
                    .update_task_status_with_resume_at(
                        task_id,
                        TaskStatus::Waiting,
                        Some(&current_session_id),
                        None,
                        Some(resume_at),
                    )
                    .await?;
                return Ok(());
            }
            
            // Update session_id if verification returned a new one, but only if the task is not finished
            let is_final_result = verification_result.output.contains("CLAUDE_CODE_SCHEDULER_SUCCESS") 
                || verification_result.output.contains("CLAUDE_CODE_SCHEDULER_FAILED");
            
            if !is_final_result {
                if let Some(new_session_id) = &verification_result.session_id {
                    current_session_id = new_session_id.clone();
                    // Update database with the latest session_id
                    self.db
                        .update_task_status(task_id, TaskStatus::Running, Some(&current_session_id), None)
                        .await?;
                }
            }

            if !verification_result.success {
                self.db
                    .update_task_status(
                        task_id,
                        TaskStatus::Failed,
                        None,
                        Some(Utc::now().naive_utc()),
                    )
                    .await?;
                return Err(CcschedError::ClaudeExecution(
                    "Claude verification execution failed".to_string(),
                ));
            }

            if verification_result
                .output
                .contains("CLAUDE_CODE_SCHEDULER_SUCCESS")
            {
                info!("Task {} completed successfully", task_id);
                self.db
                    .update_task_status(
                        task_id,
                        TaskStatus::Done,
                        Some(&current_session_id),
                        Some(Utc::now().naive_utc()),
                    )
                    .await?;
                return Ok(());
            } else if verification_result
                .output
                .contains("CLAUDE_CODE_SCHEDULER_FAILED")
            {
                info!("Task {} failed as reported by Claude", task_id);
                self.db
                    .update_task_status(
                        task_id,
                        TaskStatus::Failed,
                        Some(&current_session_id),
                        Some(Utc::now().naive_utc()),
                    )
                    .await?;
                return Err(CcschedError::ClaudeExecution(
                    "Task failed as reported by Claude".to_string(),
                ));
            }

            max_retries -= 1;
            if max_retries <= 0 {
                warn!("Task {} exceeded maximum verification retries", task_id);
                self.db
                    .update_task_status(
                        task_id,
                        TaskStatus::Failed,
                        Some(&current_session_id),
                        Some(Utc::now().naive_utc()),
                    )
                    .await?;
                return Err(CcschedError::ClaudeExecution(
                    "Exceeded maximum verification retries".to_string(),
                ));
            }

            info!("Task {} requires additional verification attempts", task_id);
        }
    }

    async fn run_claude_initial(
        &self,
        task: &Task,
        task_log_path: &str,
        task_id: i64,
    ) -> Result<ClaudeResult> {
        self.run_claude_command(task, &task.prompt, None, task_log_path, task_id)
            .await
    }

    async fn run_claude_verification(
        &self,
        task: &Task,
        session_id: &str,
        prompt: &str,
        task_log_path: &str,
        task_id: i64,
    ) -> Result<ClaudeResult> {
        self.run_claude_command(task, prompt, Some(session_id), task_log_path, task_id)
            .await
    }

    async fn run_claude_command(
        &self,
        task: &Task,
        prompt: &str,
        session_id: Option<&str>,
        task_log_path: &str,
        task_id: i64,
    ) -> Result<ClaudeResult> {
        let mut cmd = Command::new(&self.config.claude_path);
        cmd.args([
            "--output-format",
            "stream-json",
            "--verbose",
            "--dangerously-skip-permissions",
        ]);

        if let Some(session_id) = session_id {
            cmd.args(["-r", session_id]);
        }

        info!("Running command: {:?}", cmd);
        cmd.current_dir(&task.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .envs(&self.config.env_vars);

        let mut child = cmd.spawn()?;

        if let Some(stdin) = child.stdin.take() {
            let mut stdin = stdin;
            stdin.write_all(prompt.as_bytes()).await?;
            stdin.shutdown().await?;
        }

        let stdout = child.stdout.take().unwrap();
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        let mut session_id = None;
        let mut last_line = None;
        let mut output_lines = Vec::new();

        let mut log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(task_log_path)
            .await?;

        while let Some(line) = lines.next_line().await? {
            // Write stdout directly to JSONL file without any wrapping
            let log_msg = format!("{}\n", line);
            if let Err(e) = log_file.write_all(log_msg.as_bytes()).await {
                warn!("Failed to write to task log: {}", e);
            } else {
                // Flush immediately to ensure real-time logging
                if let Err(e) = log_file.flush().await {
                    warn!("Failed to flush task log: {}", e);
                }
            }

            if let Ok(json_value) = serde_json::from_str::<Value>(&line) {
                if let Some(sid) = json_value.get("session_id").and_then(|v| v.as_str()) {
                    // Output session_id update to stdout immediately
                    let session_update = json!({
                        "session_id": sid
                    });
                    println!("{}", session_update);
                    
                    // Update database with session_id immediately, regardless of current state
                    if let Err(e) = self.db
                        .update_task_status(task_id, TaskStatus::Running, Some(sid), None)
                        .await
                    {
                        warn!("Failed to update task {} with session_id {}: {}", task_id, sid, e);
                    }
                    
                    if session_id.is_none() {
                        session_id = Some(sid.to_string());
                    }
                }

                if json_value.get("type").and_then(|v| v.as_str()) == Some("result") {
                    last_line = Some(json_value);
                }
            }

            output_lines.push(line);
        }

        let stderr = child.stderr.take().unwrap();
        let stderr_reader = BufReader::new(stderr);
        let mut stderr_lines = stderr_reader.lines();

        while let Some(line) = stderr_lines.next_line().await? {
            // Write stderr directly to JSONL file without any wrapping
            let log_msg = format!("{}\n", line);
            if let Err(e) = log_file.write_all(log_msg.as_bytes()).await {
                warn!("Failed to write to task log: {}", e);
            } else {
                // Flush immediately to ensure real-time logging
                if let Err(e) = log_file.flush().await {
                    warn!("Failed to flush task log: {}", e);
                }
            }
        }

        let exit_status = child.wait().await?;
        let success = exit_status.success()
            && last_line
                .as_ref()
                .and_then(|v| v.get("subtype"))
                .and_then(|v| v.as_str())
                == Some("success")
            && last_line
                .as_ref()
                .and_then(|v| v.get("is_error"))
                .and_then(|v| v.as_bool())
                == Some(false);

        let output = output_lines.join("\n");

        // Check for rate limit error
        let mut rate_limit_timestamp = None;
        if let Some(last) = &last_line {
            if last.get("is_error").and_then(|v| v.as_bool()) == Some(true) {
                if let Some(result) = last.get("result").and_then(|v| v.as_str()) {
                    if result.starts_with("Claude AI usage limit reached|") {
                        if let Some(timestamp_str) = result.strip_prefix("Claude AI usage limit reached|") {
                            if let Ok(timestamp) = timestamp_str.parse::<i64>() {
                                rate_limit_timestamp = Some(timestamp);
                            }
                        }
                    }
                }
            }
        }

        Ok(ClaudeResult {
            success,
            session_id,
            output,
            rate_limit_timestamp,
        })
    }
}

#[derive(Debug)]
struct ClaudeResult {
    success: bool,
    session_id: Option<String>,
    output: String,
    rate_limit_timestamp: Option<i64>,
}
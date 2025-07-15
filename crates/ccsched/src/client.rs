use crate::cli::*;
use crate::models::*;
use anyhow::{anyhow, Result};
use chrono::Utc;
use is_terminal::IsTerminal;
use std::env;
use std::io::{self, Read};
use std::process::Command;
use tracing::{error, info};

pub async fn add_task(args: AddArgs) -> Result<()> {
    // Read the file content as prompt
    let prompt = std::fs::read_to_string(&args.filename)
        .map_err(|e| anyhow!("Failed to read file '{}': {}", args.filename, e))?;

    if prompt.trim().is_empty() {
        return Err(anyhow!("File '{}' is empty", args.filename));
    }

    let cwd = args.cwd.unwrap_or_else(|| {
        env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    });

    let depends_on = if let Some(deps) = &args.depends {
        deps.split(',')
            .map(|s| s.trim().parse::<i64>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("Invalid dependency ID: {}", e))?
    } else {
        Vec::new()
    };

    let request = CreateTaskRequest {
        name: args.filename.clone(), // Use filename as task name
        prompt,
        cwd,
        depends_on,
    };

    let client = reqwest::Client::new();
    let url = format!("http://{}:{}/submit", 
                      args.host.as_ref().unwrap_or(&"localhost".to_string()), 
                      args.port.unwrap_or(39512));

    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await?
        .error_for_status()?;

    let task_response: CreateTaskResponse = response.json().await?;

    println!("Task submitted successfully. Task ID: {}", task_response.task_id);
    Ok(())
}

pub async fn submit_task(args: SubmitArgs) -> Result<()> {
    let prompt = if let Some(prompt_file) = &args.prompt_file {
        // Prompt file was explicitly provided
        std::fs::read_to_string(prompt_file)?
    } else if !io::stdin().is_terminal() {
        // Input is redirected/piped, read directly from stdin
        let mut prompt = String::new();
        io::stdin().read_to_string(&mut prompt)?;
        prompt
    } else {
        // Interactive mode: open editor for user to input prompt
        launch_editor_for_new_prompt()?
    };

    let cwd = args.cwd.unwrap_or_else(|| {
        env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    });

    let depends_on = if let Some(deps) = &args.depends {
        deps.split(',')
            .map(|s| s.trim().parse::<i64>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("Invalid dependency ID: {}", e))?
    } else {
        Vec::new()
    };

    let request = CreateTaskRequest {
        name: args.name.clone(),
        prompt,
        cwd,
        depends_on,
    };

    let client = reqwest::Client::new();
    let url = format!("http://{}:{}/submit", 
                      args.host.as_ref().unwrap_or(&"localhost".to_string()), 
                      args.port.unwrap_or(39512));

    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await?
        .error_for_status()?;

    let task_response: CreateTaskResponse = response.json().await?;

    println!("Task submitted successfully. Task ID: {}", task_response.task_id);
    Ok(())
}

pub async fn list_tasks(args: ListArgs) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("http://{}:{}/list", 
                      args.host.as_ref().unwrap_or(&"localhost".to_string()), 
                      args.port.unwrap_or(39512));

    let response = client.get(&url).send().await?.error_for_status()?;
    let task_list: TaskListResponse = response.json().await?;

    if task_list.tasks.is_empty() {
        println!("No tasks found.");
        return Ok(());
    }

    if args.detail {
        // Detailed view with timestamps and session IDs
        println!("{:<4} {:<25} {:<11} {:<20} {:<20} {:<36}", 
                 "ID", "Name", "Status", "Submitted", "Finished", "Session ID");
        println!("{}", "-".repeat(125));

        for task in &task_list.tasks {
            let finished = task.finished_at
                .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| "-".to_string());

            let session_id = task.session_id.clone().unwrap_or_else(|| "-".to_string());

            println!("{:<4} {:<25} {:<10} {:<20} {:<20} {:<36}",
                     task.id,
                     truncate(&task.name, 25),
                     format_status(&task.status),
                     task.submitted_at.format("%Y-%m-%d %H:%M:%S"),
                     finished,
                     truncate(&session_id, 36));
        }
    } else {
        // Simple view with just ID, name, and status
        println!("{:<4} {:<40} {:<10}", "ID", "Name", "Status");
        println!("{}", "-".repeat(56));

        for task in &task_list.tasks {
            println!("{:<4} {:<40} {:<10}",
                     task.id,
                     truncate(&task.name, 40),
                     format_status(&task.status));
        }
    }

    // Show waiting task information
    let waiting_tasks: Vec<_> = task_list.tasks.iter()
        .filter(|task| matches!(task.status, TaskStatus::Waiting))
        .collect();
    
    if !waiting_tasks.is_empty() {
        println!("\n⚠️  Waiting Tasks Information:");
        for task in waiting_tasks {
            if let Some(resume_at) = task.resume_at {
                let now = Utc::now().naive_utc();
                if resume_at > now {
                    let remaining = resume_at.signed_duration_since(now);
                    println!("   Task {} is waiting due to rate limits, will resume in {} minutes", 
                           task.id, remaining.num_minutes());
                } else {
                    println!("   Task {} is ready to resume (rate limit expired)", task.id);
                }
            } else {
                println!("   Task {} is waiting (reason unknown)", task.id);
            }
        }
    }

    Ok(())
}

pub async fn show_task(args: ShowArgs) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("http://{}:{}/task/{}", 
                      args.host.as_ref().unwrap_or(&"localhost".to_string()), 
                      args.port.unwrap_or(39512), 
                      args.task_id);

    let response = client.get(&url).send().await?.error_for_status()?;
    let task: TaskInfoWithPrompt = response.json().await?;

    println!("Task Details:");
    println!("=============");
    println!("ID: {}", task.id);
    println!("Name: {}", task.name);
    println!("Status: {}", format_status(&task.status));
    println!("Submitted: {}", task.submitted_at.format("%Y-%m-%d %H:%M:%S UTC"));
    
    if let Some(finished) = task.finished_at {
        println!("Finished: {}", finished.format("%Y-%m-%d %H:%M:%S UTC"));
    }
    
    if let Some(session_id) = &task.session_id {
        println!("Session ID: {}", session_id);
    }
    
    if let Some(resume_at) = task.resume_at {
        println!("Resume At: {}", resume_at.format("%Y-%m-%d %H:%M:%S UTC"));
    }
    
    println!("\nPrompt:");
    println!("-------");
    println!("{}", task.prompt);
    
    if let Some(result) = &task.result {
        println!("\nResult:");
        println!("-------");
        println!("{}", result);
    }

    Ok(())
}

pub async fn resume_task(args: ResumeArgs) -> Result<()> {
    if !is_local_host(&args.host.as_ref().unwrap_or(&"localhost".to_string())) {
        return Err(anyhow!("Resume command can only be used with local scheduler instances"));
    }

    let client = reqwest::Client::new();
    
    let task_info = if args.task_or_session_id.parse::<i64>().is_ok() {
        // It's a valid number, treat as task ID
        let task_id: i64 = args.task_or_session_id.parse().unwrap();
        let url = format!("http://{}:{}/task/{}", 
                          args.host.as_ref().unwrap_or(&"localhost".to_string()), 
                          args.port.unwrap_or(39512), 
                          task_id);
        let response = client.get(&url).send().await?.error_for_status()?;
        response.json::<TaskInfo>().await?
    } else {
        // Not a number, treat as session ID
        let url = format!("http://{}:{}/task/session/{}", 
                          args.host.as_ref().unwrap_or(&"localhost".to_string()), 
                          args.port.unwrap_or(39512), 
                          args.task_or_session_id);
        let response = client.get(&url).send().await?.error_for_status()?;
        response.json::<TaskInfo>().await?
    };

    let session_id = task_info.session_id
        .ok_or_else(|| anyhow!("Task has no session ID. Cannot resume."))?;

    info!("Resuming task {} with session ID {} in directory {}", task_info.id, session_id, task_info.cwd);

    let mut cmd = Command::new("claude");
    cmd.arg("-r").arg(&session_id);
    cmd.args(&args.claude_args);
    cmd.current_dir(&task_info.cwd);

    let status = cmd.status()?;
    
    if !status.success() {
        error!("Claude command failed with exit code: {:?}", status.code());
    }

    Ok(())
}

pub async fn delete_task(args: DeleteArgs) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("http://{}:{}/task/{}", 
                      args.host.as_ref().unwrap_or(&"localhost".to_string()), 
                      args.port.unwrap_or(39512), 
                      args.task_id);

    let response = client.delete(&url).send().await?.error_for_status()?;
    
    if response.status().is_success() {
        println!("Task {} deleted successfully.", args.task_id);
    } else {
        return Err(anyhow!("Failed to delete task {}", args.task_id));
    }

    Ok(())
}

pub async fn rename_task(args: RenameArgs) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("http://{}:{}/task/{}/rename", 
                      args.host.as_ref().unwrap_or(&"localhost".to_string()), 
                      args.port.unwrap_or(39512), 
                      args.task_id);

    let request = serde_json::json!({
        "name": args.new_name
    });

    let response = client.put(&url)
        .json(&request)
        .send()
        .await?
        .error_for_status()?;
    
    if response.status().is_success() {
        println!("Task {} renamed to '{}'.", args.task_id, args.new_name);
    } else {
        return Err(anyhow!("Failed to rename task {}", args.task_id));
    }

    Ok(())
}

pub async fn edit_task(args: EditArgs) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("http://{}:{}/task/{}", 
                      args.host.as_ref().unwrap_or(&"localhost".to_string()), 
                      args.port.unwrap_or(39512), 
                      args.task_id);
    let response = client.get(&url).send().await?.error_for_status()?;
    let task: TaskInfoWithPrompt = response.json().await?;
    
    // Check if task is completed and warn user
    if matches!(task.status, TaskStatus::Done | TaskStatus::Failed) {
        println!("⚠️  Warning: Task {} is already completed (status: {}).", task.id, format_status(&task.status));
        println!("Editing this task will reset it to pending status and start a new execution.");
        println!("The task will continue using the previous session ID: {}", 
                task.session_id.as_deref().unwrap_or("(none)"));
        print!("Do you want to continue? (y/N): ");
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        
        if !input.trim().to_lowercase().starts_with('y') {
            println!("Edit cancelled.");
            return Ok(());
        }
    }

    let prompt = if let Some(prompt_file) = &args.prompt_file {
        // Prompt file was explicitly provided
        std::fs::read_to_string(prompt_file)?
    } else if !io::stdin().is_terminal() {
        // Input is redirected/piped, read directly from stdin
        let mut prompt = String::new();
        io::stdin().read_to_string(&mut prompt)?;
        prompt
    } else {
        // Interactive mode: get current prompt and edit it
        launch_editor_for_existing_prompt(&task.prompt)?
    };

    if prompt.is_empty() {
        return Err(anyhow!("Prompt cannot be empty"));
    }

    let client = reqwest::Client::new();
    let url = format!("http://{}:{}/task/{}/edit", 
                      args.host.as_ref().unwrap_or(&"localhost".to_string()), 
                      args.port.unwrap_or(39512), 
                      args.task_id);

    let request = serde_json::json!({
        "prompt": prompt
    });

    let response = client.put(&url)
        .json(&request)
        .send()
        .await?
        .error_for_status()?;
    
    if response.status().is_success() {
        println!("Task {} prompt updated successfully.", args.task_id);
    } else {
        return Err(anyhow!("Failed to update task {} prompt", args.task_id));
    }

    Ok(())
}

fn is_local_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "0.0.0.0")
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

fn format_status(status: &TaskStatus) -> String {
    match status {
        TaskStatus::Pending => "⏳ pending".to_string(),
        TaskStatus::Running => "🔄 running".to_string(),
        TaskStatus::Done => "✅ done".to_string(),
        TaskStatus::Failed => "❌ failed".to_string(),
        TaskStatus::Waiting => "⏸️ waiting".to_string(),
    }
}

fn launch_editor_for_new_prompt() -> Result<String> {
    let initial_content = "<!-- Please write your task prompt below this line. This comment will be automatically removed. -->\n\n";
    
    // Create temporary .md file for better markdown highlighting
    let temp_file = std::env::temp_dir().join(format!("ccsched_prompt_{}.md", std::process::id()));
    std::fs::write(&temp_file, initial_content)?;
    
    // Launch editor and wait for completion
    edit::edit_file(&temp_file)
        .map_err(|e| anyhow!("Failed to launch editor: {}", e))?;
    
    // Read the content back from the file
    let content = std::fs::read_to_string(&temp_file)?;
    
    // Clean up temporary file
    let _ = std::fs::remove_file(&temp_file);
    
    // Remove the initial comment if it's still there
    let content = if content.starts_with("<!-- Please write your task prompt below this line. This comment will be automatically removed. -->") {
        content.lines()
            .skip_while(|line| line.trim().is_empty() || line.trim().starts_with("<!--"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    } else {
        content.trim().to_string()
    };
    
    if content.is_empty() {
        return Err(anyhow!("Prompt cannot be empty"));
    }
    
    Ok(content)
}

fn launch_editor_for_existing_prompt(existing_content: &str) -> Result<String> {
    // Create temporary .md file for better markdown highlighting
    let temp_file = std::env::temp_dir().join(format!("ccsched_prompt_{}.md", std::process::id()));
    std::fs::write(&temp_file, existing_content)?;
    
    // Launch editor and wait for completion
    edit::edit_file(&temp_file)
        .map_err(|e| anyhow!("Failed to launch editor: {}", e))?;
    
    // Read the content back from the file
    let content = std::fs::read_to_string(&temp_file)?;
    
    // Clean up temporary file
    let _ = std::fs::remove_file(&temp_file);
    
    let content = content.trim().to_string();
    
    if content.is_empty() {
        return Err(anyhow!("Prompt cannot be empty"));
    }
    
    Ok(content)
}
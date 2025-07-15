use crate::error::{CcschedError, Result};
use crate::models::{Task, TaskStatus};
use chrono::{NaiveDateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub async fn new(database_url: &str) -> Result<Self> {
        // Extract file path from database URL
        let file_path = if database_url.starts_with("sqlite:") {
            database_url.strip_prefix("sqlite:").unwrap_or(database_url)
        } else {
            database_url
        };

        // Create parent directory if it doesn't exist
        if let Some(parent) = std::path::Path::new(file_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Open database connection
        let conn = Connection::open(file_path)?;
        
        // Run migrations
        Self::run_migrations(&conn)?;
        
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn run_migrations(conn: &Connection) -> Result<()> {
        // Create tasks table
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS tasks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                prompt TEXT NOT NULL,
                cwd TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'done', 'failed', 'waiting')),
                session_id TEXT,
                submitted_at DATETIME NOT NULL DEFAULT (datetime('now', 'utc')),
                finished_at DATETIME,
                output TEXT,
                result TEXT,
                resume_at DATETIME
            )
            "#,
            [],
        )?;

        // Create task_dependencies table
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS task_dependencies (
                task_id INTEGER NOT NULL,
                depends_on_id INTEGER NOT NULL,
                PRIMARY KEY (task_id, depends_on_id),
                FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
                FOREIGN KEY (depends_on_id) REFERENCES tasks(id) ON DELETE CASCADE
            )
            "#,
            [],
        )?;

        // Create indexes for better performance
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tasks_session_id ON tasks(session_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_task_dependencies_task_id ON task_dependencies(task_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_task_dependencies_depends_on_id ON task_dependencies(depends_on_id)",
            [],
        )?;

        // Migration: Add resume_at column if it doesn't exist
        let _ = conn.execute("ALTER TABLE tasks ADD COLUMN resume_at DATETIME", []);
        
        // Migration: Add result column if it doesn't exist
        let _ = conn.execute("ALTER TABLE tasks ADD COLUMN result TEXT", []);

        Ok(())
    }

    pub async fn create_task(
        &self,
        name: &str,
        prompt: &str,
        cwd: &str,
        dependencies: &[i64],
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;

        let status = TaskStatus::Pending.to_string();
        let submitted_at = Utc::now().naive_utc();

        tx.execute(
            "INSERT INTO tasks (name, prompt, cwd, status, submitted_at) VALUES (?, ?, ?, ?, ?)",
            params![name, prompt, cwd, status, submitted_at],
        )?;
        let task_id = tx.last_insert_rowid();

        // Insert dependencies
        for &dep_id in dependencies {
            tx.execute(
                "INSERT INTO task_dependencies (task_id, depends_on_id) VALUES (?, ?)",
                params![task_id, dep_id],
            )?;
        }

        tx.commit()?;
        Ok(task_id)
    }

    pub async fn get_task(&self, id: i64) -> Result<Task> {
        let conn = self.conn.lock().unwrap();
        
        let row = conn.query_row(
            "SELECT id, name, prompt, cwd, status, session_id, submitted_at, finished_at, output, result, resume_at FROM tasks WHERE id = ?",
            params![id],
            |row| {
                Ok(Task {
                    id: row.get("id")?,
                    name: row.get("name")?,
                    prompt: row.get("prompt")?,
                    cwd: row.get("cwd")?,
                    status: TaskStatus::from_str(&row.get::<_, String>("status")?).unwrap_or(TaskStatus::Failed),
                    session_id: row.get("session_id")?,
                    submitted_at: row.get("submitted_at")?,
                    finished_at: row.get("finished_at")?,
                    output: row.get("output")?,
                    result: row.get("result")?,
                    resume_at: row.get("resume_at")?,
                })
            },
        ).optional()?
        .ok_or_else(|| CcschedError::Config(format!("Task not found: {id}")))?;

        Ok(row)
    }

    pub async fn get_task_by_session_id(&self, session_id: &str) -> Result<Task> {
        let conn = self.conn.lock().unwrap();
        
        let row = conn.query_row(
            "SELECT id, name, prompt, cwd, status, session_id, submitted_at, finished_at, output, result, resume_at FROM tasks WHERE session_id = ?",
            params![session_id],
            |row| {
                Ok(Task {
                    id: row.get("id")?,
                    name: row.get("name")?,
                    prompt: row.get("prompt")?,
                    cwd: row.get("cwd")?,
                    status: TaskStatus::from_str(&row.get::<_, String>("status")?).unwrap_or(TaskStatus::Failed),
                    session_id: row.get("session_id")?,
                    submitted_at: row.get("submitted_at")?,
                    finished_at: row.get("finished_at")?,
                    output: row.get("output")?,
                    result: row.get("result")?,
                    resume_at: row.get("resume_at")?,
                })
            },
        ).optional()?
        .ok_or_else(|| CcschedError::Config(format!("Task not found for session_id: {session_id}")))?;

        Ok(row)
    }

    pub async fn list_tasks(&self) -> Result<Vec<Task>> {
        let conn = self.conn.lock().unwrap();
        
        let mut stmt = conn.prepare(
            "SELECT id, name, prompt, cwd, status, session_id, submitted_at, finished_at, output, result, resume_at FROM tasks ORDER BY submitted_at ASC"
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(Task {
                id: row.get("id")?,
                name: row.get("name")?,
                prompt: row.get("prompt")?,
                cwd: row.get("cwd")?,
                status: TaskStatus::from_str(&row.get::<_, String>("status")?).unwrap_or(TaskStatus::Failed),
                session_id: row.get("session_id")?,
                submitted_at: row.get("submitted_at")?,
                finished_at: row.get("finished_at")?,
                output: row.get("output")?,
                result: row.get("result")?,
                resume_at: row.get("resume_at")?,
            })
        })?;

        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(row?);
        }

        Ok(tasks)
    }

    pub async fn update_task_status(
        &self,
        id: i64,
        status: TaskStatus,
        session_id: Option<&str>,
        finished_at: Option<NaiveDateTime>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let status_str = status.to_string();

        conn.execute(
            "UPDATE tasks SET status = ?, session_id = COALESCE(?, session_id), finished_at = COALESCE(?, finished_at) WHERE id = ?",
            params![status_str, session_id, finished_at, id],
        )?;

        Ok(())
    }

    pub async fn update_task_status_with_resume_at(
        &self,
        id: i64,
        status: TaskStatus,
        session_id: Option<&str>,
        finished_at: Option<NaiveDateTime>,
        resume_at: Option<NaiveDateTime>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let status_str = status.to_string();

        conn.execute(
            "UPDATE tasks SET status = ?, session_id = COALESCE(?, session_id), finished_at = COALESCE(?, finished_at), resume_at = ? WHERE id = ?",
            params![status_str, session_id, finished_at, resume_at, id],
        )?;

        Ok(())
    }

    pub async fn update_task_result(&self, id: i64, result: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute(
            "UPDATE tasks SET result = ? WHERE id = ?",
            params![result, id],
        )?;
        
        if updated == 0 {
            return Err(CcschedError::Config(format!("Task not found: {id}")));
        }
        
        Ok(())
    }

    pub async fn update_task_output_and_result(&self, id: i64, output: Option<&str>, result: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute(
            "UPDATE tasks SET output = ?, result = ? WHERE id = ?",
            params![output, result, id],
        )?;
        
        if updated == 0 {
            return Err(CcschedError::Config(format!("Task not found: {id}")));
        }
        
        Ok(())
    }

    pub async fn get_and_claim_next_task(&self) -> Result<Option<Task>> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        
        // First check if there's already a running task that's not in waiting state
        // We allow waiting tasks to be resumed even if there are other running tasks
        let running_count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM tasks WHERE status = 'running'",
            [],
            |row| row.get(0)
        )?;
        
        // If there's already a running task, only allow waiting tasks to be resumed
        let allow_only_waiting = running_count > 0;
        
        // Find the next ready task and claim it atomically
        let status_condition = if allow_only_waiting {
            "(t.status = 'waiting' AND (t.resume_at IS NULL OR t.resume_at <= datetime('now', 'utc')))"
        } else {
            "(t.status = 'pending' OR (t.status = 'waiting' AND (t.resume_at IS NULL OR t.resume_at <= datetime('now', 'utc'))))"
        };
        
        let query = format!(
            r#"
            SELECT DISTINCT t.id, t.name, t.prompt, t.cwd, t.status, t.session_id, t.submitted_at, t.finished_at, t.output, t.result, t.resume_at
            FROM tasks t
            LEFT JOIN task_dependencies td ON t.id = td.task_id
            LEFT JOIN tasks dep ON td.depends_on_id = dep.id
            WHERE {}
            GROUP BY t.id, t.name, t.prompt, t.cwd, t.status, t.session_id, t.submitted_at, t.finished_at, t.output, t.result, t.resume_at
            HAVING COUNT(CASE WHEN dep.status IS NOT NULL AND dep.status != 'done' THEN 1 END) = 0
            ORDER BY t.submitted_at ASC
            LIMIT 1
            "#,
            status_condition
        );
        
        let task_opt = tx.query_row(
            &query,
            [],
            |row| {
                Ok(Task {
                    id: row.get("id")?,
                    name: row.get("name")?,
                    prompt: row.get("prompt")?,
                    cwd: row.get("cwd")?,
                    status: TaskStatus::from_str(&row.get::<_, String>("status")?).unwrap_or(TaskStatus::Failed),
                    session_id: row.get("session_id")?,
                    submitted_at: row.get("submitted_at")?,
                    finished_at: row.get("finished_at")?,
                    output: row.get("output")?,
                    result: row.get("result")?,
                    resume_at: row.get("resume_at")?,
                })
            }
        ).optional()?;
        
        if let Some(task) = task_opt {
            // Atomically claim this task by marking it as running
            let updated = tx.execute(
                "UPDATE tasks SET status = 'running' WHERE id = ? AND status IN ('pending', 'waiting')",
                params![task.id]
            )?;
            
            if updated == 1 {
                tx.commit()?;
                // Return the task with updated status
                let mut claimed_task = task;
                claimed_task.status = TaskStatus::Running;
                Ok(Some(claimed_task))
            } else {
                // Another process claimed this task, rollback
                tx.rollback()?;
                Ok(None)
            }
        } else {
            tx.commit()?;
            Ok(None)
        }
    }

    pub async fn get_ready_tasks(&self) -> Result<Vec<Task>> {
        // This method is kept for backward compatibility but should not be used for scheduling
        // Use get_and_claim_next_task instead
        match self.get_and_claim_next_task().await? {
            Some(task) => Ok(vec![task]),
            None => Ok(vec![]),
        }
    }

    pub async fn validate_dependencies(&self, dependencies: &[i64]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        for &dep_id in dependencies {
            let exists = conn.query_row(
                "SELECT id FROM tasks WHERE id = ?",
                params![dep_id],
                |_| Ok(()),
            ).optional()?;

            if exists.is_none() {
                return Err(CcschedError::Config(format!("Dependency task {dep_id} does not exist")));
            }
        }

        Ok(())
    }

    pub async fn check_circular_dependency(&self, task_id: i64, dependencies: &[i64]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        // Get all existing dependencies
        let mut stmt = conn.prepare("SELECT task_id, depends_on_id FROM task_dependencies")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>("task_id")?, row.get::<_, i64>("depends_on_id")?))
        })?;

        let mut dependency_graph: HashMap<i64, Vec<i64>> = HashMap::new();
        for row in rows {
            let (task, dep) = row?;
            dependency_graph.entry(task).or_default().push(dep);
        }

        // Add the new dependencies
        for &dep_id in dependencies {
            dependency_graph.entry(task_id).or_default().push(dep_id);
        }

        // Check for circular dependencies using DFS
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        fn has_cycle(
            node: i64,
            graph: &HashMap<i64, Vec<i64>>,
            visited: &mut HashSet<i64>,
            rec_stack: &mut HashSet<i64>,
        ) -> bool {
            visited.insert(node);
            rec_stack.insert(node);

            if let Some(neighbors) = graph.get(&node) {
                for &neighbor in neighbors {
                    if !visited.contains(&neighbor) {
                        if has_cycle(neighbor, graph, visited, rec_stack) {
                            return true;
                        }
                    } else if rec_stack.contains(&neighbor) {
                        return true;
                    }
                }
            }

            rec_stack.remove(&node);
            false
        }

        for &node in dependency_graph.keys() {
            if !visited.contains(&node)
                && has_cycle(node, &dependency_graph, &mut visited, &mut rec_stack) {
                    return Err(CcschedError::Config("Circular dependency detected".to_string()));
                }
        }

        Ok(())
    }

    pub async fn delete_task(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;

        // Delete dependencies first
        tx.execute(
            "DELETE FROM task_dependencies WHERE task_id = ? OR depends_on_id = ?",
            params![id, id],
        )?;

        // Delete the task
        let deleted = tx.execute("DELETE FROM tasks WHERE id = ?", params![id])?;
        
        if deleted == 0 {
            return Err(CcschedError::Config(format!("Task not found: {id}")));
        }

        tx.commit()?;
        Ok(())
    }

    pub async fn update_task_name(&self, id: i64, name: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute("UPDATE tasks SET name = ? WHERE id = ?", params![name, id])?;
        
        if updated == 0 {
            return Err(CcschedError::Config(format!("Task not found: {id}")));
        }
        
        Ok(())
    }

    pub async fn update_task_prompt(&self, id: i64, prompt: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute("UPDATE tasks SET prompt = ? WHERE id = ?", params![prompt, id])?;
        
        if updated == 0 {
            return Err(CcschedError::Config(format!("Task not found: {id}")));
        }
        
        Ok(())
    }

    pub async fn update_task_prompt_and_reset_status(&self, id: i64, prompt: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute(
            "UPDATE tasks SET prompt = ?, status = 'pending', finished_at = NULL, output = NULL, result = NULL, resume_at = NULL WHERE id = ?", 
            params![prompt, id]
        )?;
        
        if updated == 0 {
            return Err(CcschedError::Config(format!("Task not found: {id}")));
        }
        
        Ok(())
    }

    pub async fn get_tasks_by_status(&self, status: TaskStatus) -> Result<Vec<Task>> {
        let conn = self.conn.lock().unwrap();
        let status_str = status.to_string();
        
        let mut stmt = conn.prepare(
            "SELECT id, name, prompt, cwd, status, session_id, submitted_at, finished_at, output, result, resume_at FROM tasks WHERE status = ? ORDER BY submitted_at ASC"
        )?;

        let rows = stmt.query_map([status_str], |row| {
            Ok(Task {
                id: row.get("id")?,
                name: row.get("name")?,
                prompt: row.get("prompt")?,
                cwd: row.get("cwd")?,
                status: TaskStatus::from_str(&row.get::<_, String>("status")?).unwrap_or(TaskStatus::Failed),
                session_id: row.get("session_id")?,
                submitted_at: row.get("submitted_at")?,
                finished_at: row.get("finished_at")?,
                output: row.get("output")?,
                result: row.get("result")?,
                resume_at: row.get("resume_at")?,
            })
        })?;

        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(row?);
        }

        Ok(tasks)
    }

    pub async fn get_waiting_tasks_ready_for_resume(&self) -> Result<Vec<Task>> {
        let conn = self.conn.lock().unwrap();
        
        let mut stmt = conn.prepare(
            "SELECT id, name, prompt, cwd, status, session_id, submitted_at, finished_at, output, result, resume_at FROM tasks WHERE status = 'waiting' AND (resume_at IS NULL OR resume_at <= datetime('now', 'utc')) ORDER BY submitted_at ASC"
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(Task {
                id: row.get("id")?,
                name: row.get("name")?,
                prompt: row.get("prompt")?,
                cwd: row.get("cwd")?,
                status: TaskStatus::from_str(&row.get::<_, String>("status")?).unwrap_or(TaskStatus::Failed),
                session_id: row.get("session_id")?,
                submitted_at: row.get("submitted_at")?,
                finished_at: row.get("finished_at")?,
                output: row.get("output")?,
                result: row.get("result")?,
                resume_at: row.get("resume_at")?,
            })
        })?;

        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(row?);
        }

        Ok(tasks)
    }

    pub async fn cleanup_orphaned_running_tasks(&self) -> Result<Vec<i64>> {
        let conn = self.conn.lock().unwrap();
        
        // Find running tasks without session_id (orphaned tasks)
        let mut stmt = conn.prepare(
            "SELECT id FROM tasks WHERE status = 'running' AND session_id IS NULL"
        )?;
        
        let rows = stmt.query_map([], |row| {
            Ok(row.get::<_, i64>("id")?)
        })?;
        
        let mut orphaned_ids = Vec::new();
        for row in rows {
            orphaned_ids.push(row?);
        }
        
        // Reset orphaned tasks to pending status
        if !orphaned_ids.is_empty() {
            let ids_str = orphaned_ids.iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",");
            
            conn.execute(
                &format!("UPDATE tasks SET status = 'pending' WHERE id IN ({})", ids_str),
                [],
            )?;
        }
        
        Ok(orphaned_ids)
    }
}
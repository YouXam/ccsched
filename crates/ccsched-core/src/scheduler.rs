use crate::config::Config;
use crate::db::Database;
use crate::error::Result;
use crate::models::{Task, TaskStatus};
use crate::worker::Worker;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::time;
use tracing::{error, info, warn};
use chrono::{DateTime, Utc};

pub struct Scheduler {
    db: Arc<Database>,
    task_sender: mpsc::Sender<Task>,
    check_interval: Duration,
    pause_sender: watch::Sender<Option<DateTime<Utc>>>,
    rate_limit_receiver: mpsc::Receiver<DateTime<Utc>>,
}

impl Scheduler {
    pub fn new(db: Database, config: Config) -> Self {
        let db = Arc::new(db);
        let (task_sender, task_receiver) = mpsc::channel::<Task>(100);
        let (pause_sender, pause_receiver) = watch::channel(None);
        let (rate_limit_sender, rate_limit_receiver) = mpsc::channel::<DateTime<Utc>>(10);
        
        let worker = Arc::new(Worker::new(db.as_ref().clone(), config, rate_limit_sender));
        let worker_clone = worker.clone();
        tokio::spawn(async move {
            worker_clone.run(task_receiver, pause_receiver).await;
        });

        Self {
            db,
            task_sender,
            check_interval: Duration::from_secs(5),
            pause_sender,
            rate_limit_receiver,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("Starting task scheduler");
        
        // Clean up orphaned running tasks on startup
        match self.db.cleanup_orphaned_running_tasks().await {
            Ok(orphaned_ids) => {
                if !orphaned_ids.is_empty() {
                    info!("Cleaned up {} orphaned running tasks: {:?}", orphaned_ids.len(), orphaned_ids);
                }
            }
            Err(e) => {
                error!("Failed to cleanup orphaned running tasks: {}", e);
            }
        }
        
        let mut interval = time::interval(self.check_interval);
        let mut paused_until: Option<DateTime<Utc>> = None;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Check if we're currently paused
                    if let Some(resume_time) = paused_until {
                        let now = Utc::now();
                        if now < resume_time {
                            // Still paused, skip scheduling
                            continue;
                        } else {
                            // Resume time reached, clear pause and resume waiting tasks
                            info!("Resuming scheduler, resume time reached");
                            paused_until = None;
                            if let Err(e) = self.pause_sender.send(None) {
                                error!("Failed to send resume signal: {}", e);
                            }
                            if let Err(e) = self.resume_waiting_tasks().await {
                                error!("Error resuming waiting tasks: {}", e);
                            }
                        }
                    }
                    
                    if paused_until.is_none() {
                        if let Err(e) = self.schedule_ready_tasks().await {
                            error!("Error during task scheduling: {}", e);
                        }
                    }
                }
                rate_limit_time = self.rate_limit_receiver.recv() => {
                    if let Some(resume_time) = rate_limit_time {
                        warn!("Received rate limit signal, pausing scheduler until {:?}", resume_time);
                        paused_until = Some(resume_time);
                        
                        // Send pause signal to worker
                        if let Err(e) = self.pause_sender.send(Some(resume_time)) {
                            error!("Failed to send pause signal: {}", e);
                        }
                        
                        // Convert any running tasks to waiting
                        if let Err(e) = self.convert_running_to_waiting(resume_time).await {
                            error!("Error converting running tasks to waiting: {}", e);
                        }
                    }
                }
            }
        }
    }

    async fn schedule_ready_tasks(&self) -> Result<()> {
        // Use the new atomic method to get and claim the next task
        match self.db.get_and_claim_next_task().await? {
            Some(task) => {
                tracing::trace!("Scheduling task {} for execution: {}", task.id, task.name);
                
                if let Err(e) = self.task_sender.send(task.clone()).await {
                    error!("Failed to send task {} to worker: {}", task.id, e);
                    // If sending fails, revert task status back to pending
                    if let Err(revert_err) = self.db.update_task_status(task.id, TaskStatus::Pending, None, None).await {
                        error!("Failed to revert task {} status after send failure: {}", task.id, revert_err);
                    }
                }
            }
            None => {
                // No tasks ready to schedule, which is normal
                tracing::trace!("No tasks ready for scheduling");
            }
        }

        Ok(())
    }

    async fn convert_running_to_waiting(&self, resume_time: DateTime<Utc>) -> Result<()> {
        let running_tasks = self.db.get_tasks_by_status(TaskStatus::Running).await?;
        
        for task in running_tasks {
            info!("Converting running task {} to waiting due to rate limit", task.id);
            self.db.update_task_status_with_resume_at(
                task.id,
                TaskStatus::Waiting,
                task.session_id.as_deref(),
                None,
                Some(resume_time.naive_utc()),
            ).await?;
        }
        
        Ok(())
    }
    
    async fn resume_waiting_tasks(&self) -> Result<()> {
        let waiting_tasks = self.db.get_waiting_tasks_ready_for_resume().await?;
        
        for task in waiting_tasks {
            info!("Resuming waiting task {}", task.id);
            self.db.update_task_status(
                task.id,
                TaskStatus::Pending,
                task.session_id.as_deref(),
                None,
            ).await?;
        }
        
        Ok(())
    }

    pub fn get_db(&self) -> Arc<Database> {
        self.db.clone()
    }
}
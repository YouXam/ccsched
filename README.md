# Claude Code Scheduler

A task scheduler for Claude Code that enables intelligent task distribution with dependency management and topological sorting.

## Features

- **Task Scheduling**: Submit tasks with dependencies and let the scheduler execute them in the correct order
- **Dependency Management**: Define task dependencies with automatic circular dependency detection
- **HTTP API**: RESTful API for task management and monitoring
- **Logging**: Comprehensive logging with separate log files for each task
- **Database**: SQLite-based persistent storage for task state
- **Resume Support**: Resume interrupted Claude Code sessions

## Installation

```bash
cargo build --release
```

## Usage

### Starting the Scheduler Service

```bash
ccsched start
# or with custom options:
ccsched start --host 0.0.0.0 --port 8080 --claude-path /path/to/claude
```

### Submitting Tasks

```bash
# Submit a task with a prompt file
ccsched submit "My Task" prompt.txt

# Submit a task with piped input
echo "Count files in directory" | ccsched submit "Count Files"

# Submit a task with redirected input
ccsched submit "Task from file" < input.txt

# Submit a task interactively (opens $EDITOR)
ccsched submit "Interactive Task"

# Submit a task with dependencies
ccsched submit "Task 2" prompt2.txt --depends 1

# Submit a task with custom working directory
ccsched submit "Task 3" prompt3.txt --cwd /path/to/project
```

### Listing Tasks

```bash
ccsched list
```

### Resuming Tasks

```bash
# Resume by task ID
ccsched resume 1

# Resume by session ID
ccsched resume fc40b756-d837-494e-a7a4-b7c4dbdc5ddb
```
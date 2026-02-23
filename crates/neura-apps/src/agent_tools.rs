/// NeuraOS Agent Tools — gives the AI agent full OS control.
///
/// All tools operate directly on VFS data (JSON files the apps use) or via
/// shared slots/queues that the relevant apps poll each tick.
use std::sync::{Arc, Mutex};
use std::pin::Pin;
use std::future::Future;
use serde_json::Value;
use chrono::Utc;
use neura_storage::vfs::Vfs;
use neura_ai_core::{Tool, ToolParam, ToolError, ToolResult};
use neura_app_framework::consts::{OS_NAME, OS_VERSION};

// ─────────────────────────────────────────────────────────────────────────────
// Shared state types — created in main.rs, passed into both the apps and tools
// ─────────────────────────────────────────────────────────────────────────────

/// Commands the AI agent can send to MediaApp's audio thread (via tick).
#[derive(Debug)]
pub enum MediaAgentCmd {
    /// Play the first track whose title/artist/album contains this query.
    PlayQuery(String),
    Stop,
    Pause,
    Resume,
    Next,
    Previous,
    /// Set volume 0.0–1.0.
    SetVolume(f32),
}

/// Shared slot: tools write, MediaApp.tick() reads and clears.
pub type MediaCmdSlot = Arc<Mutex<Option<MediaAgentCmd>>>;

/// Snapshot of what's currently playing — MediaApp updates each tick.
#[derive(Debug, Clone, Default)]
pub struct NowPlayingSnapshot {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub is_playing: bool,
    pub is_paused: bool,
    pub elapsed_secs: f64,
    pub duration_secs: u32,
    pub volume: f32,
}

/// Shared slot: MediaApp writes, tools read.
pub type NowPlayingSlot = Arc<Mutex<NowPlayingSnapshot>>;

/// Actions the OS agent wants the main loop to perform after the response.
#[derive(Debug)]
pub enum OsAction {
    /// Open an app by ID (e.g. "notes", "chat", "media").
    OpenApp(String),
}

/// Shared queue: tools push, main loop drains after each AI response.
pub type OsActionQueue = Arc<Mutex<Vec<OsAction>>>;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

type HandlerFn = Box<dyn Fn(Value) -> Pin<Box<dyn Future<Output = ToolResult> + Send>> + Send + Sync>;

fn make_handler<F, Fut>(f: F) -> HandlerFn
where
    F: Fn(Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ToolResult> + Send + 'static,
{
    Box::new(move |args| Box::pin(f(args)))
}

fn err(msg: impl Into<String>) -> ToolError {
    ToolError::ExecutionFailed(msg.into())
}

// ─────────────────────────────────────────────────────────────────────────────
// Main builder
// ─────────────────────────────────────────────────────────────────────────────

/// Build the full OS tool set for the AI agent.
pub fn build_os_tools(
    vfs: Arc<Vfs>,
    username: String,
    media_cmd: MediaCmdSlot,
    now_playing: NowPlayingSlot,
    os_actions: OsActionQueue,
) -> Vec<Tool> {
    vec![
        // Notes (6)
        list_notes(vfs.clone(), username.clone()),
        read_note(vfs.clone(), username.clone()),
        search_notes(vfs.clone(), username.clone()),
        create_note(vfs.clone(), username.clone()),
        update_note(vfs.clone(), username.clone()),
        delete_note(vfs.clone(), username.clone()),
        // Tasks (5)
        list_tasks(vfs.clone(), username.clone()),
        create_task(vfs.clone(), username.clone()),
        complete_task(vfs.clone(), username.clone()),
        delete_task(vfs.clone(), username.clone()),
        search_tasks(vfs.clone(), username.clone()),
        // Contacts (4)
        list_contacts(vfs.clone(), username.clone()),
        search_contacts(vfs.clone(), username.clone()),
        get_contact(vfs.clone(), username.clone()),
        create_contact(vfs.clone(), username.clone()),
        // Mail (3)
        send_email(vfs.clone(), username.clone()),
        read_sent_mail(vfs.clone(), username.clone()),
        search_emails(vfs.clone(), username.clone()),
        // Media (9)
        list_tracks(vfs.clone(), username.clone()),
        play_track_by_name(media_cmd.clone()),
        stop_playback(media_cmd.clone()),
        pause_playback(media_cmd.clone()),
        resume_playback(media_cmd.clone()),
        next_track(media_cmd.clone()),
        previous_track(media_cmd.clone()),
        set_volume(media_cmd.clone()),
        get_now_playing(now_playing),
        // System (4)
        get_current_time(),
        list_available_apps(),
        open_app(os_actions),
        get_system_info(),
        // Files (2)
        read_vfs_file(vfs.clone()),
        list_vfs_directory(vfs.clone()),
        // Memory (4)
        remember(vfs.clone(), username.clone()),
        recall(vfs.clone(), username.clone()),
        list_memories(vfs.clone(), username.clone()),
        delete_memory(vfs.clone(), username.clone()),
    ]
}

// ─────────────────────────────────────────────────────────────────────────────
// MEMORY (Long-term Knowledge)
// ─────────────────────────────────────────────────────────────────────────────

fn remember(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "remember".to_string(),
        description: "Save a fact or important information to long-term memory. Use this for user preferences, important dates, or key project details.".to_string(),
        parameters: vec![
            ToolParam { name: "content".to_string(), param_type: "string".to_string(), description: "The fact to remember".to_string(), required: true },
            ToolParam { name: "tags".to_string(), param_type: "string".to_string(), description: "Comma-separated tags for categorization (e.g. 'user,preference' or 'project,deadlines')".to_string(), required: false },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let content = args.get("content").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'content'"))?.to_string();
                let tags_str = args.get("tags").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let tags: Vec<String> = tags_str.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                
                let path = format!("/home/{}/memory.json", username);
                let data = vfs.read_file(&path).await.unwrap_or_default();
                let mut memories: Vec<Value> = serde_json::from_slice(&data).unwrap_or_default();
                
                // Add new memory
                let id = uuid::Uuid::new_v4().to_string();
                let new_memory = serde_json::json!({
                    "id": id,
                    "content": content,
                    "tags": tags,
                    "created_at": Utc::now().to_rfc3339()
                });
                memories.push(new_memory);
                
                let data = serde_json::to_vec_pretty(&memories).map_err(|e| err(e.to_string()))?;
                vfs.write_file(&path, data, &username).await.map_err(|e| err(e.to_string()))?;
                
                Ok(serde_json::json!({ "status": "remembered", "content": content, "id": id }))
            }
        }),
    }
}

fn recall(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "recall".to_string(),
        description: "Search long-term memory for facts matching a keyword.".to_string(),
        parameters: vec![
            ToolParam { name: "query".to_string(), param_type: "string".to_string(), description: "Keyword to search for".to_string(), required: true },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'query'"))?.to_lowercase();
                let path = format!("/home/{}/memory.json", username);
                let data = vfs.read_file(&path).await.unwrap_or_default();
                let memories: Vec<Value> = serde_json::from_slice(&data).unwrap_or_default();
                
                let results: Vec<&Value> = memories.iter().filter(|m| {
                    let content = m.get("content").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    let tags = m.get("tags").and_then(|v| v.as_array()).map(|arr| {
                        arr.iter().map(|v| v.as_str().unwrap_or("").to_lowercase()).collect::<Vec<_>>().join(" ")
                    }).unwrap_or_default();
                    content.contains(&query) || tags.contains(&query)
                }).collect();
                
                Ok(serde_json::json!({ "query": query, "count": results.len(), "results": results }))
            }
        }),
    }
}

fn list_memories(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "list_memories".to_string(),
        description: "List all stored memories. Use this to review what you know.".to_string(),
        parameters: vec![],
        handler: make_handler(move |_args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let path = format!("/home/{}/memory.json", username);
                let data = vfs.read_file(&path).await.unwrap_or_default();
                let memories: Vec<Value> = serde_json::from_slice(&data).unwrap_or_default();
                Ok(serde_json::json!({ "count": memories.len(), "memories": memories }))
            }
        }),
    }
}

fn delete_memory(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "delete_memory".to_string(),
        description: "Delete a specific memory by its ID.".to_string(),
        parameters: vec![
            ToolParam { name: "id".to_string(), param_type: "string".to_string(), description: "The ID of the memory to delete".to_string(), required: true },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let id = args.get("id").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'id'"))?.to_string();
                let path = format!("/home/{}/memory.json", username);
                let data = vfs.read_file(&path).await.unwrap_or_default();
                let memories: Vec<Value> = serde_json::from_slice(&data).unwrap_or_default();
                
                let initial_len = memories.len();
                let filtered: Vec<Value> = memories.into_iter().filter(|m| {
                    m.get("id").and_then(|v| v.as_str()).unwrap_or("") != id
                }).collect();
                
                if filtered.len() == initial_len {
                    return Err(err(format!("No memory found with ID '{}'", id)));
                }
                
                let data = serde_json::to_vec_pretty(&filtered).map_err(|e| err(e.to_string()))?;
                vfs.write_file(&path, data, &username).await.map_err(|e| err(e.to_string()))?;
                
                Ok(serde_json::json!({ "status": "deleted", "id": id }))
            }
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// NOTES
// ─────────────────────────────────────────────────────────────────────────────

fn list_notes(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "list_notes".to_string(),
        description: "List all notes with titles and short previews. Call this first to discover what notes exist.".to_string(),
        parameters: vec![],
        handler: make_handler(move |_args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let dir = format!("/home/{}/notes", username);
                let mut notes = Vec::new();
                for name in vfs.list_dir(&dir).await.unwrap_or_default() {
                    let path = format!("{}/{}", dir, name);
                    if let Ok(data) = vfs.read_file(&path).await {
                        if let Ok(n) = serde_json::from_slice::<Value>(&data) {
                            let title = n.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled").to_string();
                            let content = n.get("content").and_then(|v| v.as_str()).unwrap_or("");
                            let preview: String = content.chars().take(120).collect();
                            let modified = n.get("modified_at").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            notes.push(serde_json::json!({ "title": title, "preview": preview, "modified_at": modified }));
                        }
                    }
                }
                Ok(serde_json::json!({ "count": notes.len(), "notes": notes }))
            }
        }),
    }
}

fn read_note(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "read_note".to_string(),
        description: "Read the full content of a specific note by title (partial match). Returns the complete note text.".to_string(),
        parameters: vec![
            ToolParam { name: "title".to_string(), param_type: "string".to_string(), description: "Note title or partial title to match".to_string(), required: true },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let query = args.get("title").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'title'"))?.to_lowercase();
                let dir = format!("/home/{}/notes", username);
                for name in vfs.list_dir(&dir).await.unwrap_or_default() {
                    let path = format!("{}/{}", dir, name);
                    if let Ok(data) = vfs.read_file(&path).await {
                        if let Ok(note) = serde_json::from_slice::<Value>(&data) {
                            let t = note.get("title").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                            if t.contains(&query) { return Ok(note); }
                        }
                    }
                }
                Err(err(format!("No note found matching '{}'", query)))
            }
        }),
    }
}

fn search_notes(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "search_notes".to_string(),
        description: "Search all notes for a keyword in title or content. Returns matching notes with previews.".to_string(),
        parameters: vec![
            ToolParam { name: "query".to_string(), param_type: "string".to_string(), description: "Keyword or phrase to search for in notes".to_string(), required: true },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'query'"))?.to_lowercase();
                let dir = format!("/home/{}/notes", username);
                let mut results = Vec::new();
                for name in vfs.list_dir(&dir).await.unwrap_or_default() {
                    let path = format!("{}/{}", dir, name);
                    if let Ok(data) = vfs.read_file(&path).await {
                        if let Ok(note) = serde_json::from_slice::<Value>(&data) {
                            let title = note.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let content = note.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let modified = note.get("modified_at").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            if title.to_lowercase().contains(&query) || content.to_lowercase().contains(&query) {
                                let preview: String = content.chars().take(200).collect();
                                results.push(serde_json::json!({ "title": title, "preview": preview, "modified_at": modified }));
                            }
                        }
                    }
                }
                Ok(serde_json::json!({ "query": query, "count": results.len(), "results": results }))
            }
        }),
    }
}

fn create_note(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "create_note".to_string(),
        description: "Create a new note with a title and full content body. Use for long-form content. For task lists, prefer create_task (called repeatedly). Saved permanently in NeuraNotes.".to_string(),
        parameters: vec![
            ToolParam { name: "title".to_string(), param_type: "string".to_string(), description: "Note title".to_string(), required: true },
            ToolParam { name: "content".to_string(), param_type: "string".to_string(), description: "Note body/content".to_string(), required: true },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let title = args.get("title").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'title'"))?.to_string();
                let content = args.get("content").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'content'"))?.to_string();
                let dir = format!("/home/{}/notes", username);
                let _ = vfs.mkdir(&dir, &username).await;
                let now = Utc::now().to_rfc3339();
                let note = serde_json::json!({ "title": title, "content": content, "created_at": now, "modified_at": now });
                let filename: String = title.replace(' ', "_").to_lowercase().chars().filter(|c| c.is_alphanumeric() || *c == '_').collect();
                let path = format!("{}/{}.json", dir, filename);
                let data = serde_json::to_vec_pretty(&note).map_err(|e| err(e.to_string()))?;
                vfs.write_file(&path, data, &username).await.map_err(|e| err(e.to_string()))?;
                Ok(serde_json::json!({ "status": "created", "title": title }))
            }
        }),
    }
}

fn update_note(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "update_note".to_string(),
        description: "Replace the content of an existing note (partial title match).".to_string(),
        parameters: vec![
            ToolParam { name: "title".to_string(), param_type: "string".to_string(), description: "Partial title of note to update".to_string(), required: true },
            ToolParam { name: "content".to_string(), param_type: "string".to_string(), description: "New content to replace existing body".to_string(), required: true },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let query = args.get("title").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'title'"))?.to_lowercase();
                let new_content = args.get("content").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'content'"))?.to_string();
                let dir = format!("/home/{}/notes", username);
                for name in vfs.list_dir(&dir).await.unwrap_or_default() {
                    let path = format!("{}/{}", dir, name);
                    if let Ok(data) = vfs.read_file(&path).await {
                        if let Ok(mut note) = serde_json::from_slice::<Value>(&data) {
                            let t = note.get("title").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                            if t.contains(&query) {
                                let actual = note.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                note["content"] = Value::String(new_content);
                                note["modified_at"] = Value::String(Utc::now().to_rfc3339());
                                let data = serde_json::to_vec_pretty(&note).map_err(|e| err(e.to_string()))?;
                                vfs.write_file(&path, data, &username).await.map_err(|e| err(e.to_string()))?;
                                return Ok(serde_json::json!({ "status": "updated", "title": actual }));
                            }
                        }
                    }
                }
                Err(err(format!("No note found matching '{}'", query)))
            }
        }),
    }
}

fn delete_note(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "delete_note".to_string(),
        description: "Permanently delete a note by title (partial match).".to_string(),
        parameters: vec![
            ToolParam { name: "title".to_string(), param_type: "string".to_string(), description: "Partial title of note to delete".to_string(), required: true },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let query = args.get("title").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'title'"))?.to_lowercase();
                let dir = format!("/home/{}/notes", username);
                for name in vfs.list_dir(&dir).await.unwrap_or_default() {
                    let path = format!("{}/{}", dir, name);
                    if let Ok(data) = vfs.read_file(&path).await {
                        if let Ok(note) = serde_json::from_slice::<Value>(&data) {
                            let t = note.get("title").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                            if t.contains(&query) {
                                let actual = note.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                vfs.remove(&path).await.map_err(|e| err(e.to_string()))?;
                                return Ok(serde_json::json!({ "status": "deleted", "title": actual }));
                            }
                        }
                    }
                }
                Err(err(format!("No note found matching '{}'", query)))
            }
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TASKS
// ─────────────────────────────────────────────────────────────────────────────

fn list_tasks(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "list_tasks".to_string(),
        description: "List todo tasks. Filter by 'all' (default), 'pending', or 'done'.".to_string(),
        parameters: vec![
            ToolParam { name: "filter".to_string(), param_type: "string".to_string(), description: "Filter: 'all', 'pending', or 'done'".to_string(), required: false },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let filter = args.get("filter").and_then(|v| v.as_str()).unwrap_or("all").to_lowercase();
                let path = format!("/home/{}/tasks.json", username);
                let data = vfs.read_file(&path).await.unwrap_or_default();
                let tasks: Vec<Value> = serde_json::from_slice(&data).unwrap_or_default();
                let filtered: Vec<&Value> = tasks.iter().filter(|t| {
                    let done = t.get("done").and_then(|v| v.as_bool()).unwrap_or(false);
                    match filter.as_str() { "pending" => !done, "done" => done, _ => true }
                }).collect();
                Ok(serde_json::json!({ "filter": filter, "count": filtered.len(), "tasks": filtered }))
            }
        }),
    }
}

fn create_task(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "create_task".to_string(),
        description: "Create ONE new todo task. For a list (shopping list, ingredient list, etc.) call this tool MULTIPLE TIMES — once per item. Priority: 'Low', 'Medium' (default), or 'High'.".to_string(),
        parameters: vec![
            ToolParam { name: "title".to_string(), param_type: "string".to_string(), description: "Task title/description".to_string(), required: true },
            ToolParam { name: "priority".to_string(), param_type: "string".to_string(), description: "Priority: 'Low', 'Medium', or 'High'".to_string(), required: false },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let title = args.get("title").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'title'"))?.to_string();
                let priority = args.get("priority").and_then(|v| v.as_str()).unwrap_or("Medium").to_string();
                let path = format!("/home/{}/tasks.json", username);
                let data = vfs.read_file(&path).await.unwrap_or_default();
                let mut tasks: Vec<Value> = serde_json::from_slice(&data).unwrap_or_default();
                tasks.push(serde_json::json!({ "title": title, "done": false, "priority": priority, "created_at": Utc::now().to_rfc3339() }));
                let data = serde_json::to_vec_pretty(&tasks).map_err(|e| err(e.to_string()))?;
                vfs.write_file(&path, data, &username).await.map_err(|e| err(e.to_string()))?;
                Ok(serde_json::json!({ "status": "created", "title": title, "priority": priority }))
            }
        }),
    }
}

fn complete_task(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "complete_task".to_string(),
        description: "Mark a task as done. Partial title match.".to_string(),
        parameters: vec![
            ToolParam { name: "title".to_string(), param_type: "string".to_string(), description: "Partial title of task to complete".to_string(), required: true },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let query = args.get("title").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'title'"))?.to_lowercase();
                let path = format!("/home/{}/tasks.json", username);
                let data = vfs.read_file(&path).await.unwrap_or_default();
                let mut tasks: Vec<Value> = serde_json::from_slice(&data).unwrap_or_default();
                let mut found = String::new();
                for task in tasks.iter_mut() {
                    let t = task.get("title").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    if t.contains(&query) && found.is_empty() {
                        found = task.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        task["done"] = Value::Bool(true);
                        break;
                    }
                }
                if found.is_empty() { return Err(err(format!("No task matching '{}'", query))); }
                let data = serde_json::to_vec_pretty(&tasks).map_err(|e| err(e.to_string()))?;
                vfs.write_file(&path, data, &username).await.map_err(|e| err(e.to_string()))?;
                Ok(serde_json::json!({ "status": "completed", "title": found }))
            }
        }),
    }
}

fn delete_task(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "delete_task".to_string(),
        description: "Permanently delete a task by title (partial match).".to_string(),
        parameters: vec![
            ToolParam { name: "title".to_string(), param_type: "string".to_string(), description: "Partial title of task to delete".to_string(), required: true },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let query = args.get("title").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'title'"))?.to_lowercase();
                let path = format!("/home/{}/tasks.json", username);
                let data = vfs.read_file(&path).await.unwrap_or_default();
                let tasks: Vec<Value> = serde_json::from_slice(&data).unwrap_or_default();
                let mut removed = String::new();
                let filtered: Vec<Value> = tasks.into_iter().filter(|t| {
                    let title = t.get("title").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    if title.contains(&query) && removed.is_empty() {
                        removed = t.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        false
                    } else { true }
                }).collect();
                if removed.is_empty() { return Err(err(format!("No task matching '{}'", query))); }
                let data = serde_json::to_vec_pretty(&filtered).map_err(|e| err(e.to_string()))?;
                vfs.write_file(&path, data, &username).await.map_err(|e| err(e.to_string()))?;
                Ok(serde_json::json!({ "status": "deleted", "title": removed }))
            }
        }),
    }
}

fn search_tasks(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "search_tasks".to_string(),
        description: "Search tasks by keyword in their title.".to_string(),
        parameters: vec![
            ToolParam { name: "query".to_string(), param_type: "string".to_string(), description: "Keyword to search".to_string(), required: true },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'query'"))?.to_lowercase();
                let path = format!("/home/{}/tasks.json", username);
                let data = vfs.read_file(&path).await.unwrap_or_default();
                let tasks: Vec<Value> = serde_json::from_slice(&data).unwrap_or_default();
                let results: Vec<&Value> = tasks.iter().filter(|t| {
                    t.get("title").and_then(|v| v.as_str()).unwrap_or("").to_lowercase().contains(&query)
                }).collect();
                Ok(serde_json::json!({ "query": query, "count": results.len(), "results": results }))
            }
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CONTACTS
// ─────────────────────────────────────────────────────────────────────────────

fn list_contacts(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "list_contacts".to_string(),
        description: "List all contacts with name, email, and phone. Use this to find someone's email before sending.".to_string(),
        parameters: vec![],
        handler: make_handler(move |_args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let path = format!("/home/{}/contacts.json", username);
                let data = vfs.read_file(&path).await.unwrap_or_default();
                let contacts: Vec<Value> = serde_json::from_slice(&data).unwrap_or_default();
                Ok(serde_json::json!({ "count": contacts.len(), "contacts": contacts }))
            }
        }),
    }
}

fn search_contacts(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "search_contacts".to_string(),
        description: "Search contacts by name or email (partial match). Returns matching contacts including their email addresses. Always search here first before sending an email.".to_string(),
        parameters: vec![
            ToolParam { name: "query".to_string(), param_type: "string".to_string(), description: "Name or email to search for".to_string(), required: true },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'query'"))?.to_lowercase();
                let path = format!("/home/{}/contacts.json", username);
                let data = vfs.read_file(&path).await.unwrap_or_default();
                let contacts: Vec<Value> = serde_json::from_slice(&data).unwrap_or_default();
                let results: Vec<&Value> = contacts.iter().filter(|c| {
                    let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    let email = c.get("email").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    name.contains(&query) || email.contains(&query)
                }).collect();
                Ok(serde_json::json!({ "query": query, "count": results.len(), "results": results }))
            }
        }),
    }
}

fn get_contact(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "get_contact".to_string(),
        description: "Get full details for one contact by name. Returns name, email, phone.".to_string(),
        parameters: vec![
            ToolParam { name: "name".to_string(), param_type: "string".to_string(), description: "Contact name (partial match)".to_string(), required: true },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let query = args.get("name").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'name'"))?.to_lowercase();
                let path = format!("/home/{}/contacts.json", username);
                let data = vfs.read_file(&path).await.unwrap_or_default();
                let contacts: Vec<Value> = serde_json::from_slice(&data).unwrap_or_default();
                for c in &contacts {
                    let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    if name.contains(&query) { return Ok(c.clone()); }
                }
                Ok(serde_json::json!({ "found": false, "message": format!("No contact named '{}'", query) }))
            }
        }),
    }
}

fn create_contact(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "create_contact".to_string(),
        description: "Create a new contact with name, email, and optional phone number.".to_string(),
        parameters: vec![
            ToolParam { name: "name".to_string(), param_type: "string".to_string(), description: "Contact's full name".to_string(), required: true },
            ToolParam { name: "email".to_string(), param_type: "string".to_string(), description: "Contact's email address".to_string(), required: true },
            ToolParam { name: "phone".to_string(), param_type: "string".to_string(), description: "Contact's phone number (optional)".to_string(), required: false },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let name = args.get("name").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'name'"))?.to_string();
                let email = args.get("email").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'email'"))?.to_string();
                let phone = args.get("phone").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let path = format!("/home/{}/contacts.json", username);
                let data = vfs.read_file(&path).await.unwrap_or_default();
                let mut contacts: Vec<Value> = serde_json::from_slice(&data).unwrap_or_default();
                contacts.push(serde_json::json!({ "name": name, "email": email, "phone": phone, "created_at": Utc::now().to_rfc3339() }));
                let data = serde_json::to_vec_pretty(&contacts).map_err(|e| err(e.to_string()))?;
                vfs.write_file(&path, data, &username).await.map_err(|e| err(e.to_string()))?;
                Ok(serde_json::json!({ "status": "created", "name": name, "email": email }))
            }
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MAIL
// ─────────────────────────────────────────────────────────────────────────────

fn send_email(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "send_email".to_string(),
        description: "Send an email using the configured mail account. Always verify recipient's email via search_contacts or search_emails first. Requires Mail app account to be set up.".to_string(),
        parameters: vec![
            ToolParam { name: "to".to_string(), param_type: "string".to_string(), description: "Recipient email address".to_string(), required: true },
            ToolParam { name: "subject".to_string(), param_type: "string".to_string(), description: "Email subject".to_string(), required: true },
            ToolParam { name: "body".to_string(), param_type: "string".to_string(), description: "Email body text".to_string(), required: true },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let to = args.get("to").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'to'"))?.to_string();
                let subject = args.get("subject").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'subject'"))?.to_string();
                let body = args.get("body").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'body'"))?.to_string();

                let account_path = format!("/home/{}/mail_account.json", username);
                let account_data = vfs.read_file(&account_path).await
                    .map_err(|_| err("No mail account configured. Open the Mail app and set up your account first."))?;
                let account: Value = serde_json::from_slice(&account_data)
                    .map_err(|e| err(format!("Invalid account config: {}", e)))?;

                let from_email = account.get("email").and_then(|v| v.as_str()).ok_or_else(|| err("Missing email in account"))?.to_string();
                let password = account.get("password").and_then(|v| v.as_str()).ok_or_else(|| err("Missing password"))?.to_string();
                let display_name = account.get("display_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let smtp_host = account.get("smtp_host").and_then(|v| v.as_str()).ok_or_else(|| err("Missing smtp_host"))?.to_string();
                let smtp_port = account.get("smtp_port").and_then(|v| v.as_u64()).unwrap_or(587) as u16;
                let smtp_starttls = account.get("smtp_starttls").and_then(|v| v.as_bool()).unwrap_or(true);

                smtp_send(&from_email, &display_name, &to, &subject, &body, &smtp_host, smtp_port, smtp_starttls, &password).await?;

                let sent_path = format!("/home/{}/mail_sent.json", username);
                let sent_data = vfs.read_file(&sent_path).await.unwrap_or_default();
                let mut sent: Vec<Value> = serde_json::from_slice(&sent_data).unwrap_or_default();
                sent.push(serde_json::json!({ "to": to, "subject": subject, "body": body, "sent_at": Utc::now().to_rfc3339() }));
                if let Ok(data) = serde_json::to_vec_pretty(&sent) {
                    let _ = vfs.write_file(&sent_path, data, &username).await;
                }
                Ok(serde_json::json!({ "status": "sent", "to": to, "subject": subject }))
            }
        }),
    }
}

async fn smtp_send(
    from: &str, display_name: &str, to: &str, subject: &str, body: &str,
    host: &str, port: u16, starttls: bool, password: &str,
) -> Result<(), ToolError> {
    use lettre::{
        message::header::ContentType,
        transport::smtp::authentication::Credentials,
        AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    };
    let from_str = if display_name.is_empty() { from.to_string() } else { format!("{} <{}>", display_name, from) };
    let email = Message::builder()
        .from(from_str.parse().map_err(|e: lettre::address::AddressError| err(e.to_string()))?)
        .to(to.parse().map_err(|e: lettre::address::AddressError| err(e.to_string()))?)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())
        .map_err(|e| err(e.to_string()))?;
    let creds = Credentials::new(from.to_string(), password.to_string());
    if starttls {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
            .map_err(|e| err(e.to_string()))?.port(port).credentials(creds).build()
            .send(email).await.map_err(|e| err(format!("SMTP: {}", e)))?;
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::relay(host)
            .map_err(|e| err(e.to_string()))?.port(port).credentials(creds).build()
            .send(email).await.map_err(|e| err(format!("SMTP: {}", e)))?;
    }
    Ok(())
}

fn read_sent_mail(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "read_sent_mail".to_string(),
        description: "Read sent emails (most recent first). Useful for checking past correspondence.".to_string(),
        parameters: vec![
            ToolParam { name: "limit".to_string(), param_type: "number".to_string(), description: "Max emails to return (default 20)".to_string(), required: false },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
                let path = format!("/home/{}/mail_sent.json", username);
                let data = vfs.read_file(&path).await.unwrap_or_default();
                let sent: Vec<Value> = serde_json::from_slice(&data).unwrap_or_default();
                let total = sent.len();
                let recent: Vec<Value> = sent.into_iter().rev().take(limit).collect();
                Ok(serde_json::json!({ "total": total, "count": recent.len(), "emails": recent }))
            }
        }),
    }
}

fn search_emails(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "search_emails".to_string(),
        description: "Search sent email history for a keyword. Useful for finding a person's email from past correspondence when not in contacts.".to_string(),
        parameters: vec![
            ToolParam { name: "query".to_string(), param_type: "string".to_string(), description: "Keyword to search in recipient, subject, or body".to_string(), required: true },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'query'"))?.to_lowercase();
                let path = format!("/home/{}/mail_sent.json", username);
                let data = vfs.read_file(&path).await.unwrap_or_default();
                let sent: Vec<Value> = serde_json::from_slice(&data).unwrap_or_default();
                let results: Vec<Value> = sent.iter().filter(|e| {
                    let to = e.get("to").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    let sub = e.get("subject").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    let body = e.get("body").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    to.contains(&query) || sub.contains(&query) || body.contains(&query)
                }).map(|e| {
                    let preview: String = e.get("body").and_then(|v| v.as_str()).unwrap_or("").chars().take(100).collect();
                    serde_json::json!({
                        "to": e.get("to").and_then(|v| v.as_str()).unwrap_or(""),
                        "subject": e.get("subject").and_then(|v| v.as_str()).unwrap_or(""),
                        "preview": preview,
                        "sent_at": e.get("sent_at").and_then(|v| v.as_str()).unwrap_or(""),
                    })
                }).collect();
                Ok(serde_json::json!({ "query": query, "count": results.len(), "results": results }))
            }
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MEDIA  (actual playback control via MediaCmdSlot)
// ─────────────────────────────────────────────────────────────────────────────

fn list_tracks(vfs: Arc<Vfs>, username: String) -> Tool {
    Tool {
        name: "list_tracks".to_string(),
        description: "List music tracks in the library. Optionally filter by title/artist/album keyword.".to_string(),
        parameters: vec![
            ToolParam { name: "query".to_string(), param_type: "string".to_string(), description: "Optional keyword filter".to_string(), required: false },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            let username = username.clone();
            async move {
                let query = args.get("query").and_then(|v| v.as_str()).map(|s| s.to_lowercase());
                let path = format!("/home/{}/media.json", username);
                let data = vfs.read_file(&path).await.unwrap_or_default();
                let lib: Value = serde_json::from_slice(&data).unwrap_or_else(|_| serde_json::json!({"tracks":[]}));
                let empty = vec![];
                let tracks = lib.get("tracks").and_then(|v| v.as_array()).unwrap_or(&empty);
                let filtered: Vec<Value> = tracks.iter().filter(|t| {
                    let q = match &query { Some(q) => q.as_str(), None => return true };
                    let title = t.get("title").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    let artist = t.get("artist").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    let album = t.get("album").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    title.contains(q) || artist.contains(q) || album.contains(q)
                }).map(|t| serde_json::json!({
                    "title": t.get("title").and_then(|v| v.as_str()).unwrap_or("Unknown"),
                    "artist": t.get("artist").and_then(|v| v.as_str()).unwrap_or("Unknown"),
                    "album": t.get("album").and_then(|v| v.as_str()).unwrap_or("Unknown"),
                    "duration_secs": t.get("duration_secs").and_then(|v| v.as_f64()).unwrap_or(0.0),
                })).collect();
                Ok(serde_json::json!({ "total_in_library": tracks.len(), "count": filtered.len(), "tracks": filtered }))
            }
        }),
    }
}

fn send_media_cmd(slot: MediaCmdSlot, cmd: MediaAgentCmd) -> ToolResult {
    match slot.lock() {
        Ok(mut g) => { *g = Some(cmd); Ok(serde_json::json!({ "status": "command_queued" })) }
        Err(e) => Err(err(format!("Media slot lock failed: {}", e))),
    }
}

fn play_track_by_name(slot: MediaCmdSlot) -> Tool {
    Tool {
        name: "play_track".to_string(),
        description: "Play a song in the Media player. Searches the library by title, artist, or album keyword and plays the first match. The Media app must have songs imported.".to_string(),
        parameters: vec![
            ToolParam { name: "query".to_string(), param_type: "string".to_string(), description: "Song title, artist name, or album name to play".to_string(), required: true },
        ],
        handler: make_handler(move |args| {
            let slot = slot.clone();
            async move {
                let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'query'"))?.to_string();
                send_media_cmd(slot, MediaAgentCmd::PlayQuery(query.clone()))
                    .map(|_| serde_json::json!({ "status": "playing", "query": query }))
            }
        }),
    }
}

fn stop_playback(slot: MediaCmdSlot) -> Tool {
    Tool {
        name: "stop_playback".to_string(),
        description: "Stop the currently playing music.".to_string(),
        parameters: vec![],
        handler: make_handler(move |_args| {
            let slot = slot.clone();
            async move { send_media_cmd(slot, MediaAgentCmd::Stop) }
        }),
    }
}

fn pause_playback(slot: MediaCmdSlot) -> Tool {
    Tool {
        name: "pause_playback".to_string(),
        description: "Pause the currently playing music.".to_string(),
        parameters: vec![],
        handler: make_handler(move |_args| {
            let slot = slot.clone();
            async move { send_media_cmd(slot, MediaAgentCmd::Pause) }
        }),
    }
}

fn resume_playback(slot: MediaCmdSlot) -> Tool {
    Tool {
        name: "resume_playback".to_string(),
        description: "Resume paused music playback.".to_string(),
        parameters: vec![],
        handler: make_handler(move |_args| {
            let slot = slot.clone();
            async move { send_media_cmd(slot, MediaAgentCmd::Resume) }
        }),
    }
}

fn next_track(slot: MediaCmdSlot) -> Tool {
    Tool {
        name: "next_track".to_string(),
        description: "Skip to the next track in the queue.".to_string(),
        parameters: vec![],
        handler: make_handler(move |_args| {
            let slot = slot.clone();
            async move { send_media_cmd(slot, MediaAgentCmd::Next) }
        }),
    }
}

fn previous_track(slot: MediaCmdSlot) -> Tool {
    Tool {
        name: "previous_track".to_string(),
        description: "Go back to the previous track (or restart current if more than 3s played).".to_string(),
        parameters: vec![],
        handler: make_handler(move |_args| {
            let slot = slot.clone();
            async move { send_media_cmd(slot, MediaAgentCmd::Previous) }
        }),
    }
}

fn set_volume(slot: MediaCmdSlot) -> Tool {
    Tool {
        name: "set_volume".to_string(),
        description: "Set music playback volume. Level is 0.0 (mute) to 1.0 (max). Use 0.5 for 50%, 0.8 for 80%, etc.".to_string(),
        parameters: vec![
            ToolParam { name: "level".to_string(), param_type: "number".to_string(), description: "Volume level from 0.0 (mute) to 1.0 (max)".to_string(), required: true },
        ],
        handler: make_handler(move |args| {
            let slot = slot.clone();
            async move {
                let level = args.get("level").and_then(|v| v.as_f64()).ok_or_else(|| err("Missing 'level'"))? as f32;
                let level = level.clamp(0.0, 1.0);
                send_media_cmd(slot, MediaAgentCmd::SetVolume(level))
                    .map(|_| serde_json::json!({ "status": "volume_set", "level": level }))
            }
        }),
    }
}

fn get_now_playing(slot: NowPlayingSlot) -> Tool {
    Tool {
        name: "get_now_playing".to_string(),
        description: "Get information about the currently playing track in the Media player.".to_string(),
        parameters: vec![],
        handler: make_handler(move |_args| {
            let slot = slot.clone();
            async move {
                match slot.lock() {
                    Ok(np) => Ok(serde_json::json!({
                        "title": np.title,
                        "artist": np.artist,
                        "album": np.album,
                        "is_playing": np.is_playing,
                        "is_paused": np.is_paused,
                        "elapsed_secs": np.elapsed_secs,
                        "duration_secs": np.duration_secs,
                        "volume": np.volume,
                        "playing": !np.title.is_empty(),
                    })),
                    Err(e) => Err(err(format!("Failed to read now playing: {}", e))),
                }
            }
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SYSTEM
// ─────────────────────────────────────────────────────────────────────────────

fn get_current_time() -> Tool {
    Tool {
        name: "get_current_time".to_string(),
        description: "Get the current date, time, day of week in UTC. Useful for scheduling and timestamps.".to_string(),
        parameters: vec![],
        handler: make_handler(|_args| async move {
            let now = Utc::now();
            Ok(serde_json::json!({
                "datetime_utc": now.to_rfc3339(),
                "date": now.format("%Y-%m-%d").to_string(),
                "time_utc": now.format("%H:%M:%S").to_string(),
                "day_of_week": now.format("%A").to_string(),
            }))
        }),
    }
}

fn list_available_apps() -> Tool {
    Tool {
        name: "list_available_apps".to_string(),
        description: "List all available NeuraOS apps by ID and description. Use open_app to open one.".to_string(),
        parameters: vec![],
        handler: make_handler(|_args| async move {
            Ok(serde_json::json!({ "apps": [
                { "id": "notes",    "name": "NeuraNotes",    "description": "Note-taking" },
                { "id": "tasks",    "name": "NeuraTasks",    "description": "Todo task manager" },
                { "id": "contacts", "name": "NeuraContacts", "description": "Contact address book" },
                { "id": "mail",     "name": "NeuraMail",     "description": "Email client (IMAP/SMTP)" },
                { "id": "chat",     "name": "NeuraChat",     "description": "AI assistant (current app)" },
                { "id": "media",    "name": "NeuraMedia",    "description": "Music player" },
                { "id": "browser",  "name": "NeuraBrowser",  "description": "Web browser" },
                { "id": "files",    "name": "NeuraFiles",    "description": "File manager" },
                { "id": "calendar", "name": "NeuraCalendar", "description": "Calendar" },
                { "id": "calc",     "name": "NeuraCalc",     "description": "Calculator" },
                { "id": "clock",    "name": "NeuraClock",    "description": "World clocks" },
                { "id": "weather",  "name": "NeuraWeather",  "description": "Weather forecast" },
                { "id": "terminal", "name": "NeuraTerminal", "description": "Terminal emulator" },
                { "id": "dev",      "name": "NeuraDev",      "description": "Code editor" },
                { "id": "monitor",  "name": "NeuraMonitor",  "description": "System monitor" },
                { "id": "settings", "name": "NeuraSettings", "description": "System settings" },
                { "id": "backup",   "name": "NeuraBackup",   "description": "Backup manager" },
                { "id": "logs",     "name": "NeuraLogs",     "description": "System logs" },
                { "id": "sysinfo",  "name": "NeuraSysInfo",  "description": "Hardware & OS info" },
                { "id": "ssh",      "name": "NeuraSSH",      "description": "SSH Client" },
                { "id": "ftp",      "name": "NeuraFTP",      "description": "FTP Client" },
                { "id": "db",       "name": "NeuraDB",       "description": "Database Manager" },
                { "id": "sync",     "name": "NeuraSync",     "description": "Cloud Sync" },
                { "id": "store",    "name": "NeuraStore",    "description": "App Store" },
            ]}))
        }),
    }
}

fn open_app(queue: OsActionQueue) -> Tool {
    Tool {
        name: "open_app".to_string(),
        description: "Open a NeuraOS app by its ID. The app will open after the AI response is complete. Use list_available_apps to get valid app IDs.".to_string(),
        parameters: vec![
            ToolParam { name: "app_id".to_string(), param_type: "string".to_string(), description: "App ID to open (e.g. 'notes', 'media', 'mail', 'tasks', 'contacts')".to_string(), required: true },
        ],
        handler: make_handler(move |args| {
            let queue = queue.clone();
            async move {
                let app_id = args.get("app_id").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'app_id'"))?.to_string();
                
                // Validate app ID
                let valid_apps = [
                    "notes", "tasks", "contacts", "mail", "chat", "media", "browser", "files", 
                    "calendar", "calc", "clock", "weather", "terminal", "dev", "monitor", 
                    "settings", "backup", "logs", "sysinfo", "ssh", "ftp", "db", "sync", "store"
                ];
                
                if !valid_apps.contains(&app_id.as_str()) {
                    return Err(err(format!("Invalid app ID: '{}'. Use list_available_apps to see valid IDs.", app_id)));
                }

                match queue.lock() {
                    Ok(mut q) => {
                        q.push(OsAction::OpenApp(app_id.clone()));
                        Ok(serde_json::json!({ "status": "opening", "app": app_id }))
                    }
                    Err(e) => Err(err(format!("Queue lock failed: {}", e))),
                }
            }
        }),
    }
}

fn get_system_info() -> Tool {
    Tool {
        name: "get_system_info".to_string(),
        description: "Get NeuraOS system information including platform, version, and current time.".to_string(),
        parameters: vec![],
        handler: make_handler(|_args| async move {
            let now = Utc::now();
            Ok(serde_json::json!({
                "os": OS_NAME,
                "version": OS_VERSION,
                "platform": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "current_time_utc": now.to_rfc3339(),
                "description": "AI-native TUI operating system built with Rust + ratatui",
            }))
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// VFS FILES
// ─────────────────────────────────────────────────────────────────────────────

fn read_vfs_file(vfs: Arc<Vfs>) -> Tool {
    Tool {
        name: "read_vfs_file".to_string(),
        description: "Read a raw file from the NeuraOS virtual filesystem. Path must start with /home/.".to_string(),
        parameters: vec![
            ToolParam { name: "path".to_string(), param_type: "string".to_string(), description: "VFS file path (e.g. /home/root/notes/todo.json)".to_string(), required: true },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            async move {
                let path = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'path'"))?.to_string();
                let data = vfs.read_file(&path).await.map_err(|e| err(e.to_string()))?;
                let content = String::from_utf8_lossy(&data).to_string();
                Ok(serde_json::json!({ "path": path, "size": data.len(), "content": content }))
            }
        }),
    }
}

fn list_vfs_directory(vfs: Arc<Vfs>) -> Tool {
    Tool {
        name: "list_vfs_directory".to_string(),
        description: "List files and directories at a VFS path.".to_string(),
        parameters: vec![
            ToolParam { name: "path".to_string(), param_type: "string".to_string(), description: "VFS directory path (e.g. /home/root)".to_string(), required: true },
        ],
        handler: make_handler(move |args| {
            let vfs = vfs.clone();
            async move {
                let path = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| err("Missing 'path'"))?.to_string();
                let entries = vfs.list_dir(&path).await.map_err(|e| err(e.to_string()))?;
                Ok(serde_json::json!({ "path": path, "count": entries.len(), "entries": entries }))
            }
        }),
    }
}

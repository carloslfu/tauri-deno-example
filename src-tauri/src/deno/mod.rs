#![allow(clippy::print_stdout)]
#![allow(clippy::print_stderr)]

mod module_loader;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::sync::Mutex;

use deno_runtime::deno_core::error::AnyError;
use deno_runtime::deno_core::op2;
use deno_runtime::deno_core::ModuleSpecifier;
use deno_runtime::deno_fs::RealFs;
use deno_runtime::deno_permissions::set_prompter;
use deno_runtime::deno_permissions::PermissionPrompter;
use deno_runtime::deno_permissions::Permissions;
use deno_runtime::deno_permissions::PermissionsContainer;
use deno_runtime::deno_permissions::PromptResponse;
use deno_runtime::permissions::RuntimePermissionDescriptorParser;
use deno_runtime::worker::MainWorker;
use deno_runtime::worker::WorkerOptions;
use deno_runtime::worker::WorkerServiceOptions;
use module_loader::TypescriptModuleLoader;
use once_cell::sync::Lazy;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone)]
pub enum PermissionsResponse {
    Allow,
    Deny,
    AllowAll,
}

impl PermissionsResponse {
    pub fn as_str(&self) -> &str {
        match self {
            PermissionsResponse::Allow => "Allow",
            PermissionsResponse::Deny => "Deny",
            PermissionsResponse::AllowAll => "AllowAll",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "Allow" => PermissionsResponse::Allow,
            "Deny" => PermissionsResponse::Deny,
            "AllowAll" => PermissionsResponse::AllowAll,
            _ => panic!("Invalid permissions response: {}", s),
        }
    }

    pub fn to_prompt_response(&self) -> PromptResponse {
        match self {
            PermissionsResponse::Allow => PromptResponse::Allow,
            PermissionsResponse::Deny => PromptResponse::Deny,
            PermissionsResponse::AllowAll => PromptResponse::AllowAll,
        }
    }
}

impl serde::Serialize for PermissionsResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for PermissionsResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self::from_str(&s))
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PermissionPrompt {
    message: String,
    name: String,
    api_name: Option<String>,
    is_unary: bool,
    response: Option<PermissionsResponse>,
}

static PERMISSION_CHANNELS: Lazy<Mutex<HashMap<String, Sender<PermissionsResponse>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Task {
    id: String,
    state: String, // running, completed, error, stopped, waiting_for_permission
    error: String,
    return_value: String,
    permission_prompt: Option<PermissionPrompt>,
    permission_history: Vec<PermissionPrompt>,
}

impl Task {
    fn new(id: String, initial_state: String) -> Self {
        Self {
            id,
            state: initial_state,
            error: "".to_string(),
            return_value: "".to_string(),
            permission_prompt: None,
            permission_history: Vec::new(),
        }
    }
}

static TASK_STATE: Lazy<Mutex<HashMap<String, Task>>> = Lazy::new(|| Mutex::new(HashMap::new()));

#[op2(fast)]
fn return_value(#[string] task_id: &str, #[string] value: &str) {
    let mut state_lock = TASK_STATE.lock().unwrap();
    let task = state_lock.get_mut(task_id).unwrap();
    task.return_value = value.to_string();
}

deno_runtime::deno_core::extension!(
  runtime_extension,
  ops = [return_value],
  esm_entry_point = "ext:runtime_extension/bootstrap.js",
  esm = [dir "src/deno", "bootstrap.js"]
);

pub async fn run(
    app_handle: AppHandle,
    app_path: &Path,
    task_id: &str,
    code: &str,
) -> Result<(), AnyError> {
    // create temp dir
    let temp_dir = std::env::temp_dir().join("tauri_deno_example");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let temp_code_path = temp_dir.join(format!("temp_code_{}.ts", task_id));

    println!("Writing code to {}", temp_code_path.display());

    let augmented_code = format!("globalThis.RuntimeExtension.taskId = \"{task_id}\";\n\n{code}");

    std::fs::write(&temp_code_path, augmented_code).unwrap();

    let main_module = ModuleSpecifier::from_file_path(&temp_code_path).unwrap();
    println!("Running {main_module}...");

    let fs = Arc::new(RealFs);
    let permission_desc_parser = Arc::new(RuntimePermissionDescriptorParser::new(fs.clone()));

    let source_map_store = Rc::new(RefCell::new(HashMap::new()));

    let permission_container =
        PermissionsContainer::new(permission_desc_parser, Permissions::none_with_prompt());

    // Create channel for permission prompts
    let (tx, rx) = channel();
    PERMISSION_CHANNELS
        .lock()
        .unwrap()
        .insert(task_id.to_string(), tx);

    // Initialize task state
    TASK_STATE.lock().unwrap().insert(
        task_id.to_string(),
        Task::new(task_id.to_string(), "running".to_string()),
    );

    let app_handle = app_handle.clone();

    // Clone app_handle before moving it into CustomPrompter
    let app_handle_clone = app_handle.clone();
    set_prompter(Box::new(CustomPrompter::new(
        app_handle_clone,
        task_id.to_string(),
        rx,
    )));

    let mut worker = MainWorker::bootstrap_from_options(
        main_module.clone(),
        WorkerServiceOptions {
            module_loader: Rc::new(TypescriptModuleLoader {
                source_maps: source_map_store,
            }),
            // File only loader
            // module_loader: Rc::new(FsModuleLoader),
            permissions: permission_container,
            blob_store: Default::default(),
            broadcast_channel: Default::default(),
            feature_checker: Default::default(),
            node_services: Default::default(),
            npm_process_state_provider: Default::default(),
            root_cert_store_provider: Default::default(),
            shared_array_buffer_store: Default::default(),
            compiled_wasm_module_store: Default::default(),
            v8_code_cache: Default::default(),
            fs,
        },
        WorkerOptions {
            extensions: vec![runtime_extension::init_ops_and_esm()],
            ..Default::default()
        },
    );

    let result = worker.execute_main_module(&main_module).await;
    if let Err(e) = result {
        let mut state_lock = TASK_STATE.lock().unwrap();
        let task = state_lock.get_mut(task_id).unwrap();
        task.state = "error".to_string();
        task.error = e.to_string();

        let app_handle = app_handle.clone();
        emit_task_state_changed(app_handle, task.clone());
        std::fs::remove_file(&temp_code_path).unwrap();

        return Ok(());
    }

    let result = worker.run_event_loop(false).await;

    if let Err(e) = result {
        let mut state_lock = TASK_STATE.lock().unwrap();
        let task = state_lock.get_mut(task_id).unwrap();
        task.state = "error".to_string();
        task.error = e.to_string();

        emit_task_state_changed(app_handle, task.clone());
        std::fs::remove_file(&temp_code_path).unwrap();

        return Ok(());
    }

    let mut state_lock = TASK_STATE.lock().unwrap();
    let task = state_lock.get_mut(task_id).unwrap();
    task.state = "completed".to_string();

    std::fs::remove_file(&temp_code_path).unwrap();
    emit_task_state_changed(app_handle, task.clone());

    // Clean up permission channel
    PERMISSION_CHANNELS.lock().unwrap().remove(task_id);

    Ok(())
}

pub fn get_task_state(task_id: &str) -> Option<Task> {
    TASK_STATE.lock().unwrap().get(task_id).cloned()
}

pub fn clear_completed_tasks() {
    let mut state_lock = TASK_STATE.lock().unwrap();
    state_lock.retain(|_, task| task.state == "running");
}

pub fn update_task_state(app_handle: AppHandle, task_id: &str, state: &str) {
    println!("Updating task state --");
    let mut state_lock = TASK_STATE.lock().unwrap();
    let task = state_lock.get_mut(task_id).unwrap();
    task.state = state.to_string();

    println!("Emitting task state changed -- 2");
    emit_task_state_changed(app_handle, task.clone());
}

fn emit_task_state_changed(app_handle: AppHandle, task: Task) {
    println!(
        "Emitting task state changed, task_id: {}, state: {}",
        task.id, task.state
    );

    let app_handle = app_handle.clone();

    tokio::spawn({
        async move {
            let result = app_handle.emit("task-state-changed", task);
            if result.is_err() {
                println!("Failed to emit task state changed");
            }

            println!("Task state changed emitted");
        }
    });

    println!("Task state changed emitted 2 --");
}

pub fn respond_to_permission_prompt(task_id: &str, response: PermissionsResponse) {
    if let Some(tx) = PERMISSION_CHANNELS.lock().unwrap().get(task_id) {
        let mut state_lock = TASK_STATE.lock().unwrap();
        if let Some(task) = state_lock.get_mut(task_id) {
            // Update the latest prompt with the response
            if let Some(prompt) = &mut task.permission_prompt {
                prompt.response = Some(response.clone());
            }

            // Update the permission history
            if let Some(last) = task.permission_history.last_mut() {
                last.response = Some(response.clone());
            }
        }

        let _ = tx.send(response);
    }
}

struct CustomPrompter {
    app_handle: AppHandle,
    task_id: String,
    receiver: Arc<Mutex<Receiver<PermissionsResponse>>>,
}

impl CustomPrompter {
    fn new(
        app_handle: AppHandle,
        task_id: String,
        receiver: Receiver<PermissionsResponse>,
    ) -> Self {
        Self {
            app_handle: app_handle.clone(),
            task_id,
            receiver: Arc::new(Mutex::new(receiver)),
        }
    }
}

impl PermissionPrompter for CustomPrompter {
    fn prompt(
        &mut self,
        message: &str,
        name: &str,
        api_name: Option<&str>,
        is_unary: bool,
    ) -> PromptResponse {
        let prompt = PermissionPrompt {
            message: message.to_string(),
            name: name.to_string(),
            api_name: api_name.map(|s| s.to_string()),
            is_unary,
            response: None,
        };

        println!("Prompting for permission: {}", prompt.message);

        let mut state_lock = TASK_STATE.lock().unwrap();
        if let Some(task) = state_lock.get_mut(&self.task_id) {
            // Store as latest prompt
            task.permission_prompt = Some(prompt.clone());
            // Add to history
            task.permission_history.push(prompt);
        }

        println!("Emitting ----");

        // Emit the task state changed event
        let app_handle = self.app_handle.clone();
        update_task_state(app_handle, &self.task_id, "waiting_for_permission");

        println!("Waiting for permission response...");

        match self.receiver.lock().unwrap().recv() {
            Ok(response) => {
                let app_handle = self.app_handle.clone();
                update_task_state(app_handle, &self.task_id, "running");

                response.to_prompt_response()
            }
            Err(_) => {
                let app_handle = self.app_handle.clone();
                update_task_state(app_handle, &self.task_id, "error");

                PromptResponse::Deny
            }
        }
    }
}

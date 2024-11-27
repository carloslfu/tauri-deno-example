#![allow(clippy::print_stdout)]
#![allow(clippy::print_stderr)]

mod module_loader;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

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
use std::sync::Mutex;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskState {
    state: String,
    error: String,
    return_value: String,
}

impl TaskState {
    fn new(initial_state: String) -> Self {
        Self {
            state: initial_state,
            error: "".to_string(),
            return_value: "".to_string(),
        }
    }
}

static TASK_STATE: Lazy<Mutex<HashMap<String, TaskState>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[op2(fast)]
fn return_value(#[string] task_id: &str, #[string] value: &str) {
    let mut state_lock = TASK_STATE.lock().unwrap();
    let task_state = state_lock.get_mut(task_id).unwrap();
    task_state.return_value = value.to_string();
}

deno_runtime::deno_core::extension!(
  runtime_extension,
  ops = [return_value],
  esm_entry_point = "ext:runtime_extension/bootstrap.js",
  esm = [dir "src/deno", "bootstrap.js"]
);

pub async fn run(app_path: &Path, task_id: &str, code: &str) -> Result<(), AnyError> {
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

    set_prompter(Box::new(CustomPrompter::new(task_id.to_string())));

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

    // initialize task state
    TASK_STATE
        .lock()
        .unwrap()
        .insert(task_id.to_string(), TaskState::new("running".to_string()));

    let result = worker.execute_main_module(&main_module).await;
    if let Err(e) = result {
        let mut state_lock = TASK_STATE.lock().unwrap();
        let task_state = state_lock.get_mut(task_id).unwrap();
        task_state.state = "error".to_string();
        task_state.error = e.to_string();
    }

    let result = worker.run_event_loop(false).await;

    if let Err(e) = result {
        let mut state_lock = TASK_STATE.lock().unwrap();
        let task_state = state_lock.get_mut(task_id).unwrap();
        task_state.state = "error".to_string();
        task_state.error = e.to_string();
    }

    std::fs::remove_file(&temp_code_path).unwrap();

    let mut state_lock = TASK_STATE.lock().unwrap();
    let task_state = state_lock.get_mut(task_id).unwrap();
    task_state.state = "completed".to_string();

    Ok(())
}

pub fn get_task_state(task_id: &str) -> Option<TaskState> {
    TASK_STATE.lock().unwrap().get(task_id).cloned()
}

pub fn clear_completed_tasks() {
    let mut state_lock = TASK_STATE.lock().unwrap();
    state_lock.retain(|_, task_state| task_state.state == "running");
}

struct CustomPrompter {
    task_id: String,
}

impl CustomPrompter {
    fn new(task_id: String) -> Self {
        Self { task_id }
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
        println!(
            "{}\n{} {}\n{} {}\n{} {:?}\n{} {:?}\n{} {}",
            "Script is trying to access APIs and needs permission:",
            "Task ID:",
            self.task_id,
            "Message:",
            message,
            "Name:",
            name,
            "API:",
            api_name,
            "Is unary:",
            is_unary
        );
        // println!("Allow? [y/n]");

        return PromptResponse::Allow;

        // let mut input = String::new();
        // if std::io::stdin().read_line(&mut input).is_ok() {
        //     match input.trim().to_lowercase().as_str() {
        //         "y" | "yes" => PromptResponse::Allow,
        //         _ => PromptResponse::Deny,
        //     }
        // } else {
        //     println!("Failed to read input, denying permission");
        //     PromptResponse::Deny
        // }
    }
}

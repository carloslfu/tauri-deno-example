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
use deno_runtime::deno_permissions::Permissions;
use deno_runtime::deno_permissions::PermissionsContainer;
use deno_runtime::permissions::RuntimePermissionDescriptorParser;
use deno_runtime::worker::MainWorker;
use deno_runtime::worker::WorkerOptions;
use deno_runtime::worker::WorkerServiceOptions;
use module_loader::TypescriptModuleLoader;
use once_cell::sync::Lazy;
use std::sync::Mutex;

static RETURN_VALUES: Lazy<Mutex<HashMap<String, String>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[op2(fast)]
fn return_value(#[string] task_id: &str, #[string] value: &str) {
    RETURN_VALUES
        .lock()
        .unwrap()
        .insert(task_id.to_string(), value.to_string());
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

    let temp_code_path = temp_dir.join("temp_code.ts");

    println!("Writing code to {}", temp_code_path.display());

    let augmented_code = format!("globalThis.RuntimeExtension.taskId = \"{task_id}\";\n\n{code}");

    std::fs::write(&temp_code_path, augmented_code).unwrap();

    let main_module = ModuleSpecifier::from_file_path(&temp_code_path).unwrap();
    println!("Running {main_module}...");
    let fs = Arc::new(RealFs);
    let permission_desc_parser = Arc::new(RuntimePermissionDescriptorParser::new(fs.clone()));

    let source_map_store = Rc::new(RefCell::new(HashMap::new()));

    let permission_container =
        PermissionsContainer::new(permission_desc_parser, Permissions::allow_all());

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
        RETURN_VALUES
            .lock()
            .unwrap()
            .insert(task_id.to_string(), e.to_string());
    }

    let result = worker.run_event_loop(false).await;

    if let Err(e) = result {
        RETURN_VALUES
            .lock()
            .unwrap()
            .insert(task_id.to_string(), e.to_string());
    }

    Ok(())
}

pub fn get_return_value(task_id: &str) -> String {
    RETURN_VALUES
        .lock()
        .unwrap()
        .get(task_id)
        .cloned()
        .unwrap_or_default()
}

pub fn clear_return_value(task_id: &str) {
    RETURN_VALUES.lock().unwrap().remove(task_id);
}

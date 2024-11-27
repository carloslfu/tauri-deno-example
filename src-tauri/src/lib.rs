use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;

mod deno;

// Store thread handles and their status
static THREAD_HANDLES: Lazy<Mutex<HashMap<String, std::thread::JoinHandle<Result<(), String>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// shutdown channel map
static SHUTDOWN_CHANNELS: Lazy<Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[tauri::command]
fn run_task(task_id: &str, code: &str) -> Result<(), String> {
    let code = code.to_string();

    let task_id = task_id.to_string();
    let task_id_clone = task_id.clone();

    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();

    SHUTDOWN_CHANNELS
        .lock()
        .unwrap()
        .insert(task_id.clone(), stop_tx);

    let handle = std::thread::spawn(move || {
        println!("Starting runtime");

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;

        println!("Starting async task");

        let _ = runtime.block_on(async {
            tokio::select! {
                _ = deno::run(&task_id_clone, &code) => {},
                _ = stop_rx => {
                    println!("Task cancelled");
                }
            }
        });

        println!("Runtime shutdown");

        // clean up
        SHUTDOWN_CHANNELS.lock().unwrap().remove(&task_id_clone);
        THREAD_HANDLES.lock().unwrap().remove(&task_id_clone);

        Ok(())
    });

    // Store the handle
    THREAD_HANDLES.lock().unwrap().insert(task_id, handle);

    Ok(())
}

#[tauri::command]
fn stop_task(task_id: String) -> Result<(), String> {
    let mut handles = THREAD_HANDLES.lock().unwrap();

    if let Some(handle) = handles.remove(&task_id) {
        // Thread is already finished
        if handle.is_finished() {
            return Ok(());
        }

        // Attempt to stop the thread
        std::thread::spawn(move || {
            // send shutdown message
            let result = SHUTDOWN_CHANNELS
                .lock()
                .unwrap()
                .remove(&task_id)
                .unwrap()
                .send(());

            if result.is_err() {
                println!("Failed to send shutdown message");
            }

            // Wait for thread to complete
            match handle.join() {
                Ok(_) => {}
                Err(_) => {
                    println!("Failed to stop thread");
                }
            };

            deno::update_task_state(&task_id, "stopped");
        });
    }

    Ok(())
}

#[tauri::command]
fn get_task_state(task_id: String) -> Result<deno::Task, String> {
    let Some(task_state) = deno::get_task_state(&task_id) else {
        return Err("Task not found".to_string());
    };

    Ok(task_state)
}

#[tauri::command]
fn clear_completed_tasks() {
    deno::clear_completed_tasks();
}

#[tauri::command]
fn respond_to_permission_prompt(task_id: String, response: String) {
    deno::respond_to_permission_prompt(&task_id, deno::PermissionsResponse::from_str(&response));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // deno::set_app_handle(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            run_task,
            stop_task,
            get_task_state,
            clear_completed_tasks,
            respond_to_permission_prompt
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Manager;

mod deno;

// Store thread handles and their status
static THREAD_HANDLES: Lazy<Mutex<HashMap<String, std::thread::JoinHandle<Result<(), String>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// shutdown channel map
static SHUTDOWN_CHANNELS: Lazy<Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[tauri::command]
fn run_code(app_handle: tauri::AppHandle, task_id: &str, code: &str) -> Result<(), String> {
    let app_path = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;

    let app_path = app_path.clone();
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

        let task = runtime.block_on(async {
            tokio::select! {
                _ = deno::run(&app_path, &task_id_clone, &code) => {},
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
fn stop_code(task_id: String) -> Result<(), String> {
    let mut handles = THREAD_HANDLES.lock().unwrap();

    if let Some(handle) = handles.remove(&task_id) {
        // Thread is already finished
        if handle.is_finished() {
            return Ok(());
        }

        // Attempt to stop the thread
        std::thread::spawn(move || {
            // send shutdown message
            SHUTDOWN_CHANNELS
                .lock()
                .unwrap()
                .remove(&task_id)
                .unwrap()
                .send(())
                .map_err(|_| "Failed to send shutdown signal".to_string())?;

            // Wait for thread to complete
            match handle.join() {
                Ok(_) => Ok(()),
                Err(_) => Err("Failed to stop thread".to_string()),
            }
        });
    }

    Ok(())
}

#[tauri::command]
fn get_return_value(task_id: String) -> Result<String, String> {
    let handles = THREAD_HANDLES.lock().unwrap();

    // Check if thread is still running
    if let Some(handle) = handles.get(&task_id) {
        if !handle.is_finished() {
            return Err("Task still running".to_string());
        }
    }

    let return_value = deno::get_return_value(&task_id);
    deno::clear_return_value(&task_id);
    Ok(return_value)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            run_code,
            stop_code,
            get_return_value
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

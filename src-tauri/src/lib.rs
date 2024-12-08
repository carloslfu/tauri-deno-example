mod deno;

#[tauri::command]
fn run_task(task_id: &str, code: &str) -> Result<(), String> {
    deno::run_task(task_id, code)
}

#[tauri::command]
fn stop_task(task_id: &str) -> Result<(), String> {
    deno::stop_task(task_id)
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
        .setup(move |app| {
            deno::init_listener(app.handle().clone());

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

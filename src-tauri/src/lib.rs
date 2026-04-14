pub mod models;
pub mod core;
pub mod db;
pub mod migration;
pub mod strategies;

#[cfg(feature = "tauri-app")]
mod app {
    #[tauri::command]
    fn greet(name: &str) -> String {
        format!("Hello, {}! Welcome to Bitcoin Trader.", name)
    }

    pub fn run() {
        tauri::Builder::default()
            .invoke_handler(tauri::generate_handler![greet])
            .run(tauri::generate_context!())
            .expect("error while running tauri application");
    }
}

#[cfg(feature = "tauri-app")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    app::run();
}

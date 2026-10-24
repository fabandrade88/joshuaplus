//! Joshua+ desktop application: Tauri shell around `joshua-core` and
//! `joshua-engine`.
#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod backfill;
mod commands;
mod devices;
mod error;
mod inbox;
mod jobs;
mod player;
mod recorder;
mod settings;
mod state;
mod ticker;
mod video;
mod watchdog;

use tauri::Manager;

use crate::jobs::JobQueue;
use crate::state::AppState;

pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(if cfg!(debug_assertions) { log::LevelFilter::Debug } else { log::LevelFilter::Info })
                .build(),
        )
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let models_dir = app.path().app_data_dir()?.join("models");
            app.manage(AppState::new(config_dir.join("settings.json"), models_dir, JobQueue::default()));
            let handle = app.handle().clone();
            let state = app.state::<AppState>();
            state.start_engine();
            devices::restart_inputs(&state);
            devices::connect_midi(&handle);
            state.jobs.start_worker(handle.clone());
            inbox::start(handle.clone());
            backfill::start(handle.clone());
            watchdog::start(handle.clone());
            ticker::start(handle);
            Ok(())
        })
        // The video studio needs the webcam; the microphone is never needed
        // (audio comes from the mixer), and nothing else is granted.
        .on_permission_request(|_, kind| match kind {
            tauri::webview::PermissionKind::Camera => tauri::webview::PermissionResponse::Allow,
            _ => tauri::webview::PermissionResponse::Deny,
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                ticker::flush_dirty(&window.state::<AppState>());
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::set_locale,
            commands::set_separator,
            commands::set_workspace,
            commands::list_songs,
            commands::rename_song,
            commands::import_files,
            commands::cancel_job,
            commands::delete_song,
            commands::load_song,
            commands::play,
            commands::pause,
            commands::seek,
            commands::set_loop,
            commands::update_track,
            commands::update_metronome,
            commands::edit_tempo,
            commands::reanalyze_tempo,
            commands::update_song_bus,
            commands::set_master_gain,
            commands::list_output_devices,
            commands::set_output_device,
            commands::list_folders,
            commands::create_folder,
            commands::update_folder,
            commands::delete_folder,
            commands::move_song,
            commands::reorder_folder,
            commands::download_model,
            commands::set_cloud_key,
            commands::clear_cloud_key,
            devices::list_audio_inputs,
            devices::get_inputs,
            devices::add_audio_input,
            devices::remove_audio_input,
            devices::update_audio_input,
            devices::get_drums,
            devices::set_midi_port,
            devices::set_drum_brand,
            devices::drum_learn,
            commands::start_recording,
            commands::stop_recording,
            commands::update_take,
            commands::delete_take,
            commands::set_record_offset,
            commands::export_mix,
            commands::reveal_export,
            commands::list_exports,
            commands::open_export,
            commands::open_export_dir,
            commands::set_export_dir,
            video::render_clip_audio,
            video::video_save_begin,
            video::video_save_chunk,
            video::video_save_end,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("Joshua+ failed to start: {e}");
        std::process::exit(1);
    }
}

mod atp;
pub mod audit_labor;
pub mod audit_profile;
mod audit_runtime;
mod bundle;
mod commands;
mod p2p;
mod state;
mod store;
mod worker;

use commands::{
    accept_offer, approve_result, connect_peer, create_audit, create_protocol_campaign,
    export_campaign_report, get_campaign_snapshot, get_credit_summary, get_network_info, get_peers,
    list_audits, list_local_model_models, list_local_model_providers, list_protocol_campaigns,
    migrate_legacy_jobs, offer_audit, record_campaign_contribution, route_audit, run_audit,
    run_campaign_audit_skill, start_node, verify_campaign_contribution,
};
use state::P2pState;
use store::AtpStore;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show/Hide", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let mut tray = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                    } else {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        });

    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }

    tray.build(app)?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let store = AtpStore::open_default().expect("failed to initialize ATP database");
    tauri::Builder::default()
        .manage(P2pState::default())
        .manage(store)
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            setup_tray(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_node,
            get_network_info,
            connect_peer,
            get_peers,
            list_audits,
            list_local_model_providers,
            list_local_model_models,
            list_protocol_campaigns,
            get_campaign_snapshot,
            create_audit,
            create_protocol_campaign,
            record_campaign_contribution,
            run_campaign_audit_skill,
            verify_campaign_contribution,
            export_campaign_report,
            get_credit_summary,
            offer_audit,
            accept_offer,
            route_audit,
            run_audit,
            approve_result,
            migrate_legacy_jobs
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

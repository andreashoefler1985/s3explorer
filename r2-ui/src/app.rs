//! r2-ui — Main application logic
//!
//! GTK4 Application with dual-pane browser, profile manager, and menu bar.

use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Label,
    MenuButton, Orientation, Paned, PopoverMenu, ToggleButton,
};
use std::sync::Arc;
use tracing::{error, info};

use r2_core::cache::manager::{CacheManager, SqliteCacheManager};
use r2_core::credentials::storage::{CredentialStorage, LibsecretCredentialStorage};
use r2_core::events::PaneId;

use crate::pane::S3Pane;
use crate::profile_manager::ProfileManagerDialog;

/// Main r2 application
pub struct R2App {
    app: Application,
    storage: Arc<dyn CredentialStorage>,
    cache: Arc<dyn CacheManager>,
    main_window: Option<ApplicationWindow>,
}

impl R2App {
    /// Create a new r2 application
    pub fn new() -> Self {
        let app = Application::builder()
            .application_id("com.r2.s3-browser")
            .build();

        let storage = match LibsecretCredentialStorage::new() {
            Ok(s) => {
                info!("Credential storage initialized");
                Arc::new(s) as Arc<dyn CredentialStorage>
            }
            Err(e) => {
                error!("Failed to initialize credential storage: {}", e);
                let config_dir = dirs_config_dir();
                Arc::new(
                    r2_core::credentials::storage::EncryptedFileBackend::new(config_dir)
                ) as Arc<dyn CredentialStorage>
            }
        };

        let cache = match SqliteCacheManager::new(dirs_config_dir()) {
            Ok(c) => {
                info!("Cache manager initialized");
                Arc::new(c) as Arc<dyn CacheManager>
            }
            Err(e) => {
                error!("Failed to initialize cache: {}", e);
                let config_dir = dirs_config_dir();
                Arc::new(
                    SqliteCacheManager::new(config_dir).expect("Failed to create cache fallback")
                ) as Arc<dyn CacheManager>
            }
        };

        Self {
            app,
            storage,
            cache,
            main_window: None,
        }
    }

    /// Run the application
    pub fn run(&mut self) {
        let storage = self.storage.clone();
        let cache = self.cache.clone();

        self.app.connect_activate(move |app| {
            // Create main window
            let window = ApplicationWindow::builder()
                .application(app)
                .title("r2 — S3 Object Storage Browser")
                .default_width(1200)
                .default_height(800)
                .build();

            // Create menu bar
            let menu_bar = create_menu_bar(app, &storage);

            // ── Global Toolbar ──
            let global_toolbar = create_global_toolbar();

            // ── Dual-Pane Layout ──
            let paned = Paned::builder()
                .orientation(Orientation::Horizontal)
                .hexpand(true)
                .vexpand(true)
                .wide_handle(true)
                .resize_start_child(false)
                .resize_end_child(false)
                .shrink_start_child(false)
                .shrink_end_child(false)
                .build();

            // Create Pane A
            let pane_a = std::sync::Mutex::new(S3Pane::new(PaneId::A));
            {
                let mut pane = pane_a.lock().expect("Lock pane A");
                pane.set_parent_window(&window);
                pane.set_cache(cache.clone());
            }
            paned.set_start_child(Some(&pane_a.lock().expect("Lock pane A").container));

            // Create Pane B
            let pane_b = std::sync::Mutex::new(S3Pane::new(PaneId::B));
            {
                let mut pane = pane_b.lock().expect("Lock pane B");
                pane.set_parent_window(&window);
                pane.set_cache(cache.clone());
            }
            paned.set_end_child(Some(&pane_b.lock().expect("Lock pane B").container));

            // Set initial position (50:50)
            paned.set_position(600);

            // ── Transfer Queue Panel (collapsed placeholder) ──
            let transfer_queue_box = create_transfer_queue_panel();

            // ── Status Bar ──
            let status_bar = create_status_bar();

            // ── Main Layout ──
            let main_layout = GtkBox::builder()
                .orientation(Orientation::Vertical)
                .build();
            main_layout.append(&menu_bar);
            main_layout.append(&global_toolbar);
            main_layout.append(&paned);
            main_layout.append(&transfer_queue_box);
            main_layout.append(&status_bar);

            window.set_child(Some(&main_layout));
            window.present();

            // Show profile manager on startup
            let storage_clone = storage.clone();
            let window_clone = window.clone();

            glib::MainContext::default().spawn_local(async move {
                let profile_manager = ProfileManagerDialog::new(&window_clone, storage_clone);
                profile_manager.show();
                info!("Dual-pane browser ready");
            });
        });

        self.app.run();
    }
}

/// Create the menu bar
fn create_menu_bar(app: &Application, storage: &Arc<dyn CredentialStorage>) -> GtkBox {
    let menu_bar = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(4)
        .margin_start(4)
        .margin_end(4)
        .margin_top(4)
        .margin_bottom(4)
        .css_classes(["toolbar"])
        .build();

    // File menu
    let file_menu = create_file_menu(app, storage);
    menu_bar.append(&file_menu);

    // View menu
    let view_menu = create_view_menu(app);
    menu_bar.append(&view_menu);

    // Help menu
    let help_menu = create_help_menu(app);
    menu_bar.append(&help_menu);

    menu_bar
}

/// Create the File menu
fn create_file_menu(app: &Application, storage: &Arc<dyn CredentialStorage>) -> MenuButton {
    let menu_button = MenuButton::builder()
        .label("Datei")
        .primary(true)
        .build();

    let popover = PopoverMenu::from_model(None::<&gio::Menu>);
    let menu_model = gio::Menu::new();

    // Profile manager action
    let storage_clone = storage.clone();
    let app_clone = app.clone();
    let profile_action = gio::SimpleAction::new("profiles", None);
    profile_action.connect_activate(move |_, _| {
        if let Some(window) = app_clone.active_window() {
            if let Some(app_window) = window.downcast_ref::<ApplicationWindow>() {
                let profile_manager = ProfileManagerDialog::new(app_window, storage_clone.clone());
                profile_manager.show();
            }
        }
    });
    app.add_action(&profile_action);
    menu_model.append(Some("Profile"), Some("app.profiles"));

    // Separator
    menu_model.append(Some("Trennlinie"), None);

    // Quit action
    let app_clone2 = app.clone();
    let quit_action = gio::SimpleAction::new("quit", None);
    quit_action.connect_activate(move |_, _| {
        app_clone2.quit();
    });
    app.add_action(&quit_action);
    menu_model.append(Some("Beenden"), Some("app.quit"));

    popover.set_menu_model(Some(&menu_model));
    menu_button.set_popover(Some(&popover));

    menu_button
}

/// Create the View menu
fn create_view_menu(app: &Application) -> MenuButton {
    let menu_button = MenuButton::builder()
        .label("Ansicht")
        .primary(true)
        .build();

    let popover = PopoverMenu::from_model(None::<&gio::Menu>);
    let menu_model = gio::Menu::new();

    // Refresh action
    let refresh_action = gio::SimpleAction::new("refresh", None);
    refresh_action.connect_activate(move |_, _| {
        info!("Refresh requested");
    });
    app.add_action(&refresh_action);
    menu_model.append(Some("↻ Aktualisieren"), Some("app.refresh"));

    // Toggle transfer queue
    let toggle_queue_action = gio::SimpleAction::new("toggle-queue", None);
    toggle_queue_action.connect_activate(move |_, _| {
        info!("Toggle transfer queue");
    });
    app.add_action(&toggle_queue_action);
    menu_model.append(Some("🔲 Transfer-Queue umschalten"), Some("app.toggle-queue"));

    popover.set_menu_model(Some(&menu_model));
    menu_button.set_popover(Some(&popover));

    menu_button
}

/// Create the Help menu
fn create_help_menu(app: &Application) -> MenuButton {
    let menu_button = MenuButton::builder()
        .label("Hilfe")
        .primary(true)
        .build();

    let popover = PopoverMenu::from_model(None::<&gio::Menu>);
    let menu_model = gio::Menu::new();

    // About action
    let app_clone = app.clone();
    let about_action = gio::SimpleAction::new("about", None);
    about_action.connect_activate(move |_, _| {
        if let Some(window) = app_clone.active_window() {
            let about = gtk4::AboutDialog::builder()
                .program_name("r2")
                .version("0.1.0")
                .comments("S3-kompatibler Object-Storage-Browser")
                .license_type(gtk4::License::MitX11)
                .build();
            about.set_transient_for(Some(&window));
            about.present();
        }
    });
    app.add_action(&about_action);
    menu_model.append(Some("Über r2"), Some("app.about"));

    popover.set_menu_model(Some(&menu_model));
    menu_button.set_popover(Some(&popover));

    menu_button
}

/// Create the global toolbar (above the panes)
fn create_global_toolbar() -> GtkBox {
    let toolbar = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_start(8)
        .margin_end(8)
        .margin_top(4)
        .margin_bottom(4)
        .css_classes(["toolbar"])
        .build();

    let actions_label = Label::builder()
        .label("Aktionen:")
        .build();
    toolbar.append(&actions_label);

    let actions_btn = MenuButton::builder()
        .label("≡ Aktionen")
        .primary(true)
        .build();

    let popover = PopoverMenu::from_model(None::<&gio::Menu>);
    let menu_model = gio::Menu::new();
    menu_model.append(Some("📤 Upload..."), Some("app.upload"));
    menu_model.append(Some("📁 Neuen Ordner erstellen"), Some("app.new-folder"));
    menu_model.append(Some("Trennlinie"), None);
    menu_model.append(Some("🔲 Transfer-Queue anzeigen"), Some("app.toggle-queue"));

    popover.set_menu_model(Some(&menu_model));
    actions_btn.set_popover(Some(&popover));
    toolbar.append(&actions_btn);

    toolbar
}

/// Create the transfer queue panel (collapsed by default)
fn create_transfer_queue_panel() -> GtkBox {
    let panel = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .margin_start(8)
        .margin_end(8)
        .margin_top(4)
        .margin_bottom(4)
        .css_classes(["transfer-queue"])
        .build();

    let header = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .build();

    let toggle_btn = ToggleButton::builder()
        .label("🔲 Transfer-Queue ▸")
        .build();

    let queue_info = Label::builder()
        .label("0 aktiv")
        .halign(gtk4::Align::Start)
        .hexpand(true)
        .build();

    header.append(&toggle_btn);
    header.append(&queue_info);
    panel.append(&header);

    // Queue content (hidden by default, will be expanded in Sprint 3)
    let queue_content = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .margin_start(16)
        .build();

    let empty_label = Label::builder()
        .label("Keine aktiven Transfers. Ziehe Dateien zwischen Panes, um einen Transfer zu starten.")
        .halign(gtk4::Align::Start)
        .build();
    queue_content.append(&empty_label);

    // Initially hidden
    queue_content.set_visible(false);

    let queue_content_clone = queue_content.clone();
    toggle_btn.connect_toggled(move |btn| {
        let visible = btn.is_active();
        queue_content_clone.set_visible(visible);
        if visible {
            btn.set_label("🔲 Transfer-Queue ▾");
        } else {
            btn.set_label("🔲 Transfer-Queue ▸");
        }
    });

    panel.append(&queue_content);

    panel
}

/// Create the status bar
fn create_status_bar() -> GtkBox {
    let status_bar = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_start(8)
        .margin_end(8)
        .margin_top(4)
        .margin_bottom(4)
        .css_classes(["statusbar"])
        .build();

    let status_label = Label::builder()
        .label("Bereit")
        .halign(gtk4::Align::Start)
        .hexpand(true)
        .build();
    status_bar.append(&status_label);

    status_bar
}

/// Get the config directory path (~/.config/r2)
fn dirs_config_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/tmp".to_string());
    std::path::PathBuf::from(home).join(".config").join("r2")
}

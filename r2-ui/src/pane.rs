//! r2-ui — S3Pane widget for the dual-pane browser
//!
//! Each S3Pane is an independent browser pane with its own:
//! - Bucket selector (GtkDropDown)
//! - Breadcrumb navigation (path entry)
//! - Object list (GtkColumnView with lazy loading)
//! - Status bar
//! - Context menus for objects and buckets

use chrono::{DateTime, Utc};
use gtk4::gdk::{ContentProvider, DragAction};
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, ColumnView, ColumnViewColumn,
    DragSource, DropDown, DropTarget, Entry, Label, NoSelection,
    Orientation, ScrolledWindow, SignalListItemFactory,
    StringList, StringObject,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use r2_core::cache::manager::CacheManager;
use r2_core::events::PaneId;
use r2_core::s3_client::client::S3Client;
use r2_core::s3_client::types::{BucketInfo, ObjectInfo};
use r2_core::transfer::{TransferDirection, TransferJob, TransferSource, TransferDestination};

use crate::dialogs::properties_dialog::{bytes_to_human, format_relative_time};

/// Number of objects to load per page
const PAGE_SIZE: i32 = 100;

/// Data for a single object row in the ColumnView
#[derive(Debug, Clone)]
struct ObjectRow {
    key: String,
    size: i64,
    last_modified: Option<DateTime<Utc>>,
    storage_class: Option<String>,
    is_prefix: bool,
    display_name: String,
    display_size: String,
    display_type: String,
    display_modified: String,
}

impl ObjectRow {
    fn from_object_info(obj: &ObjectInfo) -> Self {
        let key = obj.key.clone();
        // Extract display name (last segment after /)
        let display_name = if obj.is_prefix {
            // Remove trailing slash for display
            key.trim_end_matches('/').to_string()
        } else {
            // Get the last segment after the last /
            key.rsplit('/').next().unwrap_or(&key).to_string()
        };

        let display_size = if obj.is_prefix {
            String::new()
        } else {
            bytes_to_human(obj.size)
        };

        let display_type = if obj.is_prefix {
            "Ordner".to_string()
        } else {
            obj.storage_class.clone().unwrap_or_else(|| "STANDARD".to_string())
        };

        let display_modified = obj.last_modified
            .as_ref()
            .map(|dt| format_relative_time(dt))
            .unwrap_or_default();

        Self {
            key,
            size: obj.size,
            last_modified: obj.last_modified,
            storage_class: obj.storage_class.clone(),
            is_prefix: obj.is_prefix,
            display_name,
            display_size,
            display_type,
            display_modified,
        }
    }
}

/// S3Pane widget — an independent browser pane
pub struct S3Pane {
    pub container: gtk4::Box,
    pub bucket_selector: DropDown,
    pub path_entry: Entry,
    pub object_list: ColumnView,
    pub status_bar: Label,
    pub refresh_btn: Button,
    pub up_btn: Button,
    pub profile_selector: DropDown,

    // Internal state
    profile_id: Option<Uuid>,
    current_bucket: Option<String>,
    current_prefix: String,
    s3_client: Option<Arc<dyn S3Client>>,
    cache: Option<Arc<dyn CacheManager>>,
    pane_id: PaneId,

    // Object list state
    objects: Vec<ObjectRow>,
    all_loaded: bool,
    is_loading: Arc<AtomicBool>,
    sort_column: u32,
    sort_ascending: bool,

    // Bucket list
    buckets: Vec<BucketInfo>,
    bucket_names: Vec<String>,

    // Profile names for the profile selector
    profile_names: Vec<String>,
    profile_ids: Vec<Uuid>,

    // Parent window reference for dialogs
    parent_window: Option<gtk4::ApplicationWindow>,
}

impl S3Pane {
    /// Create a new S3Pane widget
    pub fn new(pane_id: PaneId) -> Self {
        let container = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(0)
            .build();

        // ── Pane Header ──
        let header = create_pane_header(pane_id);
        container.append(&header);

        // ── Toolbar: Profile + Bucket + Path ──
        let toolbar = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(4)
            .margin_start(4)
            .margin_end(4)
            .margin_top(4)
            .margin_bottom(4)
            .css_classes(["toolbar"])
            .build();

        // Profile selector
        let profile_store = StringList::new(&[] as &[&str]);
        let profile_selector = DropDown::builder()
            .model(&profile_store)
            .hexpand(false)
            .build();
        profile_selector.set_tooltip_text(Some("Profil auswählen"));

        let profile_label = Label::builder()
            .label("Profil:")
            .build();
        toolbar.append(&profile_label);
        toolbar.append(&profile_selector);

        // Bucket selector
        let bucket_store = StringList::new(&[] as &[&str]);
        let bucket_selector = DropDown::builder()
            .model(&bucket_store)
            .hexpand(false)
            .build();
        bucket_selector.set_tooltip_text(Some("Bucket auswählen"));

        let bucket_label = Label::builder()
            .label("Bucket:")
            .build();
        toolbar.append(&bucket_label);
        toolbar.append(&bucket_selector);

        // Path entry (breadcrumb)
        let path_entry = Entry::builder()
            .placeholder_text("Pfad (z.B. images/2024/)")
            .hexpand(true)
            .build();
        path_entry.set_tooltip_text(Some("Pfad eingeben und Enter drücken"));

        toolbar.append(&path_entry);

        // Refresh button
        let refresh_btn = Button::builder()
            .label("↻")
            .tooltip_text("Aktualisieren")
            .build();
        toolbar.append(&refresh_btn);

        // Up button
        let up_btn = Button::builder()
            .label("↑")
            .tooltip_text("Eine Ebene höher")
            .build();
        toolbar.append(&up_btn);

        container.append(&toolbar);

        // ── Object List (ColumnView) ──
        let (object_list, scrolled) = create_object_list();
        container.append(&scrolled);

        // ── Status Bar ──
        let status_bar = Label::builder()
            .label("Bereit")
            .halign(Align::Start)
            .margin_start(8)
            .margin_end(8)
            .margin_top(2)
            .margin_bottom(2)
            .css_classes(["statusbar"])
            .build();
        container.append(&status_bar);

        Self {
            container,
            bucket_selector,
            path_entry,
            object_list,
            status_bar,
            refresh_btn,
            up_btn,
            profile_selector,
            profile_id: None,
            current_bucket: None,
            current_prefix: String::new(),
            s3_client: None,
            cache: None,
            pane_id,
            objects: Vec::new(),
            all_loaded: false,
            is_loading: Arc::new(AtomicBool::new(false)),
            sort_column: 0,
            sort_ascending: true,
            buckets: Vec::new(),
            bucket_names: Vec::new(),
            profile_names: Vec::new(),
            profile_ids: Vec::new(),
            parent_window: None,
        }
    }

    /// Set the parent window for dialogs
    pub fn set_parent_window(&mut self, window: &gtk4::ApplicationWindow) {
        self.parent_window = Some(window.clone());
    }

    /// Get a reference to the parent window
    fn parent(&self) -> Option<gtk4::ApplicationWindow> {
        self.parent_window.clone()
    }

    /// Set the S3 client for this pane
    pub fn set_s3_client(&mut self, client: Arc<dyn S3Client>) {
        self.s3_client = Some(client);
    }

    /// Set the cache manager for this pane
    pub fn set_cache(&mut self, cache: Arc<dyn CacheManager>) {
        self.cache = Some(cache);
    }

    /// Set the profile for this pane
    pub fn set_profile(&mut self, profile_id: Uuid) {
        self.profile_id = Some(profile_id);
        self.current_bucket = None;
        self.current_prefix = String::new();
        self.objects.clear();
        self.all_loaded = false;
        self.update_status("Profil geladen. Bucket auswählen...");
    }

    /// Get the current profile ID
    pub fn profile_id(&self) -> Option<Uuid> {
        self.profile_id
    }

    /// Get the current bucket name
    pub fn current_bucket(&self) -> Option<&str> {
        self.current_bucket.as_deref()
    }

    /// Get the current prefix
    pub fn current_prefix(&self) -> &str {
        &self.current_prefix
    }

    /// Update the profile selector with available profiles
    pub fn update_profiles(&mut self, names: Vec<String>, ids: Vec<Uuid>) {
        self.profile_names = names.clone();
        self.profile_ids = ids;

        let store = StringList::new(&[]);
        let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        store.splice(0, 0, &refs);
        self.profile_selector.set_model(Some(&store));
    }

    /// Load buckets from the S3 client
    pub fn load_buckets(&mut self) {
        let client = match self.s3_client.clone() {
            Some(c) => c,
            None => {
                self.update_status("Kein S3-Client verbunden");
                return;
            }
        };

        let profile_id = match self.profile_id {
            Some(id) => id,
            None => {
                self.update_status("Kein Profil ausgewählt");
                return;
            }
        };

        let cache = self.cache.clone();
        let status_clone = self.status_bar.clone();
        let bucket_selector = self.bucket_selector.clone();
        let pane_id = self.pane_id;

        self.update_status("Lade Buckets...");

        glib::MainContext::default().spawn_local(async move {
            // Try cache first
            if let Some(ref cache) = cache {
                match cache.get_cached_buckets(&profile_id) {
                    Ok(cached_buckets) if !cached_buckets.is_empty() => {
                        let names: Vec<String> = cached_buckets.iter().map(|b| b.name.clone()).collect();
                        let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
                        let store = StringList::new(&[]);
                        store.splice(0, 0, &refs);
                        bucket_selector.set_model(Some(&store));
                        status_clone.set_label(&format!("📡 Offline — {} Buckets (Cache)", names.len()));
                        debug!(pane = %pane_id, count = names.len(), "Buckets loaded from cache");
                        return;
                    }
                    Ok(_) => {}
                    Err(e) => {
                        warn!(pane = %pane_id, error = %e, "Cache read failed");
                    }
                }
            }

            // Network request
            match client.list_buckets().await {
                Ok(buckets) => {
                    let names: Vec<String> = buckets.iter().map(|b| b.name.clone()).collect();
                    let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
                    let store = StringList::new(&[]);
                    store.splice(0, 0, &refs);
                    bucket_selector.set_model(Some(&store));

                    // Update cache
                    if let Some(ref cache) = cache {
                        if let Err(e) = cache.cache_buckets(&profile_id, &buckets) {
                            warn!(pane = %pane_id, error = %e, "Failed to cache buckets");
                        }
                    }

                    status_clone.set_label(&format!("{} Buckets geladen", names.len()));
                    debug!(pane = %pane_id, count = names.len(), "Buckets loaded from network");
                }
                Err(e) => {
                    error!(pane = %pane_id, error = %e, "Failed to load buckets");
                    status_clone.set_label(&format!("❌ Fehler: {}", e));
                }
            }
        });
    }

    /// Load objects for the current bucket and prefix
    pub fn load_objects(&mut self, reset: bool) {
        if reset {
            self.objects.clear();
            self.all_loaded = false;
        }

        if self.all_loaded {
            return;
        }

        if self.is_loading.load(Ordering::SeqCst) {
            return;
        }
        self.is_loading.store(true, Ordering::SeqCst);

        let client = match self.s3_client.clone() {
            Some(c) => c,
            None => {
                self.is_loading.store(false, Ordering::SeqCst);
                self.update_status("Kein S3-Client verbunden");
                return;
            }
        };

        let bucket = match self.current_bucket.clone() {
            Some(b) => b,
            None => {
                self.is_loading.store(false, Ordering::SeqCst);
                self.update_status("Kein Bucket ausgewählt");
                return;
            }
        };

        let prefix = self.current_prefix.clone();
        let profile_id = match self.profile_id {
            Some(id) => id,
            None => {
                self.is_loading.store(false, Ordering::SeqCst);
                return;
            }
        };

        let cache = self.cache.clone();
        let status_clone = self.status_bar.clone();
        let pane_id = self.pane_id;
        let is_loading = self.is_loading.clone();

        // Determine start_after for pagination
        let start_after = if reset {
            None
        } else {
            self.objects.last().map(|o| o.key.clone())
        };

        self.update_status("Lade Objekte...");

        glib::MainContext::default().spawn_local(async move {
            // Try cache first if reset (initial load)
            if reset {
                if let Some(ref cache) = cache {
                    match cache.get_cached_objects(&profile_id, &bucket, &prefix) {
                        Ok(cached_objects) if !cached_objects.is_empty() => {
                            let total_size: i64 = cached_objects.iter().map(|o| o.size).sum();
                            status_clone.set_label(&format!(
                                "📡 Offline — {} Objekte | {} (Cache)",
                                cached_objects.len(),
                                bytes_to_human(total_size)
                            ));
                            is_loading.store(false, Ordering::SeqCst);
                            return;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            warn!(pane = %pane_id, error = %e, "Cache read failed");
                        }
                    }
                }
            }

            // Network request
            match client.list_objects(&bucket, &prefix, "/", PAGE_SIZE, start_after).await {
                Ok(new_objects) => {
                    // Update cache on initial load
                    if reset {
                        if let Some(ref cache) = cache {
                            if let Err(e) = cache.cache_objects(&profile_id, &bucket, &prefix, &new_objects) {
                                warn!(pane = %pane_id, error = %e, "Failed to cache objects");
                            }
                        }
                    }

                    let total = new_objects.len();
                    let total_size: i64 = new_objects.iter().map(|o| o.size).sum();
                    status_clone.set_label(&format!("{} Objekte | {}", total, bytes_to_human(total_size)));
                    is_loading.store(false, Ordering::SeqCst);
                }
                Err(e) => {
                    error!(pane = %pane_id, error = %e, "Failed to load objects");
                    status_clone.set_label(&format!("❌ Fehler: {}", e));
                    is_loading.store(false, Ordering::SeqCst);
                }
            }
        });
    }

    /// Navigate to a prefix
    pub fn navigate_to(&mut self, prefix: &str) {
        self.current_prefix = prefix.to_string();
        self.path_entry.set_text(&self.current_prefix);
        self.load_objects(true);
    }

    /// Navigate one level up
    pub fn navigate_up(&mut self) {
        let parent = parent_prefix(&self.current_prefix);
        self.navigate_to(&parent);
    }

    /// Refresh the current view
    pub fn refresh(&mut self) {
        if self.current_bucket.is_some() {
            self.load_objects(true);
        } else {
            self.load_buckets();
        }
    }

    /// Update the status bar text
    fn update_status(&self, text: &str) {
        self.status_bar.set_label(text);
    }

    /// Get the selected object keys
    pub fn selected_objects(&self) -> Vec<String> {
        self.objects.iter().map(|o| o.key.clone()).collect()
    }

    /// Delete an object by key
    pub fn delete_object(&mut self, key: &str) {
        let client = match self.s3_client.clone() {
            Some(c) => c,
            None => return,
        };
        let bucket = match self.current_bucket.clone() {
            Some(b) => b,
            None => return,
        };
        let key_owned = key.to_string();
        let status_clone = self.status_bar.clone();
        let pane_id = self.pane_id;

        self.update_status(&format!("Lösche {}...", key));

        glib::MainContext::default().spawn_local(async move {
            match client.delete_object(&bucket, &key_owned).await {
                Ok(()) => {
                    info!(pane = %pane_id, key = %key_owned, "Object deleted");
                    status_clone.set_label(&format!("🗑️ {} gelöscht", key_owned));
                }
                Err(e) => {
                    error!(pane = %pane_id, key = %key_owned, error = %e, "Failed to delete object");
                    status_clone.set_label(&format!("❌ Fehler beim Löschen: {}", e));
                }
            }
        });
    }

    /// Rename an object (copy + delete)
    pub fn rename_object(&mut self, old_key: &str, new_key: &str) {
        let client = match self.s3_client.clone() {
            Some(c) => c,
            None => return,
        };
        let bucket = match self.current_bucket.clone() {
            Some(b) => b,
            None => return,
        };
        let old_key_owned = old_key.to_string();
        let new_key_owned = new_key.to_string();
        let status_clone = self.status_bar.clone();
        let pane_id = self.pane_id;

        self.update_status(&format!("Benenne {} um...", old_key));

        glib::MainContext::default().spawn_local(async move {
            // Copy to new key
            match client.copy_object(&bucket, &old_key_owned, &bucket, &new_key_owned).await {
                Ok(()) => {
                    // Delete old key
                    match client.delete_object(&bucket, &old_key_owned).await {
                        Ok(()) => {
                            info!(pane = %pane_id, old = %old_key_owned, new = %new_key_owned, "Object renamed");
                            status_clone.set_label(&format!("✏️ {} → {}", old_key_owned, new_key_owned));
                        }
                        Err(e) => {
                            error!(pane = %pane_id, error = %e, "Failed to delete old key after copy");
                            status_clone.set_label(&format!("⚠️ Kopiert, aber altes Objekt konnte nicht gelöscht werden: {}", e));
                        }
                    }
                }
                Err(e) => {
                    error!(pane = %pane_id, error = %e, "Failed to copy object for rename");
                    status_clone.set_label(&format!("❌ Fehler beim Umbenennen: {}", e));
                }
            }
        });
    }

    /// Show the context menu for an object
    pub fn show_object_context_menu(&self, _key: &str, _is_prefix: bool) {
        let parent = match self.parent() {
            Some(p) => p,
            None => return,
        };

        let popover = gtk4::PopoverMenu::from_model(None::<&gio::Menu>);
        let menu_model = gio::Menu::new();

        if _is_prefix {
            menu_model.append(Some("📂 Ordner öffnen"), Some("pane.open-folder"));
        } else {
            menu_model.append(Some("📥 Herunterladen..."), Some("pane.download"));
        }
        menu_model.append(Some("📋 Pfad kopieren"), Some("pane.copy-path"));
        menu_model.append(Some("🔗 URL kopieren"), Some("pane.copy-url"));
        menu_model.append(Some("Trennlinie"), None);
        menu_model.append(Some("✏️ Umbenennen"), Some("pane.rename"));
        menu_model.append(Some("Trennlinie"), None);
        // Versioning entry (only for non-prefix objects)
        if !_is_prefix {
            menu_model.append(Some("🔄 Versionen anzeigen"), Some("pane.show-versions"));
        }
        menu_model.append(Some("🔒 ACL bearbeiten..."), Some("pane.edit-acl"));
        menu_model.append(Some("Trennlinie"), None);
        menu_model.append(Some("ℹ️ Eigenschaften"), Some("pane.properties"));
        menu_model.append(Some("Trennlinie"), None);
        menu_model.append(Some("🗑️ Löschen..."), Some("pane.delete"));

        popover.set_menu_model(Some(&menu_model));
        popover.set_parent(&parent);
        popover.popup();
    }

    /// Show the context menu for a bucket
    pub fn show_bucket_context_menu(&self, _bucket_name: &str) {
        let parent = match self.parent() {
            Some(p) => p,
            None => return,
        };

        let popover = gtk4::PopoverMenu::from_model(None::<&gio::Menu>);
        let menu_model = gio::Menu::new();

        menu_model.append(Some("📂 Bucket öffnen"), Some("pane.open-bucket"));
        menu_model.append(Some("Trennlinie"), None);
        menu_model.append(Some("➕ Bucket erstellen..."), Some("pane.create-bucket"));
        menu_model.append(Some("🗑️ Bucket löschen..."), Some("pane.delete-bucket"));
        menu_model.append(Some("Trennlinie"), None);
        menu_model.append(Some("🔄 Versioning aktivieren/deaktivieren"), Some("pane.toggle-versioning"));
        menu_model.append(Some("🔒 ACL bearbeiten..."), Some("pane.edit-bucket-acl"));
        menu_model.append(Some("Trennlinie"), None);
        menu_model.append(Some("ℹ️ Eigenschaften"), Some("pane.bucket-properties"));

        popover.set_menu_model(Some(&menu_model));
        popover.set_parent(&parent);
        popover.popup();
    }

    /// Set the offline indicator in the status bar
    pub fn set_offline_indicator(&self, offline: bool) {
        if offline {
            self.status_bar.set_label("📡 Offline — Zeige gecachte Daten");
            self.status_bar.add_css_class("offline");
        } else {
            self.status_bar.remove_css_class("offline");
        }
    }
}

/// Create the pane header with a label
fn create_pane_header(pane_id: PaneId) -> GtkBox {
    let header = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(4)
        .margin_start(8)
        .margin_end(8)
        .margin_top(4)
        .margin_bottom(2)
        .build();

    let label_text = match pane_id {
        PaneId::A => "Pane A (Quelle)",
        PaneId::B => "Pane B (Ziel)",
    };

    let label = Label::builder()
        .label(label_text)
        .css_classes(["heading"])
        .halign(Align::Start)
        .hexpand(true)
        .build();
    header.append(&label);

    header
}

/// Create the object list ColumnView
fn create_object_list() -> (ColumnView, ScrolledWindow) {
    // Create a StringList as the backing model
    let store = StringList::new(&[] as &[&str]);
    let selection = NoSelection::new(Some(store));
    let model = selection;

    let column_view = ColumnView::builder()
        .model(&model)
        .hexpand(true)
        .vexpand(true)
        .build();

    // Name column
    let name_factory = SignalListItemFactory::new();
    name_factory.connect_setup(move |_, list_item| {
        let list_item = list_item.downcast_ref::<gtk4::ListItem>().expect("Needs ListItem");
        let label = Label::builder()
            .halign(Align::Start)
            .margin_start(4)
            .margin_end(4)
            .build();
        list_item.set_child(Some(&label));
    });
    name_factory.connect_bind(move |_, list_item| {
        let list_item = list_item.downcast_ref::<gtk4::ListItem>().expect("Needs ListItem");
        if let Some(item) = list_item.item() {
            if let Some(string_item) = item.downcast_ref::<StringObject>() {
                if let Some(child) = list_item.child() {
                    if let Some(label) = child.downcast_ref::<Label>() {
                        label.set_label(&string_item.string());
                    }
                }
            }
        }
    });

    let name_column = ColumnViewColumn::builder()
        .title("Name")
        .factory(&name_factory)
        .expand(true)
        .resizable(true)
        .build();

    column_view.append_column(&name_column);

    // Size column
    let size_factory = SignalListItemFactory::new();
    size_factory.connect_setup(move |_, list_item| {
        let list_item = list_item.downcast_ref::<gtk4::ListItem>().expect("Needs ListItem");
        let label = Label::builder()
            .halign(Align::End)
            .margin_start(4)
            .margin_end(4)
            .build();
        list_item.set_child(Some(&label));
    });
    size_factory.connect_bind(move |_, list_item| {
        let list_item = list_item.downcast_ref::<gtk4::ListItem>().expect("Needs ListItem");
        if let Some(item) = list_item.item() {
            if let Some(string_item) = item.downcast_ref::<StringObject>() {
                if let Some(child) = list_item.child() {
                    if let Some(label) = child.downcast_ref::<Label>() {
                        label.set_label(&string_item.string());
                    }
                }
            }
        }
    });

    let size_column = ColumnViewColumn::builder()
        .title("Größe")
        .factory(&size_factory)
        .fixed_width(100)
        .resizable(true)
        .build();

    column_view.append_column(&size_column);

    // Type column
    let type_factory = SignalListItemFactory::new();
    type_factory.connect_setup(move |_, list_item| {
        let list_item = list_item.downcast_ref::<gtk4::ListItem>().expect("Needs ListItem");
        let label = Label::builder()
            .halign(Align::Start)
            .margin_start(4)
            .margin_end(4)
            .build();
        list_item.set_child(Some(&label));
    });
    type_factory.connect_bind(move |_, list_item| {
        let list_item = list_item.downcast_ref::<gtk4::ListItem>().expect("Needs ListItem");
        if let Some(item) = list_item.item() {
            if let Some(string_item) = item.downcast_ref::<StringObject>() {
                if let Some(child) = list_item.child() {
                    if let Some(label) = child.downcast_ref::<Label>() {
                        label.set_label(&string_item.string());
                    }
                }
            }
        }
    });

    let type_column = ColumnViewColumn::builder()
        .title("Typ")
        .factory(&type_factory)
        .fixed_width(120)
        .resizable(true)
        .build();

    column_view.append_column(&type_column);

    // Modified column
    let modified_factory = SignalListItemFactory::new();
    modified_factory.connect_setup(move |_, list_item| {
        let list_item = list_item.downcast_ref::<gtk4::ListItem>().expect("Needs ListItem");
        let label = Label::builder()
            .halign(Align::Start)
            .margin_start(4)
            .margin_end(4)
            .build();
        list_item.set_child(Some(&label));
    });
    modified_factory.connect_bind(move |_, list_item| {
        let list_item = list_item.downcast_ref::<gtk4::ListItem>().expect("Needs ListItem");
        if let Some(item) = list_item.item() {
            if let Some(string_item) = item.downcast_ref::<StringObject>() {
                if let Some(child) = list_item.child() {
                    if let Some(label) = child.downcast_ref::<Label>() {
                        label.set_label(&string_item.string());
                    }
                }
            }
        }
    });

    let modified_column = ColumnViewColumn::builder()
        .title("Zuletzt geändert")
        .factory(&modified_factory)
        .fixed_width(160)
        .resizable(true)
        .build();

    column_view.append_column(&modified_column);

    let scrolled = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .min_content_width(300)
        .child(&column_view)
        .build();

    (column_view, scrolled)
}

/// Get the parent prefix (one level up)
pub fn parent_prefix(prefix: &str) -> String {
    if prefix.is_empty() {
        return String::new();
    }

    let trimmed = prefix.trim_end_matches('/');
    if let Some(last_slash) = trimmed.rfind('/') {
        trimmed[..=last_slash].to_string()
    } else {
        String::new()
    }
}

// ---------------------------------------------------------------------------
// Drag & Drop support
// ---------------------------------------------------------------------------

impl S3Pane {
    /// Set up drag source for S3 objects (drag from this pane)
    pub fn setup_drag_source(&self) {
        let drag_source = DragSource::new();
        drag_source.set_actions(DragAction::COPY);

        let objects: Vec<String> = self.objects.iter()
            .filter(|o| !o.is_prefix)
            .map(|o| o.key.clone())
            .collect();

        if objects.is_empty() {
            return;
        }

        let objects_clone = objects.clone();
        drag_source.connect_prepare(move |_source, _x, _y| {
            let value = glib::Value::from(&objects_clone);
            Some(ContentProvider::for_value(&value))
        });

        self.object_list.add_controller(drag_source);
    }

    /// Set up drop target for S3 objects and files (drop onto this pane)
    pub fn setup_drop_target(&self) {
        // Drop target for S3 objects
        let drop_target = DropTarget::new(
            glib::Type::STRING,
            DragAction::COPY,
        );

        let pane_id = self.pane_id;
        let container = self.container.clone();

        drop_target.connect_drop(move |_target, value, _x, _y| {
            if let Ok(text) = value.get::<String>() {
                info!(pane = %pane_id, data = %text, "S3 objects dropped on pane");
                return true;
            }
            false
        });

        let container_clone = container.clone();
        drop_target.connect_enter(move |_target, _x, _y| {
            container_clone.add_css_class("drag-over");
            DragAction::COPY
        });

        drop_target.connect_leave(move |_target| {
            container.remove_css_class("drag-over");
        });

        self.container.add_controller(drop_target);

        // Drop target for file URIs (from file manager)
        let file_drop = DropTarget::new(
            glib::Type::STRING,
            DragAction::COPY,
        );

        let pane_id2 = self.pane_id;
        let container2 = self.container.clone();

        file_drop.connect_drop(move |_target, value, _x, _y| {
            if let Ok(text) = value.get::<String>() {
                info!(pane = %pane_id2, uris = %text, "Files dropped on pane");
                for uri in text.lines() {
                    let uri = uri.trim();
                    if uri.is_empty() {
                        continue;
                    }
                    if let Some(path) = uri.strip_prefix("file://") {
                        info!(pane = %pane_id2, path = %path, "File dropped from file manager");
                    }
                }
                return true;
            }
            false
        });

        let container3 = self.container.clone();
        file_drop.connect_enter(move |_target, _x, _y| {
            container3.add_css_class("drag-over");
            DragAction::COPY
        });

        file_drop.connect_leave(move |_target| {
            container2.remove_css_class("drag-over");
        });

        self.container.add_controller(file_drop);
    }

    /// Create a transfer job for drag & drop between panes
    pub fn create_transfer_job(
        &self,
        source_pane: &S3Pane,
        object_keys: &[String],
    ) -> Vec<TransferJob> {
        let mut jobs = Vec::new();

        let src_profile = match source_pane.profile_id() {
            Some(id) => id,
            None => return jobs,
        };
        let src_bucket = match source_pane.current_bucket() {
            Some(b) => b.to_string(),
            None => return jobs,
        };

        let dst_profile = match self.profile_id {
            Some(id) => id,
            None => return jobs,
        };
        let dst_bucket = match self.current_bucket.clone() {
            Some(b) => b,
            None => return jobs,
        };
        let dst_prefix = self.current_prefix.clone();

        for key in object_keys {
            // Extract filename from key
            let filename = key.rsplit('/').next().unwrap_or(key);
            let dest_key = if dst_prefix.is_empty() {
                filename.to_string()
            } else {
                format!("{}{}", dst_prefix, filename)
            };

            let job = TransferJob::new(
                TransferDirection::S3ToS3,
                TransferSource::S3Object {
                    profile_id: src_profile,
                    bucket: src_bucket.clone(),
                    key: key.clone(),
                },
                TransferDestination::S3Object {
                    profile_id: dst_profile,
                    bucket: dst_bucket.clone(),
                    key: dest_key,
                },
                0, // total_bytes unknown until HeadObject
            );
            jobs.push(job);
        }

        jobs
    }
}

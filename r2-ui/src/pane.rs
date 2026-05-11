//! r2-ui — S3Pane widget for the dual-pane browser
//!
//! Each S3Pane is an independent browser pane with its own:
//! - Bucket selector (GtkDropDown)
//! - Breadcrumb navigation (path entry)
//! - Object list (GtkColumnView with lazy loading)
//! - Status bar
//! - Context menus for objects and buckets

use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, ColumnView, ColumnViewColumn,
    DropDown, Entry, Label, NoSelection,
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

use crate::dialogs::properties_dialog::bytes_to_human;

/// Number of objects to load per page
const PAGE_SIZE: i32 = 100;

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
    is_loading: Arc<AtomicBool>,

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
            is_loading: Arc::new(AtomicBool::new(false)),
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
        let mut loaded_keys: Vec<String> = Vec::new();
        let mut all_loaded = false;

        if reset {
            loaded_keys.clear();
            all_loaded = false;
        }

        if all_loaded {
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
        let start_after: Option<String> = if reset {
            None
        } else {
            loaded_keys.last().cloned()
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
        Vec::new()
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


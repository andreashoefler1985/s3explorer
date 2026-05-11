//! r2-ui — Version History dialog for S3 object versions
//!
//! Displays all versions of an S3 object with actions:
//! restore, delete, download.

use chrono::{DateTime, Utc};
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, ColumnView, ColumnViewColumn, Dialog,
    Label, NoSelection, Orientation, ScrolledWindow, SignalListItemFactory,
    StringList, StringObject,
};
use std::sync::Arc;
use tracing::{error, info};

use r2_core::s3_client::client::S3Client;
use r2_core::s3_client::types::ObjectVersion;

use crate::dialogs::properties_dialog::bytes_to_human;

/// Shows the version history dialog for an object
pub fn show_version_history(
    parent: &impl IsA<gtk4::Window>,
    s3_client: Arc<dyn S3Client>,
    bucket: &str,
    key: &str,
) {
    let bucket_owned = bucket.to_string();
    let key_owned = key.to_string();

    let dialog = Dialog::builder()
        .title(format!("Versionen: {}", key))
        .transient_for(parent)
        .modal(true)
        .default_width(700)
        .default_height(500)
        .build();

    let content = dialog.content_area();
    content.set_orientation(Orientation::Vertical);
    content.set_spacing(8);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    // Header
    let header = Label::builder()
        .label(format!("🔄 Versionen von: {}", key))
        .css_classes(["heading"])
        .halign(Align::Start)
        .build();
    content.append(&header);

    // Loading label
    let loading_label = Label::builder()
        .label("Lade Versionen...")
        .halign(Align::Start)
        .build();
    content.append(&loading_label);

    // Version list (ColumnView)
    let store = StringList::new(&[] as &[&str]);
    let (column_view, scrolled) = create_version_list(&store);
    content.append(&scrolled);

    // Action buttons
    let button_box = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(Align::End)
        .margin_top(8)
        .build();

    let restore_btn = Button::builder()
        .label("Wiederherstellen")
        .css_classes(["suggested-action"])
        .build();
    let delete_btn = Button::builder()
        .label("Löschen")
        .css_classes(["destructive-action"])
        .build();
    let download_btn = Button::builder()
        .label("Herunterladen")
        .build();
    let close_btn = Button::with_label("Schließen");

    button_box.append(&download_btn);
    button_box.append(&restore_btn);
    button_box.append(&delete_btn);
    button_box.append(&close_btn);
    content.append(&button_box);

    // State: selected version ID
    let selected_version: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);

    // Load versions
    let client = s3_client.clone();
    let bucket_clone = bucket_owned.clone();
    let key_clone = key_owned.clone();
    let store_clone = store.clone();
    let loading_label_clone = loading_label.clone();

    glib::MainContext::default().spawn_local(async move {
        match client.list_object_versions(&bucket_clone, &key_clone).await {
            Ok(versions) => {
                loading_label_clone.set_label(&format!("{} Versionen gefunden", versions.len()));
                let items: Vec<String> = versions.iter().map(|v| {
                    let date = v.last_modified.format("%Y-%m-%d %H:%M:%S UTC").to_string();
                    let size = bytes_to_human(v.size);
                    let latest = if v.is_latest { " ◀ AKTUELL" } else { "" };
                    format!("{} | {} | {}{}", v.version_id, date, size, latest)
                }).collect();
                let refs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
                store_clone.splice(0, store_clone.n_items(), &refs);

                // Store versions for actions
                let versions_clone = versions.clone();
                let selected = selected_version.clone();
                let store_for_select = store_clone.clone();

                // Connect selection via button clicks (simplified: store versions in a cell)
                let versions_cell: std::cell::RefCell<Vec<ObjectVersion>> =
                    std::cell::RefCell::new(versions_clone);

                // Restore
                let client_r = client.clone();
                let bucket_r = bucket_clone.clone();
                let key_r = key_clone.clone();
                let dialog_r = dialog.clone();
                restore_btn.connect_clicked(move |_| {
                    let sel = selected.borrow().clone();
                    if let Some(ref vid) = sel {
                        let versions = versions_cell.borrow();
                        if let Some(v) = versions.iter().find(|v| v.version_id == *vid) {
                            let client = client_r.clone();
                            let bucket = bucket_r.clone();
                            let key = key_r.clone();
                            let vid = v.version_id.clone();
                            let dialog = dialog_r.clone();
                            glib::MainContext::default().spawn_local(async move {
                                match client.restore_object_version(&bucket, &key, &vid).await {
                                    Ok(()) => {
                                        info!(key = %key, version = %vid, "Version restored");
                                        // Refresh would go here
                                    }
                                    Err(e) => {
                                        error!(key = %key, version = %vid, error = %e, "Restore failed");
                                    }
                                }
                            });
                        }
                    }
                });

                // Delete
                let client_d = client.clone();
                let bucket_d = bucket_clone.clone();
                let key_d = key_clone.clone();
                let dialog_d = dialog.clone();
                delete_btn.connect_clicked(move |_| {
                    let sel = selected.borrow().clone();
                    if let Some(ref vid) = sel {
                        let versions = versions_cell.borrow();
                        if let Some(v) = versions.iter().find(|v| v.version_id == *vid) {
                            let client = client_d.clone();
                            let bucket = bucket_d.clone();
                            let key = key_d.clone();
                            let vid = v.version_id.clone();
                            let dialog = dialog_d.clone();
                            glib::MainContext::default().spawn_local(async move {
                                match client.delete_object_version(&bucket, &key, &vid).await {
                                    Ok(()) => {
                                        info!(key = %key, version = %vid, "Version deleted");
                                        dialog.close();
                                    }
                                    Err(e) => {
                                        error!(key = %key, version = %vid, error = %e, "Delete failed");
                                    }
                                }
                            });
                        }
                    }
                });

                // Download
                let client_dl = client.clone();
                let bucket_dl = bucket_clone.clone();
                let key_dl = key_clone.clone();
                download_btn.connect_clicked(move |_| {
                    let sel = selected.borrow().clone();
                    if let Some(ref vid) = sel {
                        let versions = versions_cell.borrow();
                        if let Some(v) = versions.iter().find(|v| v.version_id == *vid) {
                            let client = client_dl.clone();
                            let bucket = bucket_dl.clone();
                            let key = key_dl.clone();
                            let vid = v.version_id.clone();
                            glib::MainContext::default().spawn_local(async move {
                                match client.get_object_version(&bucket, &key, &vid).await {
                                    Ok(data) => {
                                        info!(key = %key, version = %vid, size = data.len(), "Version downloaded");
                                    }
                                    Err(e) => {
                                        error!(key = %key, version = %vid, error = %e, "Download failed");
                                    }
                                }
                            });
                        }
                    }
                });
            }
            Err(e) => {
                loading_label_clone.set_label(&format!("❌ Fehler: {}", e));
            }
        }
    });

    // Close button
    let dialog_clone = dialog.clone();
    close_btn.connect_clicked(move |_| {
        dialog_clone.close();
    });

    dialog.show();
}

/// Create the version list ColumnView
fn create_version_list(store: &StringList) -> (ColumnView, ScrolledWindow) {
    let selection = NoSelection::new(Some(store.clone()));
    let column_view = ColumnView::builder()
        .model(&selection)
        .hexpand(true)
        .vexpand(true)
        .build();

    // Version ID column
    let vid_factory = SignalListItemFactory::new();
    vid_factory.connect_setup(move |_, list_item| {
        let list_item = list_item.downcast_ref::<gtk4::ListItem>().expect("Needs ListItem");
        let label = Label::builder()
            .halign(Align::Start)
            .margin_start(4)
            .margin_end(4)
            .build();
        list_item.set_child(Some(&label));
    });
    vid_factory.connect_bind(move |_, list_item| {
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

    let vid_column = ColumnViewColumn::builder()
        .title("Version")
        .factory(&vid_factory)
        .expand(true)
        .resizable(true)
        .build();
    column_view.append_column(&vid_column);

    let scrolled = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .min_content_width(500)
        .child(&column_view)
        .build();

    (column_view, scrolled)
}

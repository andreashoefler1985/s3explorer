//! r2-ui — Version History dialog for S3 object versions
//!
//! Displays all versions of an S3 object with actions:
//! restore, delete, download.

use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, ColumnView, ColumnViewColumn, HeaderBar,
    Label, NoSelection, Orientation, ScrolledWindow, SignalListItemFactory,
    StringList, StringObject, Window,
};
use std::cell::RefCell;
use std::rc::Rc;
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

    let window = Window::new();
    window.set_title(Some(&format!("Versionen: {}", key)));
    window.set_transient_for(Some(parent));
    window.set_modal(true);
    window.set_default_size(700, 500);

    let header = HeaderBar::new();
    header.set_show_title_buttons(true);
    window.set_titlebar(Some(&header));

    let content = GtkBox::new(Orientation::Vertical, 8);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    window.set_child(Some(&content));

    // Header
    let header_label = Label::builder()
        .label(format!("🔄 Versionen von: {}", key))
        .css_classes(["heading"])
        .halign(Align::Start)
        .build();
    content.append(&header_label);

    // Loading label
    let loading_label = Label::builder()
        .label("Lade Versionen...")
        .halign(Align::Start)
        .build();
    content.append(&loading_label);

    // Version list (ColumnView)
    let store = StringList::new(&[] as &[&str]);
    let (_column_view, scrolled) = create_version_list(&store);
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

    // Shared state using Rc<RefCell<>>
    let selected: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let versions_cell: Rc<RefCell<Vec<ObjectVersion>>> = Rc::new(RefCell::new(Vec::new()));

    // Load versions
    let client = s3_client.clone();
    let bucket_l = bucket_owned.clone();
    let key_l = key_owned.clone();
    let store_l = store.clone();
    let loading_l = loading_label.clone();
    let versions_cell_l = versions_cell.clone();
    let _selected_l = selected.clone();

    glib::MainContext::default().spawn_local(async move {
        match client.list_object_versions(&bucket_l, &key_l).await {
            Ok(versions) => {
                loading_l.set_label(&format!("{} Versionen gefunden", versions.len()));
                let items: Vec<String> = versions.iter().map(|v| {
                    let date = v.last_modified.format("%Y-%m-%d %H:%M:%S UTC").to_string();
                    let size = bytes_to_human(v.size);
                    let latest = if v.is_latest { " ◀ AKTUELL" } else { "" };
                    format!("{} | {} | {}{}", v.version_id, date, size, latest)
                }).collect();
                let refs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
                store_l.splice(0, store_l.n_items(), &refs);

                *versions_cell_l.borrow_mut() = versions;
            }
            Err(e) => {
                loading_l.set_label(&format!("❌ Fehler: {}", e));
            }
        }
    });

    // Restore button
    let client_r = s3_client.clone();
    let bucket_r = bucket_owned.clone();
    let key_r = key_owned.clone();
    let window_r = window.clone();
    let selected_r = selected.clone();
    let versions_r = versions_cell.clone();
    restore_btn.connect_clicked(move |_| {
        let sel = selected_r.borrow().clone();
        if let Some(ref vid) = sel {
            let versions = versions_r.borrow();
            if let Some(v) = versions.iter().find(|v| v.version_id == *vid) {
                let client = client_r.clone();
                let bucket = bucket_r.clone();
                let key = key_r.clone();
                let vid = v.version_id.clone();
                let window = window_r.clone();
                glib::MainContext::default().spawn_local(async move {
                    match client.restore_object_version(&bucket, &key, &vid).await {
                        Ok(()) => {
                            info!(key = %key, version = %vid, "Version restored");
                            window.close();
                        }
                        Err(e) => {
                            error!(key = %key, version = %vid, error = %e, "Restore failed");
                        }
                    }
                });
            }
        }
    });

    // Delete button
    let client_d = s3_client.clone();
    let bucket_d = bucket_owned.clone();
    let key_d = key_owned.clone();
    let window_d = window.clone();
    let selected_d = selected.clone();
    let versions_d = versions_cell.clone();
    delete_btn.connect_clicked(move |_| {
        let sel = selected_d.borrow().clone();
        if let Some(ref vid) = sel {
            let versions = versions_d.borrow();
            if let Some(v) = versions.iter().find(|v| v.version_id == *vid) {
                let client = client_d.clone();
                let bucket = bucket_d.clone();
                let key = key_d.clone();
                let vid = v.version_id.clone();
                let window = window_d.clone();
                glib::MainContext::default().spawn_local(async move {
                    match client.delete_object_version(&bucket, &key, &vid).await {
                        Ok(()) => {
                            info!(key = %key, version = %vid, "Version deleted");
                            window.close();
                        }
                        Err(e) => {
                            error!(key = %key, version = %vid, error = %e, "Delete failed");
                        }
                    }
                });
            }
        }
    });

    // Download button
    let client_dl = s3_client.clone();
    let bucket_dl = bucket_owned.clone();
    let key_dl = key_owned.clone();
    let selected_dl = selected.clone();
    let versions_dl = versions_cell.clone();
    download_btn.connect_clicked(move |_| {
        let sel = selected_dl.borrow().clone();
        if let Some(ref vid) = sel {
            let versions = versions_dl.borrow();
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

    // Close button
    let window_c = window.clone();
    close_btn.connect_clicked(move |_| {
        window_c.close();
    });

    window.present();
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

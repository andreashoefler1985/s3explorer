//! r2-ui — ACL Editor dialog for buckets and objects
//!
//! Displays and edits ACL grants for S3 buckets and objects.
//! Supports adding/removing grantees with various permission levels.

use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, DropDown, Entry, HeaderBar, Label,
    ListView, NoSelection, Orientation, ScrolledWindow, SignalListItemFactory,
    StringList, StringObject, Window,
};
use std::sync::Arc;
use tracing::{error, info};

use r2_core::s3_client::client::S3Client;
use r2_core::s3_client::types::{AclGrant, Grantee};

/// Shows the ACL editor dialog for a bucket or object
pub fn show_acl_editor(
    parent: &impl IsA<gtk4::Window>,
    s3_client: Arc<dyn S3Client>,
    bucket: &str,
    key: Option<&str>,
) {
    let bucket_owned = bucket.to_string();
    let key_owned = key.map(|s| s.to_string());
    let is_object = key.is_some();

    let title = if is_object {
        format!("ACL-Editor: {}/{}", bucket, key.unwrap())
    } else {
        format!("ACL-Editor: {}", bucket)
    };

    let window = Window::new();
    window.set_title(Some(&title));
    window.set_transient_for(Some(parent));
    window.set_modal(true);
    window.set_default_size(600, 500);

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
        .label(&title)
        .css_classes(["heading"])
        .halign(Align::Start)
        .build();
    content.append(&header_label);

    let type_label = Label::builder()
        .label(if is_object { "Typ: Objekt-ACL" } else { "Typ: Bucket-ACL" })
        .halign(Align::Start)
        .build();
    content.append(&type_label);

    // Loading / status
    let status_label = Label::builder()
        .label("Lade ACL...")
        .halign(Align::Start)
        .build();
    content.append(&status_label);

    // Grant list
    let store = StringList::new(&[] as &[&str]);
    let list_view = create_grant_list_view(&store);
    let scrolled = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .min_content_width(400)
        .min_content_height(200)
        .child(&list_view)
        .build();
    content.append(&scrolled);

    // ── Add grant section ──
    let add_section = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_top(8)
        .build();

    let grantee_types = StringList::new(&["AllUsers (öffentlich)", "AuthenticatedUsers", "CanonicalUser"]);
    let grantee_type_combo = DropDown::new(Some(grantee_types), None::<&gtk4::Expression>);
    grantee_type_combo.set_selected(0);
    add_section.append(&grantee_type_combo);

    let id_entry = Entry::builder()
        .placeholder_text("Canonical User ID (optional)")
        .hexpand(true)
        .build();
    add_section.append(&id_entry);

    let permissions = StringList::new(&["READ", "WRITE", "READ_ACP", "WRITE_ACP", "FULL_CONTROL"]);
    let permission_combo = DropDown::new(Some(permissions), None::<&gtk4::Expression>);
    permission_combo.set_selected(0);
    add_section.append(&permission_combo);

    let add_btn = Button::builder()
        .label("➕ Hinzufügen")
        .build();
    add_section.append(&add_btn);

    content.append(&add_section);

    // ── Action buttons ──
    let button_box = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(Align::End)
        .margin_top(12)
        .build();

    let save_btn = Button::builder()
        .label("💾 Speichern")
        .css_classes(["suggested-action"])
        .build();
    let cancel_btn = Button::with_label("Abbrechen");

    button_box.append(&save_btn);
    button_box.append(&cancel_btn);
    content.append(&button_box);

    // ── State ──
    let grants: std::cell::RefCell<Vec<AclGrant>> = std::cell::RefCell::new(Vec::new());

    // Load ACL
    let client = s3_client.clone();
    let bucket_l = bucket_owned.clone();
    let key_l = key_owned.clone();
    let store_l = store.clone();
    let status_l = status_label.clone();
    let grants_l = grants.clone();

    glib::MainContext::default().spawn_local(async move {
        let result = if key_l.is_some() {
            client.get_object_acl(&bucket_l, key_l.as_ref().unwrap()).await
        } else {
            client.get_bucket_acl(&bucket_l).await
        };

        match result {
            Ok(acl_grants) => {
                status_l.set_label(&format!("{} Berechtigungen", acl_grants.len()));
                *grants_l.borrow_mut() = acl_grants.clone();
                update_grant_list(&store_l, &acl_grants);
            }
            Err(e) => {
                status_l.set_label(&format!("❌ Fehler: {}", e));
            }
        }
    });

    // Add grant
    let grants_a = grants.clone();
    let store_a = store.clone();
    let status_a = status_label.clone();
    let grantee_combo_a = grantee_type_combo.clone();
    let id_entry_a = id_entry.clone();
    let perm_combo_a = permission_combo.clone();

    add_btn.connect_clicked(move |_| {
        let grantee_type_idx = grantee_combo_a.selected();
        let grantee_type = match grantee_type_idx {
            0 => "Group",
            1 => "Group",
            2 => "CanonicalUser",
            _ => "CanonicalUser",
        };
        let uri = match grantee_type_idx {
            0 => Some("http://acs.amazonaws.com/groups/global/AllUsers".to_string()),
            1 => Some("http://acs.amazonaws.com/groups/global/AuthenticatedUsers".to_string()),
            _ => None,
        };
        let id = id_entry_a.text().to_string();
        let id = if id.is_empty() { None } else { Some(id) };
        let permission_idx = perm_combo_a.selected();
        let permission = match permission_idx {
            0 => "READ",
            1 => "WRITE",
            2 => "READ_ACP",
            3 => "WRITE_ACP",
            4 => "FULL_CONTROL",
            _ => "READ",
        };

        let grant = AclGrant {
            grantee: Grantee {
                id,
                display_name: None,
                uri,
                grantee_type: grantee_type.to_string(),
            },
            permission: permission.to_string(),
        };

        grants_a.borrow_mut().push(grant.clone());
        update_grant_list(&store_a, &grants_a.borrow());
        status_a.set_label(&format!("{} Berechtigungen", grants_a.borrow().len()));
    });

    // Save
    let client_s = s3_client.clone();
    let bucket_s = bucket_owned.clone();
    let key_s = key_owned.clone();
    let window_s = window.clone();
    let status_s = status_label.clone();

    save_btn.connect_clicked(move |_| {
        let current_grants = grants.borrow().clone();
        let client = client_s.clone();
        let bucket = bucket_s.clone();
        let key = key_s.clone();
        let window = window_s.clone();
        let status = status_s.clone();

        status.set_label("Speichere ACL...");

        glib::MainContext::default().spawn_local(async move {
            let result = if key.is_some() {
                client.set_object_acl(&bucket, key.as_ref().unwrap(), &current_grants).await
            } else {
                client.set_bucket_acl(&bucket, &current_grants).await
            };

            match result {
                Ok(()) => {
                    info!(bucket = %bucket, "ACL saved successfully");
                    window.close();
                }
                Err(e) => {
                    error!(bucket = %bucket, error = %e, "Failed to save ACL");
                    status.set_label(&format!("❌ Fehler: {}", e));
                }
            }
        });
    });

    // Cancel
    let window_c = window.clone();
    cancel_btn.connect_clicked(move |_| {
        window_c.close();
    });

    window.present();
}

/// Create a ListView for ACL grants
fn create_grant_list_view(store: &StringList) -> ListView {
    let factory = SignalListItemFactory::new();

    factory.connect_setup(move |_, list_item| {
        let list_item = list_item.downcast_ref::<gtk4::ListItem>().unwrap();
        let label = Label::builder()
            .halign(Align::Start)
            .margin_start(8)
            .margin_end(8)
            .margin_top(4)
            .margin_bottom(4)
            .build();
        list_item.set_child(Some(&label));
    });

    factory.connect_bind(move |_, list_item| {
        let list_item = list_item.downcast_ref::<gtk4::ListItem>().unwrap();
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

    let selection = NoSelection::new(Some(store.clone()));
    ListView::new(Some(selection), Some(factory))
}

/// Update the grant list store from grants
fn update_grant_list(store: &StringList, grants: &[AclGrant]) {
    let items: Vec<String> = grants.iter().map(|g| {
        let grantee_name = if let Some(ref uri) = g.grantee.uri {
            if uri.contains("AllUsers") {
                "👥 Alle (öffentlich)".to_string()
            } else if uri.contains("AuthenticatedUsers") {
                "👥 Authentifizierte Benutzer".to_string()
            } else {
                uri.clone()
            }
        } else if let Some(ref name) = g.grantee.display_name {
            format!("👤 {}", name)
        } else if let Some(ref id) = g.grantee.id {
            format!("👤 {}...", &id[..std::cmp::min(8, id.len())])
        } else {
            "Unbekannt".to_string()
        };

        format!("{} | {}", grantee_name, g.permission)
    }).collect();

    store.splice(0, store.n_items(), &[]);
    let refs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
    store.splice(0, 0, &refs);
}

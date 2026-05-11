//! r2-ui — Profile Manager dialog
//!
//! GTK4 dialog for managing S3 endpoint profiles.

use gtk4::prelude::*;
use gtk4::{
    Align, ApplicationWindow, Box as GtkBox, Button, Entry, HeaderBar, Label,
    ListView, NoSelection, Orientation, PasswordEntry, SignalListItemFactory,
    Spinner, StringObject, Window,
};
use std::sync::Arc;
use std::sync::Mutex;
use tracing::{error, info};

use r2_core::credentials::storage::CredentialStorage;
use r2_core::credentials::profile::Profile;
use r2_core::s3_client::client::AwsSdkS3Client;
use r2_core::s3_client::types::S3ClientConfig;

/// Profile manager dialog
pub struct ProfileManagerDialog {
    window: Window,
    storage: Arc<dyn CredentialStorage>,
    profiles: Arc<Mutex<Vec<Profile>>>,
    list_store: gtk4::StringList,
}

impl ProfileManagerDialog {
    /// Create a new profile manager dialog
    pub fn new(parent: &ApplicationWindow, storage: Arc<dyn CredentialStorage>) -> Self {
        let window = Window::new();
        window.set_title(Some("Profil-Manager"));
        window.set_transient_for(Some(parent));
        window.set_modal(true);
        window.set_default_size(600, 400);

        let header = HeaderBar::new();
        header.set_show_title_buttons(true);
        window.set_titlebar(Some(&header));

        let content_area = GtkBox::new(Orientation::Vertical, 12);
        content_area.set_margin_start(12);
        content_area.set_margin_end(12);
        content_area.set_margin_top(12);
        content_area.set_margin_bottom(12);
        window.set_child(Some(&content_area));

        // Header
        let header = Label::builder()
            .label("S3-Endpunkt-Profile verwalten")
            .css_classes(["heading"])
            .halign(Align::Start)
            .build();
        content_area.append(&header);

        // Profile list using StringList
        let list_store = gtk4::StringList::new(&[] as &[&str]);
        let list_view = create_profile_list_view(&list_store);

        let scrolled = gtk4::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .min_content_width(400)
            .min_content_height(200)
            .build();
        scrolled.set_child(Some(&list_view));
        content_area.append(&scrolled);

        // Action buttons
        let button_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .halign(Align::End)
            .build();

        let new_btn = Button::with_label("Neu");
        let edit_btn = Button::with_label("Bearbeiten");
        let delete_btn = Button::with_label("Löschen");
        let close_btn = Button::with_label("Schließen");

        button_box.append(&new_btn);
        button_box.append(&edit_btn);
        button_box.append(&delete_btn);
        button_box.append(&close_btn);
        content_area.append(&button_box);

        let profiles: Arc<Mutex<Vec<Profile>>> = Arc::new(Mutex::new(Vec::new()));

        let mut manager = Self {
            window,
            storage,
            profiles: profiles.clone(),
            list_store: list_store.clone(),
        };

        // Connect signals
        {
            let storage = manager.storage.clone();
            let parent_clone = parent.clone();
            let profiles_clone = profiles.clone();
            let list_store_clone = list_store.clone();
            new_btn.connect_clicked(move |_| {
                show_profile_form(&parent_clone, &storage, None, None, &profiles_clone, &list_store_clone);
            });
        }

        {
            let storage = manager.storage.clone();
            let parent_clone = parent.clone();
            let profiles_clone = profiles.clone();
            let list_store_clone = list_store.clone();
            edit_btn.connect_clicked(move |_| {
                let selected = {
                    let p = profiles_clone.lock().unwrap();
                    p.first().cloned()
                };
                if let Some(profile) = selected {
                    show_profile_form(&parent_clone, &storage, Some(profile), None, &profiles_clone, &list_store_clone);
                }
            });
        }

        {
            let storage = manager.storage.clone();
            let parent_clone = parent.clone();
            let profiles_clone = profiles.clone();
            let list_store_clone = list_store.clone();
            delete_btn.connect_clicked(move |_| {
                let selected = {
                    let p = profiles_clone.lock().unwrap();
                    p.first().cloned()
                };
                if let Some(profile) = selected {
                    show_delete_confirmation(&parent_clone, &storage, &profile, &profiles_clone, &list_store_clone);
                }
            });
        }

        {
            let window = manager.window.clone();
            close_btn.connect_clicked(move |_| {
                window.close();
            });
        }

        // Load profiles
        manager.refresh_profile_list();

        manager
    }

    /// Refresh the profile list from storage
    pub fn refresh_profile_list(&mut self) {
        match self.storage.list_profiles() {
            Ok(profiles) => {
                let mut stored = self.profiles.lock().unwrap();
                *stored = profiles.clone();
                self.list_store.splice(0, self.list_store.n_items(), &[]);
                let items: Vec<String> = profiles.iter().map(|p| {
                    format!("{} — {} ({})", p.name, p.endpoint_url, p.region)
                }).collect();
                let refs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
                self.list_store.splice(0, 0, &refs);
                info!(count = profiles.len(), "Profile list refreshed");
            }
            Err(e) => {
                error!("Failed to list profiles: {}", e);
            }
        }
    }

    /// Show the dialog
    pub fn show(&self) {
        self.window.present();
    }

    /// Hide the dialog
    pub fn hide(&self) {
        self.window.close();
    }
}

/// Create a ListView for profiles
fn create_profile_list_view(store: &gtk4::StringList) -> ListView {
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

/// Show the profile form dialog
fn show_profile_form(
    parent: &ApplicationWindow,
    storage: &Arc<dyn CredentialStorage>,
    existing: Option<Profile>,
    _credentials: Option<(String, String)>,
    profiles: &Arc<Mutex<Vec<Profile>>>,
    list_store: &gtk4::StringList,
) {
    let window = Window::new();
    window.set_title(Some(if existing.is_some() { "Profil bearbeiten" } else { "Neues Profil" }));
    window.set_transient_for(Some(parent));
    window.set_modal(true);
    window.set_default_size(500, -1);

    let header = HeaderBar::new();
    header.set_show_title_buttons(true);
    window.set_titlebar(Some(&header));

    let content = GtkBox::new(Orientation::Vertical, 8);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    window.set_child(Some(&content));

    // Form fields
    let name_entry = Entry::builder()
        .placeholder_text("z.B. MinIO Local")
        .hexpand(true)
        .build();

    let endpoint_entry = Entry::builder()
        .placeholder_text("z.B. http://localhost:9000")
        .hexpand(true)
        .build();

    let access_key_entry = PasswordEntry::builder()
        .placeholder_text("Access Key")
        .hexpand(true)
        .show_peek_icon(true)
        .build();

    let secret_key_entry = PasswordEntry::builder()
        .placeholder_text("Secret Key")
        .hexpand(true)
        .show_peek_icon(true)
        .build();

    let region_entry = Entry::builder()
        .placeholder_text("z.B. us-east-1")
        .text("us-east-1")
        .hexpand(true)
        .build();

    let bucket_entry = Entry::builder()
        .placeholder_text("Optional: Default-Bucket")
        .hexpand(true)
        .build();

    // Pre-fill if editing
    if let Some(ref profile) = existing {
        name_entry.set_text(&profile.name);
        endpoint_entry.set_text(&profile.endpoint_url);
        region_entry.set_text(&profile.region);
        if let Some(ref bucket) = profile.default_bucket {
            bucket_entry.set_text(bucket);
        }
    }

    // Build form layout
    let form = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .build();

    form.append(&create_form_row("Name:", &name_entry));
    form.append(&create_form_row("Endpoint URL:", &endpoint_entry));
    form.append(&create_form_row("Access Key:", &access_key_entry));
    form.append(&create_form_row("Secret Key:", &secret_key_entry));
    form.append(&create_form_row("Region:", &region_entry));
    form.append(&create_form_row("Default Bucket:", &bucket_entry));

    content.append(&form);

    // Status label for test connection
    let status_label = Label::builder()
        .halign(Align::Start)
        .build();
    content.append(&status_label);

    // Buttons
    let button_box = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(Align::End)
        .margin_top(12)
        .build();

    let test_btn = Button::with_label("Test Connection");
    let save_btn = Button::builder()
        .label("Speichern")
        .css_classes(["suggested-action"])
        .build();
    let cancel_btn = Button::with_label("Abbrechen");
    let spinner = Spinner::new();

    button_box.append(&test_btn);
    button_box.append(&spinner);
    button_box.append(&save_btn);
    button_box.append(&cancel_btn);
    content.append(&button_box);

    // Test connection
    {
        let endpoint = endpoint_entry.clone();
        let access_key = access_key_entry.clone();
        let secret_key = secret_key_entry.clone();
        let region = region_entry.clone();
        let status = status_label.clone();
        let spinner = spinner.clone();

        test_btn.connect_clicked(move |btn| {
            btn.set_sensitive(false);
            spinner.start();
            status.set_label("Verbindung wird getestet...");

            let endpoint = endpoint.text().to_string();
            let access_key = access_key.text().to_string();
            let secret_key = secret_key.text().to_string();
            let region = region.text().to_string();

            let status_clone = status.clone();
            let spinner_clone = spinner.clone();
            let btn_clone = btn.clone();

            glib::MainContext::default().spawn_local(async move {
                let config = S3ClientConfig {
                    endpoint_url: endpoint,
                    region,
                    access_key,
                    secret_key,
                    path_style: true,
                    ..Default::default()
                };

                match AwsSdkS3Client::new(config).await {
                    Ok(client) => {
                        match client.test_connection().await {
                            Ok(true) => {
                                status_clone.set_label("✅ Verbindung erfolgreich");
                            }
                            Ok(false) => {
                                status_clone.set_label("❌ Verbindung fehlgeschlagen");
                            }
                            Err(e) => {
                                status_clone.set_label(&format!("❌ Fehler: {}", e));
                            }
                        }
                    }
                    Err(e) => {
                        status_clone.set_label(&format!("❌ Fehler: {}", e));
                    }
                }

                spinner_clone.stop();
                btn_clone.set_sensitive(true);
            });
        });
    }

    // Save
    {
        let storage = storage.clone();
        let window = window.clone();
        let name = name_entry.clone();
        let endpoint = endpoint_entry.clone();
        let access_key = access_key_entry.clone();
        let secret_key = secret_key_entry.clone();
        let region = region_entry.clone();
        let bucket = bucket_entry.clone();
        let existing = existing.clone();
        let profiles = profiles.clone();
        let list_store = list_store.clone();

        save_btn.connect_clicked(move |_| {
            let name = name.text().trim().to_string();
            let endpoint = endpoint.text().trim().to_string();
            let access_key = access_key.text().trim().to_string();
            let secret_key = secret_key.text().trim().to_string();
            let region = region.text().trim().to_string();
            let bucket_text = bucket.text().trim().to_string();
            let default_bucket = if bucket_text.is_empty() { None } else { Some(bucket_text) };

            if name.is_empty() || endpoint.is_empty() || access_key.is_empty() || secret_key.is_empty() {
                return;
            }

            let profile = if let Some(ref existing) = existing {
                Profile {
                    id: existing.id,
                    name,
                    endpoint_url: endpoint,
                    region,
                    default_bucket,
                    path_style: true,
                }
            } else {
                Profile::new(name, endpoint, region, default_bucket, true)
            };

            match storage.save_profile(&profile, &access_key, &secret_key) {
                Ok(()) => {
                    info!(name = %profile.name, "Profile saved");
                    let mut p = profiles.lock().unwrap();
                    p.retain(|x| x.id != profile.id);
                    p.push(profile);
                    let items: Vec<String> = p.iter().map(|pr| {
                        format!("{} — {} ({})", pr.name, pr.endpoint_url, pr.region)
                    }).collect();
                    let refs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
                    list_store.splice(0, list_store.n_items(), &refs);
                    window.close();
                }
                Err(e) => {
                    error!("Failed to save profile: {}", e);
                }
            }
        });
    }

    // Cancel
    {
        let window = window.clone();
        cancel_btn.connect_clicked(move |_| {
            window.close();
        });
    }

    window.present();
}

/// Show delete confirmation dialog
fn show_delete_confirmation(
    parent: &ApplicationWindow,
    storage: &Arc<dyn CredentialStorage>,
    profile: &Profile,
    profiles: &Arc<Mutex<Vec<Profile>>>,
    list_store: &gtk4::StringList,
) {
    let window = Window::new();
    window.set_title(Some("Profil löschen"));
    window.set_transient_for(Some(parent));
    window.set_modal(true);
    window.set_default_size(400, -1);

    let header = HeaderBar::new();
    header.set_show_title_buttons(true);
    window.set_titlebar(Some(&header));

    let content = GtkBox::new(Orientation::Vertical, 12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    window.set_child(Some(&content));

    let label = Label::builder()
        .label(format!(
            "Möchtest du das Profil \"{}\" wirklich löschen?\nDiese Aktion kann nicht rückgängig gemacht werden.",
            profile.name
        ))
        .wrap(true)
        .build();
    content.append(&label);

    let button_box = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(Align::End)
        .build();

    let delete_btn = Button::builder()
        .label("Löschen")
        .css_classes(["destructive-action"])
        .build();
    let cancel_btn = Button::with_label("Abbrechen");

    button_box.append(&delete_btn);
    button_box.append(&cancel_btn);
    content.append(&button_box);

    {
        let storage = storage.clone();
        let profile_id = profile.id;
        let window = window.clone();
        let profiles = profiles.clone();
        let list_store = list_store.clone();
        delete_btn.connect_clicked(move |_| {
            match storage.delete_profile(&profile_id) {
                Ok(()) => {
                    info!(profile_id = %profile_id, "Profile deleted");
                    let mut p = profiles.lock().unwrap();
                    p.retain(|x| x.id != profile_id);
                    let items: Vec<String> = p.iter().map(|pr| {
                        format!("{} — {} ({})", pr.name, pr.endpoint_url, pr.region)
                    }).collect();
                    let refs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
                    list_store.splice(0, list_store.n_items(), &refs);
                    window.close();
                }
                Err(e) => {
                    error!("Failed to delete profile: {}", e);
                }
            }
        });
    }

    {
        let window = window.clone();
        cancel_btn.connect_clicked(move |_| {
            window.close();
        });
    }

    window.present();
}

/// Create a form row with label and widget
fn create_form_row(label_text: &str, widget: &impl IsA<gtk4::Widget>) -> GtkBox {
    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_top(4)
        .margin_bottom(4)
        .build();

    let label = Label::builder()
        .label(label_text)
        .halign(Align::Start)
        .width_chars(14)
        .build();

    row.append(&label);
    row.append(widget);

    row
}

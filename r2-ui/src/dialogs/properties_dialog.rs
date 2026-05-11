//! r2-ui — Properties dialog for buckets and objects

use chrono::{DateTime, Utc};
use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Button, Dialog, Label, Orientation};

use r2_core::s3_client::types::{BucketInfo, ObjectInfo};

/// Shows bucket properties dialog
pub fn show_bucket_properties(
    parent: &impl IsA<gtk4::Window>,
    bucket: &BucketInfo,
) {
    let dialog = Dialog::builder()
        .title(format!("Bucket-Eigenschaften: {}", bucket.name))
        .transient_for(parent)
        .modal(true)
        .default_width(450)
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
        .label(format!("📦 Bucket: {}", bucket.name))
        .css_classes(["heading"])
        .halign(Align::Start)
        .build();
    content.append(&header);

    // General info
    let info_box = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .margin_top(8)
        .build();

    let created = bucket.creation_date
        .map(|d| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "Unbekannt".to_string());

    info_box.append(&property_row("Name:", &bucket.name));
    info_box.append(&property_row("Erstellt:", &created));

    content.append(&info_box);

    // Close button
    let close_btn = Button::builder()
        .label("Schließen")
        .css_classes(["suggested-action"])
        .halign(Align::End)
        .margin_top(16)
        .build();

    let dialog_clone = dialog.clone();
    close_btn.connect_clicked(move |_| {
        dialog_clone.close();
    });
    content.append(&close_btn);

    dialog.show();
}

/// Shows object properties dialog
pub fn show_object_properties(
    parent: &impl IsA<gtk4::Window>,
    obj: &ObjectInfo,
    bucket_name: &str,
) {
    let dialog = Dialog::builder()
        .title("Objekt-Informationen")
        .transient_for(parent)
        .modal(true)
        .default_width(450)
        .build();

    let content = dialog.content_area();
    content.set_orientation(Orientation::Vertical);
    content.set_spacing(8);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    // Header
    let icon = if obj.is_prefix { "📁" } else { "📄" };
    let header = Label::builder()
        .label(format!("{} {}", icon, obj.key))
        .css_classes(["heading"])
        .halign(Align::Start)
        .build();
    content.append(&header);

    // Properties
    let info_box = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .margin_top(8)
        .build();

    let size_str = bytes_to_human(obj.size);
    let type_str = if obj.is_prefix {
        "Ordner".to_string()
    } else {
        obj.storage_class.clone().unwrap_or_else(|| "STANDARD".to_string())
    };
    let modified = obj.last_modified
        .map(|d| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "Unbekannt".to_string());

    info_box.append(&property_row("Name:", &obj.key));
    info_box.append(&property_row("Größe:", &format!("{} ({} Bytes)", size_str, obj.size)));
    info_box.append(&property_row("Typ:", &type_str));
    info_box.append(&property_row("Zuletzt geändert:", &modified));
    if let Some(ref etag) = obj.e_tag {
        info_box.append(&property_row("ETag:", etag));
    }
    info_box.append(&property_row("Bucket:", bucket_name));

    content.append(&info_box);

    // Close button
    let close_btn = Button::builder()
        .label("Schließen")
        .css_classes(["suggested-action"])
        .halign(Align::End)
        .margin_top(16)
        .build();

    let dialog_clone = dialog.clone();
    close_btn.connect_clicked(move |_| {
        dialog_clone.close();
    });
    content.append(&close_btn);

    dialog.show();
}

/// Create a property row with label and value
fn property_row(label_text: &str, value: &str) -> GtkBox {
    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_top(2)
        .margin_bottom(2)
        .build();

    let label = Label::builder()
        .label(label_text)
        .halign(Align::Start)
        .width_chars(18)
        .build();

    let value_label = Label::builder()
        .label(value)
        .halign(Align::Start)
        .hexpand(true)
        .selectable(true)
        .build();

    row.append(&label);
    row.append(&value_label);
    row
}

/// Convert bytes to human-readable format
pub fn bytes_to_human(bytes: i64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx < units.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{} {}", bytes, units[unit_idx])
    } else {
        format!("{:.1} {}", size, units[unit_idx])
    }
}

/// Format a DateTime as relative time string (e.g., "vor 2 Stunden")
pub fn format_relative_time(dt: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(*dt);

    if duration.num_seconds() < 60 {
        format!("vor {} Sekunden", duration.num_seconds())
    } else if duration.num_minutes() < 60 {
        format!("vor {} Minuten", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("vor {} Stunden", duration.num_hours())
    } else if duration.num_days() < 7 {
        format!("vor {} Tagen", duration.num_days())
    } else {
        dt.format("%d.%m.%Y").to_string()
    }
}

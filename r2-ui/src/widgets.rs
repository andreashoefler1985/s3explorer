//! r2-ui — GTK4 widget helper functions

use gtk4::{Entry, Label, PasswordEntry};

/// Create a labeled entry row
pub fn labeled_entry(label_text: &str, placeholder: &str) -> (Label, Entry) {
    let label = Label::builder()
        .label(label_text)
        .halign(gtk4::Align::Start)
        .width_chars(15)
        .build();

    let entry = Entry::builder()
        .placeholder_text(placeholder)
        .hexpand(true)
        .build();

    (label, entry)
}

/// Create a labeled password entry row
pub fn labeled_password_entry(label_text: &str, placeholder: &str) -> (Label, PasswordEntry) {
    let label = Label::builder()
        .label(label_text)
        .halign(gtk4::Align::Start)
        .width_chars(15)
        .build();

    let entry = PasswordEntry::builder()
        .placeholder_text(placeholder)
        .hexpand(true)
        .show_peek_icon(true)
        .build();

    (label, entry)
}

/// Create a section header label
pub fn section_header(text: &str) -> Label {
    Label::builder()
        .label(text)
        .css_classes(["heading"])
        .halign(gtk4::Align::Start)
        .margin_bottom(8)
        .margin_top(8)
        .build()
}

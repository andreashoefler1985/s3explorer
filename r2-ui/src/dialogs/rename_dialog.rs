//! r2-ui — Rename dialog for S3 objects

use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Button, Dialog, Entry, Label, Orientation};
use std::cell::RefCell;
use std::rc::Rc;

/// Shows a rename dialog and calls `on_rename` with the new name.
pub fn show_rename_dialog(
    parent: &impl IsA<gtk4::Window>,
    current_name: &str,
    on_rename: Box<dyn Fn(String) + 'static>,
) {
    let dialog = Dialog::builder()
        .title("Objekt umbenennen")
        .transient_for(parent)
        .modal(true)
        .default_width(400)
        .build();

    let content = dialog.content_area();
    content.set_orientation(Orientation::Vertical);
    content.set_spacing(8);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    let label = Label::builder()
        .label("Neuer Name:")
        .halign(Align::Start)
        .build();
    content.append(&label);

    let entry = Entry::builder()
        .text(current_name)
        .hexpand(true)
        .build();
    // Select all text for easy replacement
    entry.select_region(0, current_name.len() as i32);
    content.append(&entry);

    let button_box = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(Align::End)
        .margin_top(12)
        .build();

    let rename_btn = Button::builder()
        .label("Umbenennen")
        .css_classes(["suggested-action"])
        .build();
    let cancel_btn = Button::with_label("Abbrechen");

    button_box.append(&cancel_btn);
    button_box.append(&rename_btn);
    content.append(&button_box);

    // Use Rc<RefCell<Option<...>>> so we can share between closures
    let on_rename: Rc<RefCell<Option<Box<dyn Fn(String)>>>> = Rc::new(RefCell::new(Some(on_rename)));

    // Rename button
    let on_rename_btn = on_rename.clone();
    let dialog_clone = dialog.clone();
    let entry_clone = entry.clone();
    rename_btn.connect_clicked(move |_| {
        let new_name = entry_clone.text().trim().to_string();
        if !new_name.is_empty() {
            if let Some(cb) = on_rename_btn.borrow_mut().take() {
                cb(new_name);
            }
            dialog_clone.close();
        }
    });

    // Cancel button
    let dialog_clone2 = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_clone2.close();
    });

    // Enter key in entry
    let on_rename_enter = on_rename.clone();
    let dialog_clone3 = dialog.clone();
    let entry_clone2 = entry.clone();
    entry.connect_activate(move |_| {
        let new_name = entry_clone2.text().trim().to_string();
        if !new_name.is_empty() {
            if let Some(cb) = on_rename_enter.borrow_mut().take() {
                cb(new_name);
            }
            dialog_clone3.close();
        }
    });

    dialog.show();
}

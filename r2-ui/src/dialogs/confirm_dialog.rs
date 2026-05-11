//! r2-ui — Confirmation dialog

use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Button, Label, Orientation, Window};
use std::cell::Cell;

/// Shows a confirmation dialog.
/// `on_confirm` is called when the user confirms.
pub fn show_confirm_dialog(
    parent: &impl IsA<gtk4::Window>,
    title: &str,
    message: &str,
    confirm_label: &str,
    destructive: bool,
    on_confirm: Box<dyn Fn() + 'static>,
) {
    let window = Window::new();
    window.set_title(Some(title));
    window.set_transient_for(Some(&parent.clone().upcast::<gtk4::Window>()));
    window.set_modal(true);
    window.set_default_size(400, -1);

    let content = GtkBox::new(Orientation::Vertical, 12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    window.set_child(Some(&content));

    let label = Label::builder()
        .label(message)
        .halign(Align::Start)
        .wrap(true)
        .build();
    content.append(&label);

    let button_box = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(Align::End)
        .margin_top(12)
        .build();

    let confirm_btn = if destructive {
        Button::builder()
            .label(confirm_label)
            .css_classes(["destructive-action"])
            .build()
    } else {
        Button::builder()
            .label(confirm_label)
            .css_classes(["suggested-action"])
            .build()
    };
    let cancel_btn = Button::with_label("Abbrechen");

    button_box.append(&cancel_btn);
    button_box.append(&confirm_btn);
    content.append(&button_box);

    let confirmed = Cell::new(false);
    let window_clone = window.clone();
    confirm_btn.connect_clicked(move |_| {
        confirmed.set(true);
        window_clone.close();
        on_confirm();
    });

    let window_clone2 = window.clone();
    cancel_btn.connect_clicked(move |_| {
        window_clone2.close();
    });

    window.present();
}

//! r2-ui — Transfer Queue Widget
//!
//! GTK4 widget that displays active, completed, and failed transfers.
//! Connects to the TransferEngine via the progress stream.

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Label, LevelBar, ListView, NoSelection,
    Orientation, SignalListItemFactory, Stack, StackSidebar, StringList,
    StringObject, ToggleButton,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use r2_core::transfer::{
    TransferEngine, TransferJob, TransferProgress, TransferStatus,
};

/// Format bytes in human-readable form (KB, MB, GB, TB)
fn format_speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec >= 1_000_000_000.0 {
        format!("{:.1} GB/s", bytes_per_sec / 1_000_000_000.0)
    } else if bytes_per_sec >= 1_000_000.0 {
        format!("{:.1} MB/s", bytes_per_sec / 1_000_000.0)
    } else if bytes_per_sec >= 1_000.0 {
        format!("{:.1} KB/s", bytes_per_sec / 1_000.0)
    } else {
        format!("{:.0} B/s", bytes_per_sec)
    }
}

/// Format bytes in human-readable form
fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Format ETA in human-readable form
fn format_eta(secs: f64) -> String {
    if secs.is_nan() || secs.is_infinite() || secs <= 0.0 {
        return "—".to_string();
    }
    let total_secs = secs as u64;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{}h {}min", hours, minutes)
    } else if minutes > 0 {
        format!("{}min {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// Transfer queue widget
pub struct TransferQueueWidget {
    pub container: GtkBox,
    pub stack: Stack,
    pub toggle_btn: ToggleButton,
    pub clear_completed_btn: Button,
    pub retry_all_btn: Button,
    pub resume_all_btn: Button,

    // Internal state
    active_jobs: Vec<TransferJob>,
    completed_jobs: Vec<TransferJob>,
    failed_jobs: Vec<TransferJob>,

    // Progress receiver
    progress_rx: Option<mpsc::UnboundedReceiver<TransferProgress>>,

    // Engine reference
    engine: Option<Arc<dyn TransferEngine>>,

    // UI update flag
    needs_update: Arc<AtomicBool>,
}

impl TransferQueueWidget {
    /// Create a new TransferQueueWidget
    pub fn new() -> Self {
        let container = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(4)
            .margin_start(8)
            .margin_end(8)
            .margin_top(4)
            .margin_bottom(4)
            .css_classes(["transfer-queue"])
            .build();

        // ── Header ──
        let header = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .build();

        let toggle_btn = ToggleButton::builder()
            .label("🔲 Transfer-Queue ▸")
            .build();

        let clear_completed_btn = Button::builder()
            .label("✕ Clear Completed")
            .tooltip_text("Alle abgeschlossenen Transfers entfernen")
            .build();

        let retry_all_btn = Button::builder()
            .label("↻ Retry All")
            .tooltip_text("Alle fehlgeschlagenen Transfers wiederholen")
            .build();

        let resume_all_btn = Button::builder()
            .label("▶ Resume All")
            .tooltip_text("Alle pausierten Transfers fortsetzen")
            .build();

        header.append(&toggle_btn);
        header.append(&clear_completed_btn);
        header.append(&retry_all_btn);
        header.append(&resume_all_btn);
        container.append(&header);

        // ── Stack with sidebar tabs ──
        let stack = Stack::builder()
            .hexpand(true)
            .vexpand(true)
            .build();

        let sidebar = StackSidebar::builder()
            .stack(&stack)
            .build();

        let stack_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(4)
            .build();
        stack_box.append(&sidebar);
        stack_box.append(&stack);

        // Active page
        let active_page = create_job_list_page("Keine aktiven Transfers");
        stack.add_titled(&active_page, Some("active"), "Aktiv");

        // Completed page
        let completed_page = create_job_list_page("Keine abgeschlossenen Transfers");
        stack.add_titled(&completed_page, Some("completed"), "Abgeschlossen");

        // Failed page
        let failed_page = create_job_list_page("Keine fehlgeschlagenen Transfers");
        stack.add_titled(&failed_page, Some("failed"), "Fehlgeschlagen");

        container.append(&stack_box);

        // Initially hidden
        container.set_visible(false);

        // Connect toggle button
        let container_clone = container.clone();
        toggle_btn.connect_toggled(move |btn| {
            let visible = btn.is_active();
            container_clone.set_visible(visible);
            if visible {
                btn.set_label("🔲 Transfer-Queue ▾");
            } else {
                btn.set_label("🔲 Transfer-Queue ▸");
            }
        });

        Self {
            container,
            stack,
            toggle_btn,
            clear_completed_btn,
            retry_all_btn,
            resume_all_btn,
            active_jobs: Vec::new(),
            completed_jobs: Vec::new(),
            failed_jobs: Vec::new(),
            progress_rx: None,
            engine: None,
            needs_update: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Set the transfer engine and subscribe to progress
    pub fn set_engine(&mut self, engine: Arc<dyn TransferEngine>) {
        self.engine = Some(engine.clone());

        // Subscribe to progress events
        let engine_clone = engine.clone();
        let needs_update = self.needs_update.clone();

        glib::MainContext::default().spawn_local(async move {
            let mut rx = engine_clone.subscribe().await;
            loop {
                match rx.recv().await {
                    Some(_progress) => {
                        needs_update.store(true, Ordering::SeqCst);
                        // Trigger UI update via idle callback
                        glib::idle_add_local(|| {
                            glib::ControlFlow::Continue
                        });
                    }
                    None => break,
                }
            }
        });
    }

    /// Refresh the job lists from the engine
    pub fn refresh(&mut self) {
        let engine = match self.engine.clone() {
            Some(e) => e,
            None => return,
        };

        let engine_clone = engine.clone();
        glib::MainContext::default().spawn_local(async move {
            match engine_clone.list_active().await {
                Ok(jobs) => {
                    // Update active jobs
                    debug!(count = jobs.len(), "Active transfers refreshed");
                }
                Err(e) => {
                    warn!(error = %e, "Failed to list active transfers");
                }
            }
        });

        let engine_clone = engine.clone();
        glib::MainContext::default().spawn_local(async move {
            match engine_clone.list_completed().await {
                Ok(jobs) => {
                    debug!(count = jobs.len(), "Completed transfers refreshed");
                }
                Err(e) => {
                    warn!(error = %e, "Failed to list completed transfers");
                }
            }
        });

        let engine_clone = engine.clone();
        glib::MainContext::default().spawn_local(async move {
            match engine_clone.list_failed().await {
                Ok(jobs) => {
                    debug!(count = jobs.len(), "Failed transfers refreshed");
                }
                Err(e) => {
                    warn!(error = %e, "Failed to list failed transfers");
                }
            }
        });
    }

    /// Add a job to the active list
    pub fn add_job(&mut self, job: TransferJob) {
        self.active_jobs.push(job);
        self.update_header();
    }

    /// Update the header with job counts
    fn update_header(&self) {
        let active_count = self.active_jobs.len();
        let completed_count = self.completed_jobs.len();
        let failed_count = self.failed_jobs.len();

        let label = format!(
            "🔲 Transfer-Queue ▾  {} aktiv · {} abgeschlossen · {} fehlgeschlagen",
            active_count, completed_count, failed_count
        );
        self.toggle_btn.set_label(&label);
    }
}

/// Create a page for the job list stack
fn create_job_list_page(empty_text: &str) -> GtkBox {
    let page = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .margin_start(8)
        .margin_end(8)
        .margin_top(4)
        .margin_bottom(4)
        .build();

    let empty_label = Label::builder()
        .label(empty_text)
        .halign(gtk4::Align::Start)
        .hexpand(true)
        .vexpand(true)
        .build();

    page.append(&empty_label);

    page
}

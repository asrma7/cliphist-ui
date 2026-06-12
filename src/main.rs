mod actions;
mod app;
mod cliphist;
mod config;
mod model;
mod thumbnails;
mod ui;

use gtk::{gio, prelude::*};
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let application = gtk::Application::builder()
        .application_id("dev.ashutosh.CliphistUI")
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    let runtime = app::AppRuntime::new();
    runtime.install_signal_handlers();
    application.connect_command_line(move |application, command_line| {
        let args = command_line.arguments();
        runtime.handle_command_line(application, &args)
    });
    application.run();
}

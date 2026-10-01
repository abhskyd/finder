//! Entry point for the TUI file manager.

mod ui;
mod file;
mod preview;
mod theme;
mod plugin;
mod config;

use ui::app::App;
use ratatui::backend::CrosstermBackend;
use std::io;

fn main() -> io::Result<()> {
    // If we panic, restore the terminal so it isn't left in raw mode /
    // alternate screen.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            io::stdout(),
            crossterm::event::DisableMouseCapture,
            crossterm::terminal::LeaveAlternateScreen
        );
        original_hook(info);
    }));

    // ----- terminal init ---------------------------------------------------
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::event::EnableMouseCapture,
        crossterm::terminal::EnterAlternateScreen
    )?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = ratatui::Terminal::new(backend)?;

    // ----- build the application -------------------------------------------
    let cfg = config::Config::load();
    let mut app = App::new(&cfg);

    // Load plugins (give them a mutable reference to the app)
    let mut plugins = std::mem::take(&mut app.plugins);
    plugins.load_all(&mut app);
    app.plugins = plugins;

    // ----- async runtime ---------------------------------------------------
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(app.run(terminal))?;

    // Persist theme + bookmark changes made during the session
    app.save_config();

    // ----- cleanup ---------------------------------------------------------
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        io::stdout(),
        crossterm::event::DisableMouseCapture,
        crossterm::terminal::LeaveAlternateScreen
    )?;
    Ok(())
}
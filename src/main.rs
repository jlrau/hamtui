mod app;
mod event;
mod hamachi;
mod tui;
mod ui;

use color_eyre::Result;

use app::App;
use event::EventHandler;
use tui::Tui;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let mut app = App::new();
    let mut tui = Tui::new()?;
    tui.enter()?;

    let mut event_handler = EventHandler::new(250);

    // Initial refresh
    app.refresh_status().await;

    loop {
        tui.draw(&mut app)?;

        let event = event_handler.next().await?;
        app.handle_event(event).await;
        app.poll_command_result().await;

        if app.should_quit {
            break;
        }
    }

    tui.exit()?;
    Ok(())
}

pub mod logger;
mod network;
pub mod state;
pub mod ui;

use state::APP;
use std::error::Error;
use tokio::task::spawn;
use ui::chat::ChatScreen;
use ui::login::LoginScreen;
use ui::tui;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    logger::initialize();

    // Setup the network loop
    let (mut network_client, network_event_loop) = network::new().await?;

    // Run it in the background
    spawn(network_event_loop.run());

    match network_client
        .start_listening("/ip4/0.0.0.0/tcp/0".parse()?)
        .await
    {
        Ok(_) => {}
        Err(e) => logger::info!("Error connecting: {:?}", e),
    }

    // Setup the UI
    let mut terminal = tui::init()?;
    terminal.clear()?;

    let mut login_screen = LoginScreen::default();
    let mut chat_screen = ChatScreen::default();

    loop {
        let app = APP.lock().unwrap();
        let screen = app.screen.clone();
        drop(app);

        // Rendering
        terminal.draw(|f| match screen {
            state::Screen::Login => login_screen.render(f),
            state::Screen::Chat => chat_screen.render(f),
        })?;

        // Events
        match screen {
            state::Screen::Login => login_screen.handle_events(&mut network_client).await?,
            state::Screen::Chat => chat_screen.handle_events(&mut network_client).await?,
        }

        let app = APP.lock().unwrap();
        let quitting = app.quitting.clone();
        drop(app);
        if quitting {
            break;
        }
    }

    tui::restore()?;
    Ok(())
}

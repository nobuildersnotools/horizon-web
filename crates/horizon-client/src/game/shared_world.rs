//! Shared worlds in the browser build: Open to LAN shares the world through
//! a relay (horizon-os net.rs), whose join code comes a moment later. The
//! window tells the player the code, or why sharing failed.
use super::Game;
use horizon_client::text::Text;

impl Game<'_> {
    /// The relay's answer to Open to LAN, in the chat.
    pub(super) fn poll_shared_world(&mut self) {
        let Some(news) = horizon_os::net::shared_world_news() else {
            return;
        };
        let message = match news {
            Ok(code) => Text::literal("Shared world opened. Join code: ")
                .append(Text::copyable(code))
                .append(" (other players type it in Join Shared World, on the title screen)"),
            Err(reason) => Text::literal(format!("Could not share the world: {reason}"))
                .with_color(horizon_client::text::RED),
        };
        self.system_message(message);
    }
}

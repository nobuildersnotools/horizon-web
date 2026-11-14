//! Auto-reconnect (`autoReconnect`, docs/extras.md): the connection's start, join and end reach
//! the tracker (`horizon_client::reconnect`), and the Disconnected screen counts down to the next
//! attempt. The engine also reads the last server from it (`server_address`, `server_is_lan`).
use crate::game::Game;
use horizon_client::{multiplayer, reconnect::Retry, title::Mode};

impl Game<'_> {
    /// Auto-reconnect: a connection to `address` as `name` starts.
    pub(in crate::game) fn extras_connecting(&mut self, address: &str, name: &str, lan: bool) {
        self.extras.reconnect.connecting(address, name, lan);
    }
    /// The server this game connected to last.
    pub(in crate::game) fn server_address(&self) -> Option<&str> {
        self.extras.reconnect.address()
    }
    /// The server this game connected to last was found on the local network.
    pub(in crate::game) fn server_is_lan(&self) -> bool {
        self.extras.reconnect.lan()
    }
    /// Auto-reconnect: the server let the player in.
    pub(in crate::game) fn extras_joined(&mut self) {
        self.extras
            .reconnect
            .joined(horizon_os::time::Instant::now());
    }
    /// Auto-reconnect: the player left the server on purpose.
    pub(in crate::game) fn extras_left(&mut self) {
        self.extras.reconnect.reset();
    }
    /// Auto-reconnect's next attempt after a disconnect, when it is on.
    pub(in crate::game) fn reconnect_after(
        &mut self,
        _reason: &str,
        _key: Option<&str>,
        _from_server: bool,
    ) -> Option<Retry> {
        // Incari's Disconnected screen has Reconnect and Auto Reconnect
        // buttons, so the server is kept with the setting off, and any reason
        // is retried, kicks too: the player turned it on there for that.
        if !self.settings.extras.auto_reconnect {
            return None;
        }
        let now = horizon_os::time::Instant::now();
        self.extras.reconnect.ended_any(now)
    }
    /// The Disconnected screen's buttons and the attempt's countdown.
    pub(in crate::game) fn offer_reconnect(&mut self, retry: Option<Retry>) {
        let offered = self.extras.reconnect.address().is_some();
        let auto = self.settings.extras.auto_reconnect;
        if let Some(title) = self.title.as_mut()
            && let Mode::Multiplayer(multiplayer::Screen::Disconnected {
                retry: slot,
                rejoin,
                ..
            }) = &mut title.mode
        {
            rejoin.offered = offered;
            rejoin.auto = auto;
            if let Some(retry) = retry {
                log::info!(
                    "Horizon: reconnecting to {} in {} s",
                    horizon_client::net::shown_address(&retry.address),
                    retry.seconds_left(horizon_os::time::Instant::now())
                );
                *slot = Some(retry);
            }
        }
    }
    /// Reconnect, and Auto Reconnect turned on or off, on the Disconnected
    /// screen.
    fn extras_rejoin_clicks(&mut self) {
        let Some(asked) = self.title.as_mut().and_then(|t| match &mut t.mode {
            Mode::Multiplayer(screen) => screen.take_asked(),
            _ => None,
        }) else {
            return;
        };
        let now = horizon_os::time::Instant::now();
        let retry = match asked {
            multiplayer::Asked::Reconnect => self.extras.reconnect.attempt_at(now),
            multiplayer::Asked::ToggleAuto => {
                let on = !self.settings.extras.auto_reconnect;
                self.settings.extras.auto_reconnect = on;
                self.settings.save(&Self::options_path());
                // Turned on here: a full countdown, attempts counted anew.
                self.extras.reconnect.restart();
                if on {
                    self.extras.reconnect.ended_any(now)
                } else {
                    None
                }
            }
        };
        let auto = self.settings.extras.auto_reconnect;
        if let Some(title) = self.title.as_mut()
            && let Mode::Multiplayer(multiplayer::Screen::Disconnected {
                retry: slot,
                rejoin,
                ..
            }) = &mut title.mode
        {
            rejoin.auto = auto;
            *slot = retry;
        }
    }
    /// Auto-reconnect: connect again once the countdown is over. An
    /// online-mode server gets the signed-in account again: `connect_server`
    /// takes it from `auth::current()`, held in memory for the whole run
    /// (the launcher's session file is gone after the first read).
    pub(in crate::game) fn extras_retry(&mut self) -> horizon_client::Result<()> {
        self.extras_rejoin_clicks();
        if let Some(retry) = self.title.as_mut().and_then(|t| t.due_retry()) {
            self.connect_server_with_options(
                &retry.address,
                &retry.name,
                None,
                retry.lan,
                self.realm_session,
            )?;
        }
        Ok(())
    }
}

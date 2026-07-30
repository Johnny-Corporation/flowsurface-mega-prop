#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsInfo {
    StartOnLaunch,
    OrderConfirmation,
    DirectConnections,
    RiskGuard,
}

impl SettingsInfo {
    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::StartOnLaunch => "Start on system launch",
            Self::OrderConfirmation => "Confirm order placement",
            Self::DirectConnections => "Direct exchange connections",
            Self::RiskGuard => "Local risk guard",
        }
    }

    pub(crate) const fn body(self) -> &'static str {
        match self {
            Self::StartOnLaunch => {
                "Opens Flowsurface when the OS user session starts. Useful for a dedicated trading workstation, but keep it disabled on shared machines."
            }
            Self::OrderConfirmation => {
                "Adds a confirmation step before manual orders. This slows one-click execution slightly, but helps prevent accidental live orders."
            }
            Self::DirectConnections => {
                "Lets the terminal talk directly to exchange APIs. It is acceptable for trusted beta subaccounts, while the proxy path should own mature risk checks."
            }
            Self::RiskGuard => {
                "Shows and enforces local safety checks where possible. Treat it as a beta guardrail, not a replacement for server-side risk controls."
            }
        }
    }
}

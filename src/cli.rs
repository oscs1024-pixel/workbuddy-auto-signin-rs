#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Auto,
    Silent,
    Growth,
    SilentPoll,
    SilentGrowth,
    Status,
    Claim,
    All,
}

impl Action {
    pub const NAMES: &'static [&'static str] = &[
        "auto",
        "silent",
        "growth",
        "silent-poll",
        "silent-growth",
        "status",
        "claim",
        "all",
    ];

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "silent" => Some(Self::Silent),
            "growth" => Some(Self::Growth),
            "silent-poll" => Some(Self::SilentPoll),
            "silent-growth" => Some(Self::SilentGrowth),
            "status" => Some(Self::Status),
            "claim" => Some(Self::Claim),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Silent => "silent",
            Self::Growth => "growth",
            Self::SilentPoll => "silent-poll",
            Self::SilentGrowth => "silent-growth",
            Self::Status => "status",
            Self::Claim => "claim",
            Self::All => "all",
        }
    }

    pub fn is_poll(self) -> bool {
        matches!(self, Self::SilentPoll | Self::SilentGrowth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legacy_silent_growth() {
        assert_eq!(Action::parse("silent-growth"), Some(Action::SilentGrowth));
        assert!(Action::SilentGrowth.is_poll());
    }

    #[test]
    fn rejects_unknown_action() {
        assert_eq!(Action::parse("wat"), None);
    }
}

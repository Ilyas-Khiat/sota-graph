use std::fmt::Display;

/// Why a config file was refused.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The text is not valid TOML for the expected shape (typos in field names included).
    #[error("{file}: {message}")]
    Parse { file: String, message: String },

    /// The text parsed, but breaks a rule. Every broken rule is listed, not just the first.
    #[error("invalid config:\n{}", bullet_list(.problems))]
    Invalid { problems: Vec<String> },
}

impl ConfigError {
    pub(crate) fn parse(file: &str, err: impl Display) -> Self {
        Self::Parse {
            file: file.to_owned(),
            message: err.to_string().trim_end().to_owned(),
        }
    }

    /// `Ok(())` when there is nothing to report.
    pub(crate) fn check(problems: Vec<String>) -> Result<(), Self> {
        if problems.is_empty() {
            Ok(())
        } else {
            Err(Self::Invalid { problems })
        }
    }
}

fn bullet_list(problems: &[String]) -> String {
    problems
        .iter()
        .map(|p| format!("  - {p}"))
        .collect::<Vec<_>>()
        .join("\n")
}

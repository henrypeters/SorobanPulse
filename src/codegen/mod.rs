//! Subscription scaffolding code generator.
//!
//! Generates Rust handler modules, SQL migrations, webhook delivery code,
//! content filter configs, and test suites from name and channel-type inputs.

pub mod filter;
pub mod subscription;
pub mod tests;
pub mod webhook;

use std::{fs, path::Path};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum ChannelType {
    Webhook,
    Email,
    Sms,
}

impl ChannelType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChannelType::Webhook => "webhook",
            ChannelType::Email => "email",
            ChannelType::Sms => "sms",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "webhook" => Ok(ChannelType::Webhook),
            "email" => Ok(ChannelType::Email),
            "sms" => Ok(ChannelType::Sms),
            other => Err(format!(
                "unknown channel type '{}'; expected webhook, email, or sms",
                other
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScaffoldConfig {
    pub name: String,
    pub snake_name: String,
    pub pascal_name: String,
    pub channel_type: ChannelType,
    pub with_filter: bool,
    pub with_tests: bool,
}

impl ScaffoldConfig {
    pub fn new(
        name: &str,
        channel_type: ChannelType,
        with_filter: bool,
        with_tests: bool,
    ) -> Self {
        ScaffoldConfig {
            name: name.to_string(),
            snake_name: to_snake_case(name),
            pascal_name: to_pascal_case(name),
            channel_type,
            with_filter,
            with_tests,
        }
    }
}

fn to_snake_case(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            ' ' | '-' => '_',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

fn to_pascal_case(s: &str) -> String {
    s.split(|c: char| c == '_' || c == '-' || c == ' ')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut chars = p.chars();
            match chars.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().to_string() + &chars.as_str().to_lowercase(),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

pub struct GeneratedFile {
    pub relative_path: String,
    pub content: String,
}

// ---------------------------------------------------------------------------
// Orchestration
// ---------------------------------------------------------------------------

pub fn generate_all(config: &ScaffoldConfig) -> Vec<GeneratedFile> {
    let mut files = vec![
        subscription::generate_handler(config),
        subscription::generate_migration(config),
        webhook::generate(config),
    ];
    if config.with_filter {
        files.push(filter::generate(config));
    }
    if config.with_tests {
        files.push(tests::generate(config));
    }
    files
}

pub fn write_files(files: &[GeneratedFile], output_dir: &Path) -> anyhow::Result<()> {
    for file in files {
        let dest = output_dir.join(&file.relative_path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest, &file.content)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Template helper
// ---------------------------------------------------------------------------

pub(crate) fn apply(template: &str, config: &ScaffoldConfig) -> String {
    template
        .replace("{{PASCAL}}", &config.pascal_name)
        .replace("{{SNAKE}}", &config.snake_name)
        .replace("{{CHANNEL}}", config.channel_type.as_str())
}

#[cfg(test)]
mod tests_mod {
    use super::*;

    #[test]
    fn snake_case_conversion() {
        assert_eq!(to_snake_case("MySubscription"), "mysubscription");
        assert_eq!(to_snake_case("my-subscription"), "my_subscription");
        assert_eq!(to_snake_case("token transfer"), "token_transfer");
    }

    #[test]
    fn pascal_case_conversion() {
        assert_eq!(to_pascal_case("token_transfer"), "TokenTransfer");
        assert_eq!(to_pascal_case("my-subscription"), "MySubscription");
        assert_eq!(to_pascal_case("nft sale"), "NftSale");
    }

    #[test]
    fn scaffold_config_derives_names() {
        let cfg = ScaffoldConfig::new("token-transfer", ChannelType::Webhook, false, false);
        assert_eq!(cfg.snake_name, "token_transfer");
        assert_eq!(cfg.pascal_name, "TokenTransfer");
    }

    #[test]
    fn channel_type_round_trips() {
        assert_eq!(ChannelType::parse("webhook").unwrap(), ChannelType::Webhook);
        assert_eq!(ChannelType::parse("EMAIL").unwrap(), ChannelType::Email);
        assert!(ChannelType::parse("fax").is_err());
    }

    #[test]
    fn generate_all_produces_expected_file_count() {
        let cfg = ScaffoldConfig::new("payment", ChannelType::Webhook, true, true);
        let files = generate_all(&cfg);
        assert_eq!(files.len(), 5); // handler, migration, webhook, filter, tests
    }
}

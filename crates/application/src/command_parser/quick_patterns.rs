//! Quick pattern matching for commands that don't need LLM parsing.

use domain::AgentCommand;

use super::{CommandParser, QuickPattern};

impl CommandParser {
    /// Build the list of quick-match patterns
    pub(super) fn build_quick_patterns() -> Vec<QuickPattern> {
        vec![
            // Echo command
            QuickPattern {
                keywords: vec!["echo", "sag", "sage"],
                builder: |input| {
                    let lower = input.to_lowercase();
                    for keyword in ["echo ", "sag ", "sage "] {
                        if lower.starts_with(keyword) {
                            // Get the original casing
                            let message = &input[keyword.len()..];
                            return Some(AgentCommand::Echo {
                                message: message.to_string(),
                            });
                        }
                    }
                    None
                },
            },
            // Help command
            QuickPattern {
                keywords: vec!["help", "?"],
                builder: |input| {
                    let lower = input.to_lowercase().trim().to_string();
                    if lower == "help" || lower == "?" {
                        return Some(AgentCommand::Help { command: None });
                    }
                    if let Some(topic) = lower.strip_prefix("help ") {
                        return Some(AgentCommand::Help {
                            command: Some(topic.trim().to_string()),
                        });
                    }
                    None
                },
            },
            // Status command
            QuickPattern {
                keywords: vec!["status", "ping"],
                builder: |input| {
                    let lower = input.to_lowercase().trim().to_string();
                    if lower == "status" || lower == "ping" {
                        return Some(AgentCommand::System(domain::SystemCommand::Status));
                    }
                    None
                },
            },
            // Version command
            QuickPattern {
                keywords: vec!["version"],
                builder: |input| {
                    if input.to_lowercase().trim() == "version" {
                        return Some(AgentCommand::System(domain::SystemCommand::Version));
                    }
                    None
                },
            },
            // Models command
            QuickPattern {
                keywords: vec!["models"],
                builder: |input| {
                    let lower = input.to_lowercase().trim().to_string();
                    if lower == "models" {
                        return Some(AgentCommand::System(domain::SystemCommand::ListModels));
                    }
                    None
                },
            },
            // Morning briefing
            QuickPattern {
                keywords: vec![
                    "briefing",
                    "morning",
                    "good morning",
                    "what's on",
                    "what is on",
                ],
                builder: |input| {
                    let lower = input.to_lowercase();
                    if lower.contains("briefing")
                        || lower == "good morning"
                        || lower.contains("what's on")
                        || lower.contains("what is on today")
                    {
                        // Parse date from input using date_parser
                        let date = crate::date_parser::extract_date_from_text(input);
                        return Some(AgentCommand::MorningBriefing { date });
                    }
                    None
                },
            },
            // Inbox summary
            QuickPattern {
                keywords: vec!["inbox", "mails", "e-mails", "emails"],
                builder: |input| {
                    let lower = input.to_lowercase();
                    if lower.contains("inbox")
                        || lower.contains("summarize mails")
                        || lower.contains("summarize email")
                    {
                        let only_important = lower.contains("important");
                        return Some(AgentCommand::SummarizeInbox {
                            count: None,
                            only_important: if only_important { Some(true) } else { None },
                        });
                    }
                    None
                },
            },
            // Web search
            QuickPattern {
                keywords: vec![
                    "suche im internet",
                    "such im internet",
                    "recherchiere",
                    "google",
                    "suche online",
                    "such online",
                    "finde heraus",
                    "was sagt das internet",
                    "search the web",
                    "search the internet",
                    "search online",
                    "look up",
                ],
                builder: |input| {
                    let lower = input.to_lowercase();

                    // Match patterns and extract the query
                    let query = Self::extract_search_query(&lower, input);
                    query.map(|q| AgentCommand::WebSearch {
                        query: q,
                        max_results: None,
                    })
                },
            },
            // List reminders
            QuickPattern {
                keywords: vec!["erinnerungen", "reminders", "was steht an"],
                builder: |input| {
                    let lower = input.to_lowercase();
                    if lower.contains("erinnerungen")
                        || lower.contains("reminders")
                        || lower.contains("was steht an")
                    {
                        let include_done = lower.contains("alle")
                            || lower.contains("all")
                            || lower.contains("erledigte")
                            || lower.contains("completed");
                        return Some(AgentCommand::ListReminders {
                            include_done: Some(include_done),
                        });
                    }
                    None
                },
            },
            // Transit search
            QuickPattern {
                keywords: vec![
                    "öpnv",
                    "verbindung",
                    "wie komme ich",
                    "how do i get to",
                    "transit to",
                    "directions to",
                    "route to",
                    "fahrt nach",
                    "bahn nach",
                    "bus nach",
                ],
                builder: |input| {
                    let lower = input.to_lowercase();

                    // Try to extract destination from common patterns
                    let destination = Self::extract_transit_destination(&lower, input);
                    destination.map(|to| AgentCommand::SearchTransit {
                        from: String::new(), // Empty means "from home/current location"
                        to,
                        departure: None,
                    })
                },
            },
        ]
    }

    /// Extract transit destination from input
    fn extract_transit_destination(lower: &str, original: &str) -> Option<String> {
        // Patterns to extract destination
        let prefixes = [
            "wie komme ich nach ",
            "wie komme ich zum ",
            "wie komme ich zur ",
            "wie komme ich zu ",
            "verbindung nach ",
            "verbindung zum ",
            "verbindung zur ",
            "verbindung zu ",
            "öpnv nach ",
            "öpnv zum ",
            "öpnv zur ",
            "öpnv zu ",
            "fahrt nach ",
            "fahrt zum ",
            "fahrt zur ",
            "bahn nach ",
            "bahn zum ",
            "bahn zur ",
            "bus nach ",
            "bus zum ",
            "bus zur ",
            "how do i get to ",
            "transit to ",
            "directions to ",
            "route to ",
        ];

        for prefix in prefixes {
            if lower.starts_with(prefix) {
                let dest = original[prefix.len()..].trim();
                // Remove trailing question mark
                let dest = dest.trim_end_matches('?').trim();
                if !dest.is_empty() {
                    return Some(dest.to_string());
                }
            }
        }

        // Also match patterns where trigger appears in middle
        for prefix in prefixes {
            if let Some(pos) = lower.find(prefix) {
                let dest = original[pos + prefix.len()..].trim();
                let dest = dest.trim_end_matches('?').trim();
                if !dest.is_empty() {
                    return Some(dest.to_string());
                }
            }
        }

        None
    }

    /// Extract search query from input based on matched pattern
    fn extract_search_query(lower: &str, original: &str) -> Option<String> {
        // Patterns with their prefixes to strip
        let prefixes = [
            "suche im internet nach ",
            "suche im internet ",
            "such im internet nach ",
            "such im internet ",
            "recherchiere nach ",
            "recherchiere ",
            "google nach ",
            "google ",
            "suche online nach ",
            "suche online ",
            "such online nach ",
            "such online ",
            "finde heraus ",
            "was sagt das internet zu ",
            "was sagt das internet über ",
            "was sagt das internet ",
            "search the web for ",
            "search the internet for ",
            "search online for ",
            "look up ",
        ];

        for prefix in prefixes {
            if lower.starts_with(prefix) {
                let query = original[prefix.len()..].trim().to_string();
                if !query.is_empty() {
                    return Some(query);
                }
            }
        }

        // Also match if keywords appear anywhere for shorter inputs
        let trigger_words = [
            "suche im internet",
            "such im internet",
            "recherchiere",
            "google",
            "suche online",
            "such online",
            "search the web",
            "search the internet",
            "search online",
        ];

        for trigger in trigger_words {
            if lower.contains(trigger) && lower.len() > trigger.len() + 3 {
                // Extract anything after the trigger word
                if let Some(pos) = lower.find(trigger) {
                    let after = &original[pos + trigger.len()..].trim();
                    // Clean up common connectors
                    let query = after
                        .trim_start_matches([' ', ':', '-', '\u{2013}', '\u{2014}'])
                        .trim_start_matches("nach ")
                        .trim_start_matches("for ")
                        .trim_start_matches("about ")
                        .trim();
                    if !query.is_empty() {
                        return Some(query.to_string());
                    }
                }
            }
        }

        None
    }
}
